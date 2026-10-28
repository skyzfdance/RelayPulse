//! 通知 IPC 协议边界。平台及业务错误在本目录转换，公共 error.rs 不依赖业务。
mod boundary;
mod error;

use super::error::CommandError;
use crate::{modules::notifications::*, startup::StartupState};
use boundary::{parse, platform_call, service};
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
    platform_call(&state, &app, |service, platform| service.load(platform)).await
}

#[tauri::command]
pub(super) async fn update_notification_settings<R: Runtime>(
    state: State<'_, StartupState>,
    app: AppHandle<R>,
    request: Request<'_>,
) -> Result<NotificationSettings, CommandError> {
    let args: SettingsArgs = parse(request, "settings")?;
    platform_call(&state, &app, move |service, platform| {
        service.save(args.settings, platform)
    })
    .await
}

#[tauri::command]
pub(super) async fn request_notification_permission<R: Runtime>(
    state: State<'_, StartupState>,
    app: AppHandle<R>,
) -> Result<NotificationPermission, CommandError> {
    platform_call(&state, &app, |service, platform| {
        service.request_permission(platform)
    })
    .await
}

#[tauri::command]
pub(super) async fn send_test_notification<R: Runtime>(
    state: State<'_, StartupState>,
    app: AppHandle<R>,
    request: Request<'_>,
) -> Result<(), CommandError> {
    let args: TestArgs = parse(request, "template")?;
    platform_call(&state, &app, move |service, platform| {
        service.send_test(args.template, platform)
    })
    .await
}

#[tauri::command]
pub(super) async fn open_notification_settings<R: Runtime>(
    state: State<'_, StartupState>,
    app: AppHandle<R>,
) -> Result<(), CommandError> {
    platform_call(&state, &app, |service, platform| {
        service.open_settings(platform)
    })
    .await
}

#[tauri::command]
pub(super) fn get_notification_template(
    state: State<'_, StartupState>,
    request: Request<'_>,
) -> Result<TemplateDetail, CommandError> {
    let args: TemplateArgs = parse(request, "id")?;
    service(&state)?.template(&args.id).map_err(Into::into)
}

#[tauri::command]
pub(super) fn update_notification_template(
    state: State<'_, StartupState>,
    request: Request<'_>,
) -> Result<TemplateDetail, CommandError> {
    let args: UpdateArgs = parse(request, "update")?;
    service(&state)?
        .save_template(&args.id, args.update)
        .map_err(Into::into)
}

#[tauri::command]
pub(super) fn get_notification_markers() -> Vec<NotificationMarker> {
    markers()
}

#[tauri::command]
pub(super) fn get_notification_variables() -> Vec<NotificationVariable> {
    variables()
}

#[cfg(test)]
#[path = "../../../tests/integration/commands/notifications.rs"]
mod tests;
