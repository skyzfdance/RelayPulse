use super::*;
use crate::modules::monitoring::{ProbeRecordsQuery, RecordError, TokenStatsQuery};
use std::{fs, path::PathBuf};

fn query() -> ProbeRecordsQuery {
    ProbeRecordsQuery {
        range: "all".into(),
        site_id: None,
        model_name: None,
        result: None,
        page: 1,
        page_size: 10,
    }
}
async fn list(f: &Fixture) -> Value {
    serde_json::to_value(f.monitoring.get_probe_records(query()).await.unwrap()).unwrap()
}
async fn latest(f: &Fixture) -> String {
    f.database
        .with_connection(|c| {
            c.query_row(
                "SELECT id FROM probe_records ORDER BY rowid DESC LIMIT 1",
                [],
                |r| r.get(0),
            )
        })
        .unwrap()
}
fn path(f: &Fixture, id: &str) -> PathBuf {
    f._directory
        .path()
        .join("com.relaypulse.app.dev/probe-records")
        .join(id)
        .join("response.jsonl")
}
fn write_log(f: &Fixture, id: &str, events: &[Value]) {
    fs::write(
        path(f, id),
        events.iter().map(|v| format!("{v}\n")).collect::<String>(),
    )
    .unwrap();
}
async fn response(f: &Fixture, id: &str) -> Value {
    serde_json::to_value(
        f.monitoring
            .get_probe_record_response(id.into())
            .await
            .unwrap(),
    )
    .unwrap()
}
async fn detail(f: &Fixture, id: &str) -> Value {
    serde_json::to_value(
        f.monitoring
            .get_probe_record_detail(id.into())
            .await
            .unwrap(),
    )
    .unwrap()
}
async fn request(f: &Fixture, id: &str) -> Value {
    serde_json::to_value(
        f.monitoring
            .get_probe_record_request(id.into())
            .await
            .unwrap(),
    )
    .unwrap()
}

#[tokio::test]
async fn queries_and_details_keep_frozen_names_policy_thresholds_request_and_provider_usage_after_edits(
) {
    let f = Fixture::new();
    let server=Server::new(vec![Response::json(json!({"status":"completed","output_text":"OK","usage":{"input_tokens":0,"output_tokens":0}}))
        .delayed(Duration::from_millis(1050))]);
    let id = f
        .store
        .create(
            &CreateSite {
                name: "frozen site".into(),
                protocol: Protocol::OpenaiResponses,
                base_url: server.url.clone(),
                secret: "secret-only-in-vault".into(),
                enabled: false,
            },
            &["frozen model".into()],
        )
        .unwrap();
    let mut connection = f.store.get(&id).unwrap().connection;
    connection.slow_total_seconds = 1;
    connection.headers =
        serde_json::from_value(json!([{"key":"X-Historical","value":"original"}])).unwrap();
    f.store.save_connection(&id, &connection).unwrap();
    f.monitoring.run(id.clone(), None).await.unwrap();
    let record = latest(&f).await;
    let before = detail(&f, &record).await;
    let request_before = request(&f, &record).await;
    assert!(request_before["headers"]
        .as_array()
        .unwrap()
        .iter()
        .any(|h| h["name"] == "user-agent"
            && h["value"].as_str().unwrap().starts_with("RelayPulse/")));
    let response_before = response(&f, &record).await;
    assert_eq!(before["result"], "slow");
    assert_eq!(before["slowThresholds"]["durationMs"], 1000);
    assert_eq!(before["slowEvaluation"]["duration"]["comparison"], "gte");
    assert_eq!(before["slowEvaluation"]["duration"]["triggered"], true);
    assert_eq!(
        before["usage"],
        json!({"inputTokens":0,"outputTokens":0,"totalTokens":null})
    );
    assert!(response_before["errorResponse"].is_null());
    connection.slow_total_seconds = 500;
    connection.headers[0].value = "new current header".into();
    f.store.save_connection(&id, &connection).unwrap();
    f.store
        .save_policy(
            &id,
            None,
            Some(&ProbePolicy {
                parameters: serde_json::from_value(json!([
        {"key":"input","value":"changed prompt","type":"string"},
        {"key":"reasoning.effort","value":"high","type":"string"}]))
                .unwrap(),
                ..Default::default()
            }),
            None,
        )
        .unwrap();
    f.store
        .save_basic(
            &id,
            &SiteDraft {
                name: "current site".into(),
                protocol: Protocol::OpenaiResponses,
                base_url: "http://127.0.0.1:1".into(),
                credential_action: CredentialAction::Replace,
                secret: "new-current-secret".into(),
            },
        )
        .unwrap();
    assert_eq!(detail(&f, &record).await, before);
    assert_eq!(request(&f, &record).await, request_before);
    assert_eq!(response(&f, &record).await, response_before);
    assert_eq!(list(&f).await["items"][0]["siteName"], "frozen site");
    let options =
        serde_json::to_value(f.monitoring.get_probe_record_options().await.unwrap()).unwrap();
    assert_eq!(options["sites"][0]["name"], "current site");
}

#[tokio::test]
async fn failure_bodies_cover_http_business_parse_interruption_large_json_sse_text_and_binary() {
    let f = Fixture::new();
    let large = format!("{{\"error\":\"{}\"}}", "中文 failure ".repeat(15_000));
    let large_sse = format!(
        "data: {{\"error\":{{\"message\":\"{}\"}}}}\n\n",
        "流式 failure ".repeat(15_000)
    );
    let business = json!({"error":{"type":"api_error","message":"business failed"}}).to_string();
    let sse="event: error\ndata: {\"error\":{\"message\":\"business failed\"}}\n\ndata: malformed-json\n\n";
    let mut interrupted = Response::raw(200, "application/json", "{\"error\":\"partial");
    interrupted.headers.push(("content-length", "2000".into()));
    let mut binary = Response::raw(503, "image/png", "");
    binary.parts[0].1 = vec![0, 255, 1, 2];
    let server = Server::new(vec![
        Response::raw(503, "application/json", large.clone()),
        Response::raw(200, "application/json", business.clone()),
        Response::raw(200, "application/json", "not json"),
        interrupted,
        Response::raw(200, "text/event-stream", sse),
        Response::raw(503, "text/html", "<h1>failure</h1>"),
        Response::raw(503, "text/plain", ""),
        binary,
        Response::raw(200, "text/event-stream", large_sse.clone()),
    ]);
    let site = f
        .store
        .create(
            &CreateSite {
                name: "failures".into(),
                protocol: Protocol::OpenaiResponses,
                base_url: server.url.clone(),
                secret: "fixture-secret".into(),
                enabled: false,
            },
            &["model".into()],
        )
        .unwrap();
    for (index, expected) in [
        Some(large.as_str()),
        Some(business.as_str()),
        Some("not json"),
        Some("{\"error\":\"partial"),
        None,
        Some("<h1>failure</h1>"),
        Some(""),
        None,
        Some(large_sse.as_str()),
    ]
    .into_iter()
    .enumerate()
    {
        f.monitoring.run(site.clone(), None).await.unwrap();
        let id = latest(&f).await;
        let read = response(&f, &id).await;
        assert_eq!(read["state"], "received");
        assert_eq!(detail(&f, &id).await["result"], "failure");
        assert_eq!(read["errorResponse"]["bodyComplete"], index != 3);
        if let Some(body) = expected {
            assert!(
                read["errorResponse"]["body"].as_str() == Some(body),
                "failure body differs in case {index}, expected {} UTF-8 bytes",
                body.len()
            );
        }
        if index == 4 {
            let body = read["errorResponse"]["body"].as_str().unwrap();
            assert!(body.contains("business failed"));
            assert!(body.contains("data: malformed-json"));
            let log = fs::read_to_string(path(&f, &id)).unwrap();
            let count = log
                .lines()
                .map(|l| serde_json::from_str::<Value>(l).unwrap())
                .filter(|v| v["event"] == "response_chunk")
                .count();
            assert_eq!(read["chunkCount"], count);
        }
        if index == 0 {
            assert!(read["errorResponse"]["body"].as_str().unwrap().len() > 64 * 1024);
            assert_eq!(read["receivedBytes"], large.len());
        }
        if index == 3 {
            assert_eq!(read["receivedBytes"], "{\"error\":\"partial".len());
        }
        if index == 6 {
            assert_eq!(read["receivedBytes"], 0);
        }
        if index == 7 {
            assert!(read["errorResponse"]["body"].is_null());
            assert_eq!(read["receivedBytes"], 4);
        }
    }
}

#[tokio::test]
async fn legacy_logs_keep_missing_headers_null_empty_snapshots_distinct_and_use_historical_counters(
) {
    let f = Fixture::new();
    let server = Server::new(vec![Response::raw(503, "text/plain", "failed")]);
    let site = f.site(&server.url, &["m"]);
    f.monitoring.run(site, None).await.unwrap();
    let id = latest(&f).await;
    // 旧日志的 JSON 请求体可保留本身的空白和字段顺序，不回写文件。
    let log=concat!("{\"event\":\"record_created\",\"formatVersion\":2}\n",
        "{\"event\":\"request_started\",\"method\":\"POST\",\"body\": { \"z\": \"中文\", \"a\": 0 }}\n",
        "{\"event\":\"response_started\",\"kind\":\"event_stream\"}\n",
        "{\"event\":\"response_chunk\",\"elapsedMs\":0,\"receivedBytes\":0,\"receivedTotal\":0}\n",
        "{\"event\":\"sse\",\"eventType\":\"message\",\"data\":\"first\",\"terminated\":true}\n",
        "{\"event\":\"sse\",\"eventType\":\"message\",\"data\":\"second\",\"terminated\":true}\n",
        "{\"event\":\"response_chunk\",\"elapsedMs\":9,\"receivedBytes\":12,\"receivedTotal\":12}\n");
    fs::write(path(&f, &id), log).unwrap();
    f.database.with_connection(|c|c.execute("UPDATE probe_records SET received_bytes=NULL,response_kind=NULL,body_complete=NULL WHERE id=?1",[&id])).unwrap();
    let req = request(&f, &id).await;
    assert_eq!(req["body"], "{ \"z\": \"中文\", \"a\": 0 }");
    assert!(req["headers"].is_null());
    assert_eq!(req["bodyBytes"], req["body"].as_str().unwrap().len());
    let read = response(&f, &id).await;
    assert_eq!(read["receivedBytes"], 12);
    assert_eq!(read["kind"], "stream");
    assert!(read["headers"].is_null());
    assert_eq!(read["chunkCount"], 2);
    assert_eq!(
        read["chunks"],
        json!([{"index":1,"elapsedMs":0,"bytes":0},{"index":2,"elapsedMs":9,"bytes":12}])
    );
    assert!(read["errorResponse"]["bodyComplete"].is_null());
    assert_eq!(fs::read_to_string(path(&f, &id)).unwrap(), log);
    let mut events = log
        .lines()
        .map(|l| serde_json::from_str::<Value>(l).unwrap())
        .collect::<Vec<_>>();
    events.push(json!({"event":"request_finished","receivedTotal":25,"bodyComplete":false}));
    write_log(&f, &id, &events);
    let read = response(&f, &id).await;
    assert_eq!(read["receivedBytes"], 25);
    assert_eq!(read["errorResponse"]["bodyComplete"], false);
    // 明确保存的空数组和空请求/响应体是数据，不等同于缺失。
    write_log(
        &f,
        &id,
        &[
            json!({"event":"request_started","method":"POST","body":"","headers":[]}),
            json!({"event":"response_started","kind":"event_stream","headers":[]}),
            json!({"event":"request_finished","receivedTotal":0,"bodyComplete":true}),
        ],
    );
    assert_eq!(
        request(&f, &id).await,
        json!({"method":"POST","bodyBytes":0,"headers":[],"body":""})
    );
    let read = response(&f, &id).await;
    assert_eq!(read["headers"], json!([]));
    assert_eq!(read["chunks"], json!([]));
    assert_eq!(read["chunkCount"], 0);
    assert_eq!(
        read["errorResponse"],
        json!({"body":"","bodyComplete":true})
    );
    write_log(
        &f,
        &id,
        &[
            json!({"event":"request_started","method":"POST"}),
            json!({"event":"response_started","kind":"event_stream"}),
        ],
    );
    let read = response(&f, &id).await;
    assert!(read["receivedBytes"].is_null());
    assert!(read["chunks"].is_null());
    assert!(read["errorResponse"]["body"].is_null());
    assert_eq!(
        detail(&f, &id).await["responseSummary"],
        json!({"state":read["state"],"kind":read["kind"],"receivedBytes":null})
    );
}

#[tokio::test]
async fn missing_corrupt_unreadable_files_report_errors_instead_of_unsaved_or_empty_content() {
    let f = Fixture::new();
    let server = Server::new(vec![Response::raw(200, "text/plain", "OK")]);
    let site = f.site(&server.url, &["m"]);
    f.monitoring.run(site, None).await.unwrap();
    let id = latest(&f).await;
    let file = path(&f, &id);
    let original = fs::read(&file).unwrap();
    for bytes in [
        b"".as_slice(),
        b"not JSON\n",
        b"{\"event\":\"request_started\",\"headers\":{}}\n",
        b"\xff\xfe",
    ] {
        fs::write(&file, bytes).unwrap();
        assert!(matches!(
            f.monitoring.get_probe_record_request(id.clone()).await,
            Err(RecordError::RequestUnavailable)
        ));
        assert!(matches!(
            f.monitoring.get_probe_record_response(id.clone()).await,
            Err(RecordError::ResponseUnavailable)
        ));
        assert!(matches!(
            f.monitoring
                .open_probe_record_log_directory(id.clone(), |_| panic!(
                    "must not open invalid log"
                ))
                .await,
            Err(RecordError::LogUnavailable)
        ));
    }
    fs::remove_file(&file).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::write(&file, &original).unwrap();
        fs::set_permissions(&file, fs::Permissions::from_mode(0o000)).unwrap();
        let unreadable = f.monitoring.get_probe_record_response(id.clone()).await;
        fs::set_permissions(&file, fs::Permissions::from_mode(0o600)).unwrap();
        assert!(matches!(unreadable, Err(RecordError::ResponseUnavailable)));
        fs::remove_file(&file).unwrap();
    }
    assert!(matches!(
        f.monitoring.get_probe_record_request(id.clone()).await,
        Err(RecordError::RequestUnavailable)
    ));
    fs::create_dir(&file).unwrap();
    assert!(matches!(
        f.monitoring.get_probe_record_response(id.clone()).await,
        Err(RecordError::ResponseUnavailable)
    ));
    fs::remove_dir(&file).unwrap();
    #[cfg(unix)]
    {
        let outside = f._directory.path().join("outside.jsonl");
        fs::write(&outside, &original).unwrap();
        std::os::unix::fs::symlink(&outside, &file).unwrap();
        assert!(matches!(
            f.monitoring.get_probe_record_response(id.clone()).await,
            Err(RecordError::ResponseUnavailable)
        ));
        fs::remove_file(&file).unwrap();
        assert_eq!(fs::read(outside).unwrap(), original);
    }
    // 明确关闭采集时才返回未保存；不会把期望存在但读取失败的文件伪装成未保存。
    f.database
        .with_connection(|c| {
            c.execute(
                "UPDATE probe_records SET capture_enabled=0 WHERE id=?1",
                [&id],
            )
        })
        .unwrap();
    let read = response(&f, &id).await;
    assert!(read["headers"].is_null());
    assert!(read["logFile"].is_null());
    assert_eq!(request(&f, &id).await["body"], Value::Null);
}

#[tokio::test]
async fn unfinished_publication_is_read_from_partial_and_large_request_text_is_not_truncated() {
    let f = Fixture::new();
    let server = Server::new(vec![Response::raw(503, "text/plain", "")]);
    let site = f.site(&server.url, &["m"]);
    f.monitoring.run(site, None).await.unwrap();
    let id = latest(&f).await;
    let large = "中文\n".repeat(100_000);
    write_log(
        &f,
        &id,
        &[
            json!({"event":"request_started","method":"POST","headers":[],"body":large}),
            json!({"event":"response_started","kind":"other","headers":[]}),
            json!({"event":"request_finished","receivedTotal":0,"bodyComplete":true}),
        ],
    );
    let read = request(&f, &id).await;
    assert_eq!(read["body"], large);
    assert_eq!(read["bodyBytes"], large.len());
    assert_eq!(
        detail(&f, &id).await["requestSummary"]["bodyBytes"],
        large.len()
    );
    let original = path(&f, &id).parent().unwrap().to_owned();
    let partial = original.with_file_name(format!("{id}.partial"));
    fs::rename(&original, &partial).unwrap();
    let partial = partial.canonicalize().unwrap();
    let read = response(&f, &id).await;
    assert_eq!(
        read["logFile"]["path"],
        partial.join("response.jsonl").to_str().unwrap()
    );
    f.monitoring
        .open_probe_record_log_directory(id, move |directory| {
            assert_eq!(directory, partial);
            Ok(())
        })
        .await
        .unwrap();
}

#[tokio::test]
async fn no_response_and_expired_records_are_not_confused_with_missing_or_unreported_content() {
    let f = Fixture::new();
    let site = f.site("http://127.0.0.1:1", &["m"]);
    f.monitoring.run(site.clone(), None).await.unwrap();
    let id = latest(&f).await;
    let read = response(&f, &id).await;
    assert_eq!(read["state"], "not_received");
    assert_eq!(read["kind"], Value::Null);
    assert_eq!(read["receivedBytes"], 0);
    assert_eq!(read["errorResponse"], Value::Null);
    assert!(read["headers"].is_null());
    assert!(read["chunks"].is_null());
    let model = f.store.get(&site).unwrap().models[0].id.clone();
    let states = f.monitoring.states(vec![model.clone()]).await.unwrap();
    let time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
        - 2 * 86_400_000;
    f.database
        .with_connection(|c| {
            c.execute(
                "UPDATE probe_records SET started_at_ms=?1,finished_at_ms=?1",
                [time],
            )
        })
        .unwrap();
    let mut settings = f.general.load().unwrap();
    settings.history_retention_days = 1;
    f.general.save(settings).unwrap();
    assert_eq!(list(&f).await["retainedCount"], 0);
    assert!(matches!(
        f.monitoring.get_probe_record_detail(id.clone()).await,
        Err(RecordError::NotFound)
    ));
    assert!(matches!(
        f.monitoring.delete_probe_record(id).await,
        Err(RecordError::NotFound)
    ));
    f.monitoring.clear_probe_records().await.unwrap();
    assert_eq!(f.count(), 0);
    assert_eq!(f.monitoring.states(vec![model]).await.unwrap(), states);
}

#[tokio::test]
async fn cancelled_waiter_keeps_history_access_and_lifecycle_drain_until_its_worker_finishes() {
    let f = Fixture::new();
    let server = Server::new(vec![Response::raw(200, "text/plain", "OK")]);
    let site = f.site(&server.url, &["m"]);
    f.monitoring.run(site, None).await.unwrap();
    let id = latest(&f).await;
    let (send, entered) = tokio::sync::oneshot::channel();
    let (release, wait) = std::sync::mpsc::channel();
    let service = f.monitoring.clone();
    let task = tokio::spawn(async move {
        service
            .open_probe_record_log_directory(id, move |_| {
                send.send(()).unwrap();
                wait.recv().unwrap();
                Ok(())
            })
            .await
    });
    entered.await.unwrap();
    task.abort();
    let _ = task.await;
    assert!(matches!(
        f.monitoring.clear_probe_records().await,
        Err(RecordError::Busy)
    ));
    release.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(2), f.monitoring.drained())
        .await
        .unwrap();
    f.monitoring.clear_probe_records().await.unwrap();
    assert_eq!(f.count(), 0);
    f.monitoring.close();
    assert!(matches!(
        f.monitoring.clear_probe_records().await,
        Err(RecordError::Unavailable)
    ));
    assert!(matches!(
        f.monitoring.get_probe_record_options().await,
        Err(RecordError::Unavailable)
    ));
}

#[tokio::test]
async fn database_delete_failure_is_reported_and_retry_does_not_require_already_deleted_files() {
    let f = Fixture::new();
    let server = Server::new(vec![Response::raw(200, "text/plain", "OK")]);
    let site = f.site(&server.url, &["m"]);
    f.monitoring.run(site.clone(), None).await.unwrap();
    let id = latest(&f).await;
    let before = f.execute(Operation::Get(site.clone())).await;
    f.database.with_connection(|c|c.execute_batch("CREATE TEMP TRIGGER reject_history_delete BEFORE DELETE ON probe_records BEGIN SELECT RAISE(ABORT,'injected'); END")).unwrap();
    assert!(matches!(
        f.monitoring.delete_probe_record(id.clone()).await,
        Err(RecordError::Unavailable)
    ));
    assert_eq!(f.count(), 1);
    assert!(!path(&f, &id).exists());
    assert!(matches!(
        f.monitoring.clear_probe_records().await,
        Err(RecordError::Unavailable)
    ));
    assert_eq!(f.count(), 1);
    assert_eq!(f.execute(Operation::Get(site.clone())).await, before);
    f.database
        .with_connection(|c| c.execute_batch("DROP TRIGGER reject_history_delete"))
        .unwrap();
    f.monitoring.delete_probe_record(id).await.unwrap();
    assert_eq!(f.count(), 0);
    assert_eq!(f.execute(Operation::Get(site)).await, before);
}

#[tokio::test]
async fn missing_file_adapter_cannot_report_success_for_history_with_saved_captures() {
    let f = Fixture::new();
    let server = Server::new(vec![Response::raw(200, "text/plain", "OK")]);
    let site = f.site(&server.url, &["m"]);
    f.monitoring.run(site, None).await.unwrap();
    let id = latest(&f).await;
    let without_files = Monitoring::new(
        f.sites.clone(),
        Arc::new(ModelClient::default()),
        f.database.clone(),
        f.general.clone(),
        f.diagnostics.clone(),
        None,
    );
    assert!(matches!(
        without_files.get_probe_record_request(id.clone()).await,
        Err(RecordError::RequestUnavailable)
    ));
    assert!(matches!(
        without_files.delete_probe_record(id.clone()).await,
        Err(RecordError::Unavailable)
    ));
    assert!(matches!(
        without_files.clear_probe_records().await,
        Err(RecordError::Unavailable)
    ));
    assert_eq!(f.count(), 1);
    assert!(path(&f, &id).exists());
    f.monitoring.delete_probe_record(id).await.unwrap();
    assert_eq!(f.count(), 0);
}

#[tokio::test]
async fn clear_and_delete_fail_on_active_capture_or_file_errors_then_retry_preserving_health_counts_and_iq(
) {
    let f = Fixture::new();
    let server = Server::new(vec![
        Response::raw(200, "text/plain", "OK"),
        Response::raw(200, "text/plain", "later").delayed(Duration::from_millis(250)),
    ]);
    let site = f.site(&server.url, &["m"]);
    f.monitoring.run(site.clone(), None).await.unwrap();
    let id = latest(&f).await;
    let service = f.monitoring.clone();
    let target = site.clone();
    let running = tokio::spawn(async move { service.run(target, None).await });
    server.wait_requests(2).await;
    assert!(matches!(
        f.monitoring.clear_probe_records().await,
        Err(RecordError::Busy)
    ));
    assert!(matches!(
        f.monitoring.delete_probe_record(id.clone()).await,
        Err(RecordError::Busy)
    ));
    running.await.unwrap().unwrap();
    let state = f
        .monitoring
        .states(vec![f.store.get(&site).unwrap().models[0].id.clone()])
        .await
        .unwrap();
    let directory = path(&f, &id).parent().unwrap().to_owned();
    let blocked = directory.join("unexpected-directory");
    fs::create_dir(&blocked).unwrap();
    assert!(matches!(
        f.monitoring.delete_probe_record(id.clone()).await,
        Err(RecordError::Unavailable)
    ));
    assert!(matches!(
        f.monitoring.clear_probe_records().await,
        Err(RecordError::Unavailable)
    ));
    assert_eq!(f.count(), 2);
    assert!(path(&f, &id).is_file());
    fs::remove_dir(&blocked).unwrap();
    let partial = directory.with_file_name(format!("{id}.partial"));
    fs::create_dir(&partial).unwrap();
    fs::write(partial.join("partial.bin"), b"pending").unwrap();
    let iq = f
        ._directory
        .path()
        .join("com.relaypulse.app.dev/iq-test/sentinel");
    fs::create_dir_all(iq.parent().unwrap()).unwrap();
    fs::write(&iq, b"IQ").unwrap();
    let options =
        serde_json::to_value(f.monitoring.get_probe_record_options().await.unwrap()).unwrap();
    f.monitoring.delete_probe_record(id.clone()).await.unwrap();
    assert_eq!(f.count(), 1);
    assert!(!directory.exists());
    assert!(!partial.exists());
    assert!(matches!(
        f.monitoring.get_probe_record_detail(id.clone()).await,
        Err(RecordError::NotFound)
    ));
    // all 清理还涵盖过期历史、遗留 partial；不从页面筛选或当前保留范围选取删除集合。
    f.database
        .with_connection(|c| c.execute("UPDATE probe_records SET finished_at_ms=0", []))
        .unwrap();
    let orphan = path(&f, "orphan")
        .parent()
        .unwrap()
        .with_file_name("orphan.partial");
    fs::create_dir(&orphan).unwrap();
    fs::write(orphan.join("response.jsonl"), b"partial").unwrap();
    let finder_metadata = orphan.parent().unwrap().join(".DS_Store");
    fs::write(&finder_metadata, b"Finder metadata").unwrap();
    f.monitoring.clear_probe_records().await.unwrap();
    assert_eq!(f.count(), 0);
    assert!(!orphan.exists());
    assert_eq!(fs::read(&finder_metadata).unwrap(), b"Finder metadata");
    assert_eq!(fs::read(&iq).unwrap(), b"IQ");
    assert_eq!(
        serde_json::to_value(f.monitoring.get_probe_record_options().await.unwrap()).unwrap(),
        options
    );
    assert_eq!(
        f.monitoring
            .states(state.keys().cloned().collect())
            .await
            .unwrap(),
        state
    );
    let stats = serde_json::to_value(
        f.monitoring
            .get_probe_token_stats(TokenStatsQuery {
                range: "all".into(),
                site_id: None,
                page: 100,
                page_size: 10,
            })
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(stats["retainedCount"], 0);
    assert_eq!(stats["page"], 1);
    assert_eq!(stats["summary"]["probeCount"], 0);
}
