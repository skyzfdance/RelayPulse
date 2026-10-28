use super::super::{
    stream,
    test_support::{request, Response, Server},
    *,
};
use futures_util::future::BoxFuture;
use serde_json::{json, Value};
use std::time::Duration;

fn thinking_cases() -> Vec<(Protocol, Value)> {
    vec![
        (
            Protocol::OpenaiCompatible,
            json!({"choices":[{"delta":{"reasoning_content":"思考片段"}}]}),
        ),
        (
            Protocol::OpenaiCompatible,
            json!({"choices":[{"delta":{"reasoning":"思考片段"}}]}),
        ),
        (
            Protocol::OpenaiCompatible,
            json!({"choices":[{"delta":{"reasoning_details":[{"type":"reasoning.text","text":"思考片段"}]}}]}),
        ),
        (
            Protocol::OpenaiCompatible,
            json!({"choices":[{"delta":{"reasoning_details":[{"type":"reasoning.summary","summary":"思考摘要"}]}}]}),
        ),
        (
            Protocol::OpenaiResponses,
            json!({"type":"response.reasoning_text.delta","delta":"思考片段"}),
        ),
        (
            Protocol::OpenaiResponses,
            json!({"type":"response.reasoning_summary_text.delta","delta":"思考摘要"}),
        ),
        (
            Protocol::Anthropic,
            json!({"type":"content_block_delta","delta":{"type":"thinking_delta","thinking":"思考片段"}}),
        ),
        (
            Protocol::Anthropic,
            json!({"type":"content_block_start","content_block":{"type":"thinking","thinking":"思考片段"}}),
        ),
    ]
}

fn answer(protocol: Protocol) -> Value {
    match protocol {
        Protocol::OpenaiCompatible => json!({"choices":[{"delta":{"content":"OK"}}]}),
        Protocol::OpenaiResponses => json!({"type":"response.output_text.delta","delta":"OK"}),
        Protocol::Anthropic => {
            json!({"type":"content_block_delta","delta":{"type":"text_delta","text":"OK"}})
        }
        Protocol::CustomHttp => unreachable!(),
    }
}

fn ended(protocol: Protocol) -> Value {
    match protocol {
        Protocol::OpenaiCompatible => json!({"choices":[{"delta":{},"finish_reason":"stop"}]}),
        Protocol::OpenaiResponses => json!({"type":"response.completed"}),
        Protocol::Anthropic => json!({"type":"message_stop"}),
        Protocol::CustomHttp => unreachable!(),
    }
}

fn wire(value: &Value) -> String {
    format!("data: {value}\n\n")
}

#[test]
fn thinking_is_first_output_but_does_not_replace_a_valid_answer() {
    for (protocol, thinking) in thinking_cases() {
        let mut content = stream::Content::default();
        assert!(content.observe(&thinking, protocol), "{thinking}");
        assert!(!content.success());
        content.observe(&ended(protocol), protocol);
        assert!(
            !content.success(),
            "thinking alone must not pass: {thinking}"
        );
    }
}

#[test]
fn empty_thinking_control_events_and_signatures_are_not_first_output() {
    for (protocol, event) in [
        (
            Protocol::OpenaiCompatible,
            json!({"choices":[{"delta":{"role":"assistant","reasoning_content":"","reasoning":null}}]}),
        ),
        (
            Protocol::OpenaiCompatible,
            json!({"choices":[{"delta":{"reasoning_details":[{"type":"reasoning.text","text":"","signature":"signature"},{"type":"reasoning.summary","summary":""},{"type":"reasoning.encrypted","data":"opaque-data","text":"not-plaintext"}]}}]}),
        ),
        (
            Protocol::OpenaiCompatible,
            json!({"choices":[{"delta":{"tool_calls":[{"function":{"name":"lookup","arguments":"{}"}}]}}]}),
        ),
        (
            Protocol::OpenaiResponses,
            json!({"type":"response.created"}),
        ),
        (
            Protocol::OpenaiResponses,
            json!({"type":"response.reasoning_text.delta","delta":""}),
        ),
        (
            Protocol::OpenaiResponses,
            json!({"type":"response.reasoning_summary_text.delta","delta":null}),
        ),
        (
            Protocol::OpenaiResponses,
            json!({"type":"response.output_item.added","item":{"type":"reasoning","summary":[],"encrypted_content":"opaque-data"}}),
        ),
        (
            Protocol::OpenaiResponses,
            json!({"type":"response.function_call_arguments.delta","delta":"{}"}),
        ),
        (
            Protocol::Anthropic,
            json!({"type":"content_block_start","content_block":{"type":"thinking","thinking":""}}),
        ),
        (
            Protocol::Anthropic,
            json!({"type":"content_block_delta","delta":{"type":"thinking_delta","thinking":""}}),
        ),
        (
            Protocol::Anthropic,
            json!({"type":"content_block_delta","delta":{"type":"signature_delta","signature":"signature"}}),
        ),
        (
            Protocol::Anthropic,
            json!({"type":"content_block_start","content_block":{"type":"redacted_thinking","data":"opaque-data"}}),
        ),
        (Protocol::Anthropic, json!({"type":"ping"})),
    ] {
        let mut content = stream::Content::default();
        assert!(!content.observe(&event, protocol), "{event}");
        assert!(!content.success());
    }
}

/// 将完整 SSE 事件对应到实际网络块时间，不按服务端发送次数假定分块边界。
#[derive(Default)]
struct Timings {
    decoder: stream::Sse,
    events: Vec<(Value, u64)>,
}
impl Timings {
    fn received(&self, expected: &Value) -> u64 {
        self.events
            .iter()
            .find_map(|(event, time)| (event == expected).then_some(*time))
            .expect("expected event was captured")
    }
}
impl CaptureSink for Timings {
    fn record<'a>(
        &'a mut self,
        event: CaptureEvent<'a>,
    ) -> BoxFuture<'a, Result<(), CaptureError>> {
        Box::pin(async move {
            if let CaptureEvent::Chunk { bytes, elapsed_ms } = event {
                for event in self.decoder.push(bytes).unwrap() {
                    self.events
                        .push((serde_json::from_str(&event).unwrap(), elapsed_ms));
                }
            }
            Ok(())
        })
    }
}

#[tokio::test]
async fn first_token_uses_thinking_arrival_and_is_not_replaced_by_later_answer() {
    for (protocol, thinking) in thinking_cases() {
        let answer = answer(protocol);
        let end = ended(protocol);
        let server = Server::new(vec![Response::sse(&[
            ("data: {}\n\n", 0),
            (&wire(&thinking), 30),
            (&wire(&answer), 80),
            (&wire(&end), 20),
        ])]);
        let mut timings = Timings::default();
        let result = ModelClient::default()
            .check_once(
                &request(&server.url, protocol),
                ExecutionOptions::default(),
                &Cancellation::default(),
                Some(&mut timings),
            )
            .await
            .unwrap();
        assert!(result.success, "{protocol:?}: {result:?}");
        assert_eq!(result.first_token_ms, Some(timings.received(&thinking)));
        assert!(timings.received(&thinking) < timings.received(&answer));
        assert!(result.duration_ms >= timings.received(&end));
        assert!(result.metadata.body_complete);
    }
}

#[tokio::test]
async fn thinking_only_or_timeout_retains_first_token_without_fabricating_success() {
    for (protocol, thinking) in thinking_cases() {
        for timeout in [false, true] {
            let server = Server::new(vec![Response::sse(&[
                (&wire(&thinking), 0),
                (&wire(&ended(protocol)), if timeout { 500 } else { 0 }),
            ])]);
            let result = ModelClient::default()
                .check_once(
                    &request(&server.url, protocol),
                    ExecutionOptions {
                        total_timeout: Duration::from_secs(2),
                        idle_timeout: Duration::from_millis(100),
                    },
                    &Cancellation::default(),
                    None,
                )
                .await
                .unwrap();
            assert!(!result.success, "{protocol:?}: {result:?}");
            assert!(result.first_token_ms.is_some());
            assert_eq!(
                result.failure,
                Some(if timeout {
                    CheckFailure::Timeout
                } else {
                    CheckFailure::InvalidResponse
                })
            );
            assert_eq!(result.metadata.body_complete, !timeout);
        }
    }
}

#[tokio::test]
async fn nonstreaming_and_custom_requests_keep_first_token_null_with_thinking() {
    for (protocol, body) in [
        (
            Protocol::OpenaiCompatible,
            json!({"choices":[{"message":{"reasoning_content":"思考片段","content":"OK"},"finish_reason":"stop"}]}),
        ),
        (
            Protocol::OpenaiResponses,
            json!({"status":"completed","output":[{"type":"reasoning","summary":[{"type":"summary_text","text":"思考摘要"}]},{"type":"message","content":[{"type":"output_text","text":"OK"}]}]}),
        ),
        (
            Protocol::Anthropic,
            json!({"content":[{"type":"thinking","thinking":"思考片段"},{"type":"text","text":"OK"}],"stop_reason":"end_turn"}),
        ),
    ] {
        let server = Server::new(vec![Response::json(body)]);
        let result = super::check(&ModelClient::default(), &request(&server.url, protocol)).await;
        assert!(result.success, "{protocol:?}: {result:?}");
        assert_eq!(result.first_token_ms, None);
    }
    let (protocol, thinking) = thinking_cases().remove(0);
    let server = Server::new(vec![Response::sse(&[
        (&wire(&thinking), 0),
        (&wire(&answer(protocol)), 0),
        (&wire(&ended(protocol)), 0),
    ])]);
    let mut input = request(&server.url, protocol);
    input.body = CheckBody::Custom { parameters: vec![] };
    let result = super::check(&ModelClient::default(), &input).await;
    assert!(result.success);
    assert_eq!(result.first_token_ms, None);
}
