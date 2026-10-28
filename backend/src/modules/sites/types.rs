use serde::{Deserialize, Serialize};

pub(crate) use crate::shared::model_client::{Parameter, ParameterType, Protocol, RequestHeader};

#[derive(Clone, Serialize)]
pub(crate) struct ProtocolOption {
    pub value: Protocol,
    pub label: &'static str,
}

pub(crate) fn protocols() -> Vec<ProtocolOption> {
    [
        (Protocol::OpenaiCompatible, "OpenAI 兼容协议"),
        (Protocol::OpenaiResponses, "OpenAI Responses"),
        (Protocol::Anthropic, "Anthropic 协议"),
        (Protocol::CustomHttp, "自定义 HTTP"),
    ]
    .into_iter()
    .map(|(value, label)| ProtocolOption { value, label })
    .collect()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Connection {
    pub models_path: String,
    pub probe_path: String,
    pub interval_seconds: u32,
    pub slow_first_token_seconds: u16,
    pub slow_total_seconds: u16,
    pub failure_threshold: u8,
    pub recovery_threshold: u8,
    pub headers: Vec<RequestHeader>,
}
impl Connection {
    pub(crate) fn for_protocol(protocol: Protocol) -> Self {
        Self {
            models_path: "/v1/models".into(),
            probe_path: match protocol {
                Protocol::OpenaiCompatible => "/v1/chat/completions",
                Protocol::OpenaiResponses => "/v1/responses",
                Protocol::Anthropic => "/v1/messages",
                Protocol::CustomHttp => "/probe",
            }
            .into(),
            interval_seconds: 300,
            slow_first_token_seconds: 5,
            slow_total_seconds: 30,
            failure_threshold: 3,
            recovery_threshold: 2,
            headers: vec![],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ProbePolicy {
    #[serde(default = "parameter_schema_version")]
    pub version: u8,
    pub parameters: Vec<Parameter>,
}
fn parameter_schema_version() -> u8 {
    2
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CustomRequest {
    #[serde(default = "parameter_schema_version")]
    pub version: u8,
    pub endpoint: String,
    pub parameters: Vec<Parameter>,
}

// 输入凭据既不实现 Debug，也不序列化。长期持有的网络凭据使用 Zeroizing。
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CreateSite {
    pub name: String,
    pub protocol: Protocol,
    pub base_url: String,
    pub secret: String,
    pub enabled: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum CredentialAction {
    Keep,
    Replace,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct SiteDraft {
    pub name: String,
    pub protocol: Protocol,
    pub base_url: String,
    pub credential_action: CredentialAction,
    pub secret: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Model {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    #[serde(skip)]
    pub created_at_ms: u64,
    pub policy: Option<ProbePolicy>,
    pub custom_request: Option<CustomRequest>,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Site {
    pub id: String,
    pub name: String,
    pub protocol: Protocol,
    pub base_url: String,
    pub enabled: bool,
    pub credential_configured: bool,
    pub connection: Connection,
    pub policy: ProbePolicy,
    pub models: Vec<Model>,
    #[serde(skip)]
    pub revision: i64,
    #[serde(skip)]
    pub priority: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RemovedModel {
    pub id: String,
    pub name: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SyncChanges {
    pub added: Vec<String>,
    pub removed: Vec<RemovedModel>,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ApplySync {
    pub models: Vec<String>,
    pub added: Vec<String>,
    pub removed: Vec<String>,
}

impl Drop for CreateSite {
    fn drop(&mut self) {
        zeroize::Zeroize::zeroize(&mut self.secret);
    }
}
impl Drop for SiteDraft {
    fn drop(&mut self) {
        zeroize::Zeroize::zeroize(&mut self.secret);
    }
}
