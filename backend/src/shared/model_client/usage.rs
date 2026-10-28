//! 协议用量为累计快照：后续事件替换已报告的字段，不把多个事件相加。
use super::{Protocol, TokenUsage};
use serde_json::Value;

impl TokenUsage {
    pub(super) fn observe(&mut self, value: &Value, protocol: Protocol) {
        let usage = match protocol {
            Protocol::OpenaiResponses => value
                .get("response")
                .and_then(|response| response.get("usage"))
                .unwrap_or(&value["usage"]),
            Protocol::Anthropic => value
                .get("message")
                .and_then(|message| message.get("usage"))
                .unwrap_or(&value["usage"]),
            _ => &value["usage"],
        };
        let input = if protocol == Protocol::OpenaiCompatible {
            "prompt_tokens"
        } else {
            "input_tokens"
        };
        let output = if protocol == Protocol::OpenaiCompatible {
            "completion_tokens"
        } else {
            "output_tokens"
        };
        for (field, key) in [
            (&mut self.input_tokens, input),
            (&mut self.output_tokens, output),
            (&mut self.total_tokens, "total_tokens"),
        ] {
            if let Some(reported) = usage[key].as_u64().filter(|n| *n <= i64::MAX as u64) {
                *field = Some(reported);
            }
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/model_client/usage.rs"]
mod tests;
