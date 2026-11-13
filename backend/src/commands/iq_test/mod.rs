//! IQ 命令只适配已审定的参数封装、直接 DTO 和公共错误封套。
mod error;
use crate::{
    commands::error::CommandError, desktop::directories::DirectoryDesktop, modules::iq_test::*,
    startup::StartupState,
};
use serde::{de::DeserializeOwned, Deserialize};
use std::sync::Arc;
use tauri::{ipc::Request, State};
fn service(state: &StartupState) -> Result<Arc<IqTest>, CommandError> {
    match state {
        StartupState::Ready(app) => Ok(app.iq_test.clone()),
        StartupState::Blocked(_) => Err(IqError::Unavailable.into()),
    }
}
fn args<T: DeserializeOwned>(request: Request<'_>, field: &'static str) -> Result<T, CommandError> {
    crate::commands::json_input::decode(request).map_err(|error| {
        let message = match error {
            crate::commands::json_input::DecodeError::Body => "IQ 请求必须是 JSON 对象。",
            crate::commands::json_input::DecodeError::Fields => {
                "IQ 请求字段缺失、类型错误或含未知字段。"
            }
        };
        CommandError::invalid(field, message)
    })
}
fn no_args(request: Request<'_>) -> Result<(), CommandError> {
    crate::commands::json_input::no_args(request)
        .map_err(|_| CommandError::invalid("request", "此 IQ 命令不接受参数。"))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Id {
    id: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input<T> {
    input: T,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SaveQuestion {
    input: IqQuestionInput,
    id: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Enabled {
    id: String,
    enabled: bool,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SnapshotId {
    snapshot_id: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Ids {
    ids: Vec<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct EntryId {
    entry_id: String,
    attempt: Option<u32>,
}

#[tauri::command]
pub(super) async fn get_iq_test_snapshot(
    state: State<'_, StartupState>,
    request: Request<'_>,
) -> Result<IqSnapshot, CommandError> {
    no_args(request)?;
    service(&state)?.snapshot().await.map_err(Into::into)
}
#[tauri::command]
pub(super) async fn save_iq_question(
    state: State<'_, StartupState>,
    request: Request<'_>,
) -> Result<IqQuestion, CommandError> {
    let SaveQuestion { input, id } = args(request, "input")?;
    service(&state)?
        .save_question(input, id)
        .await
        .map_err(Into::into)
}
#[tauri::command]
pub(super) async fn delete_iq_question(
    state: State<'_, StartupState>,
    request: Request<'_>,
) -> Result<(), CommandError> {
    let Id { id } = args(request, "id")?;
    service(&state)?
        .delete_question(id)
        .await
        .map_err(Into::into)
}
#[tauri::command]
pub(super) async fn set_iq_question_enabled(
    state: State<'_, StartupState>,
    request: Request<'_>,
) -> Result<(), CommandError> {
    let Enabled { id, enabled } = args(request, "id")?;
    service(&state)?
        .set_question_enabled(id, enabled)
        .await
        .map_err(Into::into)
}
#[tauri::command]
pub(super) async fn create_iq_participant_snapshot(
    state: State<'_, StartupState>,
    request: Request<'_>,
) -> Result<IqParticipant, CommandError> {
    let Input { input } = args(request, "input")?;
    service(&state)?
        .create_participant(input)
        .await
        .map_err(Into::into)
}
#[tauri::command]
pub(super) async fn refresh_iq_participant_snapshot(
    state: State<'_, StartupState>,
    request: Request<'_>,
) -> Result<IqParticipant, CommandError> {
    let Id { id } = args(request, "id")?;
    service(&state)?
        .refresh_participant(id)
        .await
        .map_err(Into::into)
}
#[tauri::command]
pub(super) async fn run_iq_precheck(
    state: State<'_, StartupState>,
    request: Request<'_>,
) -> Result<IqPrecheckResult, CommandError> {
    let SnapshotId { snapshot_id } = args(request, "snapshotId")?;
    service(&state)?
        .precheck(snapshot_id)
        .await
        .map_err(Into::into)
}
#[tauri::command]
pub(super) async fn remove_iq_participant(
    state: State<'_, StartupState>,
    request: Request<'_>,
) -> Result<(), CommandError> {
    let Id { id } = args(request, "id")?;
    service(&state)?
        .remove_participant(id)
        .await
        .map_err(Into::into)
}
#[tauri::command]
pub(super) async fn start_iq_run(
    state: State<'_, StartupState>,
    request: Request<'_>,
) -> Result<IqRun, CommandError> {
    let Input { input } = args(request, "input")?;
    service(&state)?.start_run(input).await.map_err(Into::into)
}
#[tauri::command]
pub(super) async fn abort_iq_run(
    state: State<'_, StartupState>,
    request: Request<'_>,
) -> Result<(), CommandError> {
    no_args(request)?;
    service(&state)?.abort_run().await.map_err(Into::into)
}
#[tauri::command]
pub(super) async fn get_iq_history_filter_options(
    state: State<'_, StartupState>,
    request: Request<'_>,
) -> Result<Vec<IqHistoryFilterOption>, CommandError> {
    no_args(request)?;
    service(&state)?.history_options().await.map_err(Into::into)
}
#[tauri::command]
pub(super) async fn get_iq_run_details(
    state: State<'_, StartupState>,
    request: Request<'_>,
) -> Result<IqRun, CommandError> {
    let Id { id } = args(request, "id")?;
    service(&state)?.run_details(id).await.map_err(Into::into)
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct EntryOutput {
    entry_id: String,
    attempt: Option<u32>,
}
#[tauri::command]
pub(super) async fn get_iq_entry_output(
    state: State<'_, StartupState>,
    request: Request<'_>,
) -> Result<IqEntryOutput, CommandError> {
    let EntryOutput { entry_id, attempt } = args(request, "entryId")?;
    service(&state)?
        .entry_output(entry_id, attempt)
        .await
        .map_err(Into::into)
}
#[tauri::command]
pub(super) async fn delete_iq_runs(
    state: State<'_, StartupState>,
    request: Request<'_>,
) -> Result<(), CommandError> {
    let Ids { ids } = args(request, "ids")?;
    service(&state)?.delete_runs(ids).await.map_err(Into::into)
}
#[tauri::command]
pub(super) async fn open_iq_raw_stream(
    state: State<'_, StartupState>,
    desktop: State<'_, DirectoryDesktop>,
    request: Request<'_>,
) -> Result<(), CommandError> {
    let EntryId { entry_id, attempt } = args(request, "entryId")?;
    let platform = desktop.0.clone();
    service(&state)?
        .open_raw(entry_id, attempt, move |path| {
            platform.open(path).map_err(|_| IqError::OpenFailed)
        })
        .await
        .map_err(Into::into)
}

#[cfg(test)]
#[path = "../../../tests/integration/commands/iq_test.rs"]
mod tests;
