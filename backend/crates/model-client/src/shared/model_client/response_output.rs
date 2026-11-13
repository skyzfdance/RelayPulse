//! 模型协议字段解码；调用方各自决定成功条件、计时和落盘行为。
use super::{OutputKind, Protocol};
use serde_json::Value;

/// 只提取协议明确的正文、思考和工具增量；usage、心跳和未知事件不续期。
pub(super) fn output<'a>(
    value: &'a Value,
    protocol: Protocol,
    streaming: bool,
) -> Vec<(OutputKind, &'a str)> {
    let mut parts = Vec::new();
    let mut add = |kind, value: &'a Value| {
        if let Some(text) = value.as_str().filter(|s| !s.is_empty()) {
            parts.push((kind, text));
        }
    };
    match protocol {
        Protocol::OpenaiCompatible | Protocol::CustomHttp => {
            if let Some(choices) = value["choices"].as_array() {
                for choice in choices {
                    let part = if streaming {
                        &choice["delta"]
                    } else {
                        &choice["message"]
                    };
                    add(OutputKind::Text, &part["content"]);
                    add(OutputKind::Text, &choice["text"]);
                    add(OutputKind::Reasoning, &part["reasoning_content"]);
                    add(OutputKind::Reasoning, &part["reasoning"]);
                    if let Some(details) = part["reasoning_details"].as_array() {
                        for detail in details {
                            match detail["type"].as_str() {
                                Some("reasoning.text") => {
                                    add(OutputKind::Reasoning, &detail["text"])
                                }
                                Some("reasoning.summary") => {
                                    add(OutputKind::Reasoning, &detail["summary"])
                                }
                                _ => {}
                            }
                        }
                    }
                    if let Some(calls) = part["tool_calls"].as_array() {
                        for call in calls {
                            add(OutputKind::Tool, &call["function"]["name"]);
                            add(OutputKind::Tool, &call["function"]["arguments"]);
                        }
                    }
                    add(OutputKind::Tool, &part["function_call"]["name"]);
                    add(OutputKind::Tool, &part["function_call"]["arguments"]);
                }
            }
        }
        Protocol::OpenaiResponses => {
            match value["type"].as_str() {
                Some("response.output_text.delta") => add(OutputKind::Text, &value["delta"]),
                Some("response.reasoning_text.delta" | "response.reasoning_summary_text.delta") => {
                    add(OutputKind::Reasoning, &value["delta"])
                }
                Some(
                    "response.function_call_arguments.delta"
                    | "response.custom_tool_call_input.delta",
                ) => add(OutputKind::Tool, &value["delta"]),
                Some("response.output_item.added")
                    if matches!(
                        value["item"]["type"].as_str(),
                        Some("function_call" | "custom_tool_call")
                    ) =>
                {
                    add(OutputKind::Tool, &value["item"]["name"])
                }
                _ => {}
            }
            if !streaming {
                if let Some(items) = value["output"].as_array() {
                    for item in items {
                        if let Some(contents) = item["content"].as_array() {
                            for content in contents {
                                if content["type"] == "output_text" {
                                    add(OutputKind::Text, &content["text"]);
                                }
                            }
                        }
                        if item["type"] == "function_call" {
                            add(OutputKind::Tool, &item["name"]);
                            add(OutputKind::Tool, &item["arguments"]);
                        }
                    }
                }
                if !parts.iter().any(|(kind, _)| *kind == OutputKind::Text) {
                    if let Some(text) = value["output_text"]
                        .as_str()
                        .filter(|text| !text.is_empty())
                    {
                        parts.push((OutputKind::Text, text));
                    }
                }
            }
        }
        Protocol::Anthropic => {
            match value["delta"]["type"].as_str() {
                Some("text_delta") => add(OutputKind::Text, &value["delta"]["text"]),
                Some("thinking_delta") => add(OutputKind::Reasoning, &value["delta"]["thinking"]),
                Some("input_json_delta") => add(OutputKind::Tool, &value["delta"]["partial_json"]),
                _ => {}
            }
            if value["type"] == "content_block_start" {
                let block = &value["content_block"];
                match block["type"].as_str() {
                    Some("text") => add(OutputKind::Text, &block["text"]),
                    Some("thinking") => add(OutputKind::Reasoning, &block["thinking"]),
                    Some("tool_use") => add(OutputKind::Tool, &block["name"]),
                    _ => {}
                }
            }
            if !streaming {
                if let Some(contents) = value["content"].as_array() {
                    for content in contents {
                        match content["type"].as_str() {
                            Some("text") => add(OutputKind::Text, &content["text"]),
                            Some("thinking") => add(OutputKind::Reasoning, &content["thinking"]),
                            Some("tool_use") => add(OutputKind::Tool, &content["name"]),
                            _ => {}
                        }
                    }
                }
            }
        }
    }
    parts
}
