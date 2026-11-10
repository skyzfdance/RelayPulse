//! Product use-case audit. Capability reads performed elsewhere have no page semantics.
use super::actions as OperationAction;
use crate::modules::{notifications::*, operation_logs::*};

pub(crate) enum NotificationBoundaryFailure {
    InvalidRequest,
    PlatformUnavailable,
    TaskFailed,
}
pub(crate) fn boundary_failure(
    logs: &OperationLogs,
    action: ActionDefinition,
    failure: NotificationBoundaryFailure,
) {
    let reason = match failure {
        NotificationBoundaryFailure::InvalidRequest => "通知请求字段缺失、类型错误或含未知字段。",
        NotificationBoundaryFailure::PlatformUnavailable => "系统通知平台尚未就绪。",
        NotificationBoundaryFailure::TaskFailed => "系统通知任务未能完成。",
    };
    logs.record_best_effort(OperationEvent::new(
        action,
        OperationSource::User,
        OperationResult::Failure,
        "通知设置",
        "通知操作未完成",
        OperationLogContent::Failure {
            activity: action.label().into(),
            target: "通知设置".into(),
            reason: reason.into(),
        },
    ));
}
fn log_result<T>(
    logs: &OperationLogs,
    action: ActionDefinition,
    target: &str,
    content: OperationLogContent,
    result: &Result<T, NotificationError>,
) {
    let (outcome, content) = match result {
        Ok(_) => (OperationResult::Success, content),
        Err(error) => (
            notification_result(error),
            OperationLogContent::Failure {
                activity: "通知操作".into(),
                target: target.into(),
                reason: notification_reason(error).into(),
            },
        ),
    };
    logs.record_best_effort(OperationEvent::new(
        action,
        OperationSource::User,
        outcome,
        target,
        if result.is_ok() {
            "已完成通知操作"
        } else {
            "通知操作未完成"
        },
        content,
    ));
}
pub(crate) fn load(
    service: &NotificationService,
    logs: &OperationLogs,
    platform: &dyn NotificationPlatform,
) -> Result<NotificationSettings, NotificationError> {
    let result = service.load(platform);
    if result.is_err() {
        log_result(
            logs,
            OperationAction::PageLoadFailed,
            "通知设置",
            OperationLogContent::Snapshot { fields: vec![] },
            &result,
        );
    }
    result
}
pub(crate) fn save(
    service: &NotificationService,
    logs: &OperationLogs,
    input: NotificationSettings,
    platform: &dyn NotificationPlatform,
) -> Result<NotificationSettings, NotificationError> {
    let result = service.save_with_snapshot(input, platform);
    let content = OperationLogContent::Changes {
        changes: result
            .as_ref()
            .map(|c| changes(&c.before, &c.after))
            .unwrap_or_default(),
    };
    log_result(
        logs,
        OperationAction::NotificationSettingsUpdated,
        "通知设置",
        content,
        &result,
    );
    result.map(|c| c.after)
}
pub(crate) fn template(
    service: &NotificationService,
    logs: &OperationLogs,
    id: &str,
) -> Result<TemplateDetail, NotificationError> {
    let result = service.template(id);
    if result.is_err() {
        log_result(
            logs,
            OperationAction::DetailReadFailed,
            "通知模板",
            OperationLogContent::Snapshot { fields: vec![] },
            &result,
        );
    }
    result
}
pub(crate) fn save_template(
    service: &NotificationService,
    logs: &OperationLogs,
    id: &str,
    update: TemplateUpdate,
) -> Result<TemplateDetail, NotificationError> {
    let result = service.save_template_with_snapshot(id, update);
    let content = OperationLogContent::Changes {
        changes: result
            .as_ref()
            .map(|c| changes(&c.before, &c.after))
            .unwrap_or_default(),
    };
    log_result(
        logs,
        OperationAction::NotificationTemplateUpdated,
        "通知模板",
        content,
        &result,
    );
    result.map(|c| c.after)
}
pub(crate) fn request_permission(
    service: &NotificationService,
    logs: &OperationLogs,
    platform: &dyn NotificationPlatform,
) -> Result<NotificationPermission, NotificationError> {
    let result = service.request_permission(platform);
    let mut event = OperationEvent::new(
        OperationAction::NotificationPermissionRequested,
        OperationSource::User,
        OperationResult::Success,
        "系统通知授权",
        "已请求系统通知授权",
        OperationLogContent::Snapshot {
            fields: snapshot_fields(&result.as_ref().ok()),
        },
    );
    if let Err(error) = &result {
        event.result = notification_result(error);
        event.content = OperationLogContent::Failure {
            activity: "请求授权".into(),
            target: "系统通知".into(),
            reason: notification_reason(error).into(),
        };
    } else if !matches!(result, Ok(NotificationPermission::Granted)) {
        event.result = OperationResult::Blocked;
    }
    logs.record_best_effort(event);
    result
}
fn record_notification(
    logs: &OperationLogs,
    action: ActionDefinition,
    source: OperationSource,
    trigger: &str,
    title: String,
    body: String,
    result: &Result<(), NotificationError>,
) {
    logs.record_best_effort(OperationEvent::new(
        action,
        source,
        result
            .as_ref()
            .err()
            .map(notification_result)
            .unwrap_or(OperationResult::Success),
        "系统通知",
        if result.is_ok() {
            "通知已提交给系统"
        } else {
            "通知未发送"
        },
        OperationLogContent::Notification {
            trigger: trigger.into(),
            title,
            body,
            send_result: result
                .as_ref()
                .err()
                .map(notification_reason)
                .unwrap_or("系统 API 已接受发送请求")
                .into(),
        },
    ));
}
pub(crate) fn send_test(
    service: &NotificationService,
    logs: &OperationLogs,
    notification: TestNotification,
    platform: &dyn NotificationPlatform,
) -> Result<(), NotificationError> {
    let (title, body) = (notification.title.clone(), notification.body.clone());
    let result = service.send_test(notification, platform);
    record_notification(
        logs,
        OperationAction::TestNotification,
        OperationSource::User,
        "用户测试通知",
        title,
        body,
        &result,
    );
    result
}
pub(crate) fn send_rule(
    service: &NotificationService,
    logs: &OperationLogs,
    source: OperationSource,
    id: &str,
    context: &NotificationContext<'_>,
    platform: &dyn NotificationPlatform,
) -> Result<(), NotificationError> {
    match service.deliver_rule(id, context, platform) {
        Ok(None) => Ok(()),
        Ok(Some(attempt)) => {
            record_notification(
                logs,
                OperationAction::AutomaticNotification,
                source,
                &attempt.trigger,
                attempt.title,
                attempt.body,
                &attempt.result,
            );
            attempt.result
        }
        Err(error) => {
            let reason = notification_reason(&error);
            logs.record_best_effort(OperationEvent::new(
                OperationAction::AutomaticNotification,
                source,
                OperationResult::Failure,
                "自动通知",
                reason,
                OperationLogContent::Failure {
                    activity: "自动通知".into(),
                    target: id.into(),
                    reason: reason.into(),
                },
            ));
            Err(error)
        }
    }
}
pub(crate) fn open_settings(
    service: &NotificationService,
    logs: &OperationLogs,
    platform: &dyn NotificationPlatform,
) -> Result<(), NotificationError> {
    let result = service.open_settings(platform);
    if result.is_err() {
        log_result(
            logs,
            OperationAction::DetailReadFailed,
            "通知系统设置",
            OperationLogContent::Snapshot { fields: vec![] },
            &result,
        );
    }
    result
}

fn notification_result(error: &NotificationError) -> OperationResult {
    match error {
        NotificationError::Disabled
        | NotificationError::Permission(_)
        | NotificationError::ApplicationIdentity => OperationResult::Blocked,
        _ => OperationResult::Failure,
    }
}
fn notification_reason(error: &NotificationError) -> &'static str {
    match error {
        NotificationError::Disabled => "通知已禁用，未尝试发送。",
        NotificationError::Permission(_) => "未取得系统通知授权，未尝试发送。",
        NotificationError::ApplicationIdentity => "应用身份不支持系统通知。",
        NotificationError::Platform(_) => "系统通知平台操作失败。",
        NotificationError::Storage(_) | NotificationError::CorruptSettings => {
            "无法读取或保存通知配置。"
        }
        NotificationError::InvalidInput { .. } | NotificationError::RuleNotFound => {
            "通知配置或模板不合法。"
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/integration/notifications/operation_history.rs"]
mod tests;
