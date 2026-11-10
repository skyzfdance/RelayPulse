use super::*;

#[tokio::test]
async fn aborting_immediately_after_start_drains_every_started_attempt_and_does_not_leave_running_metadata(
) {
    let f = Fixture::new();
    let mut stalled = success(Protocol::OpenaiCompatible);
    stalled.header_delay = Duration::from_secs(5);
    let q = f.question("immediate cancellation").await;
    for _ in 0..4 {
        let server = Server::new(vec![success(Protocol::OpenaiCompatible), stalled.clone()]);
        let site = f.site(&server.url, Protocol::OpenaiCompatible, &["model"], true);
        f.available(&site, 0, json!({})).await;
        let run = f.start(std::slice::from_ref(&q)).await;
        f.app.iq_test.abort_run().await.unwrap();
        let run = f.app.iq_test.run_details(run.id).await.unwrap();
        assert_eq!(run.status, RunStatus::Aborted);
        for entry in run.entries {
            assert_eq!(entry.status, EntryStatus::Aborted);
            if entry.attempt > 0 {
                assert!(entry.has_raw_stream);
                assert!(raw(&f.root(), &entry, entry.attempt).exists());
                assert_eq!(meta(&f.root(), &entry, entry.attempt)["status"], "aborted");
            } else {
                assert!(!entry.has_raw_stream);
                assert_eq!(entry.started_at_ms, None);
            }
        }
    }
    f.app.iq_test.stop().await;
}

#[tokio::test]
async fn participants_are_concurrent_questions_are_serial_and_entry_failures_do_not_stop_later_questions(
) {
    let f = Fixture::new();
    let mut slow = success(Protocol::OpenaiCompatible);
    slow.parts[0].0 = Duration::from_millis(300);
    let server = Server::new(vec![
        success(Protocol::OpenaiCompatible),
        success(Protocol::OpenaiCompatible),
        slow,
        Response::raw(401, "application/json", "{\"error\":\"denied\"}"),
        success(Protocol::OpenaiCompatible),
        success(Protocol::OpenaiCompatible),
    ]);
    let site = f.site(
        &server.url,
        Protocol::OpenaiCompatible,
        &["one", "two"],
        true,
    );
    f.available(&site, 0, json!(support::params())).await;
    f.available(&site, 1, json!({"temperature":0.2})).await;
    let q1 = f.question("question one").await;
    let q2 = f.question("question two").await;
    let run = f.start(&[q1.clone(), q2.clone()]).await;
    server.wait_requests(4).await;
    assert!(matches!(
        f.app
            .iq_test
            .start_run(IqStartInput {
                question_ids: vec![q1.id.clone()],
                retry_max: 0,
            })
            .await,
        Err(IqError::AlreadyRunning)
    ));
    assert!(matches!(
        f.app
            .iq_test
            .remove_participant(run.participants[0].id.clone())
            .await,
        Err(IqError::AlreadyRunning)
    ));
    assert!(matches!(
        f.app.iq_test.delete_runs(vec![run.id.clone()]).await,
        Err(IqError::AlreadyRunning)
    ));
    tokio::time::sleep(Duration::from_millis(40)).await;
    assert_eq!(
        server.requests().len(),
        4,
        "后题不能在本题慢参与者结束前开始"
    );
    let run = f.terminal(&run.id).await;
    assert_eq!(run.status, RunStatus::Completed);
    assert_eq!(
        run.summary.call_result.value,
        CallResultValue::PartialFailure
    );
    assert_eq!(
        (
            run.summary.call_result.success,
            run.summary.call_result.failed
        ),
        (3, 1)
    );
    assert_eq!(run.summary.progress_percent, 100);
    assert_eq!(run.summary.terminal_entries, 4);
    assert_eq!(run.summary.current_question, 2);
    assert!(run
        .summary
        .question_progress
        .iter()
        .all(|p| p.status == QuestionStatus::Completed && p.terminal_entries == 2));
    let requests = server.requests();
    for request in &requests[2..4] {
        assert_eq!(
            serde_json::from_slice::<Value>(&request.body).unwrap()["messages"][0]["content"],
            q1.content
        );
    }
    for request in &requests[4..6] {
        assert_eq!(
            serde_json::from_slice::<Value>(&request.body).unwrap()["messages"][0]["content"],
            q2.content
        );
    }
    assert!(run.entries.iter().all(|e| e.attempt == 1));
    assert_eq!((f.count("probe_records"), f.count("probe_states")), (0, 0));
    f.app.iq_test.stop().await;
}

#[tokio::test]
async fn manual_abort_cancels_open_connections_retains_metrics_and_converges_pending_entries_without_files(
) {
    let f = Fixture::new();
    let partial = Response::sse(&[
        ("data: {\"choices\":[{\"delta\":{\"reasoning_content\":\"thinking\"}}],\"usage\":{\"prompt_tokens\":5}}\n\n",0),
        ("data: [DONE]\n\n",5_000),
    ]);
    let mut connecting = success(Protocol::OpenaiCompatible);
    connecting.header_delay = Duration::from_secs(5);
    let server = Server::new(vec![
        success(Protocol::OpenaiCompatible),
        success(Protocol::OpenaiCompatible),
        partial,
        connecting,
    ]);
    let site = f.site(
        &server.url,
        Protocol::OpenaiCompatible,
        &["one", "two"],
        true,
    );
    f.available(&site, 0, json!({})).await;
    f.available(&site, 1, json!({})).await;
    let run = f
        .start(&[f.question("one").await, f.question("two").await])
        .await;
    server.wait_requests(4).await;
    // 给已经收到的增量一次写入机会，验证中止保存而非清空该段指标。
    tokio::time::sleep(Duration::from_millis(30)).await;
    tokio::time::timeout(Duration::from_secs(2), f.app.iq_test.abort_run())
        .await
        .unwrap()
        .unwrap();
    let run = f.app.iq_test.run_details(run.id).await.unwrap();
    assert_eq!(run.status, RunStatus::Aborted);
    assert_eq!(run.summary.call_result.value, CallResultValue::Aborted);
    assert_eq!(
        (
            run.summary.call_result.aborted,
            run.summary.call_result.not_started
        ),
        (2, 2)
    );
    assert_eq!(run.summary.progress_percent, 100);
    assert!(run.entries.iter().all(|e| e.status == EntryStatus::Aborted));
    let started: Vec<_> = run.entries.iter().filter(|e| e.attempt > 0).collect();
    assert!(started
        .iter()
        .all(|e| e.has_raw_stream && e.total_time_ms.is_some()));
    assert!(started
        .iter()
        .any(|e| e.first_token_ms.is_some() && e.input_tokens == Some(5)));
    for entry in run.entries.iter().filter(|e| e.attempt == 0) {
        assert!(!entry.has_raw_stream);
        assert_eq!(entry.started_at_ms, None);
        assert_eq!(entry.total_time_ms, None);
        assert!(!raw(&f.root(), entry, 1).exists());
    }
    for entry in started {
        let meta = meta(&f.root(), entry, 1);
        assert_eq!(meta["status"], "aborted");
        assert_eq!(meta["bodyComplete"], false);
    }
    assert_eq!(server.requests().len(), 4);
    let count: i64 = f
        .database
        .with_connection(|c| {
            c.query_row(
                "SELECT COUNT(*) FROM operation_logs WHERE action='iq_run_aborted'",
                [],
                |r| r.get(0),
            )
        })
        .unwrap();
    f.app.iq_test.abort_run().await.unwrap();
    assert_eq!(count, 1);
    f.app.iq_test.stop().await;
}

#[tokio::test]
async fn waiting_without_an_error_has_no_local_deadline_or_retry_and_can_be_aborted() {
    let f = Fixture::new();
    let mut stalled = success(Protocol::OpenaiCompatible);
    stalled.header_delay = Duration::from_secs(10);
    let server = Server::new(vec![success(Protocol::OpenaiCompatible), stalled]);
    let site = f.site(&server.url, Protocol::OpenaiCompatible, &["model"], true);
    f.available(&site, 0, json!({})).await;
    let run = f
        .start_with_retries(&[f.question("unlimited reasoning").await], 5)
        .await;
    server.wait_requests(2).await;
    tokio::time::pause();
    tokio::time::advance(Duration::from_secs(7200)).await;
    tokio::time::resume();
    let current = f.app.iq_test.run_details(run.id.clone()).await.unwrap();
    assert_eq!(current.entries[0].status, EntryStatus::Running);
    assert_eq!(current.entries[0].attempt, 1);
    f.app.iq_test.abort_run().await.unwrap();
    let ended = f.terminal(&run.id).await;
    assert_eq!(ended.entries[0].status, EntryStatus::Aborted);
    assert_eq!(ended.entries[0].attempt, 1);
    assert_eq!(server.requests().len(), 2);
    assert!(raw(&f.root(), &ended.entries[0], 1).exists());
    f.app.iq_test.stop().await;
}

#[tokio::test]
async fn shutdown_cancels_prechecks_and_runs_and_drains_owned_tasks() {
    let f = Fixture::new();
    let mut stalled = success(Protocol::OpenaiCompatible);
    stalled.header_delay = Duration::from_secs(10);
    let server = Server::new(vec![
        success(Protocol::OpenaiCompatible),
        stalled.clone(),
        stalled,
    ]);
    let site = f.site(&server.url, Protocol::OpenaiCompatible, &["model"], true);
    f.available(&site, 0, json!({})).await;
    let run = f.start(&[f.question("stopped by lifecycle").await]).await;
    server.wait_requests(2).await;
    let mut settings = f.app.configuration.load().unwrap();
    settings.continue_in_background = false;
    f.app.configuration.save(settings).unwrap();
    let lifecycle = crate::application::lifecycle::Lifecycle::start(f.app.monitoring.clone())
        .unwrap()
        .with_iq_test(f.app.iq_test.clone());
    tokio::time::timeout(Duration::from_secs(2), lifecycle.stop())
        .await
        .unwrap();
    let reopened = f.reopen();
    assert_eq!(
        reopened.iq_test.run_details(run.id).await.unwrap().status,
        RunStatus::Aborted
    );
    let p = reopened
        .iq_test
        .create_participant(IqParticipantInput {
            model_key: format!("{}/{}", site.id, site.models[0].id),
            preset: "custom".into(),
            parameters: Default::default(),
        })
        .await
        .unwrap();
    let job = tokio::spawn({
        let service = reopened.iq_test.clone();
        async move { service.precheck(p.snapshot_id).await.unwrap() }
    });
    server.wait_requests(3).await;
    tokio::time::timeout(Duration::from_secs(2), reopened.iq_test.stop())
        .await
        .unwrap();
    assert_eq!(
        job.await.unwrap().precheck.status,
        PrecheckStatus::Unavailable
    );
}
