use super::*;
use std::{fs, path::PathBuf};

impl Fixture {
    fn capture_root(&self) -> PathBuf {
        self._directory
            .path()
            .join("com.relaypulse.app.dev/probe-records")
    }
    fn record_ids(&self) -> Vec<String> {
        self.database
            .with_connection(|c| {
                c.prepare("SELECT id FROM probe_records ORDER BY finished_at_ms")?
                    .query_map([], |r| r.get(0))?
                    .collect()
            })
            .unwrap()
    }
}
fn events(path: &std::path::Path) -> Vec<Value> {
    fs::read_to_string(path.join("response.jsonl"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn text_content(path: &std::path::Path) -> String {
    events(path)
        .iter()
        .filter(|e| e["event"] == "text")
        .map(|e| e["text"].as_str().unwrap())
        .collect()
}

#[tokio::test]
async fn trial_skips_files_and_formal_capture_is_associated_redacted_and_not_in_diagnostics() {
    let f = Fixture::new();
    let server = Server::new(vec![
        Response::raw(200, "text/plain", "trial private response"),
        Response::sse(&[
            ("data: {\"message\":\"opaque:secret-only-", 0),
            ("in-vault:end\"}\n\n", 10),
        ]),
    ]);
    let id = f.site(&server.url, &["model"]);
    let model = f.store.get(&id).unwrap().models[0].id.clone();
    f.execute(Operation::Test(
        id.clone(),
        model.clone(),
        CustomRequest {
            version: 2,
            endpoint: "/draft".into(),
            parameters: vec![],
        },
    ))
    .await;
    assert!(!f.capture_root().exists());
    assert_eq!(f.count(), 0);
    f.execute(Operation::Run(id.clone(), None)).await;
    let ids = f.record_ids();
    assert_eq!(ids.len(), 1);
    let directory = f.capture_root().join(&ids[0]);
    assert!(!directory.join("response.bin").exists());
    assert_eq!(
        events(&directory)
            .iter()
            .find(|e| e["event"] == "sse")
            .unwrap()["data"]["message"],
        "opaque:[REDACTED]:end"
    );
    let metadata = events(&directory);
    assert_eq!(metadata[0]["recordId"], ids[0]);
    assert_eq!(metadata[0]["siteId"], id);
    assert_eq!(metadata[0]["modelId"], model);
    assert_eq!(metadata.last().unwrap()["bodyComplete"], true);
    assert_eq!(metadata.last().unwrap()["outcome"]["status"], "succeeded");
    let package = String::from_utf8(f.diagnostics.package().unwrap()).unwrap();
    assert!(!package.contains("opaque:"));
    assert!(!package.contains("trial private response"));
    assert!(!package.contains("secret-only-in-vault"));
    f.execute(Operation::DeleteModel(id, model)).await;
    assert_eq!(f.count(), 0);
    assert!(!directory.exists());
}

#[tokio::test]
async fn http_failure_keeps_the_actual_error_body_and_site_deletion_cleans_files() {
    let f = Fixture::new();
    let server = Server::new(vec![Response::raw(
        503,
        "text/plain",
        "actual server error",
    )]);
    let id = f.site(&server.url, &["model"]);
    f.execute(Operation::Run(id.clone(), None)).await;
    let directory = f.capture_root().join(&f.record_ids()[0]);
    assert_eq!(text_content(&directory), "actual server error");
    assert!(!directory.join("response.bin").exists());
    let metadata = events(&directory);
    assert_eq!(metadata.last().unwrap()["outcome"]["failure"], "http");
    assert_eq!(metadata.last().unwrap()["bodyComplete"], true);
    f.execute(Operation::Delete(id)).await;
    assert!(!directory.exists());
}

#[tokio::test]
async fn output_failure_and_database_rollback_do_not_publish_or_change_health() {
    let f = Fixture::new();
    let server = Server::new(vec![Response::raw(200, "text/plain", "actual success")]);
    let id = f.site(&server.url, &["model"]);
    let before = f.execute(Operation::Get(id.clone())).await;
    fs::write(f.capture_root(), b"occupied").unwrap();
    let error = f
        .service
        .execute(Operation::Run(id.clone(), None))
        .await
        .err()
        .unwrap();
    assert!(matches!(error, SiteError::Capture));
    assert!(server.requests().is_empty());
    assert_eq!(f.count(), 0);
    assert_eq!(f.execute(Operation::Get(id.clone())).await, before);
    fs::remove_file(f.capture_root()).unwrap();
    f.database.with_connection(|c| c.execute_batch("CREATE TRIGGER fail_capture_commit BEFORE INSERT ON probe_records BEGIN SELECT RAISE(ABORT,'test'); END;")).unwrap();
    assert!(f
        .service
        .execute(Operation::Run(id.clone(), None))
        .await
        .is_err());
    assert_eq!(f.count(), 0);
    assert_eq!(f.execute(Operation::Get(id)).await, before);
    let directories: Vec<_> = fs::read_dir(f.capture_root())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    assert_eq!(directories.len(), 1);
    assert!(directories[0].to_str().unwrap().ends_with(".partial"));
    assert_eq!(text_content(&directories[0]), "actual success");
}

#[tokio::test]
async fn aborted_probe_releases_resources_but_cannot_publish_an_incomplete_record() {
    let f = Fixture::new();
    let server = Server::new(vec![Response::sse(&[
        ("received", 0),
        ("never received", 10_000),
    ])]);
    let id = f.site(&server.url, &["model"]);
    let service = f.service.clone();
    let task = tokio::spawn(async move { service.execute(Operation::Run(id, None)).await });
    server.wait_requests(1).await;
    task.abort();
    assert!(matches!(task.await, Err(error) if error.is_cancelled()));
    f.monitoring.close();
    tokio::time::timeout(Duration::from_secs(2), f.monitoring.drained())
        .await
        .unwrap();
    assert_eq!(f.count(), 0);
    let paths: Vec<_> = fs::read_dir(f.capture_root())
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    assert_eq!(paths.len(), 1);
    assert!(paths[0].to_str().unwrap().ends_with(".partial"));
    assert!(!events(&paths[0])
        .iter()
        .any(|e| e["event"] == "request_finished"));
}

#[tokio::test]
async fn committed_retention_and_confirmed_model_sync_remove_only_unretained_files() {
    let f = Fixture::new();
    let server = Server::new(vec![
        Response::raw(200, "text/plain", "first"),
        Response::raw(200, "text/plain", "second"),
        Response::json(json!(["replacement"])),
    ]);
    let id = f.site(&server.url, &["old"]);
    let model = f.store.get(&id).unwrap().models[0].id.clone();
    f.execute(Operation::Run(id.clone(), None)).await;
    let first = f.capture_root().join(&f.record_ids()[0]);
    f.database
        .with_connection(|c| c.execute("UPDATE probe_records SET finished_at_ms=0", []))
        .unwrap();
    f.execute(Operation::Run(id.clone(), None)).await;
    let second = f.capture_root().join(&f.record_ids()[0]);
    f.monitoring.cleanup_records().await;
    assert!(!first.exists());
    assert!(second.exists());
    f.execute(Operation::Sync(id.clone(), vec!["old".into()]))
        .await;
    assert!(second.exists());
    f.execute(Operation::Apply(
        id,
        ApplySync {
            models: vec!["old".into()],
            added: vec!["replacement".into()],
            removed: vec![model],
        },
    ))
    .await;
    assert!(!second.exists());
}

#[cfg(unix)]
#[tokio::test]
async fn abandoned_partial_files_expire_but_recent_partial_files_are_kept() {
    let f = Fixture::new();
    fs::create_dir(f.capture_root()).unwrap();
    let old = f.capture_root().join("old.partial");
    let recent = f.capture_root().join("recent.partial");
    fs::create_dir(&old).unwrap();
    fs::create_dir(&recent).unwrap();
    fs::write(old.join("body"), b"partial").unwrap();
    fs::File::open(&old)
        .unwrap()
        .set_times(
            fs::FileTimes::new().set_modified(std::time::UNIX_EPOCH + Duration::from_secs(1)),
        )
        .unwrap();
    f.monitoring.cleanup_records().await;
    assert!(!old.exists());
    assert!(recent.exists());
}

#[tokio::test]
async fn real_binary_response_uses_a_readable_log_and_an_image_attachment() {
    let f = Fixture::new();
    let bytes = b"\x89PNG\r\n\x1a\n\x00\xff";
    let mut response = Response::raw(200, "image/png", "");
    response.parts = vec![(Duration::ZERO, bytes.to_vec())];
    let server = Server::new(vec![response]);
    let id = f.site(&server.url, &["image"]);
    f.execute(Operation::Run(id, None)).await;
    let directory = f.capture_root().join(&f.record_ids()[0]);
    let metadata = events(&directory);
    assert_eq!(
        metadata
            .iter()
            .find(|e| e["event"] == "response_started")
            .unwrap()["contentType"],
        "image/png"
    );
    assert!(metadata
        .iter()
        .any(|e| e["event"] == "binary" && e["file"] == "response.png"));
    assert_eq!(fs::read(directory.join("response.png")).unwrap(), bytes);
    assert!(!directory.join("response.bin").exists());
}
