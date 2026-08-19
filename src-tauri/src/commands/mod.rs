pub(crate) mod intelligence_ladder;

use std::{collections::HashMap, fs, future::Future, time::Instant};

use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use crate::{
    application::{
        resolve_probe_endpoint, ApplicationError, ApplicationService, ApplicationSnapshot,
        CreateModel, CreateSite, CustomProbeUpdate, ModelAggregate, ModelSyncOutcome,
        ParameterValue, ProbeRecordAggregate, ProbeSettingsUpdate, SiteAdvancedUpdate,
        SiteAggregate, UpdateSite,
    },
    desktop,
    diagnostics::{
        Diagnostics, FrontendErrorRecord, LogCleanupResult, LogStatistics, OperationAudit,
        OperationHistoryPageDto, OperationHistoryQueryDto, OperationHistoryQueryError,
        OperationRecordDetailsDto, MAX_LOG_TOTAL_MEGABYTES, MIN_LOG_TOTAL_MEGABYTES,
    },
    domain::{
        CredentialIntent, EntityId, HttpMethod, ParameterType, ProbeMode, ProbeParameter,
        ProbePolicy, ProbeResult, ProbeStatus, RequestHeader, ResponseMode, SecretValue,
        SiteProtocol, ThemeMode, TimestampMs, TrayAction,
    },
    notifications::{
        request_system_notification_permission, send_system_notification,
        send_test_system_notification, system_channel_snapshot, NotificationChannelSnapshot,
        NotificationError, NotificationRule, NotificationService, NotificationSettingsSnapshot,
        NotificationSettingsUpdate, NotificationTemplateDraft, NotificationTemplateTestResult,
    },
    startup::BackendState,
};
use uuid::Uuid;

type CommandResult<T> = std::result::Result<T, CommandError>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandError {
    code: &'static str,
    category: &'static str,
    message: &'static str,
    field: Option<&'static str>,
    retryable: bool,
    operation_id: Option<String>,
}

impl From<ApplicationError> for CommandError {
    fn from(error: ApplicationError) -> Self {
        match error {
            ApplicationError::InvalidInput { field } => Self {
                code: "invalid_input",
                category: "validation",
                message: "The submitted value is invalid.",
                field: Some(field),
                retryable: false,
                operation_id: None,
            },
            ApplicationError::NotFound { .. } => Self {
                code: "not_found",
                category: "not_found",
                message: "The requested resource was not found.",
                field: None,
                retryable: false,
                operation_id: None,
            },
            ApplicationError::CredentialUnavailable => Self {
                code: "credential_unavailable",
                category: "credential",
                message: "The local credential file is unavailable.",
                field: None,
                retryable: true,
                operation_id: None,
            },
            ApplicationError::DatabaseUnavailable => Self::database_unavailable(),
            ApplicationError::Network => Self {
                code: "network_error",
                category: "network",
                message: "RelayPulse could not reach the model catalog.",
                field: None,
                retryable: true,
                operation_id: None,
            },
            ApplicationError::Timeout => Self {
                code: "timeout",
                category: "network",
                message: "The model catalog request timed out.",
                field: None,
                retryable: true,
                operation_id: None,
            },
            ApplicationError::Authentication => Self {
                code: "authentication_failed",
                category: "credential",
                message: "The model catalog rejected the configured credential.",
                field: None,
                retryable: false,
                operation_id: None,
            },
            ApplicationError::RateLimited => Self {
                code: "rate_limited",
                category: "upstream",
                message: "The model catalog request was rate limited.",
                field: None,
                retryable: true,
                operation_id: None,
            },
            ApplicationError::Upstream => Self {
                code: "upstream_error",
                category: "upstream",
                message: "The model catalog server returned an error.",
                field: None,
                retryable: true,
                operation_id: None,
            },
            ApplicationError::Protocol => Self {
                code: "protocol_error",
                category: "protocol",
                message: "The model catalog response is not supported.",
                field: None,
                retryable: false,
                operation_id: None,
            },
            ApplicationError::Conflict => Self {
                code: "probe_conflict",
                category: "conflict",
                message: "A probe is already running for this model.",
                field: None,
                retryable: true,
                operation_id: None,
            },
            ApplicationError::Internal => Self {
                code: "internal",
                category: "internal",
                message: "RelayPulse could not complete the operation.",
                field: None,
                retryable: false,
                operation_id: None,
            },
        }
    }
}

impl From<NotificationError> for CommandError {
    fn from(error: NotificationError) -> Self {
        match error {
            NotificationError::SettingsConflict => Self {
                code: "notification_settings_conflict",
                category: "conflict",
                message: "Notification settings changed in another operation.",
                field: None,
                retryable: true,
                operation_id: None,
            },
            NotificationError::CatalogChanged => Self {
                code: "notification_catalog_changed",
                category: "conflict",
                message: "The notification catalog changed while saving.",
                field: None,
                retryable: true,
                operation_id: None,
            },
            NotificationError::InvalidSettings { field } => Self {
                code: "notification_settings_invalid",
                category: "validation",
                message: "The submitted notification settings are invalid.",
                field: Some(field),
                retryable: false,
                operation_id: None,
            },
            NotificationError::TemplateInvalid { field } => Self {
                code: "notification_template_invalid",
                category: "validation",
                message: "The submitted notification template is invalid.",
                field: Some(field),
                retryable: false,
                operation_id: None,
            },
            NotificationError::TemplateSchemaChanged => Self {
                code: "notification_template_schema_changed",
                category: "conflict",
                message: "The notification template schema changed while editing.",
                field: Some("template_schema_version"),
                retryable: true,
                operation_id: None,
            },
            NotificationError::TemplateVariableMissing => Self {
                code: "notification_template_variable_missing",
                category: "validation",
                message: "A required notification template variable is unavailable.",
                field: Some("template"),
                retryable: false,
                operation_id: None,
            },
            NotificationError::TemplateRenderFailed => Self {
                code: "notification_template_render_failed",
                category: "validation",
                message: "The notification template could not be rendered safely.",
                field: Some("template"),
                retryable: false,
                operation_id: None,
            },
            NotificationError::SettingsUnavailable => Self {
                code: "notification_settings_unavailable",
                category: "database",
                message: "Notification settings are currently unavailable.",
                field: None,
                retryable: true,
                operation_id: None,
            },
            NotificationError::ServiceDisabled => Self {
                code: "notification_service_disabled",
                category: "validation",
                message: "Notification delivery is disabled in settings.",
                field: Some("notifications_enabled"),
                retryable: false,
                operation_id: None,
            },
            NotificationError::PermissionUnknown => Self {
                code: "notification_permission_unknown",
                category: "platform",
                message: "The system notification permission could not be determined.",
                field: None,
                retryable: true,
                operation_id: None,
            },
            NotificationError::PermissionNotRequested => Self {
                code: "notification_permission_not_requested",
                category: "permission",
                message: "System notification permission has not been requested.",
                field: None,
                retryable: false,
                operation_id: None,
            },
            NotificationError::PermissionDenied => Self {
                code: "notification_permission_denied",
                category: "permission",
                message: "System notification permission was denied.",
                field: None,
                retryable: false,
                operation_id: None,
            },
            NotificationError::ChannelNotFound => Self {
                code: "notification_channel_not_found",
                category: "validation",
                message: "The notification channel was not found.",
                field: Some("channel_id"),
                retryable: false,
                operation_id: None,
            },
            NotificationError::ChannelUnsupported => Self {
                code: "notification_channel_unsupported",
                category: "platform",
                message: "The notification channel is not supported on this platform.",
                field: None,
                retryable: false,
                operation_id: None,
            },
            NotificationError::DispatchRetryable => Self {
                code: "notification_dispatch_retryable",
                category: "platform",
                message: "The system notification could not be accepted temporarily.",
                field: None,
                retryable: true,
                operation_id: None,
            },
            NotificationError::DispatchFailed => Self {
                code: "notification_dispatch_failed",
                category: "platform",
                message: "The system notification could not be accepted.",
                field: None,
                retryable: false,
                operation_id: None,
            },
        }
    }
}

impl CommandError {
    fn database_unavailable() -> Self {
        Self {
            code: "database_unavailable",
            category: "database",
            message: "The RelayPulse database is unavailable.",
            field: None,
            retryable: true,
            operation_id: None,
        }
    }

    fn desktop_operation_failed() -> Self {
        Self {
            code: "desktop_operation_failed",
            category: "platform",
            message: "RelayPulse could not complete the desktop operation.",
            field: None,
            retryable: true,
            operation_id: None,
        }
    }

    fn diagnostics_unavailable() -> Self {
        Self {
            code: "diagnostics_unavailable",
            category: "platform",
            message: "RelayPulse diagnostics are unavailable.",
            field: None,
            retryable: true,
            operation_id: None,
        }
    }

    fn diagnostics_io_failed() -> Self {
        Self {
            code: "diagnostics_io_failed",
            category: "platform",
            message: "RelayPulse could not read or update its diagnostic logs.",
            field: None,
            retryable: true,
            operation_id: None,
        }
    }

    fn operation_history(error: OperationHistoryQueryError) -> Self {
        match error {
            OperationHistoryQueryError::InvalidQuery => Self {
                code: "invalid_query",
                category: "validation",
                message: "The operation history query is invalid.",
                field: None,
                retryable: false,
                operation_id: None,
            },
            OperationHistoryQueryError::CursorExpired => Self {
                code: "cursor_expired",
                category: "conflict",
                message: "The operation history cursor is no longer valid.",
                field: None,
                retryable: true,
                operation_id: None,
            },
            OperationHistoryQueryError::QueryBusy => Self {
                code: "query_busy",
                category: "conflict",
                message: "The operation history query service is busy.",
                field: None,
                retryable: true,
                operation_id: None,
            },
            OperationHistoryQueryError::NotFound => Self {
                code: "not_found",
                category: "not_found",
                message: "The requested operation history record was not found.",
                field: None,
                retryable: false,
                operation_id: None,
            },
            OperationHistoryQueryError::BackendUnavailable => Self {
                code: "backend_unavailable",
                category: "platform",
                message: "The operation history query service is unavailable.",
                field: None,
                retryable: true,
                operation_id: None,
            },
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum SiteProtocolDto {
    OpenaiCompatible,
    Anthropic,
    OpenaiResponses,
    CustomHttp,
}

impl From<SiteProtocolDto> for SiteProtocol {
    fn from(value: SiteProtocolDto) -> Self {
        match value {
            SiteProtocolDto::OpenaiCompatible => Self::OpenAiCompatible,
            SiteProtocolDto::Anthropic => Self::Anthropic,
            SiteProtocolDto::OpenaiResponses => Self::OpenAiResponses,
            SiteProtocolDto::CustomHttp => Self::CustomHttp,
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum ProbeModeDto {
    Inherit,
    DisabledThinking,
    MinimumReasoning,
}

impl From<ProbeModeDto> for ProbeMode {
    fn from(value: ProbeModeDto) -> Self {
        match value {
            ProbeModeDto::Inherit => Self::Inherit,
            ProbeModeDto::DisabledThinking => Self::DisabledThinking,
            ProbeModeDto::MinimumReasoning => Self::MinimumReasoning,
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
enum ParameterTypeDto {
    String,
    Boolean,
    Number,
    Json,
}

impl From<ParameterTypeDto> for ParameterType {
    fn from(value: ParameterTypeDto) -> Self {
        match value {
            ParameterTypeDto::String => Self::String,
            ParameterTypeDto::Boolean => Self::Boolean,
            ParameterTypeDto::Number => Self::Number,
            ParameterTypeDto::Json => Self::Json,
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
enum ThemeModeDto {
    System,
    Light,
    Dark,
}

impl From<ThemeModeDto> for ThemeMode {
    fn from(value: ThemeModeDto) -> Self {
        match value {
            ThemeModeDto::System => Self::System,
            ThemeModeDto::Light => Self::Light,
            ThemeModeDto::Dark => Self::Dark,
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
enum TrayActionDto {
    Panel,
    Disabled,
}

impl From<TrayActionDto> for TrayAction {
    fn from(value: TrayActionDto) -> Self {
        match value {
            TrayActionDto::Panel => Self::Panel,
            TrayActionDto::Disabled => Self::Disabled,
        }
    }
}

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "lowercase")]
enum CredentialIntentDto {
    Keep,
    Replace { secret: String },
    Clear,
}

impl TryFrom<CredentialIntentDto> for CredentialIntent {
    type Error = CommandError;

    fn try_from(value: CredentialIntentDto) -> CommandResult<Self> {
        match value {
            CredentialIntentDto::Keep => Ok(Self::Keep),
            CredentialIntentDto::Replace { secret } => SecretValue::new(secret)
                .map(Self::Replace)
                .map_err(|_| invalid_field("secret")),
            CredentialIntentDto::Clear => Ok(Self::Clear),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateSiteInputDto {
    name: String,
    protocol: SiteProtocolDto,
    base_url: String,
    enabled: bool,
    credential: CredentialIntentDto,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSiteInputDto {
    name: String,
    protocol: SiteProtocolDto,
    base_url: String,
    credential: CredentialIntentDto,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateModelInputDto {
    name: String,
    enabled: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestHeaderInputDto {
    key: String,
    value: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SiteAdvancedInputDto {
    models_endpoint: String,
    probe_endpoint: String,
    probe_interval_minutes: Option<u16>,
    slow_first_token_seconds: u32,
    slow_total_seconds: u32,
    failure_threshold: u8,
    recovery_threshold: u8,
    headers: Vec<RequestHeaderInputDto>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParameterInputDto {
    key: String,
    value: String,
    r#type: ParameterTypeDto,
}

impl From<ParameterInputDto> for ParameterValue {
    fn from(value: ParameterInputDto) -> Self {
        Self {
            key: value.key,
            value_type: value.r#type.into(),
            value_json: value.value,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeSettingsInputDto {
    mode: ProbeModeDto,
    max_output_tokens: u16,
    prompt: String,
    parameters: Vec<ParameterInputDto>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomProbeInputDto {
    enabled: bool,
    endpoint: String,
    parameters: Vec<ParameterInputDto>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeneralSettingsInputDto {
    launch_at_startup: bool,
    continue_in_background: bool,
    tray_action: TrayActionDto,
    theme: ThemeModeDto,
    history_retention_days: u16,
    diagnostic_capture_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotDto {
    revision: u64,
    sites: Vec<SiteDto>,
    general_settings: GeneralSettingsDto,
    probe_records: Vec<ProbeRecordDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status")]
pub enum ModelSyncResultDto {
    #[serde(rename = "applied")]
    Applied { snapshot: SnapshotDto },
    #[serde(rename = "confirmationRequired")]
    ConfirmationRequired {
        #[serde(rename = "removedModels")]
        removed_models: Vec<ModelSyncRemovalDto>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelSyncRemovalDto {
    id: String,
    name: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct StateChangedDto {
    revision: u64,
    domains: Vec<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct CredentialSummaryDto {
    configured: bool,
    masked: Option<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SiteDto {
    id: String,
    name: String,
    protocol: &'static str,
    base_url: String,
    credential: CredentialSummaryDto,
    enabled: bool,
    priority: u32,
    status: &'static str,
    summary: String,
    models_endpoint: String,
    probe_endpoint: String,
    probe_interval_minutes: Option<u16>,
    slow_first_token_seconds: u32,
    slow_total_seconds: u32,
    failure_threshold: u8,
    recovery_threshold: u8,
    headers: Vec<RequestHeaderDto>,
    default_probe_settings: ProbeSettingsDto,
    models: Vec<ModelDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
struct RequestHeaderDto {
    key: String,
    value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct ModelDto {
    id: String,
    name: String,
    enabled: bool,
    status: &'static str,
    inference_summary: &'static str,
    first_token: String,
    total_time: String,
    latest_probe_id: Option<String>,
    next_probe_at: Option<String>,
    capabilities: Vec<&'static str>,
    probe_settings: ProbeSettingsDto,
    custom_probe: CustomProbeSettingsDto,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProbeSettingsDto {
    mode: &'static str,
    max_output_tokens: u16,
    prompt: String,
    parameters: Vec<ParameterDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct ParameterDto {
    key: String,
    value: String,
    r#type: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct CustomProbeSettingsDto {
    enabled: bool,
    endpoint: String,
    parameters: Vec<ParameterDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct GeneralSettingsDto {
    launch_at_startup: bool,
    continue_in_background: bool,
    tray_action: &'static str,
    theme: &'static str,
    language: &'static str,
    history_retention_days: u16,
    diagnostic_capture_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProbeRecordDto {
    id: String,
    started_at: String,
    status: &'static str,
    site_id: String,
    site_name: String,
    model_id: String,
    model_name: String,
    first_token_ms: Option<u32>,
    first_token_label: String,
    total_time_ms: u32,
    total_time_label: String,
    result: String,
    method: &'static str,
    endpoint: String,
    probe_mode: &'static str,
    http_status: Option<u16>,
    request_id: Option<String>,
    error_detail: Option<String>,
    token_usage: TokenUsageDto,
    exchange: ProbeExchangeDto,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct TokenUsageDto {
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    total_tokens: Option<u64>,
    source: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct ProbeExchangeDto {
    request: ProbeCaptureDto,
    response: ProbeResponseCaptureDto,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProbeCaptureDto {
    headers: Vec<RequestHeaderDto>,
    raw_body: String,
    captured_bytes: u32,
    capture_limit_bytes: u32,
    truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProbeResponseCaptureDto {
    headers: Vec<RequestHeaderDto>,
    raw_body: String,
    captured_bytes: u32,
    capture_limit_bytes: u32,
    truncated: bool,
    mode: &'static str,
    content_type: Option<String>,
    chunks: Vec<ProbeResponseChunkDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProbeResponseChunkDto {
    sequence: usize,
    received_after_ms: u32,
    captured_bytes: usize,
    raw: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticExportDto {
    status: &'static str,
    path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrontendActionInputDto {
    action_id: String,
    action: String,
    view: String,
    destination_view: Option<String>,
    target_type: Option<String>,
    target_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrontendErrorInputDto {
    error_id: String,
    source: String,
    view: String,
    error_name: String,
    fingerprint: String,
    stack_frames: Vec<String>,
    last_action_id: Option<String>,
}

#[tauri::command]
pub fn record_frontend_action(
    state: State<'_, BackendState>,
    input: FrontendActionInputDto,
) -> CommandResult<()> {
    let diagnostics = state
        .diagnostics()
        .ok_or_else(CommandError::diagnostics_unavailable)?;
    validate_diagnostic_id(&input.action_id, "action_id", "ui_")?;
    validate_frontend_action(&input.action)?;
    validate_view(&input.view, "view")?;
    if let Some(destination) = input.destination_view.as_deref() {
        validate_view(destination, "destination_view")?;
    }
    if let Some(target_type) = input.target_type.as_deref() {
        validate_frontend_target_type(target_type)?;
    }
    if let Some(target_id) = input.target_id.as_deref() {
        validate_safe_identifier(target_id, "target_id")?;
    }
    if input.target_type.is_some() != input.target_id.is_some() {
        return Err(invalid_field("target_id"));
    }
    diagnostics.record_frontend_action(
        &input.action_id,
        &input.action,
        &input.view,
        input.destination_view.as_deref(),
        input.target_type.as_deref(),
        input.target_id.as_deref(),
    );
    Ok(())
}

#[tauri::command]
pub fn record_frontend_error(
    state: State<'_, BackendState>,
    input: FrontendErrorInputDto,
) -> CommandResult<()> {
    let diagnostics = state
        .diagnostics()
        .ok_or_else(CommandError::diagnostics_unavailable)?;
    validate_diagnostic_id(&input.error_id, "error_id", "fe_")?;
    validate_frontend_error_source(&input.source)?;
    validate_view(&input.view, "view")?;
    validate_runtime_token(&input.error_name, "error_name", 64)?;
    validate_fingerprint(&input.fingerprint)?;
    if input.stack_frames.len() > 12 {
        return Err(invalid_field("stack_frames"));
    }
    for frame in &input.stack_frames {
        validate_stack_frame(frame)?;
    }
    if let Some(action_id) = input.last_action_id.as_deref() {
        if !(action_id.starts_with("ui_") || action_id.starts_with("op_")) {
            return Err(invalid_field("last_action_id"));
        }
        validate_safe_identifier(action_id, "last_action_id")?;
    }
    diagnostics.record_frontend_error(FrontendErrorRecord {
        error_id: &input.error_id,
        source: &input.source,
        view: &input.view,
        error_name: &input.error_name,
        fingerprint: &input.fingerprint,
        stack_frames: &input.stack_frames,
        last_action_id: input.last_action_id.as_deref(),
    });
    Ok(())
}

#[tauri::command]
pub async fn get_snapshot(state: State<'_, BackendState>) -> CommandResult<SnapshotDto> {
    let result = async {
        let value = service(&state)?.snapshot().await?;
        state.set_continue_in_background(value.general_settings.continue_in_background);
        snapshot(&value)
    }
    .await;
    record_command_failure(&state, "get-snapshot", &result);
    result
}

#[tauri::command]
pub async fn get_site(
    state: State<'_, BackendState>,
    id: String,
) -> CommandResult<Option<SiteDto>> {
    let result = async {
        let service = service(&state)?;
        let site_id = entity_id(id, "id")?;
        let snapshot = service.snapshot().await?;
        let latest = latest_probes(&snapshot);
        Ok(snapshot
            .sites
            .into_iter()
            .enumerate()
            .find(|(_, site)| site.site.id == site_id)
            .map(|(index, site)| site_dto(site, priority(index), &latest)))
    }
    .await;
    record_command_failure(&state, "get-site", &result);
    result
}

#[tauri::command]
pub async fn create_site(
    app: tauri::AppHandle,
    state: State<'_, BackendState>,
    input: CreateSiteInputDto,
    operation_id: Option<String>,
) -> CommandResult<SnapshotDto> {
    let diagnostics = state.diagnostics().cloned();
    observed_user_command(
        diagnostics,
        "create-site",
        operation_id,
        Some(OperationAudit {
            target_type: "site",
            target_id: None,
            parent_id: None,
            change_kind: "create",
            field_scope: vec!["name", "protocol", "baseUrl", "credential", "enabled"],
        }),
        |operation_id| async move {
            let CredentialIntentDto::Replace { secret } = input.credential else {
                return Err(invalid_field("credential"));
            };
            let secret = SecretValue::new(secret).map_err(|_| invalid_field("secret"))?;
            let result = service(&state)?
                .create_site(CreateSite {
                    name: input.name,
                    protocol: input.protocol.into(),
                    base_url: input.base_url,
                    enabled: input.enabled,
                    secret,
                })
                .await?;
            changed(&app, &result, Some(&operation_id), &["sites"])
        },
    )
    .await
}

#[tauri::command]
pub async fn update_site(
    app: tauri::AppHandle,
    state: State<'_, BackendState>,
    id: String,
    input: UpdateSiteInputDto,
    operation_id: Option<String>,
) -> CommandResult<SnapshotDto> {
    let diagnostics = state.diagnostics().cloned();
    let audit = OperationAudit {
        target_type: "site",
        target_id: Some(id.clone()),
        parent_id: None,
        change_kind: "update",
        field_scope: vec!["name", "protocol", "baseUrl", "credential"],
    };
    observed_user_command_with_result_audit(
        diagnostics,
        "update-site",
        operation_id,
        audit.clone(),
        |operation_id| async move {
            let service = service(&state)?;
            let site_id = entity_id(id, "id")?;
            let previous = service.snapshot().await?;
            let result = service
                .update_site(UpdateSite {
                    site_id: site_id.clone(),
                    name: input.name,
                    protocol: input.protocol.into(),
                    base_url: input.base_url,
                    credential: input.credential.try_into()?,
                })
                .await?;
            let fields = changed_site_fields(&previous, &result, &site_id);
            let value = changed(&app, &result, Some(&operation_id), &["sites"])?;
            Ok((value, audit_with_fields(audit, fields)))
        },
    )
    .await
}

#[tauri::command]
pub async fn delete_site(
    app: tauri::AppHandle,
    state: State<'_, BackendState>,
    id: String,
    operation_id: Option<String>,
) -> CommandResult<SnapshotDto> {
    let diagnostics = state.diagnostics().cloned();
    let audit = OperationAudit {
        target_type: "site",
        target_id: Some(id.clone()),
        parent_id: None,
        change_kind: "delete",
        field_scope: vec!["siteConfiguration"],
    };
    observed_user_command(
        diagnostics,
        "delete-site",
        operation_id,
        Some(audit),
        |operation_id| async move {
            let result = service(&state)?.delete_site(entity_id(id, "id")?).await?;
            changed(&app, &result, Some(&operation_id), &["sites", "records"])
        },
    )
    .await
}

#[tauri::command]
pub async fn reorder_sites(
    app: tauri::AppHandle,
    state: State<'_, BackendState>,
    site_ids: Vec<String>,
    operation_id: Option<String>,
) -> CommandResult<SnapshotDto> {
    let diagnostics = state.diagnostics().cloned();
    observed_user_command(
        diagnostics,
        "reorder-sites",
        operation_id,
        Some(OperationAudit {
            target_type: "site",
            target_id: None,
            parent_id: None,
            change_kind: "reorder",
            field_scope: vec!["priority"],
        }),
        |operation_id| async move {
            let site_ids = site_ids
                .into_iter()
                .map(|id| entity_id(id, "site_ids"))
                .collect::<CommandResult<Vec<_>>>()?;
            let result = service(&state)?.reorder_sites(site_ids).await?;
            changed(&app, &result, Some(&operation_id), &["sites"])
        },
    )
    .await
}

#[tauri::command]
pub async fn set_site_enabled(
    app: tauri::AppHandle,
    state: State<'_, BackendState>,
    id: String,
    enabled: bool,
    operation_id: Option<String>,
) -> CommandResult<SnapshotDto> {
    let diagnostics = state.diagnostics().cloned();
    let audit = OperationAudit {
        target_type: "site",
        target_id: Some(id.clone()),
        parent_id: None,
        change_kind: if enabled { "toggle-on" } else { "toggle-off" },
        field_scope: vec!["enabled"],
    };
    observed_user_command(
        diagnostics,
        "set-site-enabled",
        operation_id,
        Some(audit),
        |operation_id| async move {
            let result = service(&state)?
                .set_site_enabled(entity_id(id, "id")?, enabled)
                .await?;
            changed(&app, &result, Some(&operation_id), &["sites"])
        },
    )
    .await
}

#[tauri::command]
pub async fn create_model(
    app: tauri::AppHandle,
    state: State<'_, BackendState>,
    site_id: String,
    input: CreateModelInputDto,
    operation_id: Option<String>,
) -> CommandResult<SnapshotDto> {
    let diagnostics = state.diagnostics().cloned();
    let audit = OperationAudit {
        target_type: "model",
        target_id: None,
        parent_id: Some(site_id.clone()),
        change_kind: "create",
        field_scope: vec!["name", "enabled"],
    };
    observed_user_command(
        diagnostics,
        "create-model",
        operation_id,
        Some(audit),
        |operation_id| async move {
            let result = service(&state)?
                .create_model(CreateModel {
                    site_id: entity_id(site_id, "site_id")?,
                    model_key: input.name,
                    enabled: input.enabled,
                })
                .await?;
            changed(&app, &result, Some(&operation_id), &["sites"])
        },
    )
    .await
}

#[tauri::command]
pub async fn delete_model(
    app: tauri::AppHandle,
    state: State<'_, BackendState>,
    site_id: String,
    model_id: String,
    operation_id: Option<String>,
) -> CommandResult<SnapshotDto> {
    let diagnostics = state.diagnostics().cloned();
    let audit = OperationAudit {
        target_type: "model",
        target_id: Some(model_id.clone()),
        parent_id: Some(site_id.clone()),
        change_kind: "delete",
        field_scope: vec!["modelConfiguration"],
    };
    observed_user_command(
        diagnostics,
        "delete-model",
        operation_id,
        Some(audit),
        |operation_id| async move {
            let result = service(&state)?
                .delete_model(
                    entity_id(site_id, "site_id")?,
                    entity_id(model_id, "model_id")?,
                )
                .await?;
            changed(
                &app,
                &result,
                Some(&operation_id),
                &["models", "sites", "records"],
            )
        },
    )
    .await
}

#[tauri::command]
pub async fn set_model_enabled(
    app: tauri::AppHandle,
    state: State<'_, BackendState>,
    site_id: String,
    model_id: String,
    enabled: bool,
    operation_id: Option<String>,
) -> CommandResult<SnapshotDto> {
    let diagnostics = state.diagnostics().cloned();
    let audit = OperationAudit {
        target_type: "model",
        target_id: Some(model_id.clone()),
        parent_id: Some(site_id.clone()),
        change_kind: if enabled { "toggle-on" } else { "toggle-off" },
        field_scope: vec!["enabled"],
    };
    observed_user_command(
        diagnostics,
        "set-model-enabled",
        operation_id,
        Some(audit),
        |operation_id| async move {
            let result = service(&state)?
                .set_model_enabled(
                    entity_id(site_id, "site_id")?,
                    entity_id(model_id, "model_id")?,
                    enabled,
                )
                .await?;
            changed(&app, &result, Some(&operation_id), &["sites"])
        },
    )
    .await
}

#[tauri::command]
pub async fn update_site_advanced(
    app: tauri::AppHandle,
    state: State<'_, BackendState>,
    id: String,
    input: SiteAdvancedInputDto,
    operation_id: Option<String>,
) -> CommandResult<SnapshotDto> {
    let diagnostics = state.diagnostics().cloned();
    let audit = OperationAudit {
        target_type: "site",
        target_id: Some(id.clone()),
        parent_id: None,
        change_kind: "update",
        field_scope: vec![
            "modelsEndpoint",
            "probeEndpoint",
            "probeIntervalMinutes",
            "slowFirstTokenSeconds",
            "slowTotalSeconds",
            "failureThreshold",
            "recoveryThreshold",
            "headers",
        ],
    };
    observed_user_command_with_result_audit(
        diagnostics,
        "update-site-advanced",
        operation_id,
        audit.clone(),
        |operation_id| async move {
            let service = service(&state)?;
            let site_id = entity_id(id, "id")?;
            let previous = service.snapshot().await?;
            let result = service
                .update_site_advanced(SiteAdvancedUpdate {
                    site_id: site_id.clone(),
                    models_endpoint: input.models_endpoint,
                    probe_endpoint: input.probe_endpoint,
                    interval_minutes: input.probe_interval_minutes,
                    slow_first_token_ms: input
                        .slow_first_token_seconds
                        .checked_mul(1_000)
                        .ok_or_else(|| invalid_field("slow_first_token_seconds"))?,
                    slow_total_ms: input
                        .slow_total_seconds
                        .checked_mul(1_000)
                        .ok_or_else(|| invalid_field("slow_total_seconds"))?,
                    failure_threshold: input.failure_threshold,
                    recovery_threshold: input.recovery_threshold,
                    headers: input
                        .headers
                        .into_iter()
                        .map(|header| (header.key, header.value))
                        .collect(),
                })
                .await?;
            let fields = changed_site_advanced_fields(&previous, &result, &site_id);
            let value = changed(&app, &result, Some(&operation_id), &["sites"])?;
            Ok((value, audit_with_fields(audit, fields)))
        },
    )
    .await
}

#[tauri::command]
pub async fn update_site_probe_settings(
    app: tauri::AppHandle,
    state: State<'_, BackendState>,
    site_id: String,
    settings: ProbeSettingsInputDto,
    operation_id: Option<String>,
) -> CommandResult<SnapshotDto> {
    update_probe_settings(app, state, site_id, None, settings, operation_id).await
}

#[tauri::command]
pub async fn update_model_probe_settings(
    app: tauri::AppHandle,
    state: State<'_, BackendState>,
    site_id: String,
    model_id: String,
    settings: ProbeSettingsInputDto,
    operation_id: Option<String>,
) -> CommandResult<SnapshotDto> {
    update_probe_settings(app, state, site_id, Some(model_id), settings, operation_id).await
}

async fn update_probe_settings(
    app: tauri::AppHandle,
    state: State<'_, BackendState>,
    site_id: String,
    model_id: Option<String>,
    settings: ProbeSettingsInputDto,
    operation_id: Option<String>,
) -> CommandResult<SnapshotDto> {
    let diagnostics = state.diagnostics().cloned();
    let operation = if model_id.is_some() {
        "update-model-probe-settings"
    } else {
        "update-site-probe-settings"
    };
    let field_scope = if model_id.is_some() {
        vec!["probeSettings"]
    } else {
        vec!["defaultProbeSettings"]
    };
    let audit = OperationAudit {
        target_type: if model_id.is_some() { "model" } else { "site" },
        target_id: model_id.clone().or_else(|| Some(site_id.clone())),
        parent_id: model_id.as_ref().map(|_| site_id.clone()),
        change_kind: "update",
        field_scope,
    };
    observed_user_command_with_result_audit(
        diagnostics,
        operation,
        operation_id,
        audit.clone(),
        |operation_id| async move {
            let service = service(&state)?;
            let parsed_site_id = entity_id(site_id, "site_id")?;
            let parsed_model_id = model_id.map(|id| entity_id(id, "model_id")).transpose()?;
            let previous = service.snapshot().await?;
            let result = service
                .update_probe_settings(ProbeSettingsUpdate {
                    site_id: parsed_site_id.clone(),
                    model_id: parsed_model_id.clone(),
                    mode: settings.mode.into(),
                    max_output_tokens: settings.max_output_tokens,
                    prompt: settings.prompt,
                    parameters: settings.parameters.into_iter().map(Into::into).collect(),
                })
                .await?;
            let settings_changed = probe_settings_changed(
                &previous,
                &result,
                &parsed_site_id,
                parsed_model_id.as_ref(),
            );
            let fields = if settings_changed {
                audit.field_scope.clone()
            } else {
                Vec::new()
            };
            let value = changed(&app, &result, Some(&operation_id), &["sites"])?;
            Ok((value, audit_with_fields(audit, fields)))
        },
    )
    .await
}

#[tauri::command]
pub async fn update_model_custom_probe_settings(
    app: tauri::AppHandle,
    state: State<'_, BackendState>,
    site_id: String,
    model_id: String,
    settings: CustomProbeInputDto,
    operation_id: Option<String>,
) -> CommandResult<SnapshotDto> {
    let diagnostics = state.diagnostics().cloned();
    let audit = OperationAudit {
        target_type: "model",
        target_id: Some(model_id.clone()),
        parent_id: Some(site_id.clone()),
        change_kind: "update",
        field_scope: vec!["customProbe"],
    };
    observed_user_command_with_result_audit(
        diagnostics,
        "update-model-custom-probe-settings",
        operation_id,
        audit.clone(),
        |operation_id| async move {
            let service = service(&state)?;
            let parsed_site_id = entity_id(site_id, "site_id")?;
            let parsed_model_id = entity_id(model_id, "model_id")?;
            let previous = service.snapshot().await?;
            let result = service
                .update_custom_probe(CustomProbeUpdate {
                    site_id: parsed_site_id.clone(),
                    model_id: parsed_model_id.clone(),
                    enabled: settings.enabled,
                    endpoint: settings.endpoint,
                    parameters: settings.parameters.into_iter().map(Into::into).collect(),
                })
                .await?;
            let fields =
                if custom_probe_changed(&previous, &result, &parsed_site_id, &parsed_model_id) {
                    audit.field_scope.clone()
                } else {
                    Vec::new()
                };
            let value = changed(&app, &result, Some(&operation_id), &["sites"])?;
            Ok((value, audit_with_fields(audit, fields)))
        },
    )
    .await
}

#[tauri::command]
pub async fn update_general_settings(
    app: tauri::AppHandle,
    state: State<'_, BackendState>,
    settings: GeneralSettingsInputDto,
    operation_id: Option<String>,
) -> CommandResult<SnapshotDto> {
    let diagnostics = state.diagnostics().cloned();
    let audit = OperationAudit {
        target_type: "settings",
        target_id: None,
        parent_id: None,
        change_kind: "update",
        field_scope: vec![
            "launchAtStartup",
            "continueInBackground",
            "trayAction",
            "theme",
            "historyRetentionDays",
            "diagnosticCaptureEnabled",
        ],
    };
    observed_user_command_with_result_audit(
        diagnostics,
        "update-general-settings",
        operation_id,
        audit.clone(),
        |operation_id| async move {
            let service = service(&state)?;
            let previous = service.snapshot().await?;
            let tray_action: TrayAction = settings.tray_action.into();
            desktop::set_autostart(&app, settings.launch_at_startup)
                .map_err(|_| CommandError::desktop_operation_failed())?;
            if desktop::set_tray_enabled(&app, tray_action == TrayAction::Panel).is_err() {
                if desktop::set_autostart(&app, previous.general_settings.launch_at_startup)
                    .is_err()
                {
                    desktop::record_desktop_error(
                        &app,
                        "general-settings-rollback",
                        "autostart_rollback_failed",
                    );
                }
                return Err(CommandError::desktop_operation_failed());
            }
            let result = match service
                .update_general_settings(
                    settings.launch_at_startup,
                    settings.continue_in_background,
                    tray_action,
                    settings.theme.into(),
                    settings.history_retention_days,
                    settings.diagnostic_capture_enabled,
                )
                .await
            {
                Ok(result) => result,
                Err(error) => {
                    if desktop::set_autostart(&app, previous.general_settings.launch_at_startup)
                        .is_err()
                    {
                        desktop::record_desktop_error(
                            &app,
                            "general-settings-rollback",
                            "autostart_rollback_failed",
                        );
                    }
                    if desktop::set_tray_enabled(
                        &app,
                        previous.general_settings.tray_action == TrayAction::Panel,
                    )
                    .is_err()
                    {
                        desktop::record_desktop_error(
                            &app,
                            "general-settings-rollback",
                            "tray_rollback_failed",
                        );
                    }
                    return Err(error.into());
                }
            };
            state.set_continue_in_background(result.general_settings.continue_in_background);
            let fields = general_settings_changed_fields(&previous, &result);
            let value = changed(&app, &result, Some(&operation_id), &["settings"])?;
            Ok((value, audit_with_fields(audit, fields)))
        },
    )
    .await
}

#[tauri::command]
pub async fn refresh_probe_records(state: State<'_, BackendState>) -> CommandResult<SnapshotDto> {
    let result = async {
        let value = service(&state)?.snapshot().await?;
        state.set_continue_in_background(value.general_settings.continue_in_background);
        snapshot(&value)
    }
    .await;
    record_command_failure(&state, "refresh-probe-records", &result);
    result
}

#[tauri::command]
pub async fn clear_probe_records(
    app: tauri::AppHandle,
    state: State<'_, BackendState>,
    operation_id: Option<String>,
) -> CommandResult<SnapshotDto> {
    let diagnostics = state.diagnostics().cloned();
    observed_user_command(
        diagnostics,
        "clear-probe-records",
        operation_id,
        Some(OperationAudit {
            target_type: "probe-records",
            target_id: None,
            parent_id: None,
            change_kind: "clear",
            field_scope: vec!["records"],
        }),
        |operation_id| async move {
            let (cleanup, result) = service(&state)?.clear_probe_records().await?;
            if let Some(diagnostics) = state.diagnostics() {
                diagnostics.record_records_cleared(
                    Some(&operation_id),
                    cleanup.deleted,
                    cleanup.revision,
                );
            }
            changed(&app, &result, Some(&operation_id), &["records", "status"])
        },
    )
    .await
}

#[tauri::command]
pub async fn open_log_directory(
    state: State<'_, BackendState>,
    operation_id: Option<String>,
) -> CommandResult<()> {
    let diagnostics = state
        .diagnostics()
        .cloned()
        .ok_or_else(CommandError::diagnostics_unavailable)?;
    let operation_id = resolve_operation_id(operation_id)?;
    let event_operation_id = operation_id.clone();
    let action_diagnostics = diagnostics.clone();
    observed_command_with_id(
        Some(diagnostics),
        "open-log-directory",
        operation_id,
        None,
        async move {
            desktop::open_directory(action_diagnostics.log_directory())
                .map_err(|_| CommandError::desktop_operation_failed())?;
            action_diagnostics.record_log_directory_opened(Some(&event_operation_id));
            Ok(())
        },
    )
    .await
}

#[tauri::command]
pub fn get_log_statistics(state: State<'_, BackendState>) -> CommandResult<LogStatistics> {
    let result = (|| {
        state
            .diagnostics()
            .ok_or_else(CommandError::diagnostics_unavailable)?
            .statistics()
            .map_err(|_| CommandError::diagnostics_io_failed())
    })();
    record_command_failure(&state, "get-log-statistics", &result);
    result
}

#[tauri::command]
pub async fn list_operation_records(
    state: State<'_, BackendState>,
    query: OperationHistoryQueryDto,
) -> CommandResult<OperationHistoryPageDto> {
    let result = async {
        let diagnostics = state.diagnostics().cloned().ok_or_else(|| {
            CommandError::operation_history(OperationHistoryQueryError::BackendUnavailable)
        })?;
        tokio::task::spawn_blocking(move || diagnostics.list_operation_records(query))
            .await
            .map_err(|_| {
                CommandError::operation_history(OperationHistoryQueryError::BackendUnavailable)
            })?
            .map_err(CommandError::operation_history)
    }
    .await;
    record_command_failure(&state, "list-operation-records", &result);
    result
}

#[tauri::command]
pub async fn get_operation_record_details(
    state: State<'_, BackendState>,
    record_ref: String,
) -> CommandResult<OperationRecordDetailsDto> {
    let result = async {
        let diagnostics = state.diagnostics().cloned().ok_or_else(|| {
            CommandError::operation_history(OperationHistoryQueryError::BackendUnavailable)
        })?;
        tokio::task::spawn_blocking(move || diagnostics.get_operation_record_details(&record_ref))
            .await
            .map_err(|_| {
                CommandError::operation_history(OperationHistoryQueryError::BackendUnavailable)
            })?
            .map_err(CommandError::operation_history)
    }
    .await;
    record_command_failure(&state, "get-operation-record-details", &result);
    result
}

#[tauri::command]
pub async fn set_diagnostic_log_limit(
    state: State<'_, BackendState>,
    max_total_megabytes: u16,
    operation_id: Option<String>,
) -> CommandResult<LogStatistics> {
    let diagnostics = state
        .diagnostics()
        .cloned()
        .ok_or_else(CommandError::diagnostics_unavailable)?;
    let action_diagnostics = diagnostics.clone();
    observed_user_command(
        Some(diagnostics),
        "set-diagnostic-log-limit",
        operation_id,
        Some(OperationAudit {
            target_type: "settings",
            target_id: None,
            parent_id: None,
            change_kind: "update",
            field_scope: vec!["diagnosticLogMaxMegabytes"],
        }),
        |operation_id| async move {
            if !(MIN_LOG_TOTAL_MEGABYTES..=MAX_LOG_TOTAL_MEGABYTES).contains(&max_total_megabytes) {
                return Err(invalid_field("max_total_megabytes"));
            }
            let statistics = action_diagnostics
                .configure_max_total_megabytes(max_total_megabytes)
                .map_err(|_| CommandError::diagnostics_io_failed())?;
            action_diagnostics
                .record_log_limit_configured(&operation_id, statistics.configured_max_total_bytes);
            action_diagnostics
                .statistics()
                .map_err(|_| CommandError::diagnostics_io_failed())
        },
    )
    .await
}

#[tauri::command]
pub async fn clear_diagnostic_logs(
    state: State<'_, BackendState>,
    operation_id: Option<String>,
) -> CommandResult<LogCleanupResult> {
    let diagnostics = state
        .diagnostics()
        .cloned()
        .ok_or_else(CommandError::diagnostics_unavailable)?;
    let operation_id = resolve_operation_id(operation_id)?;
    let event_operation_id = operation_id.clone();
    let action_diagnostics = diagnostics.clone();
    observed_command_with_id(
        Some(diagnostics),
        "clear-diagnostic-logs",
        operation_id,
        None,
        async move {
            let mut result = action_diagnostics
                .clear_logs()
                .map_err(|_| CommandError::diagnostics_io_failed())?;
            action_diagnostics.record_logs_cleared(
                Some(&event_operation_id),
                result.deleted_files,
                result.reclaimed_bytes,
            );
            result.statistics = action_diagnostics
                .statistics()
                .map_err(|_| CommandError::diagnostics_io_failed())?;
            Ok(result)
        },
    )
    .await
}

#[tauri::command]
pub async fn export_diagnostics(
    app: tauri::AppHandle,
    state: State<'_, BackendState>,
    operation_id: Option<String>,
) -> CommandResult<DiagnosticExportDto> {
    let diagnostics = state
        .diagnostics()
        .cloned()
        .ok_or_else(CommandError::diagnostics_unavailable)?;
    let operation_id = resolve_operation_id(operation_id)?;
    let event_operation_id = operation_id.clone();
    let action_diagnostics = diagnostics.clone();
    observed_command_with_id(
        Some(diagnostics),
        "export-diagnostics",
        operation_id,
        None,
        async move {
            let file_name = format!(
                "RelayPulse-diagnostics-{}.json.gz",
                OffsetDateTime::now_utc().unix_timestamp()
            );
            let selected = app
                .dialog()
                .file()
                .set_title("导出 RelayPulse 诊断包")
                .set_file_name(file_name)
                .add_filter("压缩诊断包", &["gz"])
                .blocking_save_file();
            let Some(selected) = selected else {
                action_diagnostics.record_diagnostics_export_cancelled(&event_operation_id);
                return Ok(DiagnosticExportDto {
                    status: "cancelled",
                    path: None,
                });
            };
            let path = selected
                .into_path()
                .map_err(|_| CommandError::desktop_operation_failed())?;
            let value = match state.application_service() {
                Some(service) => match service.snapshot().await {
                    Ok(value) => Some(value),
                    Err(_) => {
                        action_diagnostics.record_background_error(
                            "diagnostics-snapshot",
                            "snapshot_unavailable",
                        );
                        None
                    }
                },
                None => None,
            };
            action_diagnostics
                .export(&path, value.as_ref(), state.startup_failure())
                .map_err(|_| CommandError::desktop_operation_failed())?;
            let file_count = action_diagnostics
                .statistics()
                .map_err(|_| CommandError::diagnostics_io_failed())?
                .file_count;
            let bundle_size_bytes = fs::metadata(&path)
                .map_err(|_| CommandError::desktop_operation_failed())?
                .len();
            action_diagnostics.record_diagnostics_exported(
                Some(&event_operation_id),
                file_count,
                bundle_size_bytes,
            );
            Ok(DiagnosticExportDto {
                status: "exported",
                path: Some(path.to_string_lossy().into_owned()),
            })
        },
    )
    .await
}

#[tauri::command]
pub async fn open_main_window(
    app: tauri::AppHandle,
    state: State<'_, BackendState>,
    view: Option<String>,
    operation_id: Option<String>,
) -> CommandResult<()> {
    let view = match view.as_deref().unwrap_or("sites") {
        "sites" => "sites",
        "general" => "general",
        "records" => "records",
        _ => return Err(invalid_field("view")),
    };
    let diagnostics = state.diagnostics().cloned();
    let operation_id = resolve_operation_id(operation_id)?;
    let event_operation_id = operation_id.clone();
    let action_diagnostics = diagnostics.clone();
    observed_command_with_id(
        diagnostics,
        "open-main-window",
        operation_id,
        None,
        async move {
            desktop::show_main_window_at(&app, view)
                .map_err(|_| CommandError::desktop_operation_failed())?;
            if let Some(diagnostics) = action_diagnostics {
                diagnostics.record_window_opened(Some(&event_operation_id), "main");
            }
            Ok(())
        },
    )
    .await
}

#[tauri::command]
pub async fn refresh_models(
    app: tauri::AppHandle,
    state: State<'_, BackendState>,
    site_id: String,
    confirmed_removed_model_ids: Option<Vec<String>>,
    operation_id: Option<String>,
) -> CommandResult<ModelSyncResultDto> {
    let diagnostics = state.diagnostics().cloned();
    let audit = OperationAudit {
        target_type: "site",
        target_id: Some(site_id.clone()),
        parent_id: None,
        change_kind: "sync",
        field_scope: vec!["catalog"],
    };
    observed_user_command(
        diagnostics,
        "refresh-models",
        operation_id,
        Some(audit),
        |operation_id| async move {
            let confirmed_removed_model_ids = confirmed_removed_model_ids
                .unwrap_or_default()
                .into_iter()
                .map(|id| entity_id(id, "confirmed_removed_model_ids"))
                .collect::<CommandResult<Vec<_>>>()?;
            let result = service(&state)?
                .refresh_models(entity_id(site_id, "site_id")?, confirmed_removed_model_ids)
                .await?;
            match result {
                ModelSyncOutcome::Applied(snapshot) => Ok(ModelSyncResultDto::Applied {
                    snapshot: changed(
                        &app,
                        &snapshot,
                        Some(&operation_id),
                        &["models", "sites", "records"],
                    )?,
                }),
                ModelSyncOutcome::ConfirmationRequired { removed_models } => {
                    if let Some(diagnostics) = state.diagnostics() {
                        diagnostics.record_model_sync_confirmation_required(
                            &operation_id,
                            u64::try_from(removed_models.len()).unwrap_or(u64::MAX),
                        );
                    }
                    Ok(ModelSyncResultDto::ConfirmationRequired {
                        removed_models: removed_models
                            .into_iter()
                            .map(|model| ModelSyncRemovalDto {
                                id: model.id.to_string(),
                                name: model.name,
                            })
                            .collect(),
                    })
                }
            }
        },
    )
    .await
}

#[tauri::command]
pub async fn run_immediate_probe(
    app: tauri::AppHandle,
    state: State<'_, BackendState>,
    operation_id: Option<String>,
) -> CommandResult<SnapshotDto> {
    let diagnostics = state.diagnostics().cloned();
    observed_user_command(
        diagnostics,
        "run-immediate-probe",
        operation_id,
        None,
        |operation_id| async move {
            let result = service(&state)?
                .probe_all_enabled_with_operation(Some(&operation_id))
                .await?;
            changed(
                &app,
                &result,
                Some(&operation_id),
                &["records", "status", "sites"],
            )
        },
    )
    .await
}

#[tauri::command]
pub async fn run_site_probe(
    app: tauri::AppHandle,
    state: State<'_, BackendState>,
    site_id: String,
    operation_id: Option<String>,
) -> CommandResult<SnapshotDto> {
    let diagnostics = state.diagnostics().cloned();
    observed_user_command(
        diagnostics,
        "run-site-probe",
        operation_id,
        None,
        |operation_id| async move {
            let result = service(&state)?
                .probe_site_with_operation(entity_id(site_id, "site_id")?, Some(&operation_id))
                .await?;
            changed(
                &app,
                &result,
                Some(&operation_id),
                &["records", "status", "sites"],
            )
        },
    )
    .await
}

#[tauri::command]
pub async fn run_model_probe(
    app: tauri::AppHandle,
    state: State<'_, BackendState>,
    site_id: String,
    model_id: String,
    operation_id: Option<String>,
) -> CommandResult<SnapshotDto> {
    let diagnostics = state.diagnostics().cloned();
    observed_user_command(
        diagnostics,
        "run-model-probe",
        operation_id,
        None,
        |operation_id| async move {
            let result = service(&state)?
                .probe_model_with_operation(
                    entity_id(site_id, "site_id")?,
                    entity_id(model_id, "model_id")?,
                    Some(&operation_id),
                )
                .await?;
            changed(
                &app,
                &result,
                Some(&operation_id),
                &["records", "status", "sites"],
            )
        },
    )
    .await
}

#[tauri::command]
pub async fn get_notification_settings(
    app: tauri::AppHandle,
    state: State<'_, BackendState>,
) -> CommandResult<NotificationSettingsSnapshot> {
    let result = async {
        notification_service(&state)?
            .settings(system_channel_snapshot(&app))
            .await
            .map_err(Into::into)
    }
    .await;
    record_command_failure(&state, "get-notification-settings", &result);
    result
}

#[tauri::command]
pub async fn update_notification_settings(
    app: tauri::AppHandle,
    state: State<'_, BackendState>,
    settings: NotificationSettingsUpdate,
    operation_id: Option<String>,
) -> CommandResult<NotificationSettingsSnapshot> {
    let diagnostics = state.diagnostics().cloned();
    let audit = OperationAudit {
        target_type: "notification-settings",
        target_id: None,
        parent_id: None,
        change_kind: "update",
        field_scope: vec!["notificationsEnabled", "rules", "templateOverrides"],
    };
    observed_user_command_with_result_audit(
        diagnostics,
        "update-notification-settings",
        operation_id,
        audit.clone(),
        |operation_id| async move {
            let service = notification_service(&state)?;
            let previous = service.settings(system_channel_snapshot(&app)).await?;
            let (snapshot, app_revision) = service
                .update_settings(settings, system_channel_snapshot(&app))
                .await?;
            notification_changed(&app, app_revision, Some(&operation_id));
            let fields = notification_settings_changed_fields(&previous, &snapshot);
            Ok((snapshot, audit_with_fields(audit, fields)))
        },
    )
    .await
}

#[tauri::command]
pub async fn send_test_notification(
    app: tauri::AppHandle,
    state: State<'_, BackendState>,
    channel_id: String,
    operation_id: Option<String>,
) -> CommandResult<()> {
    let diagnostics = state.diagnostics().cloned();
    observed_user_command(
        diagnostics,
        "send-test-notification",
        operation_id,
        Some(OperationAudit {
            target_type: "notification-channel",
            target_id: Some(channel_id.clone()),
            parent_id: None,
            change_kind: "test",
            field_scope: vec![],
        }),
        |_operation_id| async move {
            let service = notification_service(&state)?;
            let channel = system_channel_snapshot(&app);
            let settings = service.settings(channel).await?;
            if !settings.notifications_enabled {
                return Err(NotificationError::ServiceDisabled.into());
            }
            send_test_system_notification(&app, &channel_id).map_err(Into::into)
        },
    )
    .await
}

#[tauri::command]
pub async fn send_notification_template_test(
    app: tauri::AppHandle,
    state: State<'_, BackendState>,
    draft: NotificationTemplateDraft,
    operation_id: Option<String>,
) -> CommandResult<NotificationTemplateTestResult> {
    let diagnostics = state.diagnostics().cloned();
    observed_user_command(
        diagnostics,
        "send-notification-template-test",
        operation_id,
        Some(OperationAudit {
            target_type: "notification-template",
            target_id: None,
            parent_id: None,
            change_kind: "test",
            field_scope: vec![],
        }),
        |operation_id| async move {
            let service = notification_service(&state)?;
            let channel = system_channel_snapshot(&app);
            let settings = service.settings(channel).await?;
            if !settings.notifications_enabled {
                return Err(NotificationError::ServiceDisabled.into());
            }
            let channel_id = draft.channel_id.clone();
            let rendered = service.render_template_test(draft)?;
            send_system_notification(&app, &channel_id, &rendered.title, &rendered.body)?;
            Ok(NotificationTemplateTestResult {
                accepted: true,
                rendered_marker: rendered.marker,
                rendered_title: rendered.title,
                rendered_body: rendered.body,
                operation_id,
            })
        },
    )
    .await
}

#[tauri::command]
pub async fn request_notification_permission(
    app: tauri::AppHandle,
    state: State<'_, BackendState>,
    channel_id: String,
    operation_id: Option<String>,
) -> CommandResult<NotificationChannelSnapshot> {
    let diagnostics = state.diagnostics().cloned();
    observed_user_command(
        diagnostics,
        "request-notification-permission",
        operation_id,
        Some(OperationAudit {
            target_type: "notification-channel",
            target_id: Some(channel_id.clone()),
            parent_id: None,
            change_kind: "request",
            field_scope: vec![],
        }),
        |operation_id| async move {
            let result = request_system_notification_permission(&app, &channel_id)?;
            if let Some(diagnostics) = state.diagnostics() {
                diagnostics.record_notification_permission_result(
                    &operation_id,
                    &result.id,
                    result.permission.as_str(),
                );
            }
            Ok(result)
        },
    )
    .await
}

fn service(state: &State<'_, BackendState>) -> CommandResult<ApplicationService> {
    state
        .application_service()
        .cloned()
        .ok_or_else(CommandError::database_unavailable)
}

fn notification_service(state: &State<'_, BackendState>) -> CommandResult<NotificationService> {
    state
        .notification_service()
        .cloned()
        .ok_or_else(CommandError::database_unavailable)
}

fn record_command_failure<T>(
    state: &BackendState,
    operation: &'static str,
    result: &CommandResult<T>,
) {
    if let (Err(error), Some(diagnostics)) = (result, state.diagnostics()) {
        diagnostics.record_background_error(operation, error.code);
    }
}

fn notification_changed(app: &tauri::AppHandle, revision: u64, operation_id: Option<&str>) {
    if let Some(state) = app.try_state::<BackendState>() {
        if let Some(diagnostics) = state.diagnostics() {
            diagnostics.record_state_change(operation_id, revision, &["notifications"]);
        }
    }
    if app
        .emit(
            "relaypulse://state-changed",
            StateChangedDto {
                revision,
                domains: vec!["notifications"],
            },
        )
        .is_err()
    {
        desktop::record_desktop_error(
            app,
            "notification-state-change-event",
            "state_change_emit_failed",
        );
    }
}

#[cfg(test)]
async fn observed_command<T, F>(
    diagnostics: Option<Diagnostics>,
    operation: &'static str,
    future: F,
) -> CommandResult<T>
where
    F: Future<Output = CommandResult<T>>,
{
    let operation_id = format!("op_{}", Uuid::new_v4().simple());
    observed_command_with_id(diagnostics, operation, operation_id, None, future).await
}

async fn observed_user_command<T, W, F>(
    diagnostics: Option<Diagnostics>,
    operation: &'static str,
    requested_operation_id: Option<String>,
    audit: Option<OperationAudit>,
    work: W,
) -> CommandResult<T>
where
    W: FnOnce(String) -> F,
    F: Future<Output = CommandResult<T>>,
{
    let operation_id = resolve_operation_id(requested_operation_id)?;
    let future = work(operation_id.clone());
    observed_command_with_id(diagnostics, operation, operation_id, audit, future).await
}

async fn observed_user_command_with_result_audit<T, W, F>(
    diagnostics: Option<Diagnostics>,
    operation: &'static str,
    requested_operation_id: Option<String>,
    attempted_audit: OperationAudit,
    work: W,
) -> CommandResult<T>
where
    W: FnOnce(String) -> F,
    F: Future<Output = CommandResult<(T, OperationAudit)>>,
{
    let operation_id = resolve_operation_id(requested_operation_id)?;
    let started = Instant::now();
    if let Some(diagnostics) = &diagnostics {
        diagnostics.record_operation_started(&operation_id, operation, Some(&attempted_audit));
    }
    match work(operation_id.clone()).await {
        Ok((value, actual_audit)) => {
            if let Some(diagnostics) = &diagnostics {
                diagnostics.record_operation_finished(
                    &operation_id,
                    operation,
                    elapsed_millis(started),
                    None,
                    Some(&actual_audit),
                );
            }
            Ok(value)
        }
        Err(mut error) => {
            error.operation_id = Some(operation_id.clone());
            if let Some(diagnostics) = &diagnostics {
                diagnostics.record_operation_finished(
                    &operation_id,
                    operation,
                    elapsed_millis(started),
                    Some(error.code),
                    Some(&attempted_audit),
                );
            }
            Err(error)
        }
    }
}

fn audit_with_fields(mut audit: OperationAudit, field_scope: Vec<&'static str>) -> OperationAudit {
    audit.field_scope = field_scope;
    audit
}

async fn observed_command_with_id<T, F>(
    diagnostics: Option<Diagnostics>,
    operation: &'static str,
    operation_id: String,
    audit: Option<OperationAudit>,
    future: F,
) -> CommandResult<T>
where
    F: Future<Output = CommandResult<T>>,
{
    let started = Instant::now();
    if let Some(diagnostics) = &diagnostics {
        diagnostics.record_operation_started(&operation_id, operation, audit.as_ref());
    }
    match future.await {
        Ok(value) => {
            if let Some(diagnostics) = &diagnostics {
                diagnostics.record_operation_finished(
                    &operation_id,
                    operation,
                    elapsed_millis(started),
                    None,
                    audit.as_ref(),
                );
            }
            Ok(value)
        }
        Err(mut error) => {
            error.operation_id = Some(operation_id.clone());
            if let Some(diagnostics) = &diagnostics {
                diagnostics.record_operation_finished(
                    &operation_id,
                    operation,
                    elapsed_millis(started),
                    Some(error.code),
                    audit.as_ref(),
                );
            }
            Err(error)
        }
    }
}

fn elapsed_millis(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

fn entity_id(value: String, field: &'static str) -> CommandResult<EntityId> {
    EntityId::parse(value).map_err(|_| invalid_field(field))
}

fn validate_diagnostic_id(value: &str, field: &'static str, prefix: &str) -> CommandResult<()> {
    if !value.starts_with(prefix) {
        return Err(invalid_field(field));
    }
    validate_safe_identifier(value, field)
}

fn resolve_operation_id(requested_operation_id: Option<String>) -> CommandResult<String> {
    match requested_operation_id {
        Some(value) => {
            validate_diagnostic_id(&value, "operation_id", "op_")?;
            Ok(value)
        }
        None => Ok(format!("op_{}", Uuid::new_v4().simple())),
    }
}

fn validate_safe_identifier(value: &str, field: &'static str) -> CommandResult<()> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'))
    {
        return Err(invalid_field(field));
    }
    Ok(())
}

fn validate_runtime_token(value: &str, field: &'static str, max: usize) -> CommandResult<()> {
    if value.is_empty()
        || value.len() > max
        || !value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.')
        })
    {
        return Err(invalid_field(field));
    }
    Ok(())
}

fn validate_view(value: &str, field: &'static str) -> CommandResult<()> {
    if !crate::diagnostics::is_safe_frontend_view(value) {
        return Err(invalid_field(field));
    }
    Ok(())
}

fn validate_frontend_action(value: &str) -> CommandResult<()> {
    if !crate::diagnostics::is_safe_frontend_action(value) {
        return Err(invalid_field("action"));
    }
    Ok(())
}

fn validate_frontend_target_type(value: &str) -> CommandResult<()> {
    if !crate::diagnostics::is_safe_frontend_target_type(value) {
        return Err(invalid_field("target_type"));
    }
    Ok(())
}

fn validate_frontend_error_source(value: &str) -> CommandResult<()> {
    if !crate::diagnostics::is_safe_frontend_error_source(value) {
        return Err(invalid_field("source"));
    }
    Ok(())
}

fn validate_fingerprint(value: &str) -> CommandResult<()> {
    if value.len() != 16 || !value.chars().all(|character| character.is_ascii_hexdigit()) {
        return Err(invalid_field("fingerprint"));
    }
    Ok(())
}

fn validate_stack_frame(value: &str) -> CommandResult<()> {
    let lower = value.to_ascii_lowercase();
    if value.is_empty()
        || value.len() > 240
        || value
            .chars()
            .any(|character| matches!(character, '\n' | '\r' | '?' | '#'))
        || value.contains("://")
        || lower.contains("bearer ")
        || lower.contains("api_key")
        || lower.contains("apikey")
        || lower.contains("sk-")
    {
        return Err(invalid_field("stack_frames"));
    }
    Ok(())
}

fn invalid_field(field: &'static str) -> CommandError {
    CommandError {
        code: "invalid_input",
        category: "validation",
        message: "The submitted value is invalid.",
        field: Some(field),
        retryable: false,
        operation_id: None,
    }
}

fn changed_site_fields(
    previous: &ApplicationSnapshot,
    current: &ApplicationSnapshot,
    site_id: &EntityId,
) -> Vec<&'static str> {
    let (Some(previous), Some(current)) =
        (find_site(previous, site_id), find_site(current, site_id))
    else {
        return Vec::new();
    };
    let mut fields = Vec::new();
    if previous.site.name != current.site.name {
        fields.push("name");
    }
    if previous.site.protocol != current.site.protocol {
        fields.push("protocol");
    }
    if previous.site.base_url != current.site.base_url {
        fields.push("baseUrl");
    }
    if previous.site.credential_ref != current.site.credential_ref {
        fields.push("credential");
    }
    fields
}

fn changed_site_advanced_fields(
    previous: &ApplicationSnapshot,
    current: &ApplicationSnapshot,
    site_id: &EntityId,
) -> Vec<&'static str> {
    let (Some(previous), Some(current)) =
        (find_site(previous, site_id), find_site(current, site_id))
    else {
        return Vec::new();
    };
    let mut fields = Vec::new();
    if previous.site.models_endpoint != current.site.models_endpoint {
        fields.push("modelsEndpoint");
    }
    if previous.site.probe_endpoint != current.site.probe_endpoint {
        fields.push("probeEndpoint");
    }
    if previous.site.timing.interval_minutes != current.site.timing.interval_minutes {
        fields.push("probeIntervalMinutes");
    }
    if previous.site.timing.slow_first_token_ms != current.site.timing.slow_first_token_ms {
        fields.push("slowFirstTokenSeconds");
    }
    if previous.site.timing.slow_total_ms != current.site.timing.slow_total_ms {
        fields.push("slowTotalSeconds");
    }
    if previous.site.timing.failure_threshold != current.site.timing.failure_threshold {
        fields.push("failureThreshold");
    }
    if previous.site.timing.recovery_threshold != current.site.timing.recovery_threshold {
        fields.push("recoveryThreshold");
    }
    if !headers_equal(&previous.headers, &current.headers) {
        fields.push("headers");
    }
    fields
}

fn probe_settings_changed(
    previous: &ApplicationSnapshot,
    current: &ApplicationSnapshot,
    site_id: &EntityId,
    model_id: Option<&EntityId>,
) -> bool {
    let (Some(previous), Some(current)) =
        (find_site(previous, site_id), find_site(current, site_id))
    else {
        return false;
    };
    match model_id {
        Some(model_id) => {
            let previous = find_model(previous, model_id);
            let current = find_model(current, model_id);
            match (previous, current) {
                (Some(previous), Some(current)) => {
                    !probe_policy_equal(previous.policy.as_ref(), current.policy.as_ref())
                        || !probe_parameters_equal(&previous.parameters, &current.parameters)
                }
                _ => false,
            }
        }
        None => {
            !probe_policy_equal(
                Some(&previous.default_policy),
                Some(&current.default_policy),
            ) || !probe_parameters_equal(&previous.default_parameters, &current.default_parameters)
        }
    }
}

fn custom_probe_changed(
    previous: &ApplicationSnapshot,
    current: &ApplicationSnapshot,
    site_id: &EntityId,
    model_id: &EntityId,
) -> bool {
    let (Some(previous), Some(current)) = (
        find_site(previous, site_id).and_then(|site| find_model(site, model_id)),
        find_site(current, site_id).and_then(|site| find_model(site, model_id)),
    ) else {
        return false;
    };
    let custom_fields_equal = match (previous.policy.as_ref(), current.policy.as_ref()) {
        (Some(previous), Some(current)) => {
            previous.custom_endpoint == current.custom_endpoint
                && previous.custom_method == current.custom_method
                && previous.custom_stream_enabled == current.custom_stream_enabled
        }
        (None, None) => true,
        _ => false,
    };
    !custom_fields_equal || !probe_parameters_equal(&previous.parameters, &current.parameters)
}

fn general_settings_changed_fields(
    previous: &ApplicationSnapshot,
    current: &ApplicationSnapshot,
) -> Vec<&'static str> {
    let previous = &previous.general_settings;
    let current = &current.general_settings;
    let mut fields = Vec::new();
    if previous.launch_at_startup != current.launch_at_startup {
        fields.push("launchAtStartup");
    }
    if previous.continue_in_background != current.continue_in_background {
        fields.push("continueInBackground");
    }
    if previous.tray_action != current.tray_action {
        fields.push("trayAction");
    }
    if previous.theme != current.theme {
        fields.push("theme");
    }
    if previous.history_retention_days != current.history_retention_days {
        fields.push("historyRetentionDays");
    }
    if previous.diagnostic_capture_enabled != current.diagnostic_capture_enabled {
        fields.push("diagnosticCaptureEnabled");
    }
    fields
}

fn notification_settings_changed_fields(
    previous: &NotificationSettingsSnapshot,
    current: &NotificationSettingsSnapshot,
) -> Vec<&'static str> {
    let mut fields = Vec::new();
    if previous.notifications_enabled != current.notifications_enabled {
        fields.push("notificationsEnabled");
    }
    if !notification_rule_controls_equal(&previous.rules, &current.rules) {
        fields.push("rules");
    }
    if !notification_templates_equal(&previous.rules, &current.rules) {
        fields.push("templateOverrides");
    }
    fields
}

fn find_site<'a>(
    snapshot: &'a ApplicationSnapshot,
    site_id: &EntityId,
) -> Option<&'a SiteAggregate> {
    snapshot.sites.iter().find(|site| &site.site.id == site_id)
}

fn find_model<'a>(site: &'a SiteAggregate, model_id: &EntityId) -> Option<&'a ModelAggregate> {
    site.models.iter().find(|model| &model.model.id == model_id)
}

fn headers_equal(previous: &[RequestHeader], current: &[RequestHeader]) -> bool {
    previous.len() == current.len()
        && previous.iter().zip(current).all(|(previous, current)| {
            previous.name == current.name
                && previous.value_template == current.value_template
                && previous.sort_order == current.sort_order
        })
}

fn probe_policy_equal(previous: Option<&ProbePolicy>, current: Option<&ProbePolicy>) -> bool {
    match (previous, current) {
        (Some(previous), Some(current)) => {
            previous.mode == current.mode
                && previous.max_output_tokens == current.max_output_tokens
                && previous.prompt == current.prompt
        }
        (None, None) => true,
        _ => false,
    }
}

fn probe_parameters_equal(previous: &[ProbeParameter], current: &[ProbeParameter]) -> bool {
    previous.len() == current.len()
        && previous.iter().zip(current).all(|(previous, current)| {
            previous.key == current.key
                && previous.value_type == current.value_type
                && previous.value_json == current.value_json
                && previous.sort_order == current.sort_order
        })
}

fn notification_rule_controls_equal(
    previous: &[NotificationRule],
    current: &[NotificationRule],
) -> bool {
    previous.len() == current.len()
        && previous.iter().all(|previous| {
            current.iter().any(|current| {
                previous.event_type == current.event_type
                    && previous.enabled == current.enabled
                    && previous.channel_ids == current.channel_ids
            })
        })
}

fn notification_templates_equal(
    previous: &[NotificationRule],
    current: &[NotificationRule],
) -> bool {
    previous.len() == current.len()
        && previous.iter().all(|previous| {
            current.iter().any(|current| {
                previous.event_type == current.event_type
                    && previous.template_overrides == current.template_overrides
            })
        })
}

fn changed(
    app: &tauri::AppHandle,
    value: &ApplicationSnapshot,
    operation_id: Option<&str>,
    domains: &[&'static str],
) -> CommandResult<SnapshotDto> {
    if desktop::update_status_projection(app, value).is_err() {
        desktop::record_desktop_error(
            app,
            "status-projection-update",
            "status_projection_update_failed",
        );
    }
    let value = snapshot(value)?;
    if let Some(state) = app.try_state::<BackendState>() {
        if let Some(diagnostics) = state.diagnostics() {
            diagnostics.record_state_change(operation_id, value.revision, domains);
        }
    }
    if app
        .emit(
            "relaypulse://state-changed",
            StateChangedDto {
                revision: value.revision,
                domains: domains.to_vec(),
            },
        )
        .is_err()
    {
        desktop::record_desktop_error(app, "state-change-event", "state_change_emit_failed");
    }
    Ok(value)
}

pub(crate) fn publish_background_snapshot(app: &tauri::AppHandle, value: &ApplicationSnapshot) {
    if changed(app, value, None, &["records", "status", "sites"]).is_err() {
        desktop::record_desktop_error(
            app,
            "background-snapshot-publish",
            "snapshot_projection_failed",
        );
    }
}

fn snapshot(value: &ApplicationSnapshot) -> CommandResult<SnapshotDto> {
    let latest = latest_probes(value);
    Ok(SnapshotDto {
        revision: value.revision,
        sites: value
            .sites
            .iter()
            .cloned()
            .enumerate()
            .map(|(index, site)| site_dto(site, priority(index), &latest))
            .collect(),
        general_settings: GeneralSettingsDto {
            launch_at_startup: value.general_settings.launch_at_startup,
            continue_in_background: value.general_settings.continue_in_background,
            tray_action: tray_action_name(value.general_settings.tray_action),
            theme: theme_name(value.general_settings.theme),
            language: "zh-CN",
            history_retention_days: value.general_settings.history_retention_days,
            diagnostic_capture_enabled: value.general_settings.diagnostic_capture_enabled,
        },
        probe_records: value
            .probe_records
            .iter()
            .map(|record| probe_record_dto(value, record))
            .collect(),
    })
}

fn site_dto(
    value: SiteAggregate,
    priority: u32,
    latest: &HashMap<EntityId, LatestProbe>,
) -> SiteDto {
    let enabled_models = value
        .models
        .iter()
        .filter(|aggregate| aggregate.model.enabled)
        .count();
    let enabled_results = value.models.iter().filter_map(|model| {
        model
            .model
            .enabled
            .then(|| latest.get(&model.model.id).map(|probe| probe.status))
            .flatten()
    });
    let statuses = enabled_results.collect::<Vec<_>>();
    let status = if !value.site.enabled {
        "muted"
    } else if statuses
        .iter()
        .any(|status| probe_status_name(*status) == "error")
    {
        "error"
    } else if statuses.contains(&ProbeStatus::Slow) {
        "warning"
    } else if enabled_models > 0
        && statuses.len() == enabled_models
        && statuses
            .iter()
            .all(|status| *status == ProbeStatus::Healthy)
    {
        "healthy"
    } else {
        "muted"
    };
    let summary = if !value.site.enabled {
        "站点已停用".to_owned()
    } else if enabled_models == 0 {
        "0 个模型启用 · 不参与探测".to_owned()
    } else if status == "error" {
        let failures = statuses
            .iter()
            .filter(|status| probe_status_name(**status) == "error")
            .count();
        format!("{enabled_models} 个模型 · {failures} 个异常")
    } else if status == "warning" {
        format!("{enabled_models} 个模型 · 响应偏慢")
    } else if status == "healthy" {
        format!("{enabled_models} 个模型 · 全部正常")
    } else {
        format!("{enabled_models} 个模型 · 等待首次探测")
    };
    let default_settings = probe_settings_dto(
        &value.default_policy,
        &value.default_parameters,
        value.default_policy.mode,
    );
    let models = value
        .models
        .into_iter()
        .map(|model| {
            let latest_probe = latest.get(&model.model.id);
            model_dto(model, &value.default_policy, latest_probe)
        })
        .collect();
    SiteDto {
        id: value.site.id.to_string(),
        name: value.site.name,
        protocol: protocol_name(value.site.protocol),
        base_url: value.site.base_url,
        credential: CredentialSummaryDto {
            configured: value.site.credential_ref.is_some(),
            masked: value
                .site
                .credential_ref
                .as_ref()
                .map(|_| "••••••••••••••••"),
        },
        enabled: value.site.enabled,
        priority,
        status,
        summary,
        models_endpoint: value.site.models_endpoint,
        probe_endpoint: value.site.probe_endpoint,
        probe_interval_minutes: value.site.timing.interval_minutes,
        slow_first_token_seconds: value.site.timing.slow_first_token_ms / 1_000,
        slow_total_seconds: value.site.timing.slow_total_ms / 1_000,
        failure_threshold: value.site.timing.failure_threshold,
        recovery_threshold: value.site.timing.recovery_threshold,
        headers: value
            .headers
            .into_iter()
            .map(|header| RequestHeaderDto {
                key: header.name,
                value: header.value_template,
            })
            .collect(),
        default_probe_settings: default_settings,
        models,
    }
}

fn priority(index: usize) -> u32 {
    u32::try_from(index).unwrap_or(u32::MAX).saturating_add(1)
}

fn model_dto(
    value: ModelAggregate,
    default_policy: &crate::domain::ProbePolicy,
    latest: Option<&LatestProbe>,
) -> ModelDto {
    let effective_policy = value.policy.as_ref().unwrap_or(default_policy);
    let mode = value
        .policy
        .as_ref()
        .map(|policy| policy.mode)
        .unwrap_or(ProbeMode::Inherit);
    let parameters = parameters_dto(&value.parameters);
    ModelDto {
        id: value.model.id.to_string(),
        name: value
            .model
            .display_name
            .unwrap_or_else(|| value.model.model_key.clone()),
        enabled: value.model.enabled,
        status: latest.map_or("muted", |probe| probe_status_name(probe.status)),
        inference_summary: if value
            .policy
            .as_ref()
            .and_then(|policy| policy.custom_endpoint.as_ref())
            .is_some()
        {
            "自定义探针"
        } else if parameters.is_empty() {
            "默认 · low"
        } else {
            "默认 low · 透传覆盖"
        },
        first_token: latest
            .and_then(|probe| probe.result.as_ref())
            .and_then(|result| result.first_token_ms)
            .map_or_else(|| "—".to_owned(), format_duration),
        total_time: latest
            .and_then(|probe| probe.result.as_ref())
            .and_then(|result| result.total_time_ms)
            .map_or_else(|| "—".to_owned(), format_duration),
        latest_probe_id: latest
            .and_then(|probe| probe.probe_result_id.as_ref())
            .map(ToString::to_string),
        next_probe_at: latest
            .and_then(|probe| probe.next_probe_at)
            .map(timestamp_rfc3339),
        capabilities: vec!["待首次探测识别"],
        probe_settings: ProbeSettingsDto {
            mode: probe_mode_name(mode),
            max_output_tokens: effective_policy.max_output_tokens,
            prompt: effective_policy.prompt.clone(),
            parameters: parameters.clone(),
        },
        custom_probe: CustomProbeSettingsDto {
            enabled: value
                .policy
                .as_ref()
                .and_then(|policy| policy.custom_endpoint.as_ref())
                .is_some(),
            endpoint: value
                .policy
                .as_ref()
                .and_then(|policy| policy.custom_endpoint.clone())
                .unwrap_or_default(),
            parameters,
        },
    }
}

#[derive(Clone)]
struct LatestProbe {
    status: ProbeStatus,
    result: Option<ProbeResult>,
    probe_result_id: Option<EntityId>,
    next_probe_at: Option<TimestampMs>,
}

fn latest_probes(value: &ApplicationSnapshot) -> HashMap<EntityId, LatestProbe> {
    let results = value
        .latest_model_results
        .iter()
        .map(|result| (result.id.clone(), result.clone()))
        .collect::<HashMap<_, _>>();
    value
        .latest_model_statuses
        .iter()
        .map(|status| {
            (
                status.model_id.clone(),
                LatestProbe {
                    status: status.status,
                    probe_result_id: status.probe_result_id.clone(),
                    next_probe_at: status.next_probe_at,
                    result: status
                        .probe_result_id
                        .as_ref()
                        .and_then(|result_id| results.get(result_id))
                        .cloned(),
                },
            )
        })
        .collect()
}

fn probe_record_dto(
    snapshot: &ApplicationSnapshot,
    aggregate: &ProbeRecordAggregate,
) -> ProbeRecordDto {
    let result = &aggregate.result;
    let site = snapshot
        .sites
        .iter()
        .find(|site| site.site.id == result.site_id);
    let model = site.and_then(|site| {
        site.models
            .iter()
            .find(|model| model.model.id == result.model_id)
    });
    let policy = site.and_then(|site| {
        model
            .and_then(|model| model.policy.as_ref())
            .or(Some(&site.default_policy))
    });
    let endpoint = if result.endpoint.is_empty() {
        match (site, policy) {
            (Some(site), Some(policy)) => resolve_probe_endpoint(&site.site, policy)
                .map(|url| url.to_string())
                .unwrap_or_else(|_| site.site.probe_endpoint.clone()),
            _ => String::new(),
        }
    } else {
        result.endpoint.clone()
    };
    let method = match result.request_method {
        HttpMethod::Get => "GET",
        HttpMethod::Post => "POST",
    };
    let probe_mode = match result.probe_mode {
        ProbeMode::Inherit => "继承站点默认",
        ProbeMode::DisabledThinking => "低推理",
        ProbeMode::MinimumReasoning => "最低推理",
    };
    let request_headers = parse_json_array(&aggregate.capture.request_headers_json);
    let response_headers = parse_json_array(&aggregate.capture.response_headers_json);
    let response_chunks = parse_json_array(&aggregate.capture.response_chunks_json);
    let total_time_ms = result.total_time_ms.unwrap_or_default();

    ProbeRecordDto {
        id: result.id.to_string(),
        started_at: timestamp_rfc3339(result.started_at),
        status: probe_status_name(result.status),
        site_id: result.site_id.to_string(),
        site_name: site.map_or_else(|| "已删除站点".to_owned(), |site| site.site.name.clone()),
        model_id: result.model_id.to_string(),
        model_name: model.map_or_else(
            || "已删除模型".to_owned(),
            |model| {
                model
                    .model
                    .display_name
                    .clone()
                    .unwrap_or_else(|| model.model.model_key.clone())
            },
        ),
        first_token_ms: result.first_token_ms,
        first_token_label: result
            .first_token_ms
            .map_or_else(|| "—".to_owned(), format_duration),
        total_time_ms,
        total_time_label: format_duration(total_time_ms),
        result: result_label(result),
        method,
        endpoint,
        probe_mode,
        http_status: result.http_status,
        request_id: result.request_id.clone(),
        error_detail: result.error_message.clone(),
        token_usage: TokenUsageDto {
            input_tokens: result.token_usage.input_tokens,
            output_tokens: result.token_usage.output_tokens,
            total_tokens: result.token_usage.total_tokens,
            source: if result.token_usage.reported {
                "reported"
            } else {
                "unavailable"
            },
        },
        exchange: ProbeExchangeDto {
            request: ProbeCaptureDto {
                headers: request_headers,
                raw_body: aggregate.capture.request_body.clone(),
                captured_bytes: aggregate.capture.request_captured_bytes,
                capture_limit_bytes: aggregate.capture.request_limit_bytes,
                truncated: aggregate.capture.request_truncated,
            },
            response: ProbeResponseCaptureDto {
                headers: response_headers,
                raw_body: aggregate.capture.response_body.clone(),
                captured_bytes: aggregate.capture.response_captured_bytes,
                capture_limit_bytes: aggregate.capture.response_limit_bytes,
                truncated: aggregate.capture.response_truncated,
                mode: match aggregate
                    .capture
                    .response_mode
                    .unwrap_or(ResponseMode::Synchronous)
                {
                    ResponseMode::Stream => "stream",
                    ResponseMode::Synchronous => "synchronous",
                },
                content_type: aggregate.capture.response_content_type.clone(),
                chunks: response_chunks,
            },
        },
    }
}

fn parse_json_array<T: for<'de> Deserialize<'de>>(value: &str) -> Vec<T> {
    serde_json::from_str(value).unwrap_or_default()
}

fn probe_status_name(value: ProbeStatus) -> &'static str {
    match value {
        ProbeStatus::Healthy => "healthy",
        ProbeStatus::Slow => "warning",
        ProbeStatus::Unknown | ProbeStatus::Probing => "muted",
        _ => "error",
    }
}

fn result_label(result: &ProbeResult) -> String {
    match result.status {
        ProbeStatus::Healthy => "探测成功".to_owned(),
        ProbeStatus::Slow => "响应偏慢".to_owned(),
        _ => result
            .error_message
            .clone()
            .unwrap_or_else(|| "探测失败".to_owned()),
    }
}

fn format_duration(value: u32) -> String {
    if value < 1_000 {
        format!("{value} ms")
    } else {
        format!("{:.2} s", f64::from(value) / 1_000.0)
    }
}

fn timestamp_rfc3339(value: TimestampMs) -> String {
    OffsetDateTime::from_unix_timestamp_nanos(i128::from(value.as_i64()) * 1_000_000)
        .ok()
        .and_then(|value| value.format(&Rfc3339).ok())
        .unwrap_or_else(|| "1970-01-01T00:00:00Z".to_owned())
}

fn probe_settings_dto(
    policy: &crate::domain::ProbePolicy,
    parameters: &[crate::domain::ProbeParameter],
    mode: ProbeMode,
) -> ProbeSettingsDto {
    ProbeSettingsDto {
        mode: probe_mode_name(mode),
        max_output_tokens: policy.max_output_tokens,
        prompt: policy.prompt.clone(),
        parameters: parameters_dto(parameters),
    }
}

fn parameters_dto(parameters: &[crate::domain::ProbeParameter]) -> Vec<ParameterDto> {
    parameters
        .iter()
        .map(|parameter| ParameterDto {
            key: parameter.key.clone(),
            value: parameter.value_json.clone(),
            r#type: parameter_type_name(parameter.value_type),
        })
        .collect()
}

fn protocol_name(value: SiteProtocol) -> &'static str {
    match value {
        SiteProtocol::OpenAiCompatible => "openai-compatible",
        SiteProtocol::Anthropic => "anthropic",
        SiteProtocol::OpenAiResponses => "openai-responses",
        SiteProtocol::CustomHttp => "custom-http",
    }
}

fn probe_mode_name(value: ProbeMode) -> &'static str {
    match value {
        ProbeMode::Inherit => "inherit",
        ProbeMode::DisabledThinking => "disabled-thinking",
        ProbeMode::MinimumReasoning => "minimum-reasoning",
    }
}

fn parameter_type_name(value: ParameterType) -> &'static str {
    match value {
        ParameterType::String => "string",
        ParameterType::Boolean => "boolean",
        ParameterType::Number => "number",
        ParameterType::Json => "json",
    }
}

fn theme_name(value: ThemeMode) -> &'static str {
    match value {
        ThemeMode::System => "system",
        ThemeMode::Light => "light",
        ThemeMode::Dark => "dark",
    }
}

fn tray_action_name(value: TrayAction) -> &'static str {
    match value {
        TrayAction::Panel => "panel",
        TrayAction::Disabled => "disabled",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_errors_are_stable_and_do_not_include_internal_details() {
        let error = CommandError::from(ApplicationError::InvalidInput { field: "base_url" });

        assert_eq!(error.code, "invalid_input");
        assert_eq!(error.category, "validation");
        assert_eq!(error.field, Some("base_url"));
        assert!(!error.retryable);
        assert_eq!(error.operation_id, None);

        let error = CommandError::from(ApplicationError::Authentication);
        assert_eq!(error.code, "authentication_failed");
        assert_eq!(error.category, "credential");
        assert!(!error.retryable);

        let error = CommandError::from(ApplicationError::RateLimited);
        assert_eq!(error.code, "rate_limited");
        assert_eq!(error.category, "upstream");
        assert!(error.retryable);

        let operation_history_errors = [
            (
                OperationHistoryQueryError::CursorExpired,
                "cursor_expired",
                true,
            ),
            (OperationHistoryQueryError::QueryBusy, "query_busy", true),
            (
                OperationHistoryQueryError::InvalidQuery,
                "invalid_query",
                false,
            ),
            (OperationHistoryQueryError::NotFound, "not_found", false),
            (
                OperationHistoryQueryError::BackendUnavailable,
                "backend_unavailable",
                true,
            ),
        ];
        for (source, code, retryable) in operation_history_errors {
            let error = CommandError::operation_history(source);
            assert_eq!(error.code, code);
            assert_eq!(error.retryable, retryable);
            assert_eq!(error.operation_id, None);
        }
    }

    #[test]
    fn model_sync_confirmation_uses_the_frontend_contract() {
        let value = serde_json::to_value(ModelSyncResultDto::ConfirmationRequired {
            removed_models: vec![ModelSyncRemovalDto {
                id: "model-a".to_owned(),
                name: "Model A".to_owned(),
            }],
        })
        .expect("model sync confirmation serializes");

        assert_eq!(
            value,
            serde_json::json!({
                "status": "confirmationRequired",
                "removedModels": [{ "id": "model-a", "name": "Model A" }]
            })
        );
    }

    #[test]
    fn diagnostic_inputs_accept_only_safe_correlatable_metadata() {
        assert!(resolve_operation_id(Some("op_fixture-123".to_owned())).is_ok());
        assert!(resolve_operation_id(Some("wrong_fixture".to_owned())).is_err());
        assert!(validate_view("notifications", "view").is_ok());
        assert!(validate_view("operation-history", "view").is_ok());
        assert!(validate_view("intelligence-ladder", "view").is_ok());
        assert!(validate_frontend_action("open-probe-record").is_ok());
        assert!(validate_frontend_action("open-operation-record").is_ok());
        assert!(validate_frontend_action("clicked secret button").is_err());
        assert!(validate_stack_frame("render@src/App.tsx:42:7").is_ok());
        assert!(validate_stack_frame("https://example.test/App.tsx?token=secret:42:7").is_err());
        assert!(validate_fingerprint("0123456789abcdef").is_ok());
        assert!(validate_fingerprint("not-a-fingerprint").is_err());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn observed_failures_return_a_searchable_operation_id() {
        let directory = tempfile::tempdir().expect("temporary directory created");
        let diagnostics = Diagnostics::initialize(directory.path()).expect("diagnostics starts");

        let error = observed_command(Some(diagnostics), "fixture-operation", async {
            Err::<(), _>(invalid_field("fixture"))
        })
        .await
        .expect_err("fixture operation fails");

        let operation_id = error.operation_id.expect("operation id returned");
        let log = std::fs::read_to_string(directory.path().join("logs/relaypulse.log"))
            .expect("diagnostic log reads");
        assert!(log.contains(&operation_id));
        assert!(log.contains(r#""operation":"fixture-operation""#));
        assert!(log.contains(r#""outcome":"failed""#));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn successful_operations_record_actual_fields_instead_of_the_attempted_scope() {
        let directory = tempfile::tempdir().expect("temporary directory created");
        let diagnostics = Diagnostics::initialize(directory.path()).expect("diagnostics starts");
        let attempted = OperationAudit {
            target_type: "site",
            target_id: Some("site_fixture".to_owned()),
            parent_id: None,
            change_kind: "update",
            field_scope: vec!["name", "baseUrl", "credential"],
        };

        observed_user_command_with_result_audit(
            Some(diagnostics),
            "update-site",
            Some("op_actual_fields".to_owned()),
            attempted.clone(),
            |_| async move { Ok(((), audit_with_fields(attempted, vec!["baseUrl"]))) },
        )
        .await
        .expect("fixture operation succeeds");

        let log = std::fs::read_to_string(directory.path().join("logs/relaypulse.log"))
            .expect("diagnostic log reads");
        let events = log
            .lines()
            .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
            .filter(|event| event["operationId"] == "op_actual_fields")
            .collect::<Vec<_>>();
        let started = events
            .iter()
            .find(|event| event["event"] == "operation-started")
            .expect("start event exists");
        let finished = events
            .iter()
            .find(|event| event["event"] == "operation-finished")
            .expect("finish event exists");
        assert_eq!(
            started["fieldScope"],
            serde_json::json!(["name", "baseUrl", "credential"])
        );
        assert_eq!(finished["fieldScope"], serde_json::json!(["baseUrl"]));
    }

    #[test]
    fn read_failures_are_recorded_with_safe_command_and_error_codes() {
        let directory = tempfile::tempdir().expect("temporary directory created");
        let state = BackendState::initialize(directory.path().to_path_buf());
        let result = Err::<(), _>(CommandError::database_unavailable());

        record_command_failure(&state, "get-snapshot", &result);

        let log = std::fs::read_to_string(directory.path().join("logs/relaypulse.log"))
            .expect("diagnostic log reads");
        assert!(log.contains(r#""operation":"get-snapshot""#));
        assert!(log.contains(r#""errorCode":"database_unavailable""#));
    }
}
