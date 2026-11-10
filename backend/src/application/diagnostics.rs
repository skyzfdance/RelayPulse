//! 协调诊断公开能力与业务操作记录；文件事实归 shared，宿主动作由命令层适配。
use crate::application::action_definitions::actions as OperationAction;
use crate::{
    modules::operation_logs::{
        OperationEvent, OperationLogContent, OperationLogs, OperationResult, OperationSource,
    },
    shared::diagnostics::{Diagnostics, DiagnosticsError, LogStatistics},
};
use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub(crate) enum DiagnosticsOperationError {
    #[error("诊断文件操作失败")]
    Diagnostics(#[from] DiagnosticsError),
    #[error("无法打开系统日志目录")]
    OpenDirectory,
    #[error("无法取得诊断包保存位置")]
    SaveDialog,
    #[error("已取消导出诊断包")]
    Cancelled,
}
type Result<T> = std::result::Result<T, DiagnosticsOperationError>;

pub(crate) fn statistics(
    diagnostics: &Diagnostics,
    logs: Option<&OperationLogs>,
) -> Result<LogStatistics> {
    let result = diagnostics.statistics().map_err(Into::into);
    if result.is_err() {
        record(
            logs,
            OperationAction::PageLoadFailed,
            &result,
            failure("读取日志统计"),
        );
    }
    result
}

pub(crate) fn clear(
    diagnostics: &Diagnostics,
    logs: Option<&OperationLogs>,
) -> Result<LogStatistics> {
    let result = diagnostics.clear().map_err(Into::into);
    let content = match &result {
        Ok(outcome) => OperationLogContent::Cleanup {
            target: "诊断日志".into(),
            scope: "当前环境全部诊断日志".into(),
            count: outcome.removed.file_count,
            space: Some(format!("{} B", outcome.removed.total_bytes)),
        },
        Err(_) => failure("清理日志"),
    };
    record(logs, OperationAction::LogsCleared, &result, content);
    result.map(|outcome| outcome.remaining)
}

pub(crate) fn open_directory(
    diagnostics: &Diagnostics,
    logs: Option<&OperationLogs>,
    open: impl FnOnce(&Path) -> std::result::Result<(), ()>,
) -> Result<()> {
    let result = (|| {
        let path = diagnostics.log_directory()?;
        open(&path).map_err(|()| DiagnosticsOperationError::OpenDirectory)
    })();
    if result.is_err() {
        record(
            logs,
            OperationAction::DetailReadFailed,
            &result,
            failure("打开日志目录"),
        );
    }
    result
}

pub(crate) fn export(
    diagnostics: &Diagnostics,
    logs: Option<&OperationLogs>,
    destination: impl FnOnce() -> std::result::Result<Option<PathBuf>, ()>,
) -> Result<bool> {
    let mut filename = "relaypulse-diagnostics.json".to_string();
    let mut file_size = 0;
    let result = (|| {
        // 选择路径期间不持有诊断锁；取消不读取文件，也不伪装为导出成功。
        let path = destination()
            .map_err(|()| DiagnosticsOperationError::SaveDialog)?
            .ok_or(DiagnosticsOperationError::Cancelled)?;
        filename = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "relaypulse-diagnostics.json".into());
        file_size = diagnostics.export(&path)?;
        Ok(true)
    })();
    let outcome = match &result {
        Ok(_) => "已导出",
        Err(DiagnosticsOperationError::Cancelled) => "已取消",
        Err(_) => "导出失败",
    };
    record(
        logs,
        OperationAction::DiagnosticsExported,
        &result,
        OperationLogContent::Export {
            filename,
            file_size: format!("{file_size} B"),
            outcome: outcome.into(),
        },
    );
    result
}

fn failure(activity: &str) -> OperationLogContent {
    OperationLogContent::Failure {
        activity: activity.into(),
        target: "诊断日志".into(),
        reason: "文件或系统操作未能完成。".into(),
    }
}

fn record<T>(
    logs: Option<&OperationLogs>,
    action: relaypulse_operations::ActionDefinition,
    result: &Result<T>,
    content: OperationLogContent,
) {
    let Some(logs) = logs else {
        return;
    };
    let outcome = match result {
        Ok(_) => OperationResult::Success,
        Err(DiagnosticsOperationError::Cancelled) => OperationResult::Cancelled,
        Err(_) => OperationResult::Failure,
    };
    logs.record_best_effort(OperationEvent::new(
        action,
        OperationSource::User,
        outcome,
        "诊断日志",
        "诊断操作",
        content,
    ));
}

#[cfg(test)]
#[path = "../../tests/integration/diagnostics/operations.rs"]
mod tests;
