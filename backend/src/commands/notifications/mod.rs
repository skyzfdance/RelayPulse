//! 通知 IPC 协议边界。平台及业务错误在本目录转换，公共 error.rs 不依赖业务。
use crate::application::action_definitions::actions as OperationAction;
use crate::application::notifications::audit;
mod boundary;
mod error;

use super::error::CommandError;
use crate::{modules::notifications::*, startup::StartupState};
use boundary::{logs, parse, platform_call, service};
use serde::Deserialize;
use tauri::{ipc::Request, AppHandle, Runtime, State};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SettingsArgs {
    settings: NotificationSettings,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TemplateArgs {
    id: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UpdateArgs {
    id: String,
    update: TemplateUpdate,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TestArgs {
    template: TestNotification,
}

#[tauri::command]
pub(super) async fn get_notification_settings<R: Runtime>(
    state: State<'_, StartupState>,
    app: AppHandle<R>,
) -> Result<NotificationSettings, CommandError> {
    platform_call(
        &state,
        &app,
        OperationAction::PageLoadFailed,
        |service, logs, platform| audit::load(service, logs, platform),
    )
    .await
}

#[tauri::command]
pub(super) async fn update_notification_settings<R: Runtime>(
    state: State<'_, StartupState>,
    app: AppHandle<R>,
    request: Request<'_>,
) -> Result<NotificationSettings, CommandError> {
    let args: SettingsArgs = parse(
        request,
        "settings",
        &state,
        OperationAction::NotificationSettingsUpdated,
    )?;
    platform_call(
        &state,
        &app,
        OperationAction::NotificationSettingsUpdated,
        move |service, logs, platform| audit::save(service, logs, args.settings, platform),
    )
    .await
}

#[tauri::command]
pub(super) async fn request_notification_permission<R: Runtime>(
    state: State<'_, StartupState>,
    app: AppHandle<R>,
) -> Result<NotificationPermission, CommandError> {
    platform_call(
        &state,
        &app,
        OperationAction::NotificationPermissionRequested,
        |service, logs, platform| audit::request_permission(service, logs, platform),
    )
    .await
}

#[tauri::command]
pub(super) async fn send_test_notification<R: Runtime>(
    state: State<'_, StartupState>,
    app: AppHandle<R>,
    request: Request<'_>,
) -> Result<(), CommandError> {
    let args: TestArgs = parse(
        request,
        "template",
        &state,
        OperationAction::TestNotification,
    )?;
    platform_call(
        &state,
        &app,
        OperationAction::TestNotification,
        move |service, logs, platform| audit::send_test(service, logs, args.template, platform),
    )
    .await
}

#[tauri::command]
pub(super) async fn open_notification_settings<R: Runtime>(
    state: State<'_, StartupState>,
    app: AppHandle<R>,
) -> Result<(), CommandError> {
    platform_call(
        &state,
        &app,
        OperationAction::DetailReadFailed,
        |service, logs, platform| audit::open_settings(service, logs, platform),
    )
    .await
}

#[tauri::command]
pub(super) fn get_notification_template(
    state: State<'_, StartupState>,
    request: Request<'_>,
) -> Result<TemplateDetail, CommandError> {
    let args: TemplateArgs = parse(request, "id", &state, OperationAction::DetailReadFailed)?;
    audit::template(service(&state)?, logs(&state)?, &args.id).map_err(Into::into)
}

#[tauri::command]
pub(super) fn update_notification_template(
    state: State<'_, StartupState>,
    request: Request<'_>,
) -> Result<TemplateDetail, CommandError> {
    let args: UpdateArgs = parse(
        request,
        "update",
        &state,
        OperationAction::NotificationTemplateUpdated,
    )?;
    audit::save_template(service(&state)?, logs(&state)?, &args.id, args.update).map_err(Into::into)
}

#[tauri::command]
pub(super) fn get_notification_markers() -> Vec<NotificationMarker> {
    markers()
}

#[tauri::command]
pub(super) fn get_notification_variables(
    state: tauri::State<'_, crate::startup::StartupState>,
) -> Vec<NotificationVariable> {
    match state.inner() {
        crate::startup::StartupState::Ready(app) => app.notifications.variables(),
        _ => vec![],
    }
}

#[cfg(test)]
#[path = "../../../tests/integration/commands/notifications.rs"]
mod tests;
