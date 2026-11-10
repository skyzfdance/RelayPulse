use super::*;
use std::fs;

#[tokio::test]
async fn raw_response_is_complete_even_when_the_bounded_parser_rejects_an_event_and_echoes_are_not_rewritten(
) {
    let f = Fixture::new();
    let first =
        format!("data: {{\"choices\":[{{\"delta\":{{\"reasoning_content\":\"{SECRET}\"}}}}]}}\n\n");
    let oversized = format!("data: {{\"unknown\":\"{}\"}}\n\n", "x".repeat(1_100_000));
    let tail = ": preserved heartbeat\n\ndata: [DONE]\n\n";
    let expected = format!("{first}{oversized}{tail}").into_bytes();
    let server = Server::new(vec![
        success(Protocol::OpenaiCompatible),
        Response::raw(
            200,
            "text/event-stream",
            String::from_utf8(expected.clone()).unwrap(),
        ),
    ]);
    let site = f.site(&server.url, Protocol::OpenaiCompatible, &["model"], false);
    f.available(&site, 0, json!({"metadata":{"note":SECRET}}))
        .await;
    let run = f.start(&[f.question("keep raw bytes").await]).await;
    let run = f.terminal(&run.id).await;
    let entry = &run.entries[0];
    assert_eq!(entry.status, EntryStatus::Error);
    assert_eq!(entry.attempt, 1);
    assert_eq!(entry.error_code.as_deref(), Some("invalid_response"));
    assert_eq!(support::response_body(&f.root(), entry, 1), expected);
    let details = meta(&f.root(), entry, 1);
    assert_eq!(details["bodyComplete"], true);
    assert_eq!(details["streamBytes"], expected.len());
    assert!(!details.to_string().contains(SECRET));
    assert!(!serde_json::to_string(&run).unwrap().contains(SECRET));
    // 上游回显属于响应事实；不把请求秘密主动写入 meta、SQL 或 DTO。
    // 本次只有思考增量，没有回答正文，因此不生成空 TXT 或其他产物。
    assert_eq!(details["outputFiles"], json!([]));
    assert!(!raw(&f.root(), entry, 1)
        .with_file_name("output.1.txt")
        .exists());
    let stored: String = f
        .database
        .with_connection(|c| {
            c.query_row(
                "SELECT participants_json FROM iq_runs WHERE id=?1",
                [&run.id],
                |r| r.get(0),
            )
        })
        .unwrap();
    assert!(!stored.contains(SECRET));
    let diagnostics = crate::shared::diagnostics::Diagnostics::new(&f.app.environment);
    let package = diagnostics.package().unwrap();
    assert!(!String::from_utf8(package).unwrap().contains(SECRET));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let directory = raw(&f.root(), entry, 1).parent().unwrap().to_owned();
        assert_eq!(
            fs::metadata(&directory).unwrap().permissions().mode() & 0o777,
            0o700
        );
        for name in ["response.1.jsonl", "meta.1.json"] {
            assert_eq!(
                fs::metadata(directory.join(name))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
    }
    assert_eq!(
        server.requests().len(),
        2,
        "解析或 HTTP 错误不能作为无输出超时重试"
    );
    f.app.iq_test.stop().await;
}

#[tokio::test]
async fn non_streaming_fallback_keeps_ttft_absent_in_sql_meta_and_after_restart() {
    let f = Fixture::new();
    let body = json!({"choices":[{"message":{"content":"non streaming answer"},"finish_reason":"stop"}],"usage":{"prompt_tokens":1,"completion_tokens":2}});
    let server = Server::new(vec![
        success(Protocol::OpenaiCompatible),
        Response::json(body.clone()),
    ]);
    let site = f.site(&server.url, Protocol::OpenaiCompatible, &["model"], true);
    f.available(&site, 0, json!({})).await;
    let run = f.start(&[f.question("JSON fallback").await]).await;
    let run = f.terminal(&run.id).await;
    assert_eq!(run.entries[0].status, EntryStatus::Completed);
    assert_eq!(run.entries[0].first_token_ms, None);
    assert!(meta(&f.root(), &run.entries[0], 1)["firstTokenMs"].is_null());
    assert_eq!(
        support::response_body(&f.root(), &run.entries[0], 1),
        body.to_string().as_bytes()
    );
    f.app.iq_test.stop().await;
    let reopened = f.reopen();
    assert_eq!(
        reopened.iq_test.run_details(run.id).await.unwrap().entries[0].first_token_ms,
        None
    );
    reopened.iq_test.stop().await;
}

#[tokio::test]
async fn evidence_creation_failure_is_a_terminal_entry_error_without_sending_a_model_request() {
    let f = Fixture::new();
    let server = Server::new(vec![success(Protocol::OpenaiCompatible)]);
    let site = f.site(&server.url, Protocol::OpenaiCompatible, &["model"], true);
    f.available(&site, 0, json!({})).await;
    fs::remove_dir(f.root().join("runs")).unwrap();
    fs::write(f.root().join("runs"), b"unwritable evidence directory").unwrap();
    let run = f
        .start_with_retries(&[f.question("must keep a database record").await], 5)
        .await;
    let run = f.terminal(&run.id).await;
    assert_eq!(run.status, RunStatus::Completed);
    assert_eq!(
        run.entries[0].error_code.as_deref(),
        Some("evidence_write_failed")
    );
    assert!(!run.entries[0].has_raw_stream);
    assert_eq!(run.entries[0].attempt, 1);
    assert_eq!(server.requests().len(), 1);
    f.app.iq_test.stop().await;
}

#[tokio::test]
async fn failed_database_updates_cancel_the_queue_and_recovery_preserves_written_evidence() {
    let f = Fixture::new();
    let mut delayed = success(Protocol::OpenaiCompatible);
    delayed.parts[0].0 = Duration::from_millis(150);
    let server = Server::new(vec![success(Protocol::OpenaiCompatible), delayed]);
    let site = f.site(&server.url, Protocol::OpenaiCompatible, &["model"], true);
    f.available(&site, 0, json!({})).await;
    let run = f
        .start(&[f.question("one").await, f.question("never sent").await])
        .await;
    server.wait_requests(2).await;
    f.database.with_connection(|c| c.execute_batch("CREATE TRIGGER refuse_iq_updates BEFORE UPDATE ON iq_entries BEGIN SELECT RAISE(ABORT,'test storage failure'); END;")).unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if matches!(f.app.iq_test.snapshot().await, Err(IqError::Storage)) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    f.app.iq_test.stop().await;
    assert_eq!(server.requests().len(), 2);
    f.database
        .with_connection(|c| c.execute_batch("DROP TRIGGER refuse_iq_updates"))
        .unwrap();
    let reopened = f.reopen();
    let recovered = reopened.iq_test.run_details(run.id).await.unwrap();
    assert_eq!(recovered.status, RunStatus::Aborted);
    assert!(recovered
        .entries
        .iter()
        .all(|e| e.status == EntryStatus::Aborted));
    let started = &recovered.entries[0];
    assert!(started.has_raw_stream);
    assert_eq!(started.input_tokens, Some(4));
    assert!(started.first_token_ms.is_some());
    assert!(support::response_body(&f.root(), started, 1).ends_with(b"data: [DONE]\n\n"));
    assert_eq!(recovered.entries[1].attempt, 0);
    assert!(reopened
        .iq_test
        .snapshot()
        .await
        .unwrap()
        .participants
        .is_empty());
    reopened.iq_test.stop().await;
}

#[tokio::test]
async fn startup_aborts_only_unfinished_entries_preserves_known_metrics_and_never_resumes_requests()
{
    let f = Fixture::new();
    let server = Server::new(vec![success(Protocol::OpenaiCompatible); 4]);
    let site = f.site(&server.url, Protocol::OpenaiCompatible, &["model"], true);
    f.available(&site, 0, json!({})).await;
    let run = f
        .start(&[
            f.question("done").await,
            f.question("interrupted").await,
            f.question("not started").await,
        ])
        .await;
    let mut run = f.terminal(&run.id).await;
    f.app.iq_test.stop().await;
    run.status = RunStatus::Running;
    run.finished_at_ms = None;
    run.summary.duration_ms = 321;
    run.entries[1].status = EntryStatus::Running;
    run.entries[1].finished_at_ms = None;
    run.entries[1].has_raw_stream = false;
    run.entries[1].first_token_ms = None;
    run.entries[1].input_tokens = None;
    let entry = &run.entries[1];
    let meta_path = raw(&f.root(), entry, 1).with_file_name("meta.1.json");
    let mut data = meta(&f.root(), entry, 1);
    data["status"] = json!("running");
    data["bodyComplete"] = json!(false);
    fs::write(&meta_path, serde_json::to_vec(&data).unwrap()).unwrap();
    let pending = &mut run.entries[2];
    fs::remove_dir_all(raw(&f.root(), pending, 1).parent().unwrap()).unwrap();
    pending.status = EntryStatus::Pending;
    pending.attempt = 0;
    pending.started_at_ms = None;
    pending.finished_at_ms = None;
    pending.first_token_ms = None;
    pending.total_time_ms = None;
    pending.input_tokens = None;
    pending.output_tokens = None;
    pending.has_raw_stream = false;
    support::persist_interrupted_run(&f.database, &run);
    let original_raw = fs::read(raw(&f.root(), &run.entries[1], 1)).unwrap();
    let reopened = f.reopen();
    let recovered = reopened.iq_test.run_details(run.id).await.unwrap();
    assert_eq!(recovered.status, RunStatus::Aborted);
    assert_eq!(recovered.entries[0].status, EntryStatus::Completed);
    assert_eq!(recovered.entries[1].status, EntryStatus::Aborted);
    assert_eq!(
        recovered.entries[1].error_code.as_deref(),
        Some("interrupted")
    );
    assert!(recovered.entries[1].has_raw_stream);
    assert_eq!(recovered.entries[1].input_tokens, Some(4));
    assert_eq!(
        recovered.entries[1].first_token_ms,
        data["firstTokenMs"].as_u64()
    );
    assert_eq!(recovered.summary.duration_ms, 321);
    assert_eq!(recovered.summary.call_result.not_started, 1);
    assert_eq!(
        meta(&f.root(), &recovered.entries[1], 1)["status"],
        "aborted"
    );
    assert_eq!(
        meta(&f.root(), &recovered.entries[1], 1)["bodyComplete"],
        false
    );
    assert_eq!(
        fs::read(raw(&f.root(), &recovered.entries[1], 1)).unwrap(),
        original_raw
    );
    let snapshot = reopened.iq_test.snapshot().await.unwrap();
    assert!(snapshot.active_run.is_none() && snapshot.participants.is_empty());
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert_eq!(server.requests().len(), 4);
    reopened.iq_test.stop().await;
}

#[tokio::test]
async fn failed_batch_delete_rolls_back_sql_and_restores_every_directory_then_retry_deletes_only_selected_runs(
) {
    let f = Fixture::new();
    let server = Server::new(vec![success(Protocol::OpenaiCompatible); 4]);
    let site = f.site(&server.url, Protocol::OpenaiCompatible, &["model"], true);
    f.available(&site, 0, json!({})).await;
    let q = f.question("history").await;
    let a = f.start(std::slice::from_ref(&q)).await;
    let a = f.terminal(&a.id).await;
    f.available(&site, 0, json!({})).await;
    let b = f.start(&[q]).await;
    let b = f.terminal(&b.id).await;
    let before = support::files(&f.root());
    f.database.with_connection(|c| c.execute_batch("CREATE TRIGGER refuse_iq_delete BEFORE DELETE ON iq_runs BEGIN SELECT RAISE(ABORT,'delete blocked'); END;")).unwrap();
    assert!(matches!(
        f.app
            .iq_test
            .delete_runs(vec![a.id.clone(), b.id.clone()])
            .await,
        Err(IqError::Storage)
    ));
    assert_eq!(f.count("iq_runs"), 2);
    assert_eq!(support::files(&f.root()), before);
    assert!(fs::read_dir(f.root().join("trash"))
        .unwrap()
        .next()
        .is_none());
    f.database
        .with_connection(|c| c.execute_batch("DROP TRIGGER refuse_iq_delete"))
        .unwrap();
    assert!(matches!(
        f.app
            .iq_test
            .delete_runs(vec![a.id.clone(), "../outside".into()])
            .await,
        Err(IqError::Validation { field: "ids", .. })
    ));
    assert!(raw(&f.root(), &a.entries[0], 1).exists());
    f.app
        .iq_test
        .delete_runs(vec![a.id.clone(), a.id.clone()])
        .await
        .unwrap();
    assert!(!raw(&f.root(), &a.entries[0], 1).exists());
    assert!(raw(&f.root(), &b.entries[0], 1).exists());
    assert_eq!(f.count("iq_entries"), 1);
    f.app.iq_test.delete_runs(vec![a.id]).await.unwrap();
    f.app.iq_test.stop().await;
}

#[cfg(unix)]
#[tokio::test]
async fn postcommit_cleanup_failure_is_retryable_and_startup_reconciles_only_explicitly_deleted_runs(
) {
    let f = Fixture::new();
    let server = Server::new(vec![success(Protocol::OpenaiCompatible); 4]);
    let site = f.site(&server.url, Protocol::OpenaiCompatible, &["model"], true);
    f.available(&site, 0, json!({})).await;
    let q = f.question("permanent retention").await;
    let a = f.start(std::slice::from_ref(&q)).await;
    let a = f.terminal(&a.id).await;
    f.available(&site, 0, json!({})).await;
    let b = f.start(&[q]).await;
    let b = f.terminal(&b.id).await;
    let outside = f.directory.path().join("outside");
    fs::write(&outside, b"keep me").unwrap();
    let link = f.root().join("runs").join(&a.id).join("unsafe-link");
    std::os::unix::fs::symlink(&outside, &link).unwrap();
    assert!(matches!(
        f.app.iq_test.delete_runs(vec![a.id.clone()]).await,
        Err(IqError::CleanupPending)
    ));
    assert_eq!(f.count("iq_runs"), 1);
    assert!(f.root().join("trash").join(&a.id).exists());
    assert_eq!(fs::read(&outside).unwrap(), b"keep me");
    fs::remove_file(f.root().join("trash").join(&a.id).join("unsafe-link")).unwrap();
    // 还原另一个“隔离后尚未提交”的中断窗口，启动时必须恢复它。
    fs::rename(
        f.root().join("runs").join(&b.id),
        f.root().join("trash").join(&b.id),
    )
    .unwrap();
    let orphan = f.root().join("runs").join("orphan-kept");
    fs::create_dir(&orphan).unwrap();
    fs::write(orphan.join("stream.1.sse"), b"not authorized for deletion").unwrap();
    f.app.iq_test.stop().await;
    let reopened = f.reopen();
    assert!(!f.root().join("trash").join(&a.id).exists());
    assert!(raw(&f.root(), &b.entries[0], 1).exists());
    assert!(orphan.exists());
    assert_eq!(fs::read(&outside).unwrap(), b"keep me");
    reopened.iq_test.delete_runs(vec![a.id]).await.unwrap();
    reopened.iq_test.stop().await;
}
