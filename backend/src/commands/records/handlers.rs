use super::args;
use crate::{
    commands::error::CommandError,
    desktop::directories::DirectoryDesktop,
    modules::monitoring::{
        Monitoring, OpenProbeRecordLogResult, ProbeRecordAssociationLookup, ProbeRecordDetail,
        ProbeRecordListItem, ProbeRecordOperation, ProbeRecordOptions, ProbeRecordRequestContent,
        ProbeRecordResponseContent, RecordError, RecordsPage, TokenStatsPage,
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
fn record_argument_failure(state: &StartupState, operation: ProbeRecordOperation) {
    if let StartupState::Ready(app) = state {
        app.monitoring
            .record_probe_record_boundary_failure(operation);
    }
}
#[tauri::command]
pub(in crate::commands) async fn get_probe_record_options(
    state: State<'_, StartupState>,
    request: Request<'_>,
) -> Result<ProbeRecordOptions, CommandError> {
    args::no_args(request)
        .inspect_err(|_| record_argument_failure(&state, ProbeRecordOperation::Options))?;
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
    let query = args::records(request)
        .inspect_err(|_| record_argument_failure(&state, ProbeRecordOperation::List))?;
    service(&state)?
        .get_probe_records(query)
        .await
        .map_err(Into::into)
}
#[tauri::command]
pub(in crate::commands) async fn get_probe_records_by_ids(
    state: State<'_, StartupState>,
    request: Request<'_>,
) -> Result<ProbeRecordAssociationLookup, CommandError> {
    let ids = args::record_ids(request)
        .inspect_err(|_| record_argument_failure(&state, ProbeRecordOperation::Association))?;
    service(&state)?
        .get_probe_records_by_ids(ids)
        .await
        .map_err(Into::into)
}
#[tauri::command]
pub(in crate::commands) async fn get_probe_token_stats(
    state: State<'_, StartupState>,
    request: Request<'_>,
) -> Result<TokenStatsPage, CommandError> {
    let query = args::tokens(request)
        .inspect_err(|_| record_argument_failure(&state, ProbeRecordOperation::TokenStats))?;
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
    let id = args::record_id(request)
        .inspect_err(|_| record_argument_failure(&state, ProbeRecordOperation::Detail))?;
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
    let id = args::record_id(request)
        .inspect_err(|_| record_argument_failure(&state, ProbeRecordOperation::Request))?;
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
    let id = args::record_id(request)
        .inspect_err(|_| record_argument_failure(&state, ProbeRecordOperation::Response))?;
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
    let id = args::record_id(request)
        .inspect_err(|_| record_argument_failure(&state, ProbeRecordOperation::OpenLog))?;
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
    let id = args::record_id(request)
        .inspect_err(|_| record_argument_failure(&state, ProbeRecordOperation::Delete))?;
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
    args::no_args(request)
        .inspect_err(|_| record_argument_failure(&state, ProbeRecordOperation::Clear))?;
    service(&state)?
        .clear_probe_records()
        .await
        .map_err(Into::into)
}
