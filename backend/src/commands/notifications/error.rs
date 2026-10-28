use super::CommandError;
use crate::modules::notifications::{NotificationError, NotificationPermission};

impl From<NotificationError> for CommandError {
    fn from(error: NotificationError) -> Self {
        use NotificationError::*;
        if let InvalidInput { field, message } = error {
            return CommandError::invalid(field, message);
        }
        let (code, category, message, retryable) = match error {
            RuleNotFound => (
                "notification_rule_not_found",
                "validation",
                "未找到通知规则，请重新加载设置。",
                false,
            ),
            CorruptSettings => (
                "notification_configuration_corrupt",
                "storage",
                "本地通知配置缺失或损坏，请检查数据库。",
                false,
            ),
            Storage(_) => (
                "database_unavailable",
                "storage",
                "无法读写本地通知配置，请检查数据库后重试。",
                true,
            ),
            Disabled => (
                "notifications_disabled",
                "notification",
                "消息通知已关闭，请先启用消息通知。",
                false,
            ),
            Permission(NotificationPermission::Denied) => (
                "notification_permission_denied",
                "permission",
                "系统通知授权已被拒绝，请到系统设置中开启。",
                false,
            ),
            Permission(NotificationPermission::NotRequested) => (
                "notification_permission_not_requested",
                "permission",
                "请先请求系统通知授权。",
                false,
            ),
            Permission(NotificationPermission::Unsupported) => (
                "notification_unsupported",
                "platform",
                "当前平台尚不支持系统通知。",
                false,
            ),
            Permission(_) => (
                "notification_permission_unknown",
                "permission",
                "无法确认系统通知授权，请重新查询或请求授权。",
                true,
            ),
            ApplicationIdentity => (
                "notification_application_identity_unavailable",
                "platform",
                "系统通知需要有效的应用身份，请通过 pnpm dev:desktop 或已安装的应用启动。",
                false,
            ),
            Platform("permission_query") => (
                "notification_permission_query_failed",
                "platform",
                "无法读取系统通知授权，请稍后重试。",
                true,
            ),
            Platform("permission_request") => (
                "notification_permission_request_failed",
                "platform",
                "请求系统通知授权失败，请稍后重试。",
                true,
            ),
            Platform("open_settings") => (
                "notification_settings_open_failed",
                "platform",
                "无法打开系统通知设置，请手动打开系统设置中的通知页面。",
                true,
            ),
            Platform(_) => (
                "notification_send_failed",
                "platform",
                "系统未接受测试通知，请检查系统通知设置后重试。",
                true,
            ),
            InvalidInput { .. } => unreachable!(),
        };
        CommandError {
            code,
            category,
            message: message.into(),
            field: None,
            retryable,
            operation_id: None,
        }
    }
}
