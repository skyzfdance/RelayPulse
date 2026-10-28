use super::super::{
    test_support::{request, Response, Server},
    *,
};
use futures_util::future::BoxFuture;
use std::time::Duration;

#[derive(Default)]
struct Capture {
    bytes: Vec<u8>,
    status: Option<u16>,
    content_type: Option<String>,
    finished: Option<(CaptureOutcome, bool)>,
    fail_chunk: bool,
}
impl CaptureSink for Capture {
    fn record<'a>(
        &'a mut self,
        event: CaptureEvent<'a>,
    ) -> BoxFuture<'a, Result<(), CaptureError>> {
        Box::pin(async move {
            match event {
                CaptureEvent::Chunk { bytes, .. } => {
                    if self.fail_chunk {
                        return Err(CaptureError);
                    }
                    self.bytes.extend_from_slice(bytes);
                }
                CaptureEvent::Response {
                    status,
                    content_type,
                    ..
                } => {
                    self.status = Some(status);
                    self.content_type = Some(content_type.into());
                }
                CaptureEvent::Finished {
                    outcome,
                    body_complete,
                    ..
                } => self.finished = Some((outcome, body_complete)),
                _ => {}
            }
            Ok(())
        })
    }
}

#[tokio::test]
async fn optional_capture_is_per_call_and_keeps_binary_and_http_error_bodies() {
    let binary = b"\0\xffimage\x80";
    let mut response = Response::raw(200, "image/png", "");
    response.parts = vec![(Duration::ZERO, binary.to_vec())];
    let server = Server::new(vec![
        response,
        Response::raw(503, "text/plain", "remote failure"),
        Response::raw(200, "text/plain", "not captured"),
    ]);
    let client = ModelClient::default();
    let input = request(&server.url, Protocol::CustomHttp);
    let mut capture = Capture::default();
    let result = client
        .check_once(
            &input,
            ExecutionOptions::default(),
            &Cancellation::default(),
            Some(&mut capture),
        )
        .await
        .unwrap();
    assert!(result.success);
    assert_eq!(result.first_token_ms, None);
    assert_eq!(capture.bytes, binary);
    assert_eq!(capture.content_type.as_deref(), Some("image/png"));
    assert!(matches!(
        capture.finished,
        Some((CaptureOutcome::Succeeded, true))
    ));
    let mut failed = Capture::default();
    let result = client
        .check_once(
            &input,
            ExecutionOptions::default(),
            &Cancellation::default(),
            Some(&mut failed),
        )
        .await
        .unwrap();
    assert_eq!(result.failure, Some(CheckFailure::Http));
    assert_eq!(failed.status, Some(503));
    assert_eq!(failed.bytes, b"remote failure");
    assert!(matches!(
        failed.finished,
        Some((CaptureOutcome::Failed(CheckFailure::Http), true))
    ));
    assert!(
        client
            .check_once(
                &input,
                ExecutionOptions::default(),
                &Cancellation::default(),
                None
            )
            .await
            .unwrap()
            .success
    );
    assert_eq!(capture.bytes, binary);
    assert_eq!(failed.bytes, b"remote failure");
}

#[tokio::test]
async fn sse_tools_unknown_events_and_parse_failure_are_captured_before_parsing() {
    let body = concat!(
        "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"function\":{\"name\":\"lookup\",\"arguments\":\"{}\"}}]}}]}\n\n",
        "event: future-event\ndata: {\"unknown\":true}\n\n",
        "data: malformed-json\n\n",
        "data: {\"afterParserFailure\":true}\n\n",
        "data: [DONE]\n\n",
    );
    let server = Server::new(vec![Response::raw(200, "text/event-stream", body)]);
    let mut capture = Capture::default();
    let result = ModelClient::default()
        .check_once(
            &request(&server.url, Protocol::OpenaiCompatible),
            ExecutionOptions::default(),
            &Cancellation::default(),
            Some(&mut capture),
        )
        .await
        .unwrap();
    assert_eq!(result.failure, Some(CheckFailure::InvalidResponse));
    assert_eq!(capture.bytes, body.as_bytes());
    assert!(matches!(
        capture.finished,
        Some((CaptureOutcome::Failed(CheckFailure::InvalidResponse), true))
    ));
}

#[tokio::test]
async fn sink_failure_is_a_call_error_instead_of_a_model_failure() {
    let server = Server::new(vec![Response::raw(200, "text/plain", "OK")]);
    let mut capture = Capture {
        fail_chunk: true,
        ..Default::default()
    };
    let result = ModelClient::default()
        .check_once(
            &request(&server.url, Protocol::CustomHttp),
            ExecutionOptions::default(),
            &Cancellation::default(),
            Some(&mut capture),
        )
        .await;
    assert!(matches!(result, Err(ClientError::Capture)));
    assert!(matches!(
        capture.finished,
        Some((CaptureOutcome::WriteFailed, false))
    ));
}

#[tokio::test]
async fn cancellation_and_timeouts_mark_an_incomplete_body() {
    for cancel in [true, false] {
        let server = Server::new(vec![Response::sse(&[("first", 0), ("late", 500)])]);
        let cancellation = Cancellation::default();
        let mut capture = Capture::default();
        let client = ModelClient::default();
        let input = request(&server.url, Protocol::CustomHttp);
        let operation = client.check_once(
            &input,
            ExecutionOptions {
                idle_timeout: Duration::from_millis(100),
                ..Default::default()
            },
            &cancellation,
            Some(&mut capture),
        );
        let interrupt = async {
            server.wait_requests(1).await;
            if cancel {
                tokio::time::sleep(Duration::from_millis(30)).await;
                cancellation.cancel();
            }
        };
        let (result, _) = tokio::join!(operation, interrupt);
        if cancel {
            assert!(matches!(result, Err(ClientError::Cancelled)));
            assert!(matches!(
                capture.finished,
                Some((CaptureOutcome::Cancelled, false))
            ));
        } else {
            assert_eq!(result.unwrap().failure, Some(CheckFailure::Timeout));
            assert!(matches!(
                capture.finished,
                Some((CaptureOutcome::Failed(CheckFailure::Timeout), false))
            ));
        }
        assert_eq!(capture.bytes, b"first");
    }
}

#[tokio::test]
async fn capture_preserves_successful_json_and_stream_timing() {
    let json_body = "{\"choices\":[{\"message\":{\"content\":\"OK\"},\"finish_reason\":\"stop\"}]}";
    let first = "data: {\"choices\":[{\"delta\":{\"content\":\"OK\"}}]}\n\n";
    let end = "data: [DONE]\n\n";
    let server = Server::new(vec![
        Response::raw(200, "application/json", json_body),
        Response::sse(&[(first, 40), (end, 40)]),
    ]);
    let client = ModelClient::default();
    let input = request(&server.url, Protocol::OpenaiCompatible);
    let mut json = Capture::default();
    let result = client
        .check_once(
            &input,
            ExecutionOptions::default(),
            &Cancellation::default(),
            Some(&mut json),
        )
        .await
        .unwrap();
    assert!(result.success);
    assert_eq!(result.first_token_ms, None);
    assert_eq!(json.bytes, json_body.as_bytes());
    let mut stream = Capture::default();
    let result = client
        .check_once(
            &input,
            ExecutionOptions::default(),
            &Cancellation::default(),
            Some(&mut stream),
        )
        .await
        .unwrap();
    assert!(result.success);
    assert!(result.first_token_ms.unwrap() >= 30);
    assert!(result.duration_ms >= result.first_token_ms.unwrap() + 20);
    assert_eq!(stream.bytes, format!("{first}{end}").as_bytes());
}

#[tokio::test]
async fn oversized_json_keeps_streaming_to_the_sink_after_the_parsing_limit() {
    #[derive(Default)]
    struct Counter {
        bytes: usize,
        complete: bool,
    }
    impl CaptureSink for Counter {
        fn record<'a>(
            &'a mut self,
            event: CaptureEvent<'a>,
        ) -> BoxFuture<'a, Result<(), CaptureError>> {
            Box::pin(async move {
                match event {
                    CaptureEvent::Chunk { bytes, .. } => self.bytes += bytes.len(),
                    CaptureEvent::Finished { body_complete, .. } => self.complete = body_complete,
                    _ => {}
                }
                Ok(())
            })
        }
    }
    let length = 17 * 1024 * 1024;
    let server = Server::new(vec![Response::raw(
        200,
        "application/json",
        "x".repeat(length),
    )]);
    let mut capture = Counter::default();
    let result = ModelClient::default()
        .check_once(
            &request(&server.url, Protocol::OpenaiCompatible),
            ExecutionOptions::default(),
            &Cancellation::default(),
            Some(&mut capture),
        )
        .await
        .unwrap();
    assert_eq!(result.failure, Some(CheckFailure::InvalidResponse));
    assert_eq!(capture.bytes, length);
    assert!(capture.complete);
}
