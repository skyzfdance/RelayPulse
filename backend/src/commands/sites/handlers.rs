//! 站点命令处理：解析请求、取得应用服务、调用用例并转换错误。
use super::args::*;
use crate::{
    application::sites::{Operation, Reply, Sites},
    commands::error::CommandError,
    modules::sites::{self, ApplySync, ProtocolOption},
    startup::StartupState,
};
use serde::de::DeserializeOwned;
use tauri::{
    ipc::{InvokeBody, Request},
    State,
};
fn service(state: &StartupState) -> Result<&Sites, CommandError> {
    match state {
        StartupState::Ready(app) => Ok(&app.sites),
        StartupState::Blocked(_) => Err(CommandError {
            code: "backend_unavailable",
            category: "startup",
            message: "站点服务未就绪，请检查启动状态。".into(),
            field: None,
            retryable: false,
            operation_id: None,
        }),
    }
}
fn parse<T: DeserializeOwned>(request: Request<'_>) -> Result<T, CommandError> {
    let InvokeBody::Json(value) = request.body() else {
        return Err(CommandError::invalid("request", "请求必须是 JSON 对象。"));
    };
    serde_json::from_value(value.clone())
        .map_err(|_| CommandError::invalid("request", "站点请求字段缺失、类型错误或含未知字段。"))
}
#[tauri::command]
pub(in crate::commands) fn get_site_protocols() -> Vec<ProtocolOption> {
    sites::protocols()
}
#[tauri::command]
pub(in crate::commands) async fn get_sites(
    state: State<'_, StartupState>,
) -> Result<Reply, CommandError> {
    service(&state)?
        .execute(Operation::List)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub(in crate::commands) async fn get_site(
    state: State<'_, StartupState>,
    request: Request<'_>,
) -> Result<Reply, CommandError> {
    let a: SiteArgs = parse(request)?;
    service(&state)?
        .execute(Operation::Get(a.site_id))
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub(in crate::commands) async fn create_site(
    state: State<'_, StartupState>,
    request: Request<'_>,
) -> Result<Reply, CommandError> {
    let a: CreateArgs = parse(request)?;
    service(&state)?
        .execute(Operation::Create(a.input))
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub(in crate::commands) async fn save_site(
    state: State<'_, StartupState>,
    request: Request<'_>,
) -> Result<Reply, CommandError> {
    let a: SaveArgs = parse(request)?;
    service(&state)?
        .execute(Operation::Save(a.site_id, a.draft))
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub(in crate::commands) async fn update_site_connection(
    state: State<'_, StartupState>,
    request: Request<'_>,
) -> Result<Reply, CommandError> {
    let a: ConnectionArgs = parse(request)?;
    service(&state)?
        .execute(Operation::Connection(a.site_id, a.connection))
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub(in crate::commands) async fn delete_site(
    state: State<'_, StartupState>,
    request: Request<'_>,
) -> Result<Reply, CommandError> {
    let a: SiteArgs = parse(request)?;
    service(&state)?
        .execute(Operation::Delete(a.site_id))
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub(in crate::commands) async fn reorder_sites(
    state: State<'_, StartupState>,
    request: Request<'_>,
) -> Result<Reply, CommandError> {
    let a: ReorderArgs = parse(request)?;
    service(&state)?
        .execute(Operation::Reorder(a.ids))
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub(in crate::commands) async fn set_site_enabled(
    state: State<'_, StartupState>,
    request: Request<'_>,
) -> Result<Reply, CommandError> {
    let a: EnabledArgs = parse(request)?;
    service(&state)?
        .execute(Operation::Enabled(a.site_id, a.enabled))
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub(in crate::commands) async fn add_site_model(
    state: State<'_, StartupState>,
    request: Request<'_>,
) -> Result<Reply, CommandError> {
    let a: AddArgs = parse(request)?;
    service(&state)?
        .execute(Operation::AddModel(a.site_id, a.name))
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub(in crate::commands) async fn delete_site_model(
    state: State<'_, StartupState>,
    request: Request<'_>,
) -> Result<Reply, CommandError> {
    let a: ModelArgs = parse(request)?;
    service(&state)?
        .execute(Operation::DeleteModel(a.site_id, a.model_id))
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub(in crate::commands) async fn set_site_model_enabled(
    state: State<'_, StartupState>,
    request: Request<'_>,
) -> Result<Reply, CommandError> {
    let a: ModelEnabledArgs = parse(request)?;
    service(&state)?
        .execute(Operation::ModelEnabled(a.site_id, a.model_id, a.enabled))
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub(in crate::commands) async fn sync_site_models(
    state: State<'_, StartupState>,
    request: Request<'_>,
) -> Result<Reply, CommandError> {
    let a: SyncArgs = parse(request)?;
    service(&state)?
        .execute(Operation::Sync(a.site_id, a.models))
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub(in crate::commands) async fn apply_site_model_sync(
    state: State<'_, StartupState>,
    request: Request<'_>,
) -> Result<Reply, CommandError> {
    let a: ApplyArgs = parse(request)?;
    service(&state)?
        .execute(Operation::Apply(
            a.site_id,
            ApplySync {
                models: a.models,
                added: a.added,
                removed: a.removed,
            },
        ))
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub(in crate::commands) async fn test_model_probe(
    state: State<'_, StartupState>,
    request: Request<'_>,
) -> Result<Reply, CommandError> {
    let a: TestArgs = parse(request)?;
    service(&state)?
        .execute(Operation::Test(a.site_id, a.model_id, a.request))
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub(in crate::commands) async fn update_site_probe_policy(
    state: State<'_, StartupState>,
    request: Request<'_>,
) -> Result<Reply, CommandError> {
    let a: PolicyArgs = parse(request)?;
    service(&state)?
        .execute(Operation::Policy(
            a.site_id,
            a.model_id,
            a.policy,
            a.custom_request,
        ))
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub(in crate::commands) async fn run_site_probe(
    state: State<'_, StartupState>,
    request: Request<'_>,
) -> Result<Reply, CommandError> {
    let a: RunArgs = parse(request)?;
    service(&state)?
        .execute(Operation::Run(a.site_id, a.model_id))
        .await
        .map_err(Into::into)
}
