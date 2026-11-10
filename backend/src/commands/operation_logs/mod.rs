//! Strict IPC adapters. Query/write failures never recursively produce history events.
mod args;
use crate::{
    commands::error::CommandError,
    modules::operation_logs::{OperationLogError, OperationLogs},
    startup::StartupState,
};
use std::sync::Arc;
use tauri::{ipc::Request, State};
fn service(state: &StartupState) -> Result<Arc<OperationLogs>, CommandError> {
    match state {
        StartupState::Ready(app) => Ok(app.operation_logs.clone()),
        StartupState::Blocked(_) => Err(OperationLogError::Unavailable.into()),
    }
}
#[tauri::command]
pub(in crate::commands) async fn get_operation_logs(
    state: State<'_, StartupState>,
    request: Request<'_>,
) -> Result<serde_json::Value, CommandError> {
    let query = args::query(request)?;
    let page = service(&state)?.query(query).await?;
    Ok(crate::application::operation_history::wire(
        serde_json::to_value(page).expect("history DTO"),
    ))
}
#[tauri::command]
pub(in crate::commands) async fn get_operation_log_detail(
    state: State<'_, StartupState>,
    request: Request<'_>,
) -> Result<serde_json::Value, CommandError> {
    let id = args::log_id(request)?;
    let detail = service(&state)?.detail(id).await?;
    Ok(crate::application::operation_history::wire(
        serde_json::to_value(detail).expect("history DTO"),
    ))
}
#[tauri::command]
pub(in crate::commands) async fn report_operation_read_failure(
    state: State<'_, StartupState>,
    request: Request<'_>,
) -> Result<(), CommandError> {
    let input = args::read_failure(request)?;
    let service = service(&state)?;
    tauri::async_runtime::spawn_blocking(move || {
        crate::application::operation_history::report_read_failure(
            &service,
            input.target,
            input.kind,
        )
    })
    .await
    .map_err(|_| CommandError::from(OperationLogError::Unavailable))?;
    Ok(())
}
impl From<OperationLogError> for CommandError {
    fn from(error: OperationLogError) -> Self {
        let message = error.to_string();
        let (code, category, field, retryable) = match error {
            OperationLogError::Validation { field, .. } => (
                "operation_logs_validation",
                "validation",
                Some(field),
                false,
            ),
            OperationLogError::NotFound => ("operation_log_not_found", "not_found", None, false),
            OperationLogError::Unavailable => {
                ("operation_logs_unavailable", "unavailable", None, true)
            }
        };
        Self {
            code,
            category,
            message,
            field,
            retryable,
            operation_id: None,
        }
    }
}
#[cfg(test)]
#[path = "../../../tests/integration/commands/operation_logs.rs"]
mod tests;
