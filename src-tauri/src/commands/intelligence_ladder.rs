use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager, State};

use crate::{
    intelligence_ladder::{
        IntelligenceAutoFetchSettings, IntelligenceLadderError, IntelligenceLadderRuntimeState,
        IntelligenceLadderService, IntelligenceLadderSnapshot, IntelligenceRefreshResult,
    },
    startup::BackendState,
};

use super::{
    audit_with_fields, observed_user_command, observed_user_command_with_result_audit,
    record_command_failure, CommandError, CommandResult, StateChangedDto,
};
use crate::diagnostics::OperationAudit;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntelligenceRankingEntryDto {
    rank: u32,
    model: String,
    effort: String,
    iq: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntelligenceAutoFetchSettingsDto {
    enabled: bool,
    interval_minutes: u32,
    fetch_on_app_start: bool,
    fetch_on_network_restore: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum IntelligenceRefreshStateDto {
    Idle,
    Running,
    Failed,
    ScraperUnavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct IntelligenceLadderRuntimeDto {
    state: IntelligenceRefreshStateDto,
    last_successful_at: Option<String>,
    next_scheduled_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct IntelligenceSafeErrorDto {
    code: String,
    message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntelligenceLadderSnapshotDto {
    revision: u64,
    entries: Vec<IntelligenceRankingEntryDto>,
    fetched_at: Option<String>,
    is_stale: bool,
    last_error: Option<IntelligenceSafeErrorDto>,
    runtime: IntelligenceLadderRuntimeDto,
    settings: IntelligenceAutoFetchSettingsDto,
    allowed_interval_minutes: Vec<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum IntelligenceRefreshResultDto {
    Started,
    AlreadyRunning,
    ScraperUnavailable,
}

#[tauri::command]
pub async fn get_intelligence_ladder_snapshot(
    state: State<'_, BackendState>,
) -> CommandResult<IntelligenceLadderSnapshotDto> {
    let result = async {
        service(&state)?
            .snapshot()
            .await
            .map(Into::into)
            .map_err(Into::into)
    }
    .await;
    record_command_failure(&state, "intelligence-ladder-snapshot", &result);
    result
}

#[tauri::command]
pub async fn refresh_intelligence_ladder(
    state: State<'_, BackendState>,
    operation_id: Option<String>,
) -> CommandResult<IntelligenceRefreshResultDto> {
    let diagnostics = state.diagnostics().cloned();
    observed_user_command(
        diagnostics,
        "refresh-intelligence-ladder",
        operation_id,
        None,
        |operation_id| async move {
            service(&state)?
                .refresh_with_operation(Some(operation_id))
                .await
                .map(|result| match result {
                    IntelligenceRefreshResult::Started => IntelligenceRefreshResultDto::Started,
                    IntelligenceRefreshResult::AlreadyRunning => {
                        IntelligenceRefreshResultDto::AlreadyRunning
                    }
                    IntelligenceRefreshResult::ScraperUnavailable => {
                        IntelligenceRefreshResultDto::ScraperUnavailable
                    }
                })
                .map_err(Into::into)
        },
    )
    .await
}

#[tauri::command]
pub async fn update_intelligence_auto_fetch_settings(
    state: State<'_, BackendState>,
    settings: IntelligenceAutoFetchSettingsDto,
    operation_id: Option<String>,
) -> CommandResult<IntelligenceLadderSnapshotDto> {
    let diagnostics = state.diagnostics().cloned();
    let audit = OperationAudit {
        target_type: "intelligence-settings",
        target_id: None,
        parent_id: None,
        change_kind: "update",
        field_scope: vec![
            "autoFetchEnabled",
            "autoFetchIntervalMinutes",
            "fetchOnAppStart",
            "fetchOnNetworkRestore",
        ],
    };
    observed_user_command_with_result_audit(
        diagnostics,
        "update-intelligence-auto-fetch-settings",
        operation_id,
        audit.clone(),
        |operation_id| async move {
            let service = service(&state)?;
            let previous = service.snapshot().await?;
            let result = service
                .update_settings_with_operation(settings.into(), Some(operation_id))
                .await
                .map_err(CommandError::from)?;
            let mut fields = Vec::new();
            if previous.settings.enabled != result.settings.enabled {
                fields.push("autoFetchEnabled");
            }
            if previous.settings.interval_minutes != result.settings.interval_minutes {
                fields.push("autoFetchIntervalMinutes");
            }
            if previous.settings.fetch_on_app_start != result.settings.fetch_on_app_start {
                fields.push("fetchOnAppStart");
            }
            if previous.settings.fetch_on_network_restore
                != result.settings.fetch_on_network_restore
            {
                fields.push("fetchOnNetworkRestore");
            }
            Ok((result.into(), audit_with_fields(audit, fields)))
        },
    )
    .await
}

#[tauri::command]
pub async fn notify_intelligence_network_restored(
    state: State<'_, BackendState>,
) -> CommandResult<()> {
    let result = async {
        service(&state)?
            .network_restored()
            .await
            .map_err(Into::into)
    }
    .await;
    record_command_failure(&state, "intelligence-ladder-network-restore", &result);
    result
}

pub(crate) fn publish_intelligence_ladder_changed(
    app: &tauri::AppHandle,
    revision: u64,
    operation_id: Option<&str>,
) {
    if let Some(state) = app.try_state::<BackendState>() {
        if let Some(diagnostics) = state.diagnostics() {
            diagnostics.record_state_change(operation_id, revision, &["intelligence-ladder"]);
        }
    }
    if app
        .emit(
            "relaypulse://state-changed",
            StateChangedDto {
                revision,
                domains: vec!["intelligence-ladder"],
            },
        )
        .is_err()
    {
        crate::desktop::record_desktop_error(
            app,
            "intelligence-ladder-state-change-event",
            "state_change_emit_failed",
        );
    }
}

fn service(state: &BackendState) -> CommandResult<IntelligenceLadderService> {
    if let Some(service) = state.intelligence_ladder_service() {
        return Ok(service.clone());
    }

    if let Some(diagnostics) = state.diagnostics() {
        diagnostics.record_background_error("intelligence-ladder-service", "database_unavailable");
    }
    Err(CommandError::database_unavailable())
}

impl From<IntelligenceAutoFetchSettingsDto> for IntelligenceAutoFetchSettings {
    fn from(value: IntelligenceAutoFetchSettingsDto) -> Self {
        Self {
            enabled: value.enabled,
            interval_minutes: value.interval_minutes,
            fetch_on_app_start: value.fetch_on_app_start,
            fetch_on_network_restore: value.fetch_on_network_restore,
        }
    }
}

impl From<IntelligenceAutoFetchSettings> for IntelligenceAutoFetchSettingsDto {
    fn from(value: IntelligenceAutoFetchSettings) -> Self {
        Self {
            enabled: value.enabled,
            interval_minutes: value.interval_minutes,
            fetch_on_app_start: value.fetch_on_app_start,
            fetch_on_network_restore: value.fetch_on_network_restore,
        }
    }
}

impl From<IntelligenceLadderSnapshot> for IntelligenceLadderSnapshotDto {
    fn from(value: IntelligenceLadderSnapshot) -> Self {
        Self {
            revision: value.revision,
            entries: value
                .entries
                .into_iter()
                .map(|entry| IntelligenceRankingEntryDto {
                    rank: entry.rank,
                    model: entry.model,
                    effort: entry.effort,
                    iq: entry.iq,
                })
                .collect(),
            fetched_at: value.fetched_at,
            is_stale: value.is_stale,
            last_error: value.last_error.map(|error| IntelligenceSafeErrorDto {
                code: error.code,
                message: error.message,
            }),
            runtime: IntelligenceLadderRuntimeDto {
                state: match value.runtime_state {
                    IntelligenceLadderRuntimeState::Idle => IntelligenceRefreshStateDto::Idle,
                    IntelligenceLadderRuntimeState::Running => IntelligenceRefreshStateDto::Running,
                    IntelligenceLadderRuntimeState::Failed => IntelligenceRefreshStateDto::Failed,
                    IntelligenceLadderRuntimeState::ScraperUnavailable => {
                        IntelligenceRefreshStateDto::ScraperUnavailable
                    }
                },
                last_successful_at: value.last_successful_at,
                next_scheduled_at: value.next_scheduled_at,
            },
            settings: value.settings.into(),
            allowed_interval_minutes: value.allowed_interval_minutes,
        }
    }
}

impl From<IntelligenceLadderError> for CommandError {
    fn from(error: IntelligenceLadderError) -> Self {
        match error {
            IntelligenceLadderError::InvalidSettings { field } => Self {
                code: "intelligence_ladder_settings_invalid",
                category: "validation",
                message: "The submitted intelligence ladder settings are invalid.",
                field: Some(field),
                retryable: false,
                operation_id: None,
            },
            IntelligenceLadderError::DatabaseUnavailable => CommandError::database_unavailable(),
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::intelligence_ladder::{IntelligenceRankingEntry, IntelligenceSafeError};

    #[test]
    fn snapshot_serde_matches_the_frontend_contract() {
        let value = serde_json::to_value(IntelligenceLadderSnapshotDto::from(
            IntelligenceLadderSnapshot {
                revision: 7,
                entries: vec![IntelligenceRankingEntry {
                    rank: 1,
                    model: "Model A".to_owned(),
                    effort: "high".to_owned(),
                    iq: 101.5,
                }],
                fetched_at: Some("2026-08-13T10:00:00Z".to_owned()),
                is_stale: true,
                last_error: Some(IntelligenceSafeError {
                    code: "network".to_owned(),
                    message: "最近一次获取失败，当前显示上次成功数据。".to_owned(),
                }),
                runtime_state: IntelligenceLadderRuntimeState::Failed,
                last_successful_at: Some("2026-08-13T10:00:00Z".to_owned()),
                next_scheduled_at: Some("2026-08-13T11:00:00Z".to_owned()),
                settings: IntelligenceAutoFetchSettings {
                    enabled: true,
                    interval_minutes: 60,
                    fetch_on_app_start: true,
                    fetch_on_network_restore: false,
                },
                allowed_interval_minutes: vec![30, 60, 180, 360],
            },
        ))
        .expect("snapshot serializes");
        assert_eq!(value["revision"], json!(7));
        assert_eq!(value["runtime"]["state"], json!("failed"));
        assert_eq!(value["settings"]["intervalMinutes"], json!(60));
        assert_eq!(value["allowedIntervalMinutes"], json!([30, 60, 180, 360]));
    }

    #[test]
    fn refresh_results_use_the_frozen_union_shape() {
        assert_eq!(
            serde_json::to_value(IntelligenceRefreshResultDto::Started).expect("serializes"),
            json!({ "status": "started" })
        );
        assert_eq!(
            serde_json::to_value(IntelligenceRefreshResultDto::AlreadyRunning).expect("serializes"),
            json!({ "status": "already_running" })
        );
    }
}
