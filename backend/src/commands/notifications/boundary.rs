use super::CommandError;
use crate::{
    modules::notifications::{
        NotificationError, NotificationPlatform, NotificationPlatformState, NotificationService,
    },
    startup::StartupState,
};
use serde::de::DeserializeOwned;
use std::sync::Arc;
use tauri::{
    ipc::{InvokeBody, Request},
    AppHandle, Manager, Runtime,
};

pub(super) fn service(state: &StartupState) -> Result<&Arc<NotificationService>, CommandError> {
    match state {
        StartupState::Ready(application) => Ok(&application.notifications),
        StartupState::Blocked(_) => Err(unavailable()),
    }
}

fn unavailable() -> CommandError {
    CommandError {
        code: "backend_unavailable",
        category: "startup",
        message: "通知服务未就绪，请检查启动状态后重试。".into(),
        field: None,
        retryable: false,
        operation_id: None,
    }
}

pub(super) fn parse<T: DeserializeOwned>(
    request: Request<'_>,
    field: &'static str,
) -> Result<T, CommandError> {
    let InvokeBody::Json(value) = request.body() else {
        return Err(CommandError::invalid(field, "通知请求必须是 JSON 对象。"));
    };
    serde_json::from_value(value.clone())
        .map_err(|_| CommandError::invalid(field, "通知请求格式或字段取值不合法，请检查后重试。"))
}

pub(super) async fn platform_call<R: Runtime, T: Send + 'static>(
    state: &StartupState,
    app: &AppHandle<R>,
    operation: impl FnOnce(&NotificationService, &dyn NotificationPlatform) -> Result<T, NotificationError>
        + Send
        + 'static,
) -> Result<T, CommandError> {
    let service = service(state)?.clone();
    let platform = app
        .try_state::<NotificationPlatformState>()
        .ok_or_else(unavailable)?
        .0
        .clone();
    // 系统授权可能等待用户；不阻塞 WebView 主线程或 Tokio 执行线程，也不占用数据库锁。
    tauri::async_runtime::spawn_blocking(move || operation(&service, platform.as_ref()))
        .await
        .map_err(|_| unavailable())?
        .map_err(Into::into)
}
