use super::args;
use crate::{
    commands::error::CommandError,
    desktop::directories::DirectoryDesktop,
    modules::monitoring::{
        Monitoring, OpenProbeRecordLogResult, ProbeRecordDetail, ProbeRecordListItem,
        ProbeRecordOptions, ProbeRecordRequestContent, ProbeRecordResponseContent, RecordError,
        RecordsPage, TokenStatsPage,
    },
    startup::StartupState,
};
use std::sync::Arc;
use tauri::{ipc::Request, State};
fn service(state: &StartupState) -> Result<Arc<Monitoring>, CommandError> {
    match state {
        StartupState::Ready(app) => Ok(app.monitoring.clone()),
        StartupState::Blocked(_) => Err(RecordError::Unavailable.into()),
    }
}
#[tauri::command]
pub(in crate::commands) async fn get_probe_record_options(
    state: State<'_, StartupState>,
    request: Request<'_>,
) -> Result<ProbeRecordOptions, CommandError> {
    args::no_args(request)?;
    service(&state)?
        .get_probe_record_options()
        .await
        .map_err(Into::into)
}
#[tauri::command]
pub(in crate::commands) async fn get_probe_records(
    state: State<'_, StartupState>,
    request: Request<'_>,
) -> Result<RecordsPage<ProbeRecordListItem>, CommandError> {
    let query = args::records(request)?;
    service(&state)?
        .get_probe_records(query)
        .await
        .map_err(Into::into)
}
#[tauri::command]
pub(in crate::commands) async fn get_probe_token_stats(
    state: State<'_, StartupState>,
    request: Request<'_>,
) -> Result<TokenStatsPage, CommandError> {
    let query = args::tokens(request)?;
    service(&state)?
        .get_probe_token_stats(query)
        .await
        .map_err(Into::into)
}
#[tauri::command]
pub(in crate::commands) async fn get_probe_record_detail(
    state: State<'_, StartupState>,
    request: Request<'_>,
) -> Result<ProbeRecordDetail, CommandError> {
    let id = args::record_id(request)?;
    service(&state)?
        .get_probe_record_detail(id)
        .await
        .map_err(Into::into)
}
#[tauri::command]
pub(in crate::commands) async fn get_probe_record_request(
    state: State<'_, StartupState>,
    request: Request<'_>,
) -> Result<ProbeRecordRequestContent, CommandError> {
    let id = args::record_id(request)?;
    service(&state)?
        .get_probe_record_request(id)
        .await
        .map_err(Into::into)
}
#[tauri::command]
pub(in crate::commands) async fn get_probe_record_response(
    state: State<'_, StartupState>,
    request: Request<'_>,
) -> Result<ProbeRecordResponseContent, CommandError> {
    let id = args::record_id(request)?;
    service(&state)?
        .get_probe_record_response(id)
        .await
        .map_err(Into::into)
}
#[tauri::command]
pub(in crate::commands) async fn open_probe_record_log_directory(
    state: State<'_, StartupState>,
    desktop: State<'_, DirectoryDesktop>,
    request: Request<'_>,
) -> Result<OpenProbeRecordLogResult, CommandError> {
    let id = args::record_id(request)?;
    let platform = desktop.0.clone();
    service(&state)?
        .open_probe_record_log_directory(id, move |path| {
            platform.open(path).map_err(|_| RecordError::LogOpenFailed)
        })
        .await
        .map_err(Into::into)
}
#[tauri::command]
pub(in crate::commands) async fn delete_probe_record(
    state: State<'_, StartupState>,
    request: Request<'_>,
) -> Result<(), CommandError> {
    let id = args::record_id(request)?;
    service(&state)?
        .delete_probe_record(id)
        .await
        .map_err(Into::into)
}
#[tauri::command]
pub(in crate::commands) async fn clear_probe_records(
    state: State<'_, StartupState>,
    request: Request<'_>,
) -> Result<(), CommandError> {
    args::no_args(request)?;
    service(&state)?
        .clear_probe_records()
        .await
        .map_err(Into::into)
}
