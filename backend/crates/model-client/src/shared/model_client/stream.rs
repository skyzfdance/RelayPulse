//! 增量 SSE 解码；UTF-8 可跨网络块，空行结束事件，结束标记不可替代实际输出。
use super::{ClientError, Protocol, TokenUsage};
use serde_json::Value;

#[derive(Default)]
pub(super) struct Content {
    answer_text: bool,
    pub ended: bool,
    pub failed: bool,
    pub usage: TokenUsage,
}
impl Content {
    /// 返回本事件是否包含非空思考或回答文本，供流式首 Token 计时使用。
    /// 探测成功仍要求回答正文，不能仅凭思考文本或结束事件判定。
    pub fn observe(&mut self, value: &Value, protocol: Protocol) -> bool {
        self.usage.observe(value, protocol);
        let kind = value["type"].as_str().unwrap_or("");
        if value.get("error").is_some_and(|v| !v.is_null())
            || matches!(kind, "error" | "response.failed" | "response.incomplete")
            || matches!(value["status"].as_str(), Some("failed" | "incomplete"))
        {
            self.failed = true;
        }
        let has_answer = match protocol {
            Protocol::OpenaiCompatible => value["choices"].as_array().is_some_and(|choices| {
                choices.iter().any(|c| {
                    nonempty(&c["delta"]["content"])
                        || nonempty(&c["message"]["content"])
                        || nonempty(&c["text"])
                })
            }),
            Protocol::OpenaiResponses => {
                (kind == "response.output_text.delta" && nonempty(&value["delta"]))
                    || value["output"].as_array().is_some_and(|items| {
                        items.iter().any(|item| {
                            item["content"].as_array().is_some_and(|cs| {
                                cs.iter()
                                    .any(|c| c["type"] == "output_text" && nonempty(&c["text"]))
                            })
                        })
                    })
                    || nonempty(&value["output_text"])
            }
            Protocol::Anthropic => {
                (value["delta"]["type"] == "text_delta" && nonempty(&value["delta"]["text"]))
                    || (value["content_block"]["type"] == "text"
                        && nonempty(&value["content_block"]["text"]))
                    || value["content"].as_array().is_some_and(|cs| {
                        cs.iter()
                            .any(|c| c["type"] == "text" && nonempty(&c["text"]))
                    })
            }
            Protocol::CustomHttp => false,
        };
        self.answer_text |= has_answer;
        self.ended |= match protocol {
            Protocol::OpenaiCompatible => value["choices"]
                .as_array()
                .is_some_and(|cs| cs.iter().any(|c| c["finish_reason"].as_str().is_some())),
            Protocol::OpenaiResponses => {
                kind == "response.completed" || value["status"] == "completed"
            }
            Protocol::Anthropic => {
                kind == "message_stop" || value["stop_reason"].as_str().is_some()
            }
            Protocol::CustomHttp => false,
        };
        has_answer || reasoning_text(value, protocol)
    }
    pub fn success(&self) -> bool {
        self.answer_text && self.ended && !self.failed
    }
}
fn reasoning_text(value: &Value, protocol: Protocol) -> bool {
    match protocol {
        Protocol::OpenaiCompatible => value["choices"].as_array().is_some_and(|choices| {
            choices.iter().any(|choice| {
                [&choice["delta"], &choice["message"]].iter().any(|part| {
                    nonempty(&part["reasoning_content"])
                        || nonempty(&part["reasoning"])
                        || part["reasoning_details"].as_array().is_some_and(|details| {
                            details.iter().any(|detail| match detail["type"].as_str() {
                                Some("reasoning.text") => nonempty(&detail["text"]),
                                Some("reasoning.summary") => nonempty(&detail["summary"]),
                                _ => false,
                            })
                        })
                })
            })
        }),
        Protocol::OpenaiResponses => {
            matches!(
                value["type"].as_str(),
                Some("response.reasoning_text.delta" | "response.reasoning_summary_text.delta")
            ) && nonempty(&value["delta"])
        }
        Protocol::Anthropic => {
            (value["delta"]["type"] == "thinking_delta" && nonempty(&value["delta"]["thinking"]))
                || (value["content_block"]["type"] == "thinking"
                    && nonempty(&value["content_block"]["thinking"]))
        }
        Protocol::CustomHttp => false,
    }
}
fn nonempty(v: &Value) -> bool {
    v.as_str().is_some_and(|s| !s.is_empty())
}
#[derive(Default)]
pub(super) struct Sse {
    buffer: Vec<u8>,
    lines: Vec<String>,
    event_bytes: usize,
    skip_lf: bool,
}
impl Sse {
    pub fn push(&mut self, bytes: &[u8]) -> Result<Vec<String>, ClientError> {
        const MAX_BUFFERED_BYTES: usize = 1024 * 1024;
        let mut events = vec![];
        // 一个网络块可能包含很多事件；按行消费，只限制未完成的行和单个事件。
        for mut segment in bytes.split_inclusive(|byte| matches!(*byte, b'\n' | b'\r')) {
            if self.skip_lf {
                self.skip_lf = false;
                if segment.first() == Some(&b'\n') {
                    segment = &segment[1..];
                }
            }
            let Some(&last) = segment.last() else {
                continue;
            };
            let terminated = matches!(last, b'\n' | b'\r');
            let content = &segment[..segment.len() - usize::from(terminated)];
            if self.buffer.len().saturating_add(content.len()) > MAX_BUFFERED_BYTES {
                return Err(ClientError::Response);
            }
            self.buffer.extend_from_slice(content);
            if !terminated {
                continue;
            }
            self.skip_lf = last == b'\r';
            let line = std::str::from_utf8(&self.buffer).map_err(|_| ClientError::Response)?;
            if line.is_empty() {
                if !self.lines.is_empty() {
                    events.push(self.lines.join("\n"));
                    self.lines.clear();
                }
                self.event_bytes = 0;
            } else if let Some(data) = line.strip_prefix("data:") {
                let data = data.strip_prefix(' ').unwrap_or(data);
                self.event_bytes += data.len() + usize::from(!self.lines.is_empty());
                if self.event_bytes > MAX_BUFFERED_BYTES {
                    return Err(ClientError::Response);
                }
                self.lines.push(data.to_owned());
            }
            self.buffer.clear();
        }
        Ok(events)
    }
    pub fn complete(&self) -> bool {
        self.lines.is_empty() && self.buffer.iter().all(u8::is_ascii_whitespace)
    }
}
