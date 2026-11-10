#![allow(non_upper_case_globals)]
use relaypulse_operations::ActionDefinition;
pub const AutomaticNotification: ActionDefinition =
    ActionDefinition::new("automatic_notification", "自动通知", "notification");
pub const NotificationSettingsUpdated: ActionDefinition =
    ActionDefinition::new("notification_settings_updated", "修改通知设置", "update");
pub const NotificationTemplateUpdated: ActionDefinition =
    ActionDefinition::new("notification_template_updated", "修改通知模板", "update");
pub const TestNotification: ActionDefinition =
    ActionDefinition::new("test_notification", "测试通知", "notification");
pub const NotificationPermissionRequested: ActionDefinition = ActionDefinition::new(
    "notification_permission_requested",
    "请求通知授权",
    "authorization",
);
pub use crate::application::action_definitions::actions::{DetailReadFailed, PageLoadFailed};
