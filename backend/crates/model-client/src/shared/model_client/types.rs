use serde::{Deserialize, Serialize};
use std::time::Duration;
use zeroize::Zeroizing;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Protocol {
    OpenaiCompatible,
    OpenaiResponses,
    Anthropic,
    CustomHttp,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ParameterType {
    String,
    Boolean,
    Number,
    Object,
    Json,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ParameterBinding {
    Model,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Parameter {
    pub key: String,
    pub value: String,
    #[serde(rename = "type")]
    pub kind: ParameterType,
    #[serde(default = "enabled_by_default")]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub binding: Option<ParameterBinding>,
}
fn enabled_by_default() -> bool {
    true
}
impl Parameter {
    pub fn new(key: impl Into<String>, value: impl Into<String>, kind: ParameterType) -> Self {
        Self {
            key: key.into(),
            value: value.into(),
            kind,
            enabled: true,
            binding: None,
        }
    }
    pub fn model() -> Self {
        Self {
            binding: Some(ParameterBinding::Model),
            ..Self::new("model", "", ParameterType::String)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequestHeader {
    pub key: String,
    pub value: String,
}

/// 内存执行输入；不能 Debug 或序列化凭据，也不携带业务实体与标识。
#[derive(Clone)]
pub struct RequestTarget {
    pub protocol: Protocol,
    pub base_url: String,
    pub headers: Vec<RequestHeader>,
    pub secret: Zeroizing<String>,
}
#[derive(Clone, Copy)]
pub enum EndpointMode {
    Relative,
    SameOriginAppend,
}
#[derive(Clone)]
pub enum CheckBody {
    Standard { parameters: Vec<Parameter> },
    Custom { parameters: Vec<Parameter> },
}
#[derive(Clone)]
pub struct CheckRequest {
    pub target: RequestTarget,
    pub model: String,
    pub endpoint: String,
    pub endpoint_mode: EndpointMode,
    pub body: CheckBody,
}
pub struct CatalogRequest {
    pub target: RequestTarget,
    pub endpoint: String,
}
/// 完整生成的独立请求；参数是调用方已经合成的对象，不继承任何探针策略。
#[derive(Clone)]
pub struct GenerationRequest {
    pub target: RequestTarget,
    pub model: String,
    pub endpoint: String,
    pub prompt: String,
    pub parameters: serde_json::Map<String, serde_json::Value>,
}
#[derive(Clone, Copy)]
pub struct GenerationOptions {
    /// None leaves execution bounded only by cancellation and transport errors.
    pub output_timeout: Option<Duration>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GenerationFailure {
    Http,
    Network,
    Timeout,
    InvalidResponse,
    Cancelled,
}
#[derive(Debug, Clone)]
pub struct GenerationResult {
    pub status_code: Option<u16>,
    pub first_token_ms: Option<u64>,
    pub duration_ms: u64,
    pub failure: Option<GenerationFailure>,
    pub metadata: ResponseMetadata,
}
#[derive(Clone, Copy)]
pub struct ExecutionOptions {
    pub total_timeout: Duration,
    pub idle_timeout: Duration,
}
impl Default for ExecutionOptions {
    fn default() -> Self {
        Self {
            total_timeout: Duration::from_secs(600),
            idle_timeout: Duration::from_secs(120),
        }
    }
}
#[derive(Debug, Clone, thiserror::Error)]
#[error("{message}")]
pub struct InputError {
    pub field: &'static str,
    pub message: &'static str,
}
pub(super) fn invalid(field: &'static str, message: &'static str) -> InputError {
    InputError { field, message }
}
#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("请求记录写入失败。")]
    Capture,
    #[error(transparent)]
    Invalid(#[from] InputError),
    #[error("请求已取消。")]
    Cancelled,
    #[error("远端请求超时。")]
    Timeout,
    #[error("无法连接远端服务。")]
    Network,
    #[error("远端返回 HTTP {0}。")]
    Status(u16),
    #[error("远端响应格式无效或超过大小限制。")]
    Response,
}
impl ClientError {
    pub(super) fn transport(error: reqwest::Error) -> Self {
        if error.is_timeout() {
            Self::Timeout
        } else {
            Self::Network
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckFailure {
    Http,
    Network,
    Timeout,
    InvalidResponse,
}
#[derive(Debug, Clone)]
pub struct CheckResult {
    pub success: bool,
    pub status_code: Option<u16>,
    pub duration_ms: u64,
    /// 从请求开始到首段非空思考或回答文本；自定义请求及非流式响应为 None。
    pub first_token_ms: Option<u64>,
    pub failure: Option<CheckFailure>,
    pub metadata: ResponseMetadata,
}

/// 只保存供应商实际报告的用量；缺失字段为 None，不估算或相加补齐。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenUsage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub total_tokens: Option<u64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResponseMetadata {
    pub request_id: Option<String>,
    pub kind: Option<super::ResponseKind>,
    pub usage: TokenUsage,
    pub received_bytes: u64,
    pub body_complete: bool,
}
impl CheckResult {
    pub fn message(&self) -> &'static str {
        match self.failure {
            None => "远端请求已成功完成。",
            Some(CheckFailure::Http) => "远端返回非成功 HTTP 状态。",
            Some(CheckFailure::Network) => "无法完成远端网络请求。",
            Some(CheckFailure::Timeout) => "远端请求超时。",
            Some(CheckFailure::InvalidResponse) => "响应无有效输出、未正常结束或格式无效。",
        }
    }
}
