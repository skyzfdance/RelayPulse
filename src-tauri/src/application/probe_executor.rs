use std::time::Duration;

use futures_util::StreamExt;
use reqwest::{
    header::{
        HeaderMap, HeaderName, HeaderValue, AUTHORIZATION, CONTENT_LENGTH, CONTENT_TYPE, USER_AGENT,
    },
    redirect, Method, StatusCode, Url,
};
use serde::Serialize;
use serde_json::{json, Map, Value};
use tokio::time::{timeout, Instant};

use crate::domain::{
    AuthPreset, HttpMethod, Model, ParameterType, ProbeCapture, ProbeParameter, ProbePolicy,
    ProbeStatus, RequestHeader, ResponseMode, SecretValue, Site, SiteProtocol, TokenUsage,
    REQUEST_CAPTURE_LIMIT_BYTES, RESPONSE_CAPTURE_LIMIT_BYTES,
};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(120);
const STREAM_IDLE_TIMEOUT: Duration = Duration::from_secs(30);
const RESPONSE_PARSE_LIMIT_BYTES: usize = 8 * 1024 * 1024;
const SSE_EVENT_LIMIT_BYTES: usize = 256 * 1024;

#[derive(Clone)]
pub struct ProbeExecutor {
    client: reqwest::Client,
    stream_idle_timeout: Duration,
}

impl Default for ProbeExecutor {
    fn default() -> Self {
        Self {
            client: reqwest::Client::builder()
                .connect_timeout(CONNECT_TIMEOUT)
                .timeout(REQUEST_TIMEOUT)
                .redirect(redirect::Policy::none())
                .build()
                .expect("static probe HTTP client configuration is valid"),
            stream_idle_timeout: STREAM_IDLE_TIMEOUT,
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
                        "request_timeout",
                        "探针请求未能在 120 秒内完成。",
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
        let response_headers = capture_headers(response.headers(), secret);
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

        let declared_response_bytes = response.content_length();
        let mut parse_body = Vec::new();
        let mut response_total_bytes = 0usize;
        let mut stream_capture = StreamCapture::default();
        let mut sse_decoder = SseDecoder::default();
        let mut first_token_ms = None;
        let mut parsed = ParsedResponse::default();
        let response_limit = usize::try_from(RESPONSE_CAPTURE_LIMIT_BYTES).unwrap_or(usize::MAX);
        let mut stream = response.bytes_stream();
        let mut reached_eof = false;
        let mut response_too_large =
            declared_response_bytes.is_some_and(|size| size > RESPONSE_PARSE_LIMIT_BYTES as u64);
        if response_too_large {
            parsed.error = Some((
                ProbeStatus::ProtocolError,
                "response_too_large",
                "模型服务返回的响应超过解析限制。",
            ));
        }

        while !response_too_large {
            let next = match timeout(self.stream_idle_timeout, stream.next()).await {
                Ok(next) => next,
                Err(_) => {
                    parsed.error = Some((
                        ProbeStatus::Timeout,
                        "stream_idle_timeout",
                        "模型服务的响应流长时间没有新数据。",
                    ));
                    break;
                }
            };
            let Some(next) = next else {
                reached_eof = true;
                break;
            };
            let bytes = match next {
                Ok(bytes) => bytes,
                Err(error) => {
                    parsed.error = Some(if error.is_timeout() {
                        (
                            ProbeStatus::Timeout,
                            "stream_transport_timeout",
                            "探针请求在读取响应流时超时。",
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
            let Some(next_total_bytes) = response_total_bytes.checked_add(bytes.len()) else {
                response_too_large = true;
                parsed.error = Some((
                    ProbeStatus::ProtocolError,
                    "response_too_large",
                    "模型服务返回的响应超过解析限制。",
                ));
                break;
            };
            response_total_bytes = next_total_bytes;
            if response_total_bytes > RESPONSE_PARSE_LIMIT_BYTES {
                response_too_large = true;
                parsed.error = Some((
                    ProbeStatus::ProtocolError,
                    "response_too_large",
                    "模型服务返回的响应超过解析限制。",
                ));
                break;
            }
            let received_after_ms = elapsed_ms(started);
            match response_mode {
                ResponseMode::Synchronous => parse_body.extend_from_slice(&bytes),
                ResponseMode::Stream => {
                    let events = match sse_decoder.push(&bytes) {
                        Ok(events) => events,
                        Err(error) => {
                            parsed.error = Some(error.details());
                            break;
                        }
                    };
                    if !process_sse_events(
                        events,
                        site.protocol,
                        custom_probe,
                        secret,
                        received_after_ms,
                        &mut parsed,
                        &mut first_token_ms,
                        &mut stream_capture,
                        response_limit,
                    ) {
                        break;
                    }
                }
            }
        }

        if reached_eof && response_mode == ResponseMode::Stream {
            match sse_decoder.finish() {
                Ok(events) => {
                    if !process_sse_events(
                        events,
                        site.protocol,
                        custom_probe,
                        secret,
                        elapsed_ms(started),
                        &mut parsed,
                        &mut first_token_ms,
                        &mut stream_capture,
                        response_limit,
                    ) {
                        // The parsed error is returned below with the partial safe capture.
                    }
                }
                Err(error) => parsed.error = Some(error.details()),
            }
        }

        let (response_body, capture_postprocess_truncated) = match response_mode {
            ResponseMode::Stream => (stream_capture.body(), stream_capture.truncated),
            ResponseMode::Synchronous if response_too_large => {
                (r#"{"response":"[OMITTED: TOO LARGE]"}"#.to_owned(), true)
            }
            ResponseMode::Synchronous => {
                let response_body_raw = String::from_utf8_lossy(&parse_body).into_owned();
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
                let redacted = redact_capture_text(&response_body_raw, secret);
                truncate_text(&redacted, response_limit)
            }
        };

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
        let response_truncated = response_too_large
            || response_total_bytes > response_limit
            || capture_postprocess_truncated;
        let response_captured_bytes = byte_count(&response_body);
        let chunks = stream_capture.chunks;
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
        let request_captured_bytes = byte_count(&captured_request_body);
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
                request_captured_bytes,
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
    let mut base = Url::parse(site.base_url.trim()).map_err(|_| ProbeEndpointError)?;
    validate_url(base.clone())?;
    if let Ok(url) = Url::parse(endpoint) {
        let url = validate_url(url)?;
        if url.scheme() != base.scheme()
            || url.host_str() != base.host_str()
            || url.port_or_known_default() != base.port_or_known_default()
        {
            return Err(ProbeEndpointError);
        }
        return Ok(url);
    }
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
    let capture = capture_headers(&headers, secret);
    Ok((headers, capture))
}

fn capture_headers(headers: &HeaderMap, secret: Option<&SecretValue>) -> Vec<CaptureHeader> {
    headers
        .iter()
        .map(|(name, value)| CaptureHeader {
            key: name.as_str().to_owned(),
            value: if is_sensitive_header(name.as_str())
                || secret.is_some_and(|secret| {
                    value
                        .to_str()
                        .is_ok_and(|value| value.contains(secret.expose()))
                }) {
                "[REDACTED]".to_owned()
            } else {
                value.to_str().unwrap_or("[INVALID]").to_owned()
            },
        })
        .collect()
}

fn is_sensitive_header(name: &str) -> bool {
    let normalized = name.to_ascii_lowercase().replace(['-', '_'], "");
    is_sensitive_key(name)
        || normalized.contains("auth")
        || matches!(normalized.as_str(), "cookie" | "setcookie")
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

#[derive(Default)]
struct StreamCapture {
    chunks: Vec<CaptureChunk>,
    captured_bytes: usize,
    sequence: usize,
    truncated: bool,
}

impl StreamCapture {
    fn record(&mut self, raw: String, received_after_ms: u32, limit: usize) {
        let remaining = limit.saturating_sub(self.captured_bytes);
        let (raw, truncated) = truncate_text(&raw, remaining);
        self.truncated |= truncated;
        if raw.is_empty() {
            self.truncated |= remaining == 0;
            return;
        }
        self.sequence += 1;
        self.captured_bytes += raw.len();
        self.chunks.push(CaptureChunk {
            sequence: self.sequence,
            received_after_ms,
            captured_bytes: raw.len(),
            raw,
        });
    }

    fn body(&self) -> String {
        self.chunks.iter().map(|chunk| chunk.raw.as_str()).collect()
    }
}

#[derive(Debug, Clone, Copy)]
enum SseDecodeError {
    EventTooLarge,
    InvalidUtf8,
}

impl SseDecodeError {
    fn details(self) -> (ProbeStatus, &'static str, &'static str) {
        match self {
            Self::EventTooLarge => (
                ProbeStatus::ProtocolError,
                "sse_event_too_large",
                "模型服务返回的单个流式事件超过限制。",
            ),
            Self::InvalidUtf8 => (
                ProbeStatus::ProtocolError,
                "invalid_sse_text",
                "模型服务返回的流式事件不是有效文本。",
            ),
        }
    }
}

#[derive(Default)]
struct SseDecoder {
    buffer: Vec<u8>,
    data_lines: Vec<String>,
    event_bytes: usize,
}

impl SseDecoder {
    fn push(&mut self, raw: &[u8]) -> Result<Vec<String>, SseDecodeError> {
        self.buffer.extend_from_slice(raw);
        let mut events = Vec::new();
        while let Some((line_end, delimiter_len)) = sse_line_boundary(&self.buffer) {
            let line = self.buffer[..line_end].to_vec();
            self.buffer.drain(..line_end + delimiter_len);
            self.process_line(&line, &mut events)?;
        }
        if self
            .event_bytes
            .checked_add(self.buffer.len())
            .is_none_or(|size| size > SSE_EVENT_LIMIT_BYTES)
        {
            return Err(SseDecodeError::EventTooLarge);
        }
        Ok(events)
    }

    fn finish(&mut self) -> Result<Vec<String>, SseDecodeError> {
        let mut events = Vec::new();
        if !self.buffer.is_empty() {
            let mut line = std::mem::take(&mut self.buffer);
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            self.process_line(&line, &mut events)?;
        }
        self.dispatch_event(&mut events);
        Ok(events)
    }

    fn process_line(
        &mut self,
        line: &[u8],
        events: &mut Vec<String>,
    ) -> Result<(), SseDecodeError> {
        self.event_bytes = self
            .event_bytes
            .checked_add(line.len().saturating_add(1))
            .ok_or(SseDecodeError::EventTooLarge)?;
        if self.event_bytes > SSE_EVENT_LIMIT_BYTES {
            return Err(SseDecodeError::EventTooLarge);
        }
        if line.is_empty() {
            self.dispatch_event(events);
            return Ok(());
        }
        if line.first() == Some(&b':') {
            return Ok(());
        }
        let (field, value) = line
            .iter()
            .position(|byte| *byte == b':')
            .map_or((line, &[][..]), |index| {
                (&line[..index], &line[index + 1..])
            });
        if field != b"data" {
            return Ok(());
        }
        let value = value.strip_prefix(b" ").unwrap_or(value);
        let value = std::str::from_utf8(value).map_err(|_| SseDecodeError::InvalidUtf8)?;
        self.data_lines.push(value.to_owned());
        Ok(())
    }

    fn dispatch_event(&mut self, events: &mut Vec<String>) {
        if !self.data_lines.is_empty() {
            events.push(self.data_lines.join("\n"));
            self.data_lines.clear();
        }
        self.event_bytes = 0;
    }
}

fn sse_line_boundary(buffer: &[u8]) -> Option<(usize, usize)> {
    for (index, byte) in buffer.iter().enumerate() {
        match byte {
            b'\n' => return Some((index, 1)),
            b'\r' if index + 1 < buffer.len() => {
                let delimiter_len = if buffer[index + 1] == b'\n' { 2 } else { 1 };
                return Some((index, delimiter_len));
            }
            _ => {}
        }
    }
    None
}

#[allow(clippy::too_many_arguments)]
fn process_sse_events(
    events: Vec<String>,
    protocol: SiteProtocol,
    custom_probe: bool,
    secret: Option<&SecretValue>,
    received_after_ms: u32,
    parsed: &mut ParsedResponse,
    first_token_ms: &mut Option<u32>,
    capture: &mut StreamCapture,
    capture_limit: usize,
) -> bool {
    for data in events {
        let data = data.trim();
        if data.is_empty() {
            continue;
        }
        let captured = if data == "[DONE]" {
            if !custom_probe {
                parsed.ended_normally = true;
            }
            "data: [DONE]\n\n".to_owned()
        } else {
            match serde_json::from_str::<Value>(data) {
                Ok(value) => {
                    if !custom_probe {
                        parsed.first_output_seen |= starts_stream_output(protocol, &value);
                        parse_value(&value, parsed);
                        if first_token_ms.is_none() && parsed.first_output_seen {
                            *first_token_ms = Some(received_after_ms);
                        }
                    }
                    format!("data: {}\n\n", redact_json_value_text(value, secret))
                }
                Err(_) => {
                    if !custom_probe {
                        parsed.error = Some((
                            ProbeStatus::ProtocolError,
                            "invalid_sse_json",
                            "模型服务返回了无法解析的流式事件。",
                        ));
                    }
                    "data: [UNPARSEABLE SSE EVENT OMITTED]\n\n".to_owned()
                }
            }
        };
        capture.record(captured, received_after_ms, capture_limit);
        if parsed.error.is_some() && !custom_probe {
            return false;
        }
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

fn starts_stream_output(protocol: SiteProtocol, value: &Value) -> bool {
    if contains_output_text(value) {
        return true;
    }
    protocol == SiteProtocol::OpenAiResponses
        && matches!(
            value.get("type").and_then(Value::as_str).map(str::trim),
            Some("response.output_item.added" | "response.content_part.added")
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
    let mut end = limit.min(value.len());
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    (value[..end].to_owned(), true)
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
    if let Ok(value) = serde_json::from_str::<Value>(value) {
        return redact_json_value_text(value, secret);
    }
    if contains_binary_payload_marker(value) {
        return r#"{"binaryPayload":"[BINARY OMITTED]"}"#.to_owned();
    }
    secret.map_or_else(
        || value.to_owned(),
        |secret| value.replace(secret.expose(), "[REDACTED]"),
    )
}

fn redact_json_value_text(mut value: Value, secret: Option<&SecretValue>) -> String {
    if let Some(secret) = secret {
        redact_secret_value(&mut value, secret.expose());
    }
    redact_json_value(&mut value);
    serde_json::to_string(&value).unwrap_or_else(|_| "{}".to_owned())
}

fn redact_secret_value(value: &mut Value, secret: &str) {
    match value {
        Value::String(value) => {
            if value.contains(secret) {
                *value = value.replace(secret, "[REDACTED]");
            }
        }
        Value::Object(object) => object
            .values_mut()
            .for_each(|value| redact_secret_value(value, secret)),
        Value::Array(values) => values
            .iter_mut()
            .for_each(|value| redact_secret_value(value, secret)),
        _ => {}
    }
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
            let request = read_http_request(&mut stream);
            let _ = sender.send(String::from_utf8_lossy(&request).into_owned());
            for part in parts {
                if stream.write_all(&part).is_err() || stream.flush().is_err() {
                    break;
                }
                thread::sleep(delay);
            }
        });
        (format!("http://{address}"), receiver, handle)
    }

    fn serve_after_delay(
        response: Vec<u8>,
        delay: StdDuration,
    ) -> (String, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("test server binds");
        let address = listener.local_addr().expect("test server has an address");
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("test request connects");
            stream
                .set_read_timeout(Some(StdDuration::from_secs(2)))
                .expect("read timeout is configured");
            let _ = read_http_request(&mut stream);
            thread::sleep(delay);
            let _ = stream.write_all(&response);
        });
        (format!("http://{address}"), handle)
    }

    fn read_http_request(stream: &mut impl Read) -> Vec<u8> {
        let mut request = Vec::new();
        let mut buffer = [0_u8; 16 * 1024];
        loop {
            let size = stream.read(&mut buffer).expect("test request is readable");
            if size == 0 {
                break;
            }
            request.extend_from_slice(&buffer[..size]);
            let Some(header_end) = request
                .windows(4)
                .position(|window| window == b"\r\n\r\n")
                .map(|index| index + 4)
            else {
                continue;
            };
            let header_text = String::from_utf8_lossy(&request[..header_end]);
            let content_length = header_text
                .lines()
                .find_map(|line| {
                    line.split_once(':').and_then(|(name, value)| {
                        name.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse::<usize>().ok())
                            .flatten()
                    })
                })
                .unwrap_or(0);
            if request.len() >= header_end.saturating_add(content_length) {
                break;
            }
        }
        request
    }

    fn executor_with_timeouts(request_timeout: Duration, idle_timeout: Duration) -> ProbeExecutor {
        ProbeExecutor {
            client: reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(1))
                .timeout(request_timeout)
                .redirect(redirect::Policy::none())
                .build()
                .expect("test client builds"),
            stream_idle_timeout: idle_timeout,
        }
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
        let protocol = SiteProtocol::OpenAiResponses;
        assert!(!starts_stream_output(
            protocol,
            &json!({"type":"response.created"})
        ));
        assert!(!starts_stream_output(
            protocol,
            &json!({"type":"response.in_progress"})
        ));
        assert!(starts_stream_output(
            protocol,
            &json!({"type":"response.output_item.added"})
        ));
        assert!(starts_stream_output(
            protocol,
            &json!({"type":"response.content_part.added"})
        ));
        assert!(starts_stream_output(
            protocol,
            &json!({"type":"response.output_text.delta","delta":"O"})
        ));
        assert!(!starts_stream_output(
            SiteProtocol::OpenAiCompatible,
            &json!({"choices":[{"delta":{"role":"assistant"}}]})
        ));
        assert!(!starts_stream_output(
            SiteProtocol::Anthropic,
            &json!({"type":"message_start","message":{"role":"assistant"}})
        ));
    }

    #[test]
    fn probe_endpoints_cannot_escape_the_site_origin() {
        let (mut site, _, mut policy) = target("https://example.test:443/v1".to_owned(), true);
        site.probe_endpoint = "https://EXAMPLE.test/probe".to_owned();
        assert_eq!(
            resolve_probe_endpoint(&site, &policy)
                .expect("default HTTPS ports share an origin")
                .as_str(),
            "https://example.test/probe"
        );

        for endpoint in [
            "http://example.test/probe",
            "https://internal.example.test/probe",
            "https://example.test:8443/probe",
            "http://127.0.0.1/latest/meta-data",
        ] {
            policy.custom_endpoint = Some(endpoint.to_owned());
            assert!(resolve_probe_endpoint(&site, &policy).is_err());
        }
    }

    #[test]
    fn sse_decoder_joins_data_lines_and_redacts_secrets_split_across_chunks() {
        let secret = SecretValue::new("split-secret").expect("secret is valid");
        let mut decoder = SseDecoder::default();
        let first = br#"data: {
data: "choices":[{"delta":{"content":"OK"}}],
data: "echo":"split-"#;
        let second = br#"secret",
data: "api_key":"sensitive-field"
data: }

"#;
        assert!(decoder.push(first).expect("first chunk decodes").is_empty());
        let events = decoder.push(second).expect("second chunk decodes");
        assert_eq!(events.len(), 1);

        let mut parsed = ParsedResponse::default();
        let mut first_token_ms = None;
        let mut capture = StreamCapture::default();
        assert!(process_sse_events(
            events,
            SiteProtocol::OpenAiCompatible,
            false,
            Some(&secret),
            25,
            &mut parsed,
            &mut first_token_ms,
            &mut capture,
            usize::try_from(RESPONSE_CAPTURE_LIMIT_BYTES).expect("limit fits"),
        ));

        assert!(parsed.has_text);
        assert_eq!(first_token_ms, Some(25));
        assert!(!capture.body().contains("split-secret"));
        assert!(!capture.body().contains("sensitive-field"));
        assert!(capture.body().contains("[REDACTED]"));
    }

    #[test]
    fn unparseable_sse_events_never_persist_their_raw_payload() {
        let mut parsed = ParsedResponse::default();
        let mut first_token_ms = None;
        let mut capture = StreamCapture::default();
        assert!(!process_sse_events(
            vec!["not-json-with-secret-material".to_owned()],
            SiteProtocol::OpenAiCompatible,
            false,
            None,
            10,
            &mut parsed,
            &mut first_token_ms,
            &mut capture,
            1024,
        ));

        assert_eq!(
            parsed.error.as_ref().map(|(_, code, _)| *code),
            Some("invalid_sse_json")
        );
        assert!(!capture.body().contains("secret-material"));
        assert!(capture.body().contains("UNPARSEABLE SSE EVENT OMITTED"));
    }

    #[test]
    fn request_header_capture_uses_conservative_sensitive_name_rules() {
        let secret = SecretValue::new("station-secret").expect("secret is valid");
        let mut headers = HeaderMap::new();
        for (name, value) in [
            ("x-auth-token", "auth-value"),
            ("access-token", "access-value"),
            ("x-client-secret", "client-value"),
            ("x-opaque-credential", "station-secret"),
            ("x-public", "public-value"),
        ] {
            headers.insert(
                HeaderName::from_bytes(name.as_bytes()).expect("header name is valid"),
                HeaderValue::from_str(value).expect("header value is valid"),
            );
        }
        let capture = capture_headers(&headers, Some(&secret));
        let captured_value = |name: &str| {
            capture
                .iter()
                .find(|header| header.key == name)
                .map(|header| header.value.as_str())
        };

        for name in [
            "x-auth-token",
            "access-token",
            "x-client-secret",
            "x-opaque-credential",
        ] {
            assert_eq!(captured_value(name), Some("[REDACTED]"));
        }
        assert_eq!(captured_value("x-public"), Some("public-value"));
    }

    #[test]
    fn capture_truncation_preserves_utf8_and_post_redaction_budget() {
        let (truncated, was_truncated) = truncate_text("ab你cd", 4);
        assert!(was_truncated);
        assert_eq!(truncated, "ab");
        assert!(truncated.len() <= 4);

        let secret = SecretValue::new("x").expect("secret is valid");
        let raw = json!({"message": "x".repeat(128)}).to_string();
        let redacted = redact_capture_text(&raw, Some(&secret));
        let (captured, was_truncated) = truncate_text(&redacted, 64);
        assert!(was_truncated);
        assert!(captured.len() <= 64);
        assert!(captured.contains("[REDACTED]"));
    }

    #[test]
    fn synchronous_protocol_parsing_uses_the_full_body_before_capture_truncation() {
        let response_limit = usize::try_from(RESPONSE_CAPTURE_LIMIT_BYTES).expect("limit fits");
        let raw = json!({
            "model": "test-model",
            "choices": [{
                "message": {"content": "x".repeat(response_limit + 128)},
                "finish_reason": "stop"
            }]
        })
        .to_string();
        let mut parsed = ParsedResponse::default();
        parse_json_text(&raw, &mut parsed);
        let (captured, truncated) = truncate_text(&redact_capture_text(&raw, None), response_limit);

        assert!(parsed.has_text);
        assert!(parsed.ended_normally);
        assert!(truncated);
        assert!(captured.len() <= response_limit);
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

    #[tokio::test]
    async fn request_timeout_covers_waiting_for_response_headers() {
        let body = br#"{"choices":[{"message":{"content":"OK"},"finish_reason":"stop"}]}"#;
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )
        .into_bytes();
        let response = [response, body.to_vec()].concat();
        let (base_url, server) = serve_after_delay(response, StdDuration::from_millis(200));
        let (site, model, policy) = target(base_url, false);
        let secret = SecretValue::new("fixture-secret").expect("secret is valid");
        let execution =
            executor_with_timeouts(Duration::from_millis(50), Duration::from_millis(50))
                .execute(&site, &model, &policy, &[], &[], Some(&secret))
                .await;

        assert_eq!(execution.status, ProbeStatus::Timeout);
        assert_eq!(execution.error_code.as_deref(), Some("request_timeout"));
        server.join().expect("test server stops");
    }

    #[tokio::test]
    async fn stream_idle_timeout_stops_a_stalled_sse_connection() {
        let headers =
            b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n"
                .to_vec();
        let (base_url, _, server) = serve_with_delay(vec![headers], StdDuration::from_millis(200));
        let (site, model, policy) = target(base_url, true);
        let secret = SecretValue::new("fixture-secret").expect("secret is valid");
        let execution = executor_with_timeouts(Duration::from_secs(1), Duration::from_millis(50))
            .execute(&site, &model, &policy, &[], &[], Some(&secret))
            .await;

        assert_eq!(execution.status, ProbeStatus::Timeout);
        assert_eq!(execution.error_code.as_deref(), Some("stream_idle_timeout"));
        server.join().expect("test server stops");
    }

    #[tokio::test]
    async fn probe_client_does_not_follow_redirects() {
        let response = b"HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:9/latest/meta-data\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec();
        let (base_url, _, server) = serve(vec![response]);
        let (site, model, policy) = target(base_url, false);
        let secret = SecretValue::new("fixture-secret").expect("secret is valid");
        let execution = ProbeExecutor::default()
            .execute(&site, &model, &policy, &[], &[], Some(&secret))
            .await;

        assert_eq!(execution.status, ProbeStatus::ProtocolError);
        assert_eq!(
            execution.error_code.as_deref(),
            Some("unexpected_http_status")
        );
        server.join().expect("test server stops");
    }

    #[tokio::test]
    async fn synchronous_response_larger_than_capture_limit_is_still_parsed() {
        let response_limit = usize::try_from(RESPONSE_CAPTURE_LIMIT_BYTES).expect("limit fits");
        let body = json!({
            "model": "test-model",
            "choices": [{
                "message": {"content": "x".repeat(response_limit + 1024)},
                "finish_reason": "stop"
            }]
        })
        .to_string();
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let (base_url, _, server) = serve(vec![response.into_bytes()]);
        let (site, model, policy) = target(base_url, false);
        let secret = SecretValue::new("x").expect("secret is valid");
        let execution = ProbeExecutor::default()
            .execute(&site, &model, &policy, &[], &[], Some(&secret))
            .await;

        assert_eq!(execution.status, ProbeStatus::Healthy);
        assert!(execution.capture.response_truncated);
        assert!(execution.capture.response_body.contains("[REDACTED]"));
        assert_eq!(
            execution.capture.response_captured_bytes,
            byte_count(&execution.capture.response_body)
        );
        assert!(execution.capture.response_captured_bytes <= RESPONSE_CAPTURE_LIMIT_BYTES);
        server.join().expect("test server stops");
    }

    #[tokio::test]
    async fn response_above_the_parse_limit_has_an_explicit_error() {
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            RESPONSE_PARSE_LIMIT_BYTES + 1
        );
        let (base_url, _, server) = serve(vec![response.into_bytes()]);
        let (site, model, policy) = target(base_url, false);
        let secret = SecretValue::new("fixture-secret").expect("secret is valid");
        let execution = ProbeExecutor::default()
            .execute(&site, &model, &policy, &[], &[], Some(&secret))
            .await;

        assert_eq!(execution.status, ProbeStatus::ProtocolError);
        assert_eq!(execution.error_code.as_deref(), Some("response_too_large"));
        assert!(execution.capture.response_truncated);
        assert!(execution.capture.response_captured_bytes <= RESPONSE_CAPTURE_LIMIT_BYTES);
        server.join().expect("test server stops");
    }

    #[tokio::test]
    async fn successful_large_request_reports_only_the_saved_capture_bytes() {
        let body = r#"{"choices":[{"message":{"content":"OK"},"finish_reason":"stop"}]}"#;
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let (base_url, _, server) = serve(vec![response.into_bytes()]);
        let (site, model, mut policy) = target(base_url, false);
        policy.prompt = "你"
            .repeat(usize::try_from(REQUEST_CAPTURE_LIMIT_BYTES).expect("limit fits") / 3 + 1024);
        let secret = SecretValue::new("fixture-secret").expect("secret is valid");
        let execution = ProbeExecutor::default()
            .execute(&site, &model, &policy, &[], &[], Some(&secret))
            .await;

        assert_eq!(execution.status, ProbeStatus::Healthy);
        assert!(execution.capture.request_truncated);
        assert_eq!(
            execution.capture.request_captured_bytes,
            byte_count(&execution.capture.request_body)
        );
        assert!(execution.capture.request_captured_bytes <= REQUEST_CAPTURE_LIMIT_BYTES);
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
