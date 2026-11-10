use super::error::CommandError;
use crate::{
    application::startup_recovery::{
        MigrationError, MigrationOutcome, StartupRecovery, StartupStatus,
    },
    desktop::StartupDesktop,
};
use std::sync::Arc;
use tauri::State;

impl From<MigrationError> for CommandError {
    fn from(error: MigrationError) -> Self {
        Self {
            code: error.code,
            category: "startup",
            message: error.message,
            field: None,
            retryable: true,
            operation_id: None,
        }
    }
}
fn worker_failed() -> CommandError {
    MigrationError {
        code: "startup_migration_failed",
        message: "迁移任务意外中断，请重启应用检查恢复状态。".into(),
    }
    .into()
}

#[tauri::command]
pub(super) async fn get_startup_status(
    recovery: State<'_, Arc<StartupRecovery>>,
) -> Result<StartupStatus, CommandError> {
    let recovery = recovery.inner().clone();
    tauri::async_runtime::spawn_blocking(move || recovery.status())
        .await
        .map_err(|_| worker_failed())
}

#[tauri::command]
pub(super) async fn migrate_startup_data(
    recovery: State<'_, Arc<StartupRecovery>>,
) -> Result<MigrationOutcome, CommandError> {
    let recovery = recovery.inner().clone();
    tauri::async_runtime::spawn_blocking(move || recovery.migrate())
        .await
        .map_err(|_| worker_failed())?
        .map_err(Into::into)
}

#[tauri::command]
pub(super) fn restart_after_data_migration(
    recovery: State<'_, Arc<StartupRecovery>>,
    desktop: State<'_, StartupDesktop>,
) -> Result<(), CommandError> {
    if !recovery.can_restart() {
        return Err(MigrationError {
            code: "startup_migration_not_completed",
            message: "数据迁移尚未完成，不能重启进入应用。".into(),
        }
        .into());
    }
    desktop.0.restart();
    Ok(())
}

#[cfg(test)]
#[path = "../../tests/integration/commands/startup.rs"]
mod tests;
