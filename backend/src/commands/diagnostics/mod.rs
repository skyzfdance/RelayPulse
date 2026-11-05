//! 诊断 IPC 适配：取得独立诊断资源、适配宿主动作并转换错误，不编排业务操作记录。
use super::error::CommandError;
use crate::{
    application::diagnostics::{self, DiagnosticsOperationError},
    desktop::diagnostics::DiagnosticsDesktop,
    modules::operation_logs::OperationLogs,
    shared::diagnostics::{Diagnostics, DiagnosticsError, LogStatistics},
    startup::{DiagnosticsState, StartupState},
};
use std::sync::Arc;
use tauri::State;

impl From<DiagnosticsError> for CommandError {
    fn from(error: DiagnosticsError) -> Self {
        let message = match error {
            DiagnosticsError::Io(_) => "无法读写诊断文件。",
            DiagnosticsError::UnsafePath => "诊断文件路径不符合要求。",
            DiagnosticsError::Lock => "诊断资源访问失败。",
            DiagnosticsError::InvalidData(_) => "诊断日志格式损坏，无法生成诊断包。",
        };
        Self {
            code: "diagnostics_failed",
            category: "diagnostics",
            message: message.into(),
            field: None,
            retryable: true,
            operation_id: None,
        }
    }
}

impl From<DiagnosticsOperationError> for CommandError {
    fn from(error: DiagnosticsOperationError) -> Self {
        let (code, category, message, retryable) = match error {
            DiagnosticsOperationError::Diagnostics(error) => return error.into(),
            DiagnosticsOperationError::OpenDirectory => (
                "diagnostics_failed",
                "platform",
                "无法打开系统日志目录。",
                true,
            ),
            DiagnosticsOperationError::SaveDialog => (
                "diagnostics_failed",
                "platform",
                "无法取得诊断包保存位置。",
                true,
            ),
            DiagnosticsOperationError::Cancelled => (
                "diagnostics_export_cancelled",
                "cancelled",
                "已取消导出诊断包。",
                false,
            ),
        };
        Self {
            code,
            category,
            message: message.into(),
            field: None,
            retryable,
            operation_id: None,
        }
    }
}

fn resource(state: &DiagnosticsState) -> Result<Arc<Diagnostics>, CommandError> {
    state.0.clone().ok_or_else(|| CommandError {
        code: "diagnostics_unavailable",
        category: "startup",
        message: "环境目录校验未通过，诊断日志不可用。".into(),
        field: None,
        retryable: false,
        operation_id: None,
    })
}

fn operation_logs(state: &StartupState) -> Option<Arc<OperationLogs>> {
    match state {
        StartupState::Ready(app) => Some(app.operation_logs.clone()),
        StartupState::Blocked(_) => None,
    }
}

async fn blocking<T: Send + 'static>(
    action: impl FnOnce() -> Result<T, DiagnosticsOperationError> + Send + 'static,
) -> Result<T, CommandError> {
    tauri::async_runtime::spawn_blocking(action)
        .await
        .map_err(|_| CommandError {
            code: "diagnostics_task_failed",
            category: "diagnostics",
            message: "诊断操作未能完成。".into(),
            field: None,
            retryable: true,
            operation_id: None,
        })?
        .map_err(Into::into)
}

#[tauri::command]
pub(super) async fn get_log_statistics(
    state: State<'_, DiagnosticsState>,
    application: State<'_, StartupState>,
) -> Result<LogStatistics, CommandError> {
    let diagnostics = resource(&state)?;
    let logs = operation_logs(&application);
    blocking(move || diagnostics::statistics(&diagnostics, logs.as_deref())).await
}

#[tauri::command]
pub(super) async fn clear_diagnostic_logs(
    state: State<'_, DiagnosticsState>,
    application: State<'_, StartupState>,
) -> Result<LogStatistics, CommandError> {
    let diagnostics = resource(&state)?;
    let logs = operation_logs(&application);
    blocking(move || diagnostics::clear(&diagnostics, logs.as_deref())).await
}

#[tauri::command]
pub(super) async fn open_log_directory(
    state: State<'_, DiagnosticsState>,
    application: State<'_, StartupState>,
    desktop: State<'_, DiagnosticsDesktop>,
) -> Result<(), CommandError> {
    let diagnostics = resource(&state)?;
    let platform = desktop.0.clone();
    let logs = operation_logs(&application);
    blocking(move || {
        diagnostics::open_directory(&diagnostics, logs.as_deref(), |path| {
            platform.open_directory(path).map_err(|_| ())
        })
    })
    .await
}

#[tauri::command]
pub(super) async fn export_diagnostics(
    state: State<'_, DiagnosticsState>,
    application: State<'_, StartupState>,
    desktop: State<'_, DiagnosticsDesktop>,
) -> Result<bool, CommandError> {
    let diagnostics = resource(&state)?;
    let platform = desktop.0.clone();
    let logs = operation_logs(&application);
    blocking(move || {
        diagnostics::export(&diagnostics, logs.as_deref(), || {
            platform.save_destination().map_err(|_| ())
        })
    })
    .await
}

#[cfg(test)]
#[path = "../../../tests/integration/commands/diagnostics.rs"]
mod tests;
