use super::CommandError;
use crate::application::notifications::audit::{self, NotificationBoundaryFailure};
use crate::{
    modules::notifications::{
        NotificationError, NotificationPlatform, NotificationPlatformState, NotificationService,
    },
    startup::StartupState,
};
use serde::de::DeserializeOwned;
use std::sync::Arc;
use tauri::{ipc::Request, AppHandle, Manager, Runtime};

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
    state: &StartupState,
    action: relaypulse_operations::ActionDefinition,
) -> Result<T, CommandError> {
    let result = crate::commands::json_input::decode(request).map_err(|error| {
        let message = match error {
            crate::commands::json_input::DecodeError::Body => "通知请求必须是 JSON 对象。",
            crate::commands::json_input::DecodeError::Fields => {
                "通知请求格式或字段取值不合法，请检查后重试。"
            }
        };
        CommandError::invalid(field, message)
    });
    if result.is_err() {
        record_failure(state, action, NotificationBoundaryFailure::InvalidRequest);
    }
    result
}
fn record_failure(
    state: &StartupState,
    action: relaypulse_operations::ActionDefinition,
    failure: NotificationBoundaryFailure,
) {
    if let StartupState::Ready(app) = state {
        audit::boundary_failure(&app.operation_logs, action, failure);
    }
}

pub(super) async fn platform_call<R: Runtime, T: Send + 'static>(
    state: &StartupState,
    app: &AppHandle<R>,
    action: relaypulse_operations::ActionDefinition,
    operation: impl FnOnce(
            &NotificationService,
            &crate::modules::operation_logs::OperationLogs,
            &dyn NotificationPlatform,
        ) -> Result<T, NotificationError>
        + Send
        + 'static,
) -> Result<T, CommandError> {
    let service = service(state)?.clone();
    let logs = match state {
        StartupState::Ready(app) => app.operation_logs.clone(),
        _ => return Err(unavailable()),
    };
    let platform = app
        .try_state::<NotificationPlatformState>()
        .ok_or_else(|| {
            record_failure(
                state,
                action,
                NotificationBoundaryFailure::PlatformUnavailable,
            );
            unavailable()
        })?
        .0
        .clone();
    // 系统授权可能等待用户；不阻塞 WebView 主线程或 Tokio 执行线程，也不占用数据库锁。
    tauri::async_runtime::spawn_blocking(move || operation(&service, &logs, platform.as_ref()))
        .await
        .map_err(|_| {
            record_failure(state, action, NotificationBoundaryFailure::TaskFailed);
            unavailable()
        })?
        .map_err(Into::into)
}

pub(super) fn logs(
    state: &StartupState,
) -> Result<&crate::modules::operation_logs::OperationLogs, CommandError> {
    match state {
        StartupState::Ready(app) => Ok(&app.operation_logs),
        _ => Err(unavailable()),
    }
}
