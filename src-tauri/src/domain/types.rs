use std::fmt;

use zeroize::Zeroizing;

use super::{DomainError, EntityId, Result, TimestampMs};

pub const DEFAULT_PROBE_INTERVAL_MINUTES: u16 = 10;
pub const DEFAULT_SLOW_FIRST_TOKEN_MS: u32 = 2_000;
pub const DEFAULT_SLOW_TOTAL_MS: u32 = 10_000;
pub const MAX_SLOW_THRESHOLD_MS: u32 = 86_400_000;
pub const DEFAULT_FAILURE_THRESHOLD: u8 = 2;
pub const DEFAULT_RECOVERY_THRESHOLD: u8 = 2;
pub const REQUEST_CAPTURE_LIMIT_BYTES: u32 = 32 * 1024;
pub const RESPONSE_CAPTURE_LIMIT_BYTES: u32 = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SiteProtocol {
    OpenAiCompatible,
    Anthropic,
    OpenAiResponses,
    CustomHttp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpMethod {
    Get,
    Post,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthPreset {
    Bearer,
    XApiKey,
    ApiKey,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleState {
    Ready,
    CredentialPending,
    CleanupPending,
    DeletePending,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelSource {
    Discovered,
    Manual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiscoveryStatus {
    Available,
    Missing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeMode {
    Inherit,
    DisabledThinking,
    MinimumReasoning,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParameterType {
    String,
    Boolean,
    Number,
    Json,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeRunKind {
    Scheduled,
    Manual,
    Trial,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeStatus {
    Healthy,
    Slow,
    Probing,
    RateLimited,
    AuthError,
    ModelUnavailable,
    Timeout,
    ProtocolError,
    ValidationFailed,
    UpstreamError,
    NetworkError,
    Cancelled,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponseMode {
    Stream,
    Synchronous,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeMode {
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayAction {
    Panel,
    Disabled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredentialAction {
    Create,
    Replace,
    Clear,
    Delete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredentialOperationStatus {
    IntentRecorded,
    CredentialWritten,
    ConfigCommitted,
    CleanupPending,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeTimingPolicy {
    pub interval_minutes: Option<u16>,
    pub slow_first_token_ms: u32,
    pub slow_total_ms: u32,
    pub failure_threshold: u8,
    pub recovery_threshold: u8,
}

impl ProbeTimingPolicy {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        interval_minutes: Option<u16>,
        slow_first_token_ms: u32,
        slow_total_ms: u32,
        failure_threshold: u8,
        recovery_threshold: u8,
    ) -> Result<Self> {
        if interval_minutes.is_some_and(|value| !(1..=1_440).contains(&value)) {
            return Err(DomainError::invalid("probe_interval_minutes"));
        }
        if !(1..=MAX_SLOW_THRESHOLD_MS).contains(&slow_first_token_ms) {
            return Err(DomainError::invalid("slow_first_token_ms"));
        }
        if !(1..=MAX_SLOW_THRESHOLD_MS).contains(&slow_total_ms) {
            return Err(DomainError::invalid("slow_total_ms"));
        }
        if !(1..=10).contains(&failure_threshold) {
            return Err(DomainError::invalid("failure_threshold"));
        }
        if !(1..=10).contains(&recovery_threshold) {
            return Err(DomainError::invalid("recovery_threshold"));
        }

        Ok(Self {
            interval_minutes,
            slow_first_token_ms,
            slow_total_ms,
            failure_threshold,
            recovery_threshold,
        })
    }
}

impl Default for ProbeTimingPolicy {
    fn default() -> Self {
        Self {
            interval_minutes: Some(DEFAULT_PROBE_INTERVAL_MINUTES),
            slow_first_token_ms: DEFAULT_SLOW_FIRST_TOKEN_MS,
            slow_total_ms: DEFAULT_SLOW_TOTAL_MS,
            failure_threshold: DEFAULT_FAILURE_THRESHOLD,
            recovery_threshold: DEFAULT_RECOVERY_THRESHOLD,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Site {
    pub id: EntityId,
    pub name: String,
    pub protocol: SiteProtocol,
    pub base_url: String,
    pub credential_ref: Option<EntityId>,
    pub enabled: bool,
    pub sort_order: u32,
    pub models_method: HttpMethod,
    pub models_endpoint: String,
    pub probe_method: HttpMethod,
    pub probe_endpoint: String,
    pub content_type: String,
    pub auth_preset: AuthPreset,
    pub stream_enabled: bool,
    pub timing: ProbeTimingPolicy,
    pub lifecycle_state: LifecycleState,
    pub created_at: TimestampMs,
    pub updated_at: TimestampMs,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Model {
    pub id: EntityId,
    pub site_id: EntityId,
    pub model_key: String,
    pub display_name: Option<String>,
    pub source: ModelSource,
    pub discovery_status: DiscoveryStatus,
    pub enabled: bool,
    pub sort_order: u32,
    pub created_at: TimestampMs,
    pub updated_at: TimestampMs,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbePolicy {
    pub id: EntityId,
    pub site_id: EntityId,
    pub model_id: Option<EntityId>,
    pub mode: ProbeMode,
    pub max_output_tokens: u16,
    pub prompt: String,
    pub custom_endpoint: Option<String>,
    pub custom_method: Option<HttpMethod>,
    pub custom_stream_enabled: Option<bool>,
    pub created_at: TimestampMs,
    pub updated_at: TimestampMs,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeParameter {
    pub id: EntityId,
    pub policy_id: EntityId,
    pub key: String,
    pub value_type: ParameterType,
    pub value_json: String,
    pub sort_order: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestHeader {
    pub id: EntityId,
    pub site_id: EntityId,
    pub name: String,
    pub value_template: String,
    pub sort_order: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenUsage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub total_tokens: Option<u64>,
    pub reported: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeResult {
    pub id: EntityId,
    pub site_id: EntityId,
    pub model_id: EntityId,
    pub run_kind: ProbeRunKind,
    pub request_method: HttpMethod,
    pub endpoint: String,
    pub probe_mode: ProbeMode,
    pub started_at: TimestampMs,
    pub finished_at: Option<TimestampMs>,
    pub status: ProbeStatus,
    pub response_headers_ms: Option<u32>,
    pub first_token_ms: Option<u32>,
    pub total_time_ms: Option<u32>,
    pub http_status: Option<u16>,
    pub provider_model: Option<String>,
    pub finish_reason: Option<String>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub token_usage: TokenUsage,
    pub ended_normally: bool,
    pub retry_count: u16,
    pub request_id: Option<String>,
    pub revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeCapture {
    pub probe_result_id: EntityId,
    pub request_headers_json: String,
    pub request_body: String,
    pub request_captured_bytes: u32,
    pub request_limit_bytes: u32,
    pub request_truncated: bool,
    pub response_mode: Option<ResponseMode>,
    pub response_content_type: Option<String>,
    pub response_headers_json: String,
    pub response_body: String,
    pub response_chunks_json: String,
    pub response_captured_bytes: u32,
    pub response_limit_bytes: u32,
    pub response_truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LatestModelStatus {
    pub model_id: EntityId,
    pub probe_result_id: Option<EntityId>,
    pub status: ProbeStatus,
    pub consecutive_failures: u16,
    pub consecutive_successes: u16,
    pub next_probe_at: Option<TimestampMs>,
    pub revision: u64,
    pub updated_at: TimestampMs,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneralSettings {
    pub launch_at_startup: bool,
    pub continue_in_background: bool,
    pub tray_action: TrayAction,
    pub theme: ThemeMode,
    pub history_retention_days: u16,
    pub diagnostic_capture_enabled: bool,
    pub updated_at: TimestampMs,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CredentialOperation {
    pub id: EntityId,
    pub site_id: EntityId,
    pub action: CredentialAction,
    pub status: CredentialOperationStatus,
    pub new_credential_ref: Option<EntityId>,
    pub old_credential_ref: Option<EntityId>,
    pub non_secret_payload_json: String,
    pub created_at: TimestampMs,
    pub updated_at: TimestampMs,
    pub completed_at: Option<TimestampMs>,
}

pub struct SecretValue(Zeroizing<String>);

impl SecretValue {
    pub fn new(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(DomainError::invalid("secret"));
        }
        Ok(Self(Zeroizing::new(value)))
    }

    pub fn expose(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Debug for SecretValue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SecretValue([REDACTED])")
    }
}

pub enum CredentialIntent {
    Keep,
    Replace(SecretValue),
    Clear,
}

impl fmt::Debug for CredentialIntent {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Keep => formatter.write_str("Keep"),
            Self::Replace(_) => formatter.write_str("Replace([REDACTED])"),
            Self::Clear => formatter.write_str("Clear"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_timing_defaults_match_the_implementation_plan() {
        let policy = ProbeTimingPolicy::default();

        assert_eq!(policy.interval_minutes, Some(10));
        assert_eq!(policy.slow_first_token_ms, 2_000);
        assert_eq!(policy.slow_total_ms, 10_000);
        assert_eq!(policy.failure_threshold, 2);
        assert_eq!(policy.recovery_threshold, 2);
    }

    #[test]
    fn invalid_timing_relations_are_rejected_at_the_domain_boundary() {
        let error = ProbeTimingPolicy::new(Some(10), MAX_SLOW_THRESHOLD_MS + 1, 10_000, 2, 2)
            .expect_err("slow threshold must stay within the supported range");
        assert_eq!(error.field(), Some("slow_first_token_ms"));

        let error = ProbeTimingPolicy::new(Some(0), 2_000, 10_000, 2, 2)
            .expect_err("zero interval is invalid");
        assert_eq!(error.field(), Some("probe_interval_minutes"));
    }

    #[test]
    fn secret_debug_output_never_contains_the_secret() {
        let marker = "relaypulse-secret-marker";
        let intent = CredentialIntent::Replace(SecretValue::new(marker).expect("secret is valid"));
        let debug = format!("{intent:?}");

        assert!(!debug.contains(marker));
        assert_eq!(debug, "Replace([REDACTED])");
    }
}
