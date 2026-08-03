use serde::{Deserialize, Serialize};
use tauri::{Emitter, State};

use crate::{
    application::{
        ApplicationError, ApplicationService, ApplicationSnapshot, CreateModel, CreateSite,
        CustomProbeUpdate, ModelAggregate, ModelSyncOutcome, ParameterValue, ProbeSettingsUpdate,
        SiteAdvancedUpdate, SiteAggregate, UpdateSite,
    },
    domain::{
        CredentialIntent, EntityId, ParameterType, ProbeMode, SecretValue, SiteProtocol, ThemeMode,
    },
    startup::BackendState,
};

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

    fn not_implemented() -> Self {
        Self {
            code: "not_implemented",
            category: "internal",
            message: "This RelayPulse operation is not available yet.",
            field: None,
            retryable: false,
            operation_id: None,
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
    first_token_timeout_seconds: u32,
    total_timeout_seconds: u32,
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
    first_token_timeout_seconds: u32,
    total_timeout_seconds: u32,
    headers: Vec<RequestHeaderDto>,
    default_probe_settings: ProbeSettingsDto,
    models: Vec<ModelDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
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
    first_token: &'static str,
    total_time: &'static str,
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

#[tauri::command]
pub async fn get_snapshot(state: State<'_, BackendState>) -> CommandResult<SnapshotDto> {
    snapshot(&service(&state)?.snapshot().await?)
}

#[tauri::command]
pub async fn get_site(
    state: State<'_, BackendState>,
    id: String,
) -> CommandResult<Option<SiteDto>> {
    let service = service(&state)?;
    let site_id = entity_id(id, "id")?;
    let snapshot = service.snapshot().await?;
    Ok(snapshot
        .sites
        .into_iter()
        .enumerate()
        .find(|(_, site)| site.site.id == site_id)
        .map(|(index, site)| site_dto(site, priority(index))))
}

#[tauri::command]
pub async fn create_site(
    app: tauri::AppHandle,
    state: State<'_, BackendState>,
    input: CreateSiteInputDto,
) -> CommandResult<SnapshotDto> {
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
    changed(&app, &result, &["sites"])
}

#[tauri::command]
pub async fn update_site(
    app: tauri::AppHandle,
    state: State<'_, BackendState>,
    id: String,
    input: UpdateSiteInputDto,
) -> CommandResult<SnapshotDto> {
    let result = service(&state)?
        .update_site(UpdateSite {
            site_id: entity_id(id, "id")?,
            name: input.name,
            protocol: input.protocol.into(),
            base_url: input.base_url,
            credential: input.credential.try_into()?,
        })
        .await?;
    changed(&app, &result, &["sites"])
}

#[tauri::command]
pub async fn delete_site(
    app: tauri::AppHandle,
    state: State<'_, BackendState>,
    id: String,
) -> CommandResult<SnapshotDto> {
    let result = service(&state)?.delete_site(entity_id(id, "id")?).await?;
    changed(&app, &result, &["sites", "records"])
}

#[tauri::command]
pub async fn reorder_sites(
    app: tauri::AppHandle,
    state: State<'_, BackendState>,
    site_ids: Vec<String>,
) -> CommandResult<SnapshotDto> {
    let site_ids = site_ids
        .into_iter()
        .map(|id| entity_id(id, "site_ids"))
        .collect::<CommandResult<Vec<_>>>()?;
    let result = service(&state)?.reorder_sites(site_ids).await?;
    changed(&app, &result, &["sites"])
}

#[tauri::command]
pub async fn set_site_enabled(
    app: tauri::AppHandle,
    state: State<'_, BackendState>,
    id: String,
    enabled: bool,
) -> CommandResult<SnapshotDto> {
    let result = service(&state)?
        .set_site_enabled(entity_id(id, "id")?, enabled)
        .await?;
    changed(&app, &result, &["sites"])
}

#[tauri::command]
pub async fn create_model(
    app: tauri::AppHandle,
    state: State<'_, BackendState>,
    site_id: String,
    input: CreateModelInputDto,
) -> CommandResult<SnapshotDto> {
    let result = service(&state)?
        .create_model(CreateModel {
            site_id: entity_id(site_id, "site_id")?,
            model_key: input.name,
            enabled: input.enabled,
        })
        .await?;
    changed(&app, &result, &["sites"])
}

#[tauri::command]
pub async fn delete_model(
    app: tauri::AppHandle,
    state: State<'_, BackendState>,
    site_id: String,
    model_id: String,
) -> CommandResult<SnapshotDto> {
    let result = service(&state)?
        .delete_model(
            entity_id(site_id, "site_id")?,
            entity_id(model_id, "model_id")?,
        )
        .await?;
    changed(&app, &result, &["models", "sites", "records"])
}

#[tauri::command]
pub async fn set_model_enabled(
    app: tauri::AppHandle,
    state: State<'_, BackendState>,
    site_id: String,
    model_id: String,
    enabled: bool,
) -> CommandResult<SnapshotDto> {
    let result = service(&state)?
        .set_model_enabled(
            entity_id(site_id, "site_id")?,
            entity_id(model_id, "model_id")?,
            enabled,
        )
        .await?;
    changed(&app, &result, &["sites"])
}

#[tauri::command]
pub async fn update_site_advanced(
    app: tauri::AppHandle,
    state: State<'_, BackendState>,
    id: String,
    input: SiteAdvancedInputDto,
) -> CommandResult<SnapshotDto> {
    let result = service(&state)?
        .update_site_advanced(SiteAdvancedUpdate {
            site_id: entity_id(id, "id")?,
            models_endpoint: input.models_endpoint,
            probe_endpoint: input.probe_endpoint,
            interval_minutes: input.probe_interval_minutes,
            first_token_timeout_ms: input
                .first_token_timeout_seconds
                .checked_mul(1_000)
                .ok_or_else(|| invalid_field("first_token_timeout_seconds"))?,
            total_timeout_ms: input
                .total_timeout_seconds
                .checked_mul(1_000)
                .ok_or_else(|| invalid_field("total_timeout_seconds"))?,
            headers: input
                .headers
                .into_iter()
                .map(|header| (header.key, header.value))
                .collect(),
        })
        .await?;
    changed(&app, &result, &["sites"])
}

#[tauri::command]
pub async fn update_site_probe_settings(
    app: tauri::AppHandle,
    state: State<'_, BackendState>,
    site_id: String,
    settings: ProbeSettingsInputDto,
) -> CommandResult<SnapshotDto> {
    update_probe_settings(app, state, site_id, None, settings).await
}

#[tauri::command]
pub async fn update_model_probe_settings(
    app: tauri::AppHandle,
    state: State<'_, BackendState>,
    site_id: String,
    model_id: String,
    settings: ProbeSettingsInputDto,
) -> CommandResult<SnapshotDto> {
    update_probe_settings(app, state, site_id, Some(model_id), settings).await
}

async fn update_probe_settings(
    app: tauri::AppHandle,
    state: State<'_, BackendState>,
    site_id: String,
    model_id: Option<String>,
    settings: ProbeSettingsInputDto,
) -> CommandResult<SnapshotDto> {
    let result = service(&state)?
        .update_probe_settings(ProbeSettingsUpdate {
            site_id: entity_id(site_id, "site_id")?,
            model_id: model_id.map(|id| entity_id(id, "model_id")).transpose()?,
            mode: settings.mode.into(),
            max_output_tokens: settings.max_output_tokens,
            prompt: settings.prompt,
            parameters: settings.parameters.into_iter().map(Into::into).collect(),
        })
        .await?;
    changed(&app, &result, &["sites"])
}

#[tauri::command]
pub async fn update_model_custom_probe_settings(
    app: tauri::AppHandle,
    state: State<'_, BackendState>,
    site_id: String,
    model_id: String,
    settings: CustomProbeInputDto,
) -> CommandResult<SnapshotDto> {
    let result = service(&state)?
        .update_custom_probe(CustomProbeUpdate {
            site_id: entity_id(site_id, "site_id")?,
            model_id: entity_id(model_id, "model_id")?,
            enabled: settings.enabled,
            endpoint: settings.endpoint,
            parameters: settings.parameters.into_iter().map(Into::into).collect(),
        })
        .await?;
    changed(&app, &result, &["sites"])
}

#[tauri::command]
pub async fn update_general_settings(
    app: tauri::AppHandle,
    state: State<'_, BackendState>,
    settings: GeneralSettingsInputDto,
) -> CommandResult<SnapshotDto> {
    let result = service(&state)?
        .update_general_settings(
            settings.launch_at_startup,
            settings.continue_in_background,
            settings.theme.into(),
            settings.history_retention_days,
            settings.diagnostic_capture_enabled,
        )
        .await?;
    changed(&app, &result, &["settings"])
}

#[tauri::command]
pub async fn refresh_probe_records(state: State<'_, BackendState>) -> CommandResult<SnapshotDto> {
    get_snapshot(state).await
}

#[tauri::command]
pub async fn clear_probe_records(
    app: tauri::AppHandle,
    state: State<'_, BackendState>,
) -> CommandResult<SnapshotDto> {
    let (_, result) = service(&state)?.clear_probe_records().await?;
    changed(&app, &result, &["records", "status"])
}

#[tauri::command]
pub async fn refresh_models(
    app: tauri::AppHandle,
    state: State<'_, BackendState>,
    site_id: String,
    confirmed_removed_model_ids: Option<Vec<String>>,
) -> CommandResult<ModelSyncResultDto> {
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
            snapshot: changed(&app, &snapshot, &["models", "sites", "records"])?,
        }),
        ModelSyncOutcome::ConfirmationRequired { removed_models } => {
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
}

#[tauri::command]
pub async fn run_immediate_probe(_state: State<'_, BackendState>) -> CommandResult<SnapshotDto> {
    Err(CommandError::not_implemented())
}

fn service(state: &State<'_, BackendState>) -> CommandResult<ApplicationService> {
    state
        .application_service()
        .cloned()
        .ok_or_else(CommandError::database_unavailable)
}

fn entity_id(value: String, field: &'static str) -> CommandResult<EntityId> {
    EntityId::parse(value).map_err(|_| invalid_field(field))
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

fn changed(
    app: &tauri::AppHandle,
    value: &ApplicationSnapshot,
    domains: &[&'static str],
) -> CommandResult<SnapshotDto> {
    let value = snapshot(value)?;
    let _ = app.emit(
        "relaypulse://state-changed",
        StateChangedDto {
            revision: value.revision,
            domains: domains.to_vec(),
        },
    );
    Ok(value)
}

fn snapshot(value: &ApplicationSnapshot) -> CommandResult<SnapshotDto> {
    Ok(SnapshotDto {
        revision: value.revision,
        sites: value
            .sites
            .iter()
            .cloned()
            .enumerate()
            .map(|(index, site)| site_dto(site, priority(index)))
            .collect(),
        general_settings: GeneralSettingsDto {
            launch_at_startup: value.general_settings.launch_at_startup,
            continue_in_background: value.general_settings.continue_in_background,
            tray_action: "panel",
            theme: theme_name(value.general_settings.theme),
            language: "zh-CN",
            history_retention_days: value.general_settings.history_retention_days,
            diagnostic_capture_enabled: value.general_settings.diagnostic_capture_enabled,
        },
    })
}

fn site_dto(value: SiteAggregate, priority: u32) -> SiteDto {
    let enabled_models = value
        .models
        .iter()
        .filter(|aggregate| aggregate.model.enabled)
        .count();
    let status = "muted";
    let summary = if !value.site.enabled {
        "站点已停用".to_owned()
    } else if enabled_models == 0 {
        "0 个模型启用 · 不参与探测".to_owned()
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
        .map(|model| model_dto(model, &value.default_policy))
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
        first_token_timeout_seconds: value.site.timing.first_token_timeout_ms / 1_000,
        total_timeout_seconds: value.site.timing.total_timeout_ms / 1_000,
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

fn model_dto(value: ModelAggregate, default_policy: &crate::domain::ProbePolicy) -> ModelDto {
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
        status: "muted",
        inference_summary: match mode {
            ProbeMode::Inherit => "继承站点默认",
            ProbeMode::DisabledThinking => "自动 · 关闭思考",
            ProbeMode::MinimumReasoning => "自动 · 最低推理",
        },
        first_token: "—",
        total_time: "—",
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
}
