use super::*;

#[tokio::test]
async fn real_streams_extract_reported_usage_request_id_and_received_size() {
    for (protocol, parts, total) in [
        (Protocol::OpenaiCompatible, vec![
            ("data: {\"choices\":[{\"delta\":{\"content\":\"OK\"}}]}\n\n",0),
            ("data: {\"choices\":[],\"usage\":{\"prompt_tokens\":10,\"completion_tokens\":2,\"total_tokens\":12}}\n\n",10),
            ("data: [DONE]\n\n",0),
        ], Some(12)),
        (Protocol::OpenaiResponses, vec![
            ("data: {\"type\":\"response.output_text.delta\",\"delta\":\"OK\"}\n\n",0),
            ("data: {\"type\":\"response.completed\",\"response\":{\"usage\":{\"input_tokens\":10,\"output_tokens\":2,\"total_tokens\":12}}}\n\n",10),
        ], Some(12)),
        (Protocol::Anthropic, vec![
            ("data: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":10,\"output_tokens\":1}}}\n\n",0),
            ("data: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"OK\"}}\n\n",10),
            ("data: {\"type\":\"message_delta\",\"usage\":{\"output_tokens\":2}}\n\ndata: {\"type\":\"message_stop\"}\n\n",0),
        ], None),
    ] {
        let mut response = Response::sse(&parts);
        response.headers.push(("X-Request-Id","actual-provider-request".into()));
        let expected_bytes = response.parts.iter().map(|(_,bytes)|bytes.len() as u64).sum::<u64>();
        let server = Server::new(vec![response]);
        let result = check(&ModelClient::default(),&request(&server.url,protocol)).await;
        assert!(result.success,"{protocol:?}: {result:?}");
        assert!(result.first_token_ms.is_some());
        assert_eq!(result.metadata.usage, TokenUsage {input_tokens:Some(10),output_tokens:Some(2),total_tokens:total});
        assert_eq!(result.metadata.request_id.as_deref(),Some("actual-provider-request"));
        assert_eq!(result.metadata.kind,Some(ResponseKind::EventStream));
        assert!(result.metadata.body_complete);
        assert_eq!(result.metadata.received_bytes,expected_bytes);
    }
}

#[tokio::test]
async fn synchronous_usage_does_not_invent_first_token_or_missing_totals() {
    for (protocol, response) in [
        (
            Protocol::OpenaiCompatible,
            json!({"choices":[{"message":{"content":"OK"},"finish_reason":"stop"}],
            "usage":{"prompt_tokens":0,"completion_tokens":2,"total_tokens":2}}),
        ),
        (
            Protocol::OpenaiResponses,
            json!({"status":"completed","output_text":"OK",
            "usage":{"input_tokens":0,"output_tokens":2,"total_tokens":2}}),
        ),
        (
            Protocol::Anthropic,
            json!({"content":[{"type":"text","text":"OK"}],"stop_reason":"end_turn",
            "usage":{"input_tokens":0,"output_tokens":2}}),
        ),
    ] {
        let server = Server::new(vec![Response::json(response)]);
        let result = check(&ModelClient::default(), &request(&server.url, protocol)).await;
        assert!(result.success, "{result:?}");
        assert_eq!(result.first_token_ms, None);
        assert_eq!(result.metadata.kind, Some(ResponseKind::Json));
        assert_eq!(result.metadata.usage.input_tokens, Some(0));
        assert_eq!(result.metadata.usage.output_tokens, Some(2));
        assert_eq!(
            result.metadata.usage.total_tokens,
            if protocol == Protocol::Anthropic {
                None
            } else {
                Some(2)
            }
        );
    }
}

#[tokio::test]
async fn timeout_keeps_metadata_already_reported_without_claiming_a_complete_body() {
    let first = "data: {\"choices\":[{\"delta\":{\"content\":\"partial\"}}],\"usage\":{\"prompt_tokens\":10,\"completion_tokens\":1}}\n\n";
    let mut response = Response::sse(&[(first, 0), ("data: [DONE]\n\n", 500)]);
    response
        .headers
        .push(("request-id", "timeout-request".into()));
    let server = Server::new(vec![response]);
    let result = ModelClient::default()
        .check_once(
            &request(&server.url, Protocol::OpenaiCompatible),
            ExecutionOptions {
                idle_timeout: Duration::from_millis(40),
                total_timeout: Duration::from_secs(2),
            },
            &Cancellation::default(),
            None,
        )
        .await
        .unwrap();
    assert_eq!(result.failure, Some(CheckFailure::Timeout));
    assert_eq!(result.status_code, Some(200));
    assert!(!result.metadata.body_complete);
    assert_eq!(result.metadata.received_bytes, first.len() as u64);
    assert_eq!(result.metadata.usage.input_tokens, Some(10));
    assert_eq!(result.metadata.usage.output_tokens, Some(1));
    assert_eq!(result.metadata.usage.total_tokens, None);
    assert_eq!(
        result.metadata.request_id.as_deref(),
        Some("timeout-request")
    );
}
