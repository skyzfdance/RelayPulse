//! 通知配置、模板、测试发送及正式故障/恢复通知。
mod catalog;
mod error;
mod rendering;
mod service;
mod sqlite;
mod types;
mod validation;

pub use catalog::{markers, EventCatalog, RuleDefinition};
pub use error::NotificationError;
pub use service::{DeliveryAttempt, NotificationService, SettingsChange};
pub use sqlite::{SqliteNotificationStore, SCHEMA};
pub use types::*;

pub trait NotificationStore: Send + Sync {
    fn load(&self) -> Result<StoredSettings, NotificationError>;
    fn save_settings(
        &self,
        settings: &NotificationSettings,
    ) -> Result<StoredSettings, NotificationError>;
    fn save_template(
        &self,
        id: &str,
        custom: Option<&TemplateUpdate>,
    ) -> Result<StoredSettings, NotificationError>;
}

/// desktop 提供真实平台适配，测试注入替身。成功发送仅表示系统 API 接受请求。
pub trait NotificationPlatform: Send + Sync {
    fn permission(&self) -> Result<NotificationPermission, NotificationError>;
    fn request_permission(&self) -> Result<NotificationPermission, NotificationError>;
    fn send(
        &self,
        title: &str,
        body: &str,
        action: Option<NotificationAction>,
    ) -> Result<(), NotificationError>;
    fn open_settings(&self) -> Result<(), NotificationError>;
}

pub struct NotificationPlatformState(pub std::sync::Arc<dyn NotificationPlatform>);

#[cfg(test)]
#[path = "../../../../../tests/unit/notifications/mod.rs"]
pub mod tests;
