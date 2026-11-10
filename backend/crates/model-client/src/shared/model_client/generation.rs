//! 完整生成：只按有效输出续期，原始流保存与有界协议解析分别进行。
use super::{
    capture, check::elapsed, endpoint, stream, types::invalid, CaptureEvent, CaptureOutcome,
    CaptureSink, CheckFailure, ClientError, GenerationFailure, GenerationOptions,
    GenerationRequest, GenerationResult, InputError, ModelClient, OutputKind, Protocol,
    ResponseKind, ResponseMetadata,
};
use futures_util::StreamExt;
use serde_json::{json, Map, Value};
use std::time::Instant;
use tokio::time::{timeout_at, Instant as Deadline};

pub fn validate_parameters(parameters: &Map<String, Value>) -> Result<(), InputError> {
    let fail = || {
        invalid(
            "parameters",
            "请求参数包含不安全路径、无效类型或超出大小限制。",
        )
    };
    fn safe(value: &Value, depth: usize) -> bool {
        if depth > 32 {
            return false;
        }
        match value {
            Value::Object(fields) => fields.iter().all(|(key, value)| {
                !key.is_empty()
                    && !key.contains('.')
                    && !matches!(
                        key.to_ascii_lowercase().as_str(),
                        "__proto__" | "prototype" | "constructor"
                    )
                    && safe(value, depth + 1)
            }),
            Value::Array(values) => values.iter().all(|value| safe(value, depth + 1)),
            _ => true,
        }
    }
    if !safe(&Value::Object(parameters.clone()), 0)
        || serde_json::to_vec(parameters).map_err(|_| fail())?.len() > 65_536
    {
        return Err(fail());
    }
    Ok(())
}

impl ModelClient {
    pub async fn generate_once(
        &self,
        request: &GenerationRequest,
        options: GenerationOptions,
        cancellation: &super::Cancellation,
        sink: &mut dyn CaptureSink,
    ) -> Result<GenerationResult, ClientError> {
        endpoint::target(&request.target)?;
        validate_parameters(&request.parameters)?;
        let url = endpoint::endpoint(&request.target.base_url, &request.endpoint, false)?;
        let mut body = Value::Object(request.parameters.clone());
        body["model"] = json!(request.model);
        body["stream"] = json!(true);
        if request.target.protocol == Protocol::OpenaiResponses {
            body["input"] = json!([{"role":"user", "content":request.prompt}]);
        } else {
            // 自定义 HTTP 沿用连接端点，使用独立的聊天生成信封，不读取探针模板。
            body["messages"] = json!([{"role":"user", "content":request.prompt}]);
        }
        let start = Instant::now();
        let mut metadata = ResponseMetadata::default();
        let mut first = None;
        let mut status = None;
        let mut capture = Some(sink);
        let mut deadline = options
            .output_timeout
            .map(|duration| Deadline::now() + duration);
        let work = self.receive_generation(
            request,
            &body,
            url,
            &mut capture,
            start,
            &mut metadata,
            &mut first,
            &mut status,
            options,
            &mut deadline,
        );
        let result = tokio::select! {
            biased;
            _ = cancellation.cancelled() => Err(ClientError::Cancelled),
            result = work => result,
        };
        let failure = result.as_ref().err().map(|error| match error {
            ClientError::Cancelled => GenerationFailure::Cancelled,
            ClientError::Timeout => GenerationFailure::Timeout,
            ClientError::Network => GenerationFailure::Network,
            ClientError::Status(_) => GenerationFailure::Http,
            _ => GenerationFailure::InvalidResponse,
        });
        let outcome = match &result {
            Ok(()) => CaptureOutcome::Succeeded,
            Err(ClientError::Cancelled) => CaptureOutcome::Cancelled,
            Err(ClientError::Capture) => CaptureOutcome::WriteFailed,
            Err(ClientError::Timeout) => CaptureOutcome::Failed(CheckFailure::Timeout),
            Err(ClientError::Network) => CaptureOutcome::Failed(CheckFailure::Network),
            Err(ClientError::Status(_)) => CaptureOutcome::Failed(CheckFailure::Http),
            Err(_) => CaptureOutcome::Failed(CheckFailure::InvalidResponse),
        };
        capture::emit(
            &mut capture,
            CaptureEvent::Finished {
                outcome,
                body_complete: metadata.body_complete,
                elapsed_ms: elapsed(start),
                metadata: &metadata,
            },
        )
        .await?;
        if matches!(result, Err(ClientError::Capture)) {
            return Err(ClientError::Capture);
        }
        Ok(GenerationResult {
            status_code: status,
            first_token_ms: first,
            duration_ms: elapsed(start),
            failure,
            metadata,
        })
    }

    #[allow(clippy::too_many_arguments)]
    async fn receive_generation(
        &self,
        request: &GenerationRequest,
        body: &Value,
        url: reqwest::Url,
        capture: &mut Option<&mut dyn CaptureSink>,
        start: Instant,
        metadata: &mut ResponseMetadata,
        first: &mut Option<u64>,
        status: &mut Option<u16>,
        options: GenerationOptions,
        deadline: &mut Option<Deadline>,
    ) -> Result<(), ClientError> {
        let http = self
            .transport
            .generation_request(&request.target, url, body)?;
        capture::emit(
            capture,
            CaptureEvent::Started {
                protocol: request.target.protocol,
                method: "POST",
                url: http.url().as_str(),
                body,
                headers: http.headers(),
            },
        )
        .await?;
        let response = with_deadline(*deadline, self.transport.execute_generation(http))
            .await
            .map_err(|_| ClientError::Timeout)??;
        let code = response.status().as_u16();
        *status = Some(code);
        let successful = response.status().is_success();
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        let mime = content_type.split(';').next().unwrap_or("").trim();
        let sse = mime.eq_ignore_ascii_case("text/event-stream");
        let kind = if sse {
            ResponseKind::EventStream
        } else if mime.eq_ignore_ascii_case("application/json") {
            ResponseKind::Json
        } else {
            ResponseKind::Other
        };
        metadata.kind = Some(kind);
        // 元信息不接受上游的任意字符串（包括可能回显秘密的 request-id）。
        capture::emit(
            capture,
            CaptureEvent::Response {
                status: code,
                kind,
                content_type,
                headers: response.headers(),
                elapsed_ms: elapsed(start),
            },
        )
        .await?;
        let protocol = if request.target.protocol == Protocol::CustomHttp {
            Protocol::OpenaiCompatible
        } else {
            request.target.protocol
        };
        let mut decoder = stream::Sse::default();
        let mut content = stream::Content::default();
        let mut meaningful = false;
        let mut parsing_failed = false;
        let mut buffered = Vec::new();
        let mut bytes = response.bytes_stream();
        loop {
            // 已经取得 HTTP/协议错误后，继续收集响应也不能把它变成可重试超时。
            let timeout_failure = || {
                if !successful {
                    ClientError::Status(code)
                } else if parsing_failed || content.failed {
                    ClientError::Response
                } else {
                    ClientError::Timeout
                }
            };
            if deadline.is_some_and(|deadline| Deadline::now() >= deadline) {
                return Err(timeout_failure());
            }
            let chunk = with_deadline(*deadline, bytes.next())
                .await
                .map_err(|_| timeout_failure())?;
            let Some(chunk) = chunk else { break };
            let chunk = chunk.map_err(ClientError::transport)?;
            let received_at = Deadline::now();
            let received_ms = elapsed(start);
            metadata.received_bytes = metadata.received_bytes.saturating_add(chunk.len() as u64);
            capture::emit(
                capture,
                CaptureEvent::Chunk {
                    bytes: &chunk,
                    elapsed_ms: received_ms,
                },
            )
            .await?;
            if !successful || parsing_failed {
                continue;
            }
            if sse {
                let events = match decoder.push(&chunk) {
                    Ok(events) => events,
                    Err(_) => {
                        parsing_failed = true;
                        continue;
                    }
                };
                for event in events {
                    if event == "[DONE]" && protocol == Protocol::OpenaiCompatible {
                        content.ended = true;
                        continue;
                    }
                    let value: Value = match serde_json::from_str(&event) {
                        Ok(value) => value,
                        Err(_) => {
                            parsing_failed = true;
                            continue;
                        }
                    };
                    content.observe(&value, protocol);
                    metadata.usage = content.usage.clone();
                    for (kind, text) in output(&value, protocol, true) {
                        meaningful = true;
                        first.get_or_insert(received_ms);
                        *deadline = options
                            .output_timeout
                            .map(|duration| received_at + duration);
                        capture::emit(
                            capture,
                            CaptureEvent::Output {
                                kind,
                                text,
                                elapsed_ms: received_ms,
                            },
                        )
                        .await?;
                    }
                }
            } else if buffered.len().saturating_add(chunk.len()) > 16 * 1024 * 1024 {
                parsing_failed = true;
                buffered.clear();
            } else {
                buffered.extend_from_slice(&chunk);
            }
        }
        metadata.body_complete = true;
        if !successful {
            return Err(ClientError::Status(code));
        }
        if parsing_failed || (sse && !decoder.complete()) {
            return Err(ClientError::Response);
        }
        if !sse {
            let value = serde_json::from_slice(&buffered).map_err(|_| ClientError::Response)?;
            content.observe(&value, protocol);
            metadata.usage = content.usage.clone();
            for (kind, text) in output(&value, protocol, false) {
                meaningful = true;
                // 非流式响应不把整个文档的到达时刻伪装为首 Token 时间。
                capture::emit(
                    capture,
                    CaptureEvent::Output {
                        kind,
                        text,
                        elapsed_ms: elapsed(start),
                    },
                )
                .await?;
            }
        }
        if meaningful && content.ended && !content.failed {
            Ok(())
        } else {
            Err(ClientError::Response)
        }
    }
}

/// 只提取协议明确的正文、思考和工具增量；usage、心跳和未知事件不续期。
fn output<'a>(value: &'a Value, protocol: Protocol, streaming: bool) -> Vec<(OutputKind, &'a str)> {
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
                                add(OutputKind::Text, &content["text"]);
                            }
                        }
                        if item["type"] == "function_call" {
                            add(OutputKind::Tool, &item["name"]);
                            add(OutputKind::Tool, &item["arguments"]);
                        }
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

async fn with_deadline<F: std::future::Future>(
    deadline: Option<Deadline>,
    future: F,
) -> Result<F::Output, tokio::time::error::Elapsed> {
    match deadline {
        Some(deadline) => timeout_at(deadline, future).await,
        None => Ok(future.await),
    }
}

/// Reconstruct typed output from a captured response body without issuing a request.
#[derive(Debug, Default)]
pub struct DecodedGenerationOutput {
    pub parts: Vec<GenerationOutputPart>,
    pub encrypted_reasoning: bool,
    pub parsing_complete: bool,
}
#[derive(Debug, serde::Serialize)]
pub struct GenerationOutputPart {
    pub kind: OutputKind,
    pub text: String,
}
pub fn decode_generation_output(protocol: Protocol, body: &[u8]) -> DecodedGenerationOutput {
    let protocol = if protocol == Protocol::CustomHttp {
        Protocol::OpenaiCompatible
    } else {
        protocol
    };
    let mut decoded = DecodedGenerationOutput {
        parsing_complete: true,
        ..Default::default()
    };
    let mut observe = |value: &Value, streaming| {
        decoded.encrypted_reasoning |= value["item"]["type"] == "reasoning"
            && value["item"]["encrypted_content"]
                .as_str()
                .is_some_and(|value| !value.is_empty());
        for (kind, text) in output(value, protocol, streaming) {
            if let Some(previous) = decoded
                .parts
                .last_mut()
                .filter(|previous| previous.kind == kind)
            {
                previous.text.push_str(text);
            } else {
                decoded.parts.push(GenerationOutputPart {
                    kind,
                    text: text.to_owned(),
                });
            }
        }
    };
    if let Ok(value) = serde_json::from_slice::<Value>(body) {
        observe(&value, false);
        return decoded;
    }
    let mut decoder = stream::Sse::default();
    let mut valid = true;
    for chunk in body.chunks(32 * 1024) {
        let events = match decoder.push(chunk) {
            Ok(events) => events,
            Err(_) => {
                valid = false;
                break;
            }
        };
        for event in events {
            if event == "[DONE]" {
                continue;
            }
            match serde_json::from_str::<Value>(&event) {
                Ok(value) => observe(&value, true),
                Err(_) => valid = false,
            }
        }
    }
    decoded.parsing_complete = valid && decoder.complete();
    decoded
}
