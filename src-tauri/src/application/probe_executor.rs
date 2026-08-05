use std::time::Duration;

use futures_util::StreamExt;
use reqwest::{
    header::{
        HeaderMap, HeaderName, HeaderValue, AUTHORIZATION, CONTENT_LENGTH, CONTENT_TYPE, USER_AGENT,
    },
    Method, StatusCode, Url,
};
use serde::Serialize;
use serde_json::{json, Map, Value};
use tokio::time::Instant;

use crate::domain::{
    AuthPreset, HttpMethod, Model, ParameterType, ProbeCapture, ProbeParameter, ProbePolicy,
    ProbeStatus, RequestHeader, ResponseMode, SecretValue, Site, SiteProtocol, TokenUsage,
    REQUEST_CAPTURE_LIMIT_BYTES, RESPONSE_CAPTURE_LIMIT_BYTES,
};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone)]
pub struct ProbeExecutor {
    client: reqwest::Client,
}

impl Default for ProbeExecutor {
    fn default() -> Self {
        Self {
            client: reqwest::Client::builder()
                .connect_timeout(CONNECT_TIMEOUT)
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
        }
    }
}

#[derive(Debug)]
pub struct ProbeExecution {
    pub status: ProbeStatus,
    pub response_headers_ms: Option<u32>,
    pub first_token_ms: Option<u32>,
    pub total_time_ms: u32,
    pub http_status: Option<u16>,
    pub provider_model: Option<String>,
    pub finish_reason: Option<String>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub token_usage: TokenUsage,
    pub ended_normally: bool,
    pub request_id: Option<String>,
    pub endpoint: String,
    pub capture: ProbeCapture,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProbeEndpointError;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CaptureHeader {
    key: String,
    value: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CaptureChunk {
    sequence: usize,
    received_after_ms: u32,
    captured_bytes: usize,
    raw: String,
}

impl ProbeExecutor {
    pub async fn execute(
        &self,
        site: &Site,
        model: &Model,
        policy: &ProbePolicy,
        parameters: &[ProbeParameter],
        configured_headers: &[RequestHeader],
        secret: Option<&SecretValue>,
    ) -> ProbeExecution {
        let started = Instant::now();
        let custom_probe = policy.custom_endpoint.is_some();
        let endpoint = match resolve_probe_endpoint(site, policy) {
            Ok(endpoint) => endpoint,
            Err(_) => {
                return failed_execution(
                    site,
                    started,
                    ProbeStatus::ValidationFailed,
                    "invalid_probe_endpoint",
                    "探针地址无效，请检查高级设置。",
                    String::new(),
                    "{}".to_owned(),
                )
            }
        };
        let request_body = match request_body(site, model, policy, parameters) {
            Ok(body) => body,
            Err(()) => {
                return failed_execution(
                    site,
                    started,
                    ProbeStatus::ValidationFailed,
                    "invalid_probe_parameters",
                    "探针参数无效，请检查模型探针设置。",
                    endpoint.to_string(),
                    "{}".to_owned(),
                )
            }
        };
        let redacted_request_body = redact_json_body(&request_body);
        let (headers, captured_request_headers) =
            match request_headers(site, configured_headers, secret) {
                Ok(value) => value,
                Err(HeaderBuildError::MissingCredential) => {
                    return failed_execution(
                        site,
                        started,
                        ProbeStatus::AuthError,
                        "credential_missing",
                        "站点没有可用的访问凭据。",
                        endpoint.to_string(),
                        redacted_request_body,
                    )
                }
                Err(HeaderBuildError::Invalid) => {
                    return failed_execution(
                        site,
                        started,
                        ProbeStatus::ValidationFailed,
                        "invalid_probe_headers",
                        "探针请求头无效，请检查高级设置。",
                        endpoint.to_string(),
                        redacted_request_body,
                    )
                }
            };
        let method = policy.custom_method.unwrap_or(site.probe_method);
        let request_headers_json =
            serde_json::to_string(&captured_request_headers).unwrap_or_else(|_| "[]".to_owned());
        let request_limit = usize::try_from(REQUEST_CAPTURE_LIMIT_BYTES).unwrap_or(usize::MAX);
        let (captured_request_body, request_truncated) =
            truncate_text(&redacted_request_body, request_limit);

        let mut request = self
            .client
            .request(http_method(method), endpoint.clone())
            .headers(headers);
        if method == HttpMethod::Post {
            request = request.body(request_body.clone());
        }
        let response = match request.send().await {
            Ok(response) => response,
            Err(error) => {
                let (status, code, message) = if error.is_timeout() {
                    (
                        ProbeStatus::Timeout,
                        "connection_timeout",
                        "无法在 30 秒内建立连接。",
                    )
                } else {
                    (
                        ProbeStatus::NetworkError,
                        "network_error",
                        "无法连接到模型服务。",
                    )
                };
                let request_captured_bytes = byte_count(&captured_request_body);
                let mut execution = failed_execution(
                    site,
                    started,
                    status,
                    code,
                    message,
                    endpoint.to_string(),
                    captured_request_body,
                );
                execution.capture.request_headers_json = request_headers_json;
                execution.capture.request_captured_bytes = request_captured_bytes;
                execution.capture.request_truncated = request_truncated;
                return execution;
            }
        };

        let headers_elapsed = elapsed_ms(started);
        let status_code = response.status();
        let response_headers = capture_headers(response.headers());
        let response_headers_json =
            serde_json::to_string(&response_headers).unwrap_or_else(|_| "[]".to_owned());
        let content_type = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(ToOwned::to_owned);
        let request_id = ["x-request-id", "request-id", "cf-ray"]
            .iter()
            .find_map(|name| response.headers().get(*name))
            .and_then(|value| value.to_str().ok())
            .map(ToOwned::to_owned);
        let response_mode = if status_code.is_success()
            && content_type
                .as_deref()
                .is_some_and(|value| value.to_ascii_lowercase().contains("text/event-stream"))
        {
            ResponseMode::Stream
        } else {
            ResponseMode::Synchronous
        };

        let mut body = Vec::new();
        let mut response_total_bytes = 0usize;
        let mut chunks = Vec::new();
        let mut captured_chunk_bytes = 0usize;
        let mut sse_buffer = Vec::new();
        let mut first_token_ms = None;
        let mut parsed = ParsedResponse::default();
        let response_limit = usize::try_from(RESPONSE_CAPTURE_LIMIT_BYTES).unwrap_or(usize::MAX);
        let mut stream = response.bytes_stream();
        let mut sequence = 0usize;

        loop {
            let next = stream.next().await;
            let Some(next) = next else { break };
            let bytes = match next {
                Ok(bytes) => bytes,
                Err(error) => {
                    parsed.error = Some(if error.is_timeout() {
                        (
                            ProbeStatus::NetworkError,
                            "stream_transport_timeout",
                            "读取探针响应时连接中断。",
                        )
                    } else {
                        (
                            ProbeStatus::NetworkError,
                            "network_error",
                            "读取探针响应失败。",
                        )
                    });
                    break;
                }
            };
            sequence += 1;
            let received_after_ms = elapsed_ms(started);
            let raw = String::from_utf8_lossy(&bytes).into_owned();
            let captured_raw = redact_capture_text(&raw, secret);
            response_total_bytes = response_total_bytes.saturating_add(bytes.len());
            if body.len() < response_limit {
                let remaining = response_limit - body.len();
                body.extend_from_slice(&bytes[..bytes.len().min(remaining)]);
            }
            if response_mode == ResponseMode::Stream {
                if body.len() <= response_limit && captured_chunk_bytes < response_limit {
                    let (captured_raw, _) =
                        truncate_text(&captured_raw, response_limit - captured_chunk_bytes);
                    captured_chunk_bytes = captured_chunk_bytes.saturating_add(captured_raw.len());
                    chunks.push(CaptureChunk {
                        sequence,
                        received_after_ms,
                        captured_bytes: captured_raw.len(),
                        raw: captured_raw,
                    });
                }
                if !custom_probe && !parse_sse_chunk(&bytes, &mut sse_buffer, &mut parsed) {
                    break;
                }
                if !custom_probe && first_token_ms.is_none() && parsed.first_output_seen {
                    first_token_ms = Some(received_after_ms);
                }
            }
        }

        if !custom_probe && response_mode == ResponseMode::Stream && !sse_buffer.is_empty() {
            parse_sse_text(&String::from_utf8_lossy(&sse_buffer), &mut parsed);
        }

        let response_body_raw = String::from_utf8_lossy(&body).into_owned();
        if response_mode == ResponseMode::Synchronous {
            if custom_probe {
                if let Ok(value) = serde_json::from_str::<Value>(&response_body_raw) {
                    parse_value(&value, &mut parsed);
                }
            } else {
                parse_json_text(&response_body_raw, &mut parsed);
            }
            if !custom_probe
                && site.protocol == SiteProtocol::CustomHttp
                && !response_body_raw.trim().is_empty()
                && parsed
                    .error
                    .as_ref()
                    .is_some_and(|(_, code, _)| *code == "invalid_json")
            {
                parsed.error = None;
                parsed.has_text = true;
            }
        }
        let response_body = redact_capture_text(&response_body_raw, secret);
        if !custom_probe && status_code.is_success() && !parsed.has_text && parsed.error.is_none() {
            parsed.error = Some((
                ProbeStatus::ProtocolError,
                "missing_output",
                "模型服务没有返回有效输出。",
            ));
        }
        if !custom_probe
            && response_mode == ResponseMode::Stream
            && status_code.is_success()
            && parsed.has_text
            && !parsed.ended_normally
            && parsed.error.is_none()
        {
            parsed.error = Some((
                ProbeStatus::ProtocolError,
                "stream_ended_early",
                "流式响应未正常结束。",
            ));
        }
        let total_time_ms = elapsed_ms(started);
        let response_truncated = response_total_bytes > response_limit;
        let response_captured_bytes = byte_count(&response_body);
        let (probe_status, error_code, error_message) = classify_result(status_code, parsed.error);
        let status = if probe_status == ProbeStatus::Healthy
            && (first_token_ms.is_some_and(|value| value >= site.timing.slow_first_token_ms)
                || total_time_ms >= site.timing.slow_total_ms)
        {
            ProbeStatus::Slow
        } else {
            probe_status
        };

        let succeeded = matches!(status, ProbeStatus::Healthy | ProbeStatus::Slow);
        ProbeExecution {
            status,
            response_headers_ms: Some(headers_elapsed),
            first_token_ms,
            total_time_ms,
            http_status: Some(status_code.as_u16()),
            provider_model: parsed.provider_model,
            finish_reason: parsed.finish_reason,
            error_code: error_code.map(ToOwned::to_owned),
            error_message: error_message.map(ToOwned::to_owned),
            token_usage: parsed.usage.unwrap_or_else(empty_usage),
            ended_normally: succeeded
                && (custom_probe
                    || response_mode == ResponseMode::Synchronous
                    || parsed.ended_normally),
            request_id,
            endpoint: endpoint.to_string(),
            capture: ProbeCapture {
                probe_result_id: model.id.clone(),
                request_headers_json,
                request_body: captured_request_body,
                request_captured_bytes: byte_count(&request_body),
                request_limit_bytes: REQUEST_CAPTURE_LIMIT_BYTES,
                request_truncated,
                response_mode: Some(response_mode),
                response_content_type: content_type,
                response_headers_json,
                response_body,
                response_chunks_json: serde_json::to_string(&chunks)
                    .unwrap_or_else(|_| "[]".to_owned()),
                response_captured_bytes,
                response_limit_bytes: RESPONSE_CAPTURE_LIMIT_BYTES,
                response_truncated,
            },
        }
    }
}

pub fn resolve_probe_endpoint(
    site: &Site,
    policy: &ProbePolicy,
) -> Result<Url, ProbeEndpointError> {
    let endpoint = policy
        .custom_endpoint
        .as_deref()
        .unwrap_or(&site.probe_endpoint)
        .trim();
    if endpoint.is_empty() {
        return Err(ProbeEndpointError);
    }
    if let Ok(url) = Url::parse(endpoint) {
        return validate_url(url);
    }
    let mut base = Url::parse(site.base_url.trim()).map_err(|_| ProbeEndpointError)?;
    validate_url(base.clone())?;
    if !base.path().ends_with('/') {
        base.set_path(&format!("{}/", base.path()));
    }
    base.join(endpoint.trim_start_matches('/'))
        .map_err(|_| ProbeEndpointError)
        .and_then(validate_url)
}

fn validate_url(url: Url) -> Result<Url, ProbeEndpointError> {
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(ProbeEndpointError);
    }
    Ok(url)
}

fn request_body(
    site: &Site,
    model: &Model,
    policy: &ProbePolicy,
    parameters: &[ProbeParameter],
) -> Result<String, ()> {
    let custom_probe = policy.custom_endpoint.is_some();
    let mut object = if custom_probe {
        json!({"model": model.model_key})
    } else {
        match site.protocol {
            SiteProtocol::OpenAiCompatible => json!({
                "model": model.model_key,
                "messages": [{"role": "user", "content": policy.prompt}],
                "stream": true,
                "max_tokens": policy.max_output_tokens,
            }),
            SiteProtocol::OpenAiResponses => json!({
                "model": model.model_key,
                "input": policy.prompt,
                "stream": true,
                "max_output_tokens": policy.max_output_tokens,
            }),
            SiteProtocol::Anthropic => json!({
                "model": model.model_key,
                "messages": [{"role": "user", "content": policy.prompt}],
                "stream": true,
                "max_tokens": policy.max_output_tokens,
            }),
            SiteProtocol::CustomHttp => json!({"model": model.model_key}),
        }
    };
    let object = object.as_object_mut().ok_or(())?;
    if !custom_probe {
        apply_default_reasoning(object, site.protocol);
    }
    let reserved = reserved_parameter_names(site.protocol, custom_probe);
    for parameter in parameters {
        let key = parameter.key.trim();
        let path = parameter_path(key)?;
        let root = path.first().ok_or(())?.to_ascii_lowercase();
        if reserved.contains(&root.as_str()) {
            return Err(());
        }
        insert_parameter_path(object, &path, parameter_value(parameter)?)?;
    }
    serde_json::to_string(&object).map_err(|_| ())
}

fn parameter_value(parameter: &ProbeParameter) -> Result<Value, ()> {
    match parameter.value_type {
        ParameterType::String => serde_json::from_str::<Value>(&parameter.value_json)
            .ok()
            .and_then(|value| value.as_str().map(|value| Value::String(value.to_owned())))
            .or_else(|| Some(Value::String(parameter.value_json.clone())))
            .ok_or(()),
        ParameterType::Boolean => serde_json::from_str::<bool>(&parameter.value_json)
            .map(Value::Bool)
            .map_err(|_| ()),
        ParameterType::Number => serde_json::from_str::<serde_json::Number>(&parameter.value_json)
            .map(Value::Number)
            .map_err(|_| ()),
        ParameterType::Json => serde_json::from_str(&parameter.value_json).map_err(|_| ()),
    }
}

fn parameter_path(key: &str) -> Result<Vec<String>, ()> {
    let path = key
        .split('.')
        .map(str::trim)
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    if path.is_empty()
        || path.iter().any(|segment| {
            segment.is_empty()
                || matches!(
                    segment.to_ascii_lowercase().as_str(),
                    "__proto__" | "prototype" | "constructor"
                )
        })
    {
        return Err(());
    }
    Ok(path)
}

fn insert_parameter_path(
    object: &mut Map<String, Value>,
    path: &[String],
    value: Value,
) -> Result<(), ()> {
    let (leaf, parents) = path.split_last().ok_or(())?;
    let mut current = object;
    for segment in parents {
        let entry = current
            .entry(segment.clone())
            .or_insert_with(|| Value::Object(Map::new()));
        if !entry.is_object() {
            *entry = Value::Object(Map::new());
        }
        current = entry.as_object_mut().ok_or(())?;
    }
    current.insert(leaf.clone(), value);
    Ok(())
}

fn reserved_parameter_names(protocol: SiteProtocol, custom_probe: bool) -> &'static [&'static str] {
    if custom_probe {
        return &["model"];
    }
    match protocol {
        SiteProtocol::OpenAiCompatible | SiteProtocol::Anthropic => {
            &["model", "messages", "stream"]
        }
        SiteProtocol::OpenAiResponses => &["model", "input", "stream"],
        SiteProtocol::CustomHttp => &["model"],
    }
}

fn apply_default_reasoning(object: &mut Map<String, Value>, protocol: SiteProtocol) {
    match protocol {
        SiteProtocol::OpenAiCompatible => {
            object.insert(
                "reasoning_effort".to_owned(),
                Value::String("low".to_owned()),
            );
        }
        SiteProtocol::OpenAiResponses => {
            object.insert("reasoning".to_owned(), json!({ "effort": "low" }));
        }
        SiteProtocol::Anthropic | SiteProtocol::CustomHttp => {}
    }
}

enum HeaderBuildError {
    MissingCredential,
    Invalid,
}

fn request_headers(
    site: &Site,
    configured: &[RequestHeader],
    secret: Option<&SecretValue>,
) -> Result<(HeaderMap, Vec<CaptureHeader>), HeaderBuildError> {
    let mut headers = HeaderMap::new();
    headers.insert(
        CONTENT_TYPE,
        HeaderValue::from_str(&site.content_type).map_err(|_| HeaderBuildError::Invalid)?,
    );
    headers.insert(
        USER_AGENT,
        HeaderValue::from_str(&super::outbound_user_agent("model-probe"))
            .map_err(|_| HeaderBuildError::Invalid)?,
    );
    match site.auth_preset {
        AuthPreset::Bearer => {
            let secret = secret.ok_or(HeaderBuildError::MissingCredential)?;
            headers.insert(
                AUTHORIZATION,
                HeaderValue::from_str(&format!("Bearer {}", secret.expose()))
                    .map_err(|_| HeaderBuildError::Invalid)?,
            );
        }
        AuthPreset::XApiKey | AuthPreset::ApiKey => {
            let secret = secret.ok_or(HeaderBuildError::MissingCredential)?;
            let name = if site.auth_preset == AuthPreset::XApiKey {
                "x-api-key"
            } else {
                "api-key"
            };
            headers.insert(
                name,
                HeaderValue::from_str(secret.expose()).map_err(|_| HeaderBuildError::Invalid)?,
            );
        }
        AuthPreset::None => {}
    }
    if site.protocol == SiteProtocol::Anthropic {
        headers.insert("anthropic-version", HeaderValue::from_static("2023-06-01"));
    }
    for header in configured {
        let name = HeaderName::from_bytes(header.name.trim().as_bytes())
            .map_err(|_| HeaderBuildError::Invalid)?;
        if name == CONTENT_LENGTH {
            return Err(HeaderBuildError::Invalid);
        }
        let value =
            HeaderValue::from_str(&header.value_template).map_err(|_| HeaderBuildError::Invalid)?;
        headers.insert(name, value);
    }
    let capture = capture_headers(&headers);
    Ok((headers, capture))
}

fn capture_headers(headers: &HeaderMap) -> Vec<CaptureHeader> {
    headers
        .iter()
        .map(|(name, value)| CaptureHeader {
            key: name.as_str().to_owned(),
            value: if is_sensitive_header(name.as_str()) {
                "[REDACTED]".to_owned()
            } else {
                value.to_str().unwrap_or("[INVALID]").to_owned()
            },
        })
        .collect()
}

fn is_sensitive_header(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "authorization" | "proxy-authorization" | "x-api-key" | "api-key" | "cookie" | "set-cookie"
    )
}

fn http_method(method: HttpMethod) -> Method {
    match method {
        HttpMethod::Get => Method::GET,
        HttpMethod::Post => Method::POST,
    }
}

#[derive(Default)]
struct ParsedResponse {
    has_text: bool,
    first_output_seen: bool,
    ended_normally: bool,
    usage: Option<TokenUsage>,
    provider_model: Option<String>,
    finish_reason: Option<String>,
    error: Option<(ProbeStatus, &'static str, &'static str)>,
}

fn parse_sse_text(raw: &str, parsed: &mut ParsedResponse) {
    for line in raw.lines() {
        let data = line.trim().strip_prefix("data:").map(str::trim);
        let Some(data) = data else { continue };
        if data.is_empty() || data == "[DONE]" {
            if data == "[DONE]" {
                parsed.ended_normally = true;
            }
            continue;
        }
        match serde_json::from_str::<Value>(data) {
            Ok(value) => parse_value(&value, parsed),
            Err(_) => {
                parsed.error = Some((
                    ProbeStatus::ProtocolError,
                    "invalid_sse_json",
                    "模型服务返回了无法解析的流式事件。",
                ));
            }
        }
    }
}

fn parse_sse_chunk(raw: &[u8], buffer: &mut Vec<u8>, parsed: &mut ParsedResponse) -> bool {
    buffer.extend_from_slice(raw);
    if buffer.len() > usize::try_from(RESPONSE_CAPTURE_LIMIT_BYTES).unwrap_or(usize::MAX) {
        parsed.error = Some((
            ProbeStatus::ProtocolError,
            "sse_event_too_large",
            "模型服务返回的单个流式事件超过限制。",
        ));
        buffer.clear();
        return false;
    }
    while let Some(index) = buffer.iter().position(|byte| *byte == b'\n') {
        let line = String::from_utf8_lossy(&buffer[..=index]).into_owned();
        buffer.drain(..=index);
        parse_sse_text(&line, parsed);
    }
    true
}

fn parse_json_text(raw: &str, parsed: &mut ParsedResponse) {
    match serde_json::from_str::<Value>(raw) {
        Ok(value) => parse_value(&value, parsed),
        Err(_) if !raw.trim().is_empty() => {
            parsed.error = Some((
                ProbeStatus::ProtocolError,
                "invalid_json",
                "模型服务返回了无法解析的响应。",
            ))
        }
        Err(_) => {}
    }
}

fn parse_value(value: &Value, parsed: &mut ParsedResponse) {
    parsed.first_output_seen |= starts_stream_output(value);
    parsed.has_text |= contains_output_text(value);
    parsed.usage = merge_usage(parsed.usage.take(), find_usage(value));
    parsed.provider_model = parsed.provider_model.take().or_else(|| {
        value
            .get("model")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
    });
    parsed.finish_reason = parsed.finish_reason.take().or_else(|| {
        value
            .pointer("/choices/0/finish_reason")
            .or_else(|| value.get("stop_reason"))
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
    });
    parsed.ended_normally |= parsed.finish_reason.is_some()
        || matches!(
            value.get("type").and_then(Value::as_str),
            Some("response.completed" | "message_stop")
        );
}

// Match sub2api's Responses TTFT semantics: created/in-progress are preamble,
// while the first following semantic SSE event starts client output.
fn starts_stream_output(value: &Value) -> bool {
    !matches!(
        value.get("type").and_then(Value::as_str).map(str::trim),
        Some("response.created" | "response.in_progress" | "response.failed")
    )
}

fn contains_output_text(value: &Value) -> bool {
    [
        "/choices/0/delta/content",
        "/choices/0/message/content",
        "/choices/0/text",
        "/delta/text",
        "/output_text",
    ]
    .iter()
    .any(|pointer| {
        value
            .pointer(pointer)
            .and_then(Value::as_str)
            .is_some_and(|text| !text.is_empty())
    }) || (value.get("type").and_then(Value::as_str) == Some("response.output_text.delta")
        && value
            .get("delta")
            .and_then(Value::as_str)
            .is_some_and(|text| !text.is_empty()))
        || value
            .get("content")
            .and_then(Value::as_array)
            .is_some_and(|items| items.iter().any(contains_output_text))
        || value
            .get("output")
            .and_then(Value::as_array)
            .is_some_and(|items| items.iter().any(contains_output_text))
        || (value.get("type").and_then(Value::as_str) == Some("output_text")
            && value
                .get("text")
                .and_then(Value::as_str)
                .is_some_and(|text| !text.is_empty()))
}

fn find_usage(value: &Value) -> Option<TokenUsage> {
    if let Some(usage) = value.get("usage").and_then(Value::as_object) {
        let input = number(usage, &["input_tokens", "prompt_tokens"]);
        let output = number(usage, &["output_tokens", "completion_tokens"]);
        let total = number(usage, &["total_tokens"]).or_else(|| match (input, output) {
            (Some(input), Some(output)) => input.checked_add(output),
            _ => None,
        });
        if input.is_some() || output.is_some() || total.is_some() {
            return Some(TokenUsage {
                input_tokens: input,
                output_tokens: output,
                total_tokens: total,
                reported: true,
            });
        }
    }
    match value {
        Value::Object(object) => object.values().find_map(find_usage),
        Value::Array(values) => values.iter().find_map(find_usage),
        _ => None,
    }
}

fn merge_usage(current: Option<TokenUsage>, incoming: Option<TokenUsage>) -> Option<TokenUsage> {
    match (current, incoming) {
        (None, incoming) => incoming,
        (current, None) => current,
        (Some(current), Some(incoming)) => {
            let input_tokens = incoming.input_tokens.or(current.input_tokens);
            let output_tokens = incoming.output_tokens.or(current.output_tokens);
            let total_tokens = incoming.total_tokens.or(current.total_tokens).or_else(|| {
                input_tokens
                    .zip(output_tokens)
                    .and_then(|(input, output)| input.checked_add(output))
            });
            Some(TokenUsage {
                input_tokens,
                output_tokens,
                total_tokens,
                reported: current.reported || incoming.reported,
            })
        }
    }
}

fn number(object: &Map<String, Value>, names: &[&str]) -> Option<u64> {
    names
        .iter()
        .find_map(|name| object.get(*name).and_then(Value::as_u64))
}

fn classify_result(
    status: StatusCode,
    stream_error: Option<(ProbeStatus, &'static str, &'static str)>,
) -> (ProbeStatus, Option<&'static str>, Option<&'static str>) {
    if !status.is_success() {
        return match status {
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => (
                ProbeStatus::AuthError,
                Some("authentication_failed"),
                Some("模型服务拒绝了访问凭据。"),
            ),
            StatusCode::NOT_FOUND => (
                ProbeStatus::ModelUnavailable,
                Some("model_unavailable"),
                Some("模型或探针地址不存在。"),
            ),
            StatusCode::TOO_MANY_REQUESTS => (
                ProbeStatus::RateLimited,
                Some("rate_limited"),
                Some("模型服务限制了请求频率。"),
            ),
            status if status.is_server_error() => (
                ProbeStatus::UpstreamError,
                Some("upstream_error"),
                Some("模型服务返回了服务器错误。"),
            ),
            _ => (
                ProbeStatus::ProtocolError,
                Some("unexpected_http_status"),
                Some("模型服务返回了非预期状态码。"),
            ),
        };
    }
    if let Some((status, code, message)) = stream_error {
        return (status, Some(code), Some(message));
    }
    (ProbeStatus::Healthy, None, None)
}

fn failed_execution(
    site: &Site,
    started: Instant,
    status: ProbeStatus,
    code: &str,
    message: &str,
    endpoint: String,
    request_body: String,
) -> ProbeExecution {
    let request_limit = usize::try_from(REQUEST_CAPTURE_LIMIT_BYTES).unwrap_or(usize::MAX);
    let (request_body, request_truncated) = truncate_text(&request_body, request_limit);
    ProbeExecution {
        status,
        response_headers_ms: None,
        first_token_ms: None,
        total_time_ms: elapsed_ms(started),
        http_status: None,
        provider_model: None,
        finish_reason: None,
        error_code: Some(code.to_owned()),
        error_message: Some(message.to_owned()),
        token_usage: empty_usage(),
        ended_normally: false,
        request_id: None,
        endpoint,
        capture: ProbeCapture {
            probe_result_id: site.id.clone(),
            request_headers_json: "[]".to_owned(),
            request_captured_bytes: byte_count(&request_body),
            request_body,
            request_limit_bytes: REQUEST_CAPTURE_LIMIT_BYTES,
            request_truncated,
            response_mode: None,
            response_content_type: None,
            response_headers_json: "[]".to_owned(),
            response_body: String::new(),
            response_chunks_json: "[]".to_owned(),
            response_captured_bytes: 0,
            response_limit_bytes: RESPONSE_CAPTURE_LIMIT_BYTES,
            response_truncated: false,
        },
    }
}

fn empty_usage() -> TokenUsage {
    TokenUsage {
        input_tokens: None,
        output_tokens: None,
        total_tokens: None,
        reported: false,
    }
}

fn truncate_text(value: &str, limit: usize) -> (String, bool) {
    if value.len() <= limit {
        return (value.to_owned(), false);
    }
    (
        String::from_utf8_lossy(&value.as_bytes()[..limit]).into_owned(),
        true,
    )
}

fn redact_json_body(value: &str) -> String {
    let Ok(mut value) = serde_json::from_str::<Value>(value) else {
        return if contains_binary_payload_marker(value) {
            r#"{"binaryPayload":"[BINARY OMITTED]"}"#.to_owned()
        } else {
            value.to_owned()
        };
    };
    redact_json_value(&mut value);
    serde_json::to_string(&value).unwrap_or_else(|_| "{}".to_owned())
}

fn redact_capture_text(value: &str, secret: Option<&SecretValue>) -> String {
    let value = secret.map_or_else(
        || value.to_owned(),
        |secret| value.replace(secret.expose(), "[REDACTED]"),
    );
    redact_json_body(&value)
}

fn redact_json_value(value: &mut Value) {
    match value {
        Value::Object(object) => {
            for (key, value) in object {
                if is_sensitive_key(key) {
                    *value = Value::String("[REDACTED]".to_owned());
                } else if is_binary_payload_key(key) {
                    *value = Value::String("[BINARY OMITTED]".to_owned());
                } else {
                    redact_json_value(value);
                }
            }
        }
        Value::Array(values) => values.iter_mut().for_each(redact_json_value),
        _ => {}
    }
}

fn contains_binary_payload_marker(value: &str) -> bool {
    ["b64_json", "base64_json", "image_base64", "audio_base64"]
        .iter()
        .any(|key| value.contains(&format!(r#""{key}""#)))
}

fn is_binary_payload_key(key: &str) -> bool {
    matches!(
        key.to_ascii_lowercase().replace('-', "_").as_str(),
        "b64_json" | "base64_json" | "image_base64" | "audio_base64"
    )
}

fn is_sensitive_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase().replace(['-', '_'], "");
    key.contains("authorization")
        || key.contains("apikey")
        || key.ends_with("token")
        || key.contains("secret")
        || key.contains("password")
}

fn byte_count(value: &str) -> u32 {
    u32::try_from(value.len()).unwrap_or(u32::MAX)
}

fn elapsed_ms(started: Instant) -> u32 {
    u32::try_from(started.elapsed().as_millis()).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        sync::mpsc,
        thread,
        time::Duration as StdDuration,
    };

    use super::*;
    use crate::domain::{
        DiscoveryStatus, EntityId, LifecycleState, ModelSource, ProbeMode, ProbeTimingPolicy,
        TimestampMs,
    };

    fn id(value: &str) -> EntityId {
        EntityId::parse(value).expect("test id is valid")
    }

    fn target(base_url: String, stream_enabled: bool) -> (Site, Model, ProbePolicy) {
        let timing =
            ProbeTimingPolicy::new(Some(10), 500, 2_000, 2, 2).expect("test timing is valid");
        let now = TimestampMs::new(1_700_000_000_000).expect("test timestamp is valid");
        let site_id = id("site-a");
        let model_id = id("model-a");
        (
            Site {
                id: site_id.clone(),
                name: "Test".to_owned(),
                protocol: SiteProtocol::OpenAiCompatible,
                base_url,
                credential_ref: Some(id("credential-a")),
                enabled: true,
                sort_order: 0,
                models_method: HttpMethod::Get,
                models_endpoint: "/models".to_owned(),
                probe_method: HttpMethod::Post,
                probe_endpoint: "/probe".to_owned(),
                content_type: "application/json".to_owned(),
                auth_preset: AuthPreset::Bearer,
                stream_enabled,
                timing,
                lifecycle_state: LifecycleState::Ready,
                created_at: now,
                updated_at: now,
            },
            Model {
                id: model_id,
                site_id: site_id.clone(),
                model_key: "test-model".to_owned(),
                display_name: None,
                source: ModelSource::Manual,
                discovery_status: DiscoveryStatus::Available,
                enabled: true,
                sort_order: 0,
                created_at: now,
                updated_at: now,
            },
            ProbePolicy {
                id: id("policy-a"),
                site_id,
                model_id: None,
                mode: ProbeMode::DisabledThinking,
                max_output_tokens: 8,
                prompt: "Reply OK".to_owned(),
                custom_endpoint: None,
                custom_method: None,
                custom_stream_enabled: None,
                created_at: now,
                updated_at: now,
            },
        )
    }

    fn serve(parts: Vec<Vec<u8>>) -> (String, mpsc::Receiver<String>, thread::JoinHandle<()>) {
        serve_with_delay(parts, StdDuration::from_millis(10))
    }

    fn serve_with_delay(
        parts: Vec<Vec<u8>>,
        delay: StdDuration,
    ) -> (String, mpsc::Receiver<String>, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("test server binds");
        let address = listener.local_addr().expect("test server has an address");
        let (sender, receiver) = mpsc::channel();
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("test request connects");
            stream
                .set_read_timeout(Some(StdDuration::from_secs(2)))
                .expect("read timeout is configured");
            let mut buffer = [0u8; 16 * 1024];
            let size = stream.read(&mut buffer).expect("test request is readable");
            let _ = sender.send(String::from_utf8_lossy(&buffer[..size]).into_owned());
            for part in parts {
                stream.write_all(&part).expect("test response is writable");
                stream.flush().expect("test response flushes");
                thread::sleep(delay);
            }
        });
        (format!("http://{address}"), receiver, handle)
    }

    #[test]
    fn token_usage_supports_openai_and_anthropic_names() {
        let openai = json!({"usage":{"prompt_tokens":3,"completion_tokens":2,"total_tokens":5}});
        let anthropic = json!({"message":{"usage":{"input_tokens":7,"output_tokens":1}}});

        assert_eq!(
            find_usage(&openai).and_then(|usage| usage.total_tokens),
            Some(5)
        );
        assert_eq!(
            find_usage(&anthropic).and_then(|usage| usage.total_tokens),
            Some(8)
        );
    }

    #[test]
    fn real_output_text_is_required_for_a_successful_probe() {
        assert!(!contains_output_text(
            &json!({"choices":[{"delta":{"role":"assistant"}}]})
        ));
        assert!(contains_output_text(
            &json!({"choices":[{"delta":{"content":"OK"}}]})
        ));
        assert!(contains_output_text(
            &json!({"type":"response.output_text.delta","delta":"O"})
        ));
    }

    #[test]
    fn first_output_event_matches_sub2api_responses_semantics() {
        assert!(!starts_stream_output(&json!({"type":"response.created"})));
        assert!(!starts_stream_output(
            &json!({"type":"response.in_progress"})
        ));
        assert!(starts_stream_output(
            &json!({"type":"response.output_item.added"})
        ));
        assert!(starts_stream_output(
            &json!({"type":"response.content_part.added"})
        ));
        assert!(starts_stream_output(
            &json!({"type":"response.output_text.delta","delta":"O"})
        ));
    }

    #[test]
    fn unavailable_http_statuses_are_failure_states() {
        assert_eq!(
            classify_result(StatusCode::TOO_MANY_REQUESTS, None).0,
            ProbeStatus::RateLimited
        );
        assert_eq!(
            classify_result(StatusCode::BAD_GATEWAY, None).0,
            ProbeStatus::UpstreamError
        );
        assert_eq!(
            classify_result(StatusCode::SERVICE_UNAVAILABLE, None).0,
            ProbeStatus::UpstreamError
        );
    }

    #[tokio::test]
    async fn synchronous_fallback_records_reported_usage_without_fabricating_ttft() {
        let body = r#"{"model":"test-model","choices":[{"message":{"content":"OK"},"finish_reason":"stop"}],"usage":{"prompt_tokens":3,"completion_tokens":2,"total_tokens":5}}"#;
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        );
        let (base_url, request, server) = serve(vec![response.into_bytes()]);
        let (site, model, policy) = target(base_url, false);
        let secret = SecretValue::new("fixture-secret-marker").expect("secret is valid");
        let execution = ProbeExecutor::default()
            .execute(&site, &model, &policy, &[], &[], Some(&secret))
            .await;

        assert_eq!(execution.status, ProbeStatus::Healthy);
        assert_eq!(execution.first_token_ms, None);
        assert_eq!(execution.token_usage.total_tokens, Some(5));
        assert!(execution.token_usage.reported);
        assert!(execution.ended_normally);
        assert!(!execution
            .capture
            .request_headers_json
            .contains("fixture-secret-marker"));
        let captured_request = request
            .recv_timeout(StdDuration::from_secs(1))
            .expect("request was captured");
        let captured_request_lower = captured_request.to_ascii_lowercase();
        assert!(captured_request_lower.contains("authorization: bearer fixture-secret-marker"));
        assert!(captured_request_lower.contains("user-agent: relaypulse/"));
        assert!(captured_request_lower.contains("model-probe"));
        assert!(captured_request_lower.contains(r#""stream":true"#));
        server.join().expect("test server stops");
    }

    #[tokio::test]
    async fn custom_image_probe_sends_only_configured_fields_and_accepts_a_2xx_response() {
        let body = r#"{"created":1700000000,"data":[{"b64_json":"QUJDREVGRw=="}]}"#;
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        );
        let (base_url, request, server) = serve(vec![response.into_bytes()]);
        let (mut site, model, mut policy) = target(base_url, false);
        site.protocol = SiteProtocol::OpenAiResponses;
        policy.custom_endpoint = Some("/v1/images/generations".to_owned());
        policy.custom_method = Some(HttpMethod::Post);
        policy.custom_stream_enabled = Some(false);
        let parameters = [
            ProbeParameter {
                id: id("parameter-prompt"),
                policy_id: policy.id.clone(),
                key: "prompt".to_owned(),
                value_type: ParameterType::String,
                value_json: "A blue circle on white".to_owned(),
                sort_order: 0,
            },
            ProbeParameter {
                id: id("parameter-size"),
                policy_id: policy.id.clone(),
                key: "size".to_owned(),
                value_type: ParameterType::String,
                value_json: "1024x1024".to_owned(),
                sort_order: 1,
            },
            ProbeParameter {
                id: id("parameter-quality"),
                policy_id: policy.id.clone(),
                key: "quality".to_owned(),
                value_type: ParameterType::String,
                value_json: "low".to_owned(),
                sort_order: 2,
            },
            ProbeParameter {
                id: id("parameter-count"),
                policy_id: policy.id.clone(),
                key: "n".to_owned(),
                value_type: ParameterType::Number,
                value_json: "1".to_owned(),
                sort_order: 3,
            },
        ];
        let secret = SecretValue::new("fixture-secret").expect("secret is valid");

        let execution = ProbeExecutor::default()
            .execute(&site, &model, &policy, &parameters, &[], Some(&secret))
            .await;

        let captured_request = request
            .recv_timeout(StdDuration::from_secs(1))
            .expect("request was captured");
        let request_body = captured_request
            .split_once("\r\n\r\n")
            .map(|(_, body)| body)
            .expect("request contains a body");
        let request_json: Value =
            serde_json::from_str(request_body).expect("request body is valid JSON");
        assert_eq!(
            request_json,
            json!({
                "model": "test-model",
                "prompt": "A blue circle on white",
                "size": "1024x1024",
                "quality": "low",
                "n": 1,
            })
        );
        assert!(request_json.get("input").is_none());
        assert!(request_json.get("stream").is_none());
        assert!(request_json.get("max_output_tokens").is_none());
        assert!(request_json.get("reasoning").is_none());
        assert_eq!(execution.status, ProbeStatus::Healthy);
        assert_eq!(execution.first_token_ms, None);
        assert!(execution.ended_normally);
        assert!(execution.capture.response_body.contains("[BINARY OMITTED]"));
        assert!(!execution.capture.response_body.contains("QUJDREVGRw=="));
        assert_eq!(
            execution.capture.response_captured_bytes,
            byte_count(&execution.capture.response_body)
        );
        server.join().expect("test server stops");
    }

    #[tokio::test]
    async fn streaming_probe_assembles_split_events_and_validates_real_text() {
        let headers =
            b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n"
                .to_vec();
        let role = b"data: {\"choices\":[{\"delta\":{\"role\":\"assistant\"}}]}\n\n".to_vec();
        let split_a = b"data: {\"choices\":[{\"delta\":{\"cont".to_vec();
        let split_b = b"ent\":\"OK\"},\"finish_reason\":\"stop\"}],\"usage\":{\"input_tokens\":2,\"output_tokens\":1}}\n\ndata: [DONE]\n\n".to_vec();
        let (base_url, _, server) = serve(vec![headers, role, split_a, split_b]);
        let (site, model, policy) = target(base_url, true);
        let secret = SecretValue::new("fixture-secret").expect("secret is valid");
        let execution = ProbeExecutor::default()
            .execute(&site, &model, &policy, &[], &[], Some(&secret))
            .await;

        assert_eq!(execution.status, ProbeStatus::Healthy);
        assert!(execution.first_token_ms.is_some());
        assert_eq!(execution.token_usage.total_tokens, Some(3));
        assert!(execution.ended_normally);
        server.join().expect("test server stops");
    }

    #[tokio::test]
    async fn delayed_first_stream_token_is_observed_without_a_separate_timeout() {
        let headers =
            b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n"
                .to_vec();
        let output = b"data: {\"choices\":[{\"delta\":{\"content\":\"OK\"},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n".to_vec();
        let (base_url, _, server) =
            serve_with_delay(vec![headers, output], StdDuration::from_millis(1_200));
        let (site, model, policy) = target(base_url, true);
        let secret = SecretValue::new("fixture-secret").expect("secret is valid");
        let execution = ProbeExecutor::default()
            .execute(&site, &model, &policy, &[], &[], Some(&secret))
            .await;

        assert_eq!(execution.status, ProbeStatus::Slow);
        assert!(execution.first_token_ms.is_some_and(|value| value >= 1_000));
        assert_eq!(execution.error_code, None);
        assert!(execution.ended_normally);
        server.join().expect("test server stops");
    }

    #[tokio::test]
    async fn authentication_errors_are_classified_without_waiting_for_a_stream_token() {
        let response = b"HTTP/1.1 401 Unauthorized\r\nContent-Type: application/json\r\nContent-Length: 2\r\n\r\n{}".to_vec();
        let (base_url, _, server) = serve(vec![response]);
        let (site, model, policy) = target(base_url, true);
        let secret = SecretValue::new("fixture-secret").expect("secret is valid");
        let execution = ProbeExecutor::default()
            .execute(&site, &model, &policy, &[], &[], Some(&secret))
            .await;

        assert_eq!(execution.status, ProbeStatus::AuthError);
        assert_eq!(execution.first_token_ms, None);
        assert_eq!(
            execution.error_code.as_deref(),
            Some("authentication_failed")
        );
        server.join().expect("test server stops");
    }

    #[test]
    fn nested_sensitive_request_fields_are_redacted_before_capture() {
        let body = r#"{"model":"x","max_output_tokens":8,"metadata":{"api_token":"secret","safe":"value"}}"#;
        let redacted = redact_json_body(body);

        assert!(!redacted.contains("secret"));
        assert!(redacted.contains("[REDACTED]"));
        assert!(redacted.contains("value"));
        assert!(redacted.contains(r#""max_output_tokens":8"#));
    }

    #[test]
    fn default_reasoning_is_low_and_pass_through_paths_override_business_fields() {
        let (mut site, model, mut policy) = target("https://example.test".to_owned(), true);
        let compatible: Value = serde_json::from_str(
            &request_body(&site, &model, &policy, &[]).expect("request is valid"),
        )
        .expect("request is JSON");
        assert_eq!(compatible["reasoning_effort"], "low");
        assert_eq!(compatible["stream"], true);

        site.protocol = SiteProtocol::OpenAiResponses;
        let responses: Value = serde_json::from_str(
            &request_body(&site, &model, &policy, &[]).expect("request is valid"),
        )
        .expect("request is JSON");
        assert_eq!(responses["input"], "Reply OK");
        assert_eq!(responses["stream"], true);
        assert_eq!(responses["max_output_tokens"], 8);
        assert_eq!(responses["reasoning"]["effort"], "low");

        policy.mode = ProbeMode::MinimumReasoning;
        let responses: Value = serde_json::from_str(
            &request_body(&site, &model, &policy, &[]).expect("request is valid"),
        )
        .expect("request is JSON");
        assert_eq!(responses["reasoning"]["effort"], "low");

        let parameters = [
            ProbeParameter {
                id: id("parameter-effort"),
                policy_id: policy.id.clone(),
                key: "reasoning.effort".to_owned(),
                value_type: ParameterType::String,
                value_json: "max".to_owned(),
                sort_order: 0,
            },
            ProbeParameter {
                id: id("parameter-output"),
                policy_id: policy.id.clone(),
                key: "max_output_tokens".to_owned(),
                value_type: ParameterType::Number,
                value_json: "16".to_owned(),
                sort_order: 1,
            },
        ];
        let responses: Value = serde_json::from_str(
            &request_body(&site, &model, &policy, &parameters).expect("request is valid"),
        )
        .expect("request is JSON");
        assert_eq!(responses["reasoning"]["effort"], "max");
        assert_eq!(responses["max_output_tokens"], 16);
        assert!(responses.get("reasoning.effort").is_none());
    }

    #[test]
    fn transport_parameter_roots_cannot_be_overridden() {
        let (mut site, model, policy) = target("https://example.test".to_owned(), true);
        site.protocol = SiteProtocol::OpenAiResponses;
        let parameter = ProbeParameter {
            id: id("parameter-stream"),
            policy_id: policy.id.clone(),
            key: "stream.enabled".to_owned(),
            value_type: ParameterType::Boolean,
            value_json: "false".to_owned(),
            sort_order: 0,
        };

        assert!(request_body(&site, &model, &policy, &[parameter]).is_err());
    }

    #[test]
    fn sensitive_response_values_are_removed_from_capture_text() {
        let secret = SecretValue::new("fixture-sensitive-value").expect("secret is valid");
        let capture = redact_capture_text(
            r#"{"error":{"api_key":"fixture-sensitive-value"}}"#,
            Some(&secret),
        );

        assert!(!capture.contains("fixture-sensitive-value"));
        assert!(capture.contains("[REDACTED]"));
    }

    #[test]
    fn truncated_binary_json_is_replaced_instead_of_stored() {
        let capture = redact_capture_text(
            r#"{"data":[{"b64_json":"QUJDREVGR0hJSktMTU5PUFFSU1RVVldYWVo=""#,
            None,
        );

        assert_eq!(capture, r#"{"binaryPayload":"[BINARY OMITTED]"}"#);
        assert!(!capture.contains("QUJDREVGR0hJ"));
    }
}
