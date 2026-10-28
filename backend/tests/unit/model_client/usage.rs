use super::*;
use serde_json::json;

#[test]
fn cumulative_anthropic_usage_merges_without_inventing_a_total() {
    let mut usage = TokenUsage::default();
    usage.observe(
        &json!({"message":{"usage":{"input_tokens":10,"output_tokens":1}}}),
        Protocol::Anthropic,
    );
    for reported in [2, 4, 4] {
        usage.observe(
            &json!({"usage":{"output_tokens":reported}}),
            Protocol::Anthropic,
        );
    }
    assert_eq!(
        usage,
        TokenUsage {
            input_tokens: Some(10),
            output_tokens: Some(4),
            total_tokens: None,
        }
    );
}

#[test]
fn preserves_zero_and_ignores_unreported_or_invalid_values() {
    let mut usage = TokenUsage::default();
    usage.observe(
        &json!({"usage":{
            "prompt_tokens":0,"completion_tokens":5,"total_tokens":5
        }}),
        Protocol::OpenaiCompatible,
    );
    usage.observe(
        &json!({"usage":{
            "prompt_tokens":-1,"completion_tokens":"6","total_tokens":u64::MAX
        }}),
        Protocol::OpenaiCompatible,
    );
    assert_eq!(usage.input_tokens, Some(0));
    assert_eq!(usage.output_tokens, Some(5));
    assert_eq!(usage.total_tokens, Some(5));
    let mut responses = TokenUsage::default();
    responses.observe(
        &json!({"response":{"usage":{
            "input_tokens":12,"output_tokens":3,"total_tokens":15
        }}}),
        Protocol::OpenaiResponses,
    );
    assert_eq!(responses.total_tokens, Some(15));
    assert_eq!(TokenUsage::default().total_tokens, None);
}
