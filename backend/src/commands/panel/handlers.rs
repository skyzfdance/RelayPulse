use crate::{
    application::snapshots::panel::{Panel, PanelSnapshot},
    commands::error::CommandError,
    startup::StartupState,
};
use tauri::{
    ipc::{InvokeBody, Request},
    State,
};

fn no_args(request: Request<'_>) -> Result<(), CommandError> {
    match request.body() {
        InvokeBody::Json(serde_json::Value::Null) => Ok(()),
        InvokeBody::Json(serde_json::Value::Object(value)) if value.is_empty() => Ok(()),
        _ => Err(CommandError::invalid(
            "request",
            "小面板命令不接受业务参数。",
        )),
    }
}
fn service(state: &StartupState) -> Result<&Panel, CommandError> {
    match state {
        StartupState::Ready(app) => Ok(&app.panel),
        StartupState::Blocked(_) => Err(CommandError {
            code: "backend_unavailable",
            category: "startup",
            message: "小面板服务未就绪，请检查启动状态。".into(),
            field: None,
            retryable: false,
            operation_id: None,
        }),
    }
}
#[tauri::command]
pub(in crate::commands) async fn get_panel_snapshot(
    state: State<'_, StartupState>,
    request: Request<'_>,
) -> Result<PanelSnapshot, CommandError> {
    no_args(request)?;
    service(&state)?.snapshot().await.map_err(Into::into)
}
#[tauri::command]
pub(in crate::commands) async fn refresh_panel(
    state: State<'_, StartupState>,
    request: Request<'_>,
) -> Result<PanelSnapshot, CommandError> {
    no_args(request)?;
    service(&state)?.refresh().await.map_err(Into::into)
}
