use super::*;

#[tokio::test]
async fn explicit_http_and_response_errors_retry_with_the_same_snapshot_and_keep_each_attempt() {
    let f = Fixture::new();
    let server = Server::new(vec![
        success(Protocol::OpenaiCompatible),
        Response::raw(400, "application/json", "{\"error\":\"rejected\"}"),
        Response::raw(502, "application/json", "{\"error\":\"upstream failed\"}"),
        Response::raw(200, "text/event-stream", "data: not valid JSON\n\n"),
        success(Protocol::OpenaiCompatible),
    ]);
    let site = f.site(&server.url, Protocol::OpenaiCompatible, &["model"], true);
    f.available(&site, 0, json!({"temperature":0.3})).await;
    let run = f
        .start_with_retries(&[f.question("retry this question").await], 3)
        .await;
    let run = f.terminal(&run.id).await;
    let entry = &run.entries[0];
    assert_eq!(entry.status, EntryStatus::Completed);
    assert_eq!(entry.attempt, 4);
    let requests = server.requests();
    assert_eq!(requests.len(), 5);
    for request in &requests[2..] {
        assert_eq!(request.body, requests[1].body);
        assert_eq!(request.head, requests[1].head);
    }
    let first = meta(&f.root(), entry, 1);
    for attempt in 1..=4 {
        let details = meta(&f.root(), entry, attempt);
        assert_eq!(details["participant"], first["participant"]);
        assert_eq!(details["question"], first["question"]);
        assert_eq!(details["retryMax"], 3);
        assert_eq!(
            details["status"],
            if attempt == 4 { "completed" } else { "error" }
        );
        assert!(raw(&f.root(), entry, attempt).is_file());
    }
    let output = f
        .app
        .iq_test
        .entry_output(entry.id.clone(), None)
        .await
        .unwrap();
    assert_eq!(output.attempts.len(), 4);
    assert!(output.parts.iter().any(|part| part.text == "answer"));
    let failed = f
        .app
        .iq_test
        .entry_output(entry.id.clone(), Some(1))
        .await
        .unwrap();
    assert!(failed.raw_response.as_deref().unwrap().contains("rejected"));
    f.app.iq_test.stop().await;
}

#[tokio::test]
async fn retry_limit_counts_additional_requests_and_zero_disables_retries() {
    for retry_max in [0, 2] {
        let f = Fixture::new();
        let mut responses = vec![success(Protocol::OpenaiCompatible)];
        responses.extend((0..=retry_max).map(|_| Response::raw(503, "text/plain", "unavailable")));
        let server = Server::new(responses);
        let site = f.site(&server.url, Protocol::OpenaiCompatible, &["model"], true);
        f.available(&site, 0, json!({})).await;
        let run = f
            .start_with_retries(&[f.question("fail until exhausted").await], retry_max)
            .await;
        let run = f.terminal(&run.id).await;
        let entry = &run.entries[0];
        assert_eq!(entry.status, EntryStatus::Error);
        assert_eq!(entry.attempt, retry_max + 1);
        assert_eq!(entry.error_code.as_deref(), Some("upstream_http"));
        assert_eq!(server.requests().len(), retry_max as usize + 2);
        for attempt in 1..=entry.attempt {
            assert_eq!(meta(&f.root(), entry, attempt)["statusCode"], 503);
            assert_eq!(
                support::response_body(&f.root(), entry, attempt),
                b"unavailable"
            );
        }
        f.app.iq_test.stop().await;
    }
}

#[tokio::test]
async fn network_errors_use_the_retry_limit() {
    let f = Fixture::new();
    let server = Server::new(vec![success(Protocol::OpenaiCompatible)]);
    let site = f.site(&server.url, Protocol::OpenaiCompatible, &["model"], true);
    f.available(&site, 0, json!({})).await;
    drop(server);
    let run = f
        .start_with_retries(&[f.question("closed connection").await], 2)
        .await;
    let run = f.terminal(&run.id).await;
    let entry = &run.entries[0];
    assert_eq!(entry.status, EntryStatus::Error);
    assert_eq!(entry.attempt, 3);
    assert_eq!(entry.error_code.as_deref(), Some("network_error"));
    for attempt in 1..=3 {
        assert_eq!(
            meta(&f.root(), entry, attempt)["errorCode"],
            "network_error"
        );
    }
    f.app.iq_test.stop().await;
}

#[tokio::test]
async fn cancelling_a_retry_does_not_send_another_request() {
    let f = Fixture::new();
    let mut stalled = success(Protocol::OpenaiCompatible);
    stalled.header_delay = Duration::from_secs(10);
    let server = Server::new(vec![
        success(Protocol::OpenaiCompatible),
        Response::raw(502, "text/plain", "retry"),
        stalled,
    ]);
    let site = f.site(&server.url, Protocol::OpenaiCompatible, &["model"], true);
    f.available(&site, 0, json!({})).await;
    let run = f
        .start_with_retries(&[f.question("cancel while retrying").await], 5)
        .await;
    server.wait_requests(3).await;
    f.app.iq_test.abort_run().await.unwrap();
    let run = f.terminal(&run.id).await;
    let entry = &run.entries[0];
    assert_eq!(entry.status, EntryStatus::Aborted);
    assert_eq!(entry.attempt, 2);
    assert_eq!(server.requests().len(), 3);
    assert_eq!(meta(&f.root(), entry, 1)["status"], "error");
    assert_eq!(meta(&f.root(), entry, 2)["status"], "aborted");
    f.app.iq_test.stop().await;
}
