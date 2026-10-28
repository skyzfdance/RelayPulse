//! 诊断命令不要求业务数据库 Ready；阻塞文件与平台操作移出 WebView 主线程。
use super::error::CommandError;
use crate::{
    desktop::diagnostics::{DiagnosticsDesktop, DiagnosticsPlatformError},
    shared::diagnostics::{Diagnostics, DiagnosticsError, LogStatistics},
    startup::DiagnosticsState,
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

impl From<DiagnosticsPlatformError> for CommandError {
    fn from(error: DiagnosticsPlatformError) -> Self {
        Self {
            code: "diagnostics_failed",
            category: "platform",
            message: match error {
                DiagnosticsPlatformError::OpenDirectory => "无法打开系统日志目录。",
                DiagnosticsPlatformError::SaveDialog => "无法取得诊断包保存位置。",
            }
            .into(),
            field: None,
            retryable: true,
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

async fn blocking<T: Send + 'static>(
    action: impl FnOnce() -> Result<T, CommandError> + Send + 'static,
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
}

#[tauri::command]
pub(super) async fn get_log_statistics(
    state: State<'_, DiagnosticsState>,
) -> Result<LogStatistics, CommandError> {
    let diagnostics = resource(&state)?;
    blocking(move || diagnostics.statistics().map_err(Into::into)).await
}

#[tauri::command]
pub(super) async fn clear_diagnostic_logs(
    state: State<'_, DiagnosticsState>,
) -> Result<LogStatistics, CommandError> {
    let diagnostics = resource(&state)?;
    blocking(move || diagnostics.clear().map_err(Into::into)).await
}

#[tauri::command]
pub(super) async fn open_log_directory(
    state: State<'_, DiagnosticsState>,
    desktop: State<'_, DiagnosticsDesktop>,
) -> Result<(), CommandError> {
    let diagnostics = resource(&state)?;
    let platform = desktop.0.clone();
    blocking(move || {
        platform
            .open_directory(&diagnostics.log_directory()?)
            .map_err(Into::into)
    })
    .await
}

#[tauri::command]
pub(super) async fn export_diagnostics(
    state: State<'_, DiagnosticsState>,
    desktop: State<'_, DiagnosticsDesktop>,
) -> Result<bool, CommandError> {
    let diagnostics = resource(&state)?;
    let platform = desktop.0.clone();
    blocking(move || {
        // 对话框期间不占用日志锁；取消返回明确错误供前端提示，实际落盘完成才返回 true。
        let Some(destination) = platform.save_destination()? else {
            return Err(CommandError {
                code: "diagnostics_export_cancelled",
                category: "cancelled",
                message: "已取消导出诊断包。".into(),
                field: None,
                retryable: false,
                operation_id: None,
            });
        };
        diagnostics.export(&destination)?;
        Ok(true)
    })
    .await
}

#[cfg(test)]
#[path = "../../../tests/integration/commands/diagnostics.rs"]
mod tests;
