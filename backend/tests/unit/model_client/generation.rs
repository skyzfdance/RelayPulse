use super::*;
use futures_util::future::BoxFuture;

#[tokio::test]
async fn responses_json_has_the_same_text_for_probes_generation_and_replay() {
    for body in [
        json!({"status":"completed","output_text":"OK"}),
        json!({"status":"completed","output_text":"OK", "output":[
            {"type":"message","content":[{"type":"output_text","text":"OK"}]}
        ]}),
    ] {
        let server = Server::new(vec![Response::json(body.clone()); 2]);
        let client = ModelClient::default();
        assert!(
            check(&client, &request(&server.url, Protocol::OpenaiResponses))
                .await
                .success
        );
        let mut capture = Capture::default();
        let generated = client
            .generate_once(
                &input(&server.url, Protocol::OpenaiResponses),
                options(2000),
                &Cancellation::default(),
                &mut capture,
            )
            .await
            .unwrap();
        assert_eq!(generated.failure, None);
        assert_eq!(capture.output, vec![(OutputKind::Text, "OK".into())]);
        let replay =
            decode_generation_output(Protocol::OpenaiResponses, body.to_string().as_bytes());
        assert_eq!(replay.parts.len(), 1);
        assert_eq!(replay.parts[0].text, "OK");
    }
}

#[derive(Default)]
struct Capture {
    raw: Vec<u8>,
    output: Vec<(OutputKind, String)>,
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
                    self.raw.extend_from_slice(bytes);
                }
                CaptureEvent::Output { kind, text, .. } => self.output.push((kind, text.into())),
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
fn input(url: &str, protocol: Protocol) -> GenerationRequest {
    GenerationRequest {
        target: target(url, protocol),
        model: "actual-model".into(),
        endpoint: "/independent-endpoint?region=test".into(),
        prompt: "ordinary user question".into(),
        parameters: json!({"temperature":0.35,"vendor_flag":{"keep":true}})
            .as_object()
            .unwrap()
            .clone(),
    }
}
fn options(ms: u64) -> GenerationOptions {
    GenerationOptions {
        output_timeout: Some(Duration::from_millis(ms)),
    }
}

#[tokio::test]
async fn generation_uses_independent_full_user_requests_for_all_protocols_without_injecting_probe_limits(
) {
    for protocol in [
        Protocol::OpenaiCompatible,
        Protocol::OpenaiResponses,
        Protocol::Anthropic,
        Protocol::CustomHttp,
    ] {
        let server = Server::new(vec![super::test_support::success(protocol)]);
        let request = input(&server.url, protocol);
        let mut capture = Capture::default();
        let result = ModelClient::default()
            .generate_once(
                &request,
                options(2000),
                &Cancellation::default(),
                &mut capture,
            )
            .await
            .unwrap();
        assert_eq!(result.failure, None, "{protocol:?}");
        assert!(result.first_token_ms.is_some());
        assert_eq!(result.metadata.usage.input_tokens, Some(4));
        assert_eq!(result.metadata.usage.output_tokens, Some(8));
        assert!(result.metadata.body_complete);
        assert!(capture.output.iter().any(|(_, text)| text == "answer"));
        let sent = &server.requests()[0];
        assert!(sent
            .head
            .starts_with("POST /independent-endpoint?region=test "));
        let body: serde_json::Value = serde_json::from_slice(&sent.body).unwrap();
        assert_eq!(body["model"], request.model);
        assert_eq!(body["stream"], true);
        assert_eq!(body["temperature"], 0.35);
        assert_eq!(body["vendor_flag"], json!({"keep":true}));
        if protocol == Protocol::OpenaiResponses {
            assert_eq!(
                body["input"],
                json!([{"role":"user","content":request.prompt}])
            );
            assert!(body.get("messages").is_none());
        } else {
            assert_eq!(
                body["messages"],
                json!([{"role":"user","content":request.prompt}])
            );
        }
        for key in [
            "max_tokens",
            "max_completion_tokens",
            "max_output_tokens",
            "system",
        ] {
            assert!(body.get(key).is_none(), "{key}");
        }
        if protocol == Protocol::Anthropic {
            assert!(sent
                .head
                .to_lowercase()
                .contains("x-api-key: local-test-secret"));
        } else {
            assert!(sent.head.contains("Bearer local-test-secret"));
        }
    }
}

#[tokio::test]
async fn reasoning_text_and_tool_output_renew_the_deadline_without_an_overall_timeout() {
    for (protocol,events) in [
        (Protocol::OpenaiCompatible,vec![
            "data: {\"choices\":[{\"delta\":{\"reasoning_content\":\"think\"}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"function\":{\"name\":\"tool\",\"arguments\":\"{}\"}}]}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"content\":\"answer\"}}]}\n\n",
            "data: [DONE]\n\n",
        ]),
        (Protocol::OpenaiResponses,vec![
            "data: {\"type\":\"response.reasoning_text.delta\",\"delta\":\"think\"}\n\n",
            "data: {\"type\":\"response.function_call_arguments.delta\",\"delta\":\"{}\"}\n\n",
            "data: {\"type\":\"response.output_text.delta\",\"delta\":\"answer\"}\n\n",
            "data: {\"type\":\"response.completed\"}\n\n",
        ]),
        (Protocol::Anthropic,vec![
            "data: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"thinking_delta\",\"thinking\":\"think\"}}\n\n",
            "data: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{}\"}}\n\n",
            "data: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"answer\"}}\n\n",
            "data: {\"type\":\"message_stop\"}\n\n",
        ]),
    ] {
        let parts: Vec<_> = events.iter().map(|e| (*e,100)).collect();
        let server = Server::new(vec![Response::sse(&parts)]); let mut capture = Capture::default();
        let result = ModelClient::default().generate_once(&input(&server.url,protocol),options(250),&Cancellation::default(),&mut capture).await.unwrap();
        assert_eq!(result.failure,None,"{protocol:?}"); assert!(result.duration_ms >= 350);
        for kind in [OutputKind::Text,OutputKind::Reasoning,OutputKind::Tool] { assert!(capture.output.iter().any(|(k,_)| *k == kind)); }
    }
}

#[tokio::test]
async fn usage_empty_deltas_comments_and_unknown_events_do_not_keep_a_request_alive() {
    let ignored = [
        ": heartbeat\n\n",
        "data: {\"usage\":{\"prompt_tokens\":3}}\n\n",
        "data: {\"choices\":[{\"delta\":{\"content\":\"\",\"reasoning_content\":\"\"}}]}\n\n",
        "data: {\"type\":\"vendor.heartbeat\",\"delta\":\"not output\"}\n\n",
    ];
    for event in ignored {
        let server = Server::new(vec![Response::sse(&[
            (event, 0),
            (event, 100),
            (event, 100),
            (event, 100),
            ("data: [DONE]\n\n", 100),
        ])]);
        let mut capture = Capture::default();
        let result = ModelClient::default()
            .generate_once(
                &input(&server.url, Protocol::OpenaiCompatible),
                options(250),
                &Cancellation::default(),
                &mut capture,
            )
            .await
            .unwrap();
        assert_eq!(result.failure, Some(GenerationFailure::Timeout));
        assert_eq!(result.first_token_ms, None);
        assert!(!result.metadata.body_complete);
        assert!(capture.output.is_empty());
        assert!(!capture.raw.is_empty());
        assert!(matches!(
            capture.finished,
            Some((CaptureOutcome::Failed(CheckFailure::Timeout), false))
        ));
    }
}

#[tokio::test]
async fn the_same_output_deadline_covers_connection_and_first_output_and_http_failures_are_not_timeouts(
) {
    let mut connecting = super::test_support::success(Protocol::OpenaiCompatible);
    connecting.header_delay = Duration::from_millis(350);
    let mut http = Response::raw(401, "text/plain", "credential rejected");
    http.parts[0].0 = Duration::from_millis(350);
    let first = super::test_support::success(Protocol::OpenaiCompatible)
        .delayed(Duration::from_millis(350));
    let server = Server::new(vec![connecting, first, http]);
    let request = input(&server.url, Protocol::OpenaiCompatible);
    for failure in [
        GenerationFailure::Timeout,
        GenerationFailure::Timeout,
        GenerationFailure::Http,
    ] {
        let result = ModelClient::default()
            .generate_once(
                &request,
                options(150),
                &Cancellation::default(),
                &mut Capture::default(),
            )
            .await
            .unwrap();
        assert_eq!(result.failure, Some(failure));
        assert_eq!(result.first_token_ms, None);
    }
}

#[tokio::test]
async fn partial_metrics_survive_cancellation_and_capture_failure_stops_the_call() {
    let partial = Response::sse(&[("data: {\"choices\":[{\"delta\":{\"content\":\"partial\"}}],\"usage\":{\"prompt_tokens\":7}}\n\n",0),("data: [DONE]\n\n",1000)]);
    let server = Server::new(vec![partial.clone(), partial]);
    let cancellation = Cancellation::default();
    let cancel = cancellation.clone();
    let job = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(100)).await;
        cancel.cancel();
    });
    let mut capture = Capture::default();
    let client = ModelClient::default();
    let request = input(&server.url, Protocol::OpenaiCompatible);
    let result = client
        .generate_once(&request, options(2000), &cancellation, &mut capture)
        .await
        .unwrap();
    job.await.unwrap();
    assert_eq!(result.failure, Some(GenerationFailure::Cancelled));
    assert_eq!(result.metadata.usage.input_tokens, Some(7));
    assert!(result.first_token_ms.is_some());
    assert!(!result.metadata.body_complete);
    assert_eq!(capture.output[0].1, "partial");
    let mut broken = Capture {
        fail_chunk: true,
        ..Default::default()
    };
    assert!(matches!(
        client
            .generate_once(
                &request,
                options(2000),
                &Cancellation::default(),
                &mut broken
            )
            .await,
        Err(ClientError::Capture)
    ));
    assert!(matches!(
        broken.finished,
        Some((CaptureOutcome::WriteFailed, false))
    ));
}

#[tokio::test]
async fn complete_json_and_malformed_streams_preserve_response_facts_without_fabricating_stream_ttft(
) {
    let json_body = json!({"choices":[{"message":{"content":"json answer"},"finish_reason":"stop"}],"usage":{"prompt_tokens":1,"completion_tokens":2}});
    let invalid = "data: malformed-json\n\n: trailing bytes\n\n";
    let server = Server::new(vec![
        Response::json(json_body.clone()),
        Response::raw(200, "text/event-stream", invalid),
    ]);
    let client = ModelClient::default();
    let request = input(&server.url, Protocol::OpenaiCompatible);
    let mut capture = Capture::default();
    let result = client
        .generate_once(
            &request,
            options(2000),
            &Cancellation::default(),
            &mut capture,
        )
        .await
        .unwrap();
    assert_eq!(result.failure, None);
    assert_eq!(result.first_token_ms, None);
    assert_eq!(capture.raw, json_body.to_string().as_bytes());
    let mut capture = Capture::default();
    let result = client
        .generate_once(
            &request,
            options(2000),
            &Cancellation::default(),
            &mut capture,
        )
        .await
        .unwrap();
    assert_eq!(result.failure, Some(GenerationFailure::InvalidResponse));
    assert!(result.metadata.body_complete);
    assert_eq!(capture.raw, invalid.as_bytes());
}

#[tokio::test]
async fn known_protocol_errors_followed_by_a_stall_remain_errors_and_are_not_retryable_timeouts() {
    for error in [
        "data: not-json\n\n",
        "data: {\"error\":{\"message\":\"upstream failure\"}}\n\n",
    ] {
        let server = Server::new(vec![Response::sse(&[
            (error, 0),
            (": delayed tail\n\n", 400),
        ])]);
        let mut capture = Capture::default();
        let result = ModelClient::default()
            .generate_once(
                &input(&server.url, Protocol::OpenaiCompatible),
                options(150),
                &Cancellation::default(),
                &mut capture,
            )
            .await
            .unwrap();
        assert_eq!(result.failure, Some(GenerationFailure::InvalidResponse));
        assert!(!result.metadata.body_complete);
        assert_eq!(capture.raw, error.as_bytes());
    }
}

#[test]
fn captured_output_decodes_ordered_sections_and_encrypted_reasoning_without_displaying_ciphertext()
{
    let body = concat!(
        "data: {\"type\":\"response.output_item.done\",\"item\":{\"type\":\"reasoning\",\"encrypted_content\":\"opaque-secret-data\"}}\n\n",
        "data: {\"type\":\"response.reasoning_text.delta\",\"delta\":\"think\"}\n\n",
        "data: {\"type\":\"response.output_text.delta\",\"delta\":\"hello\"}\n\n",
        "data: {\"type\":\"response.output_text.delta\",\"delta\":\" world\"}\n\n",
        "data: {\"type\":\"response.function_call_arguments.delta\",\"delta\":\"{}\"}\n\n",
    );
    let result = decode_generation_output(Protocol::OpenaiResponses, body.as_bytes());
    assert!(result.encrypted_reasoning);
    assert!(result.parsing_complete);
    assert_eq!(result.parts.len(), 3);
    assert_eq!(result.parts[0].kind, OutputKind::Reasoning);
    assert_eq!(result.parts[1].text, "hello world");
    assert_eq!(result.parts[2].kind, OutputKind::Tool);
    assert!(!format!("{:?}", result.parts).contains("opaque-secret-data"));
    let partial = decode_generation_output(
        Protocol::OpenaiResponses,
        format!("{body}data: {{").as_bytes(),
    );
    assert!(!partial.parsing_complete);
    assert_eq!(partial.parts[1].text, "hello world");
}
