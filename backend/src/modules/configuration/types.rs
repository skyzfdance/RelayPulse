use serde::{Deserialize, Serialize};

use super::ConfigurationError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum ThemeMode {
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Language {
    #[serde(rename = "zh-CN")]
    ZhCn,
}

/// 与 General API 契约一致：读取、全量保存和保存结果共用这六个字段。
/// 不提供字段默认反序列化，避免漏传字段被静默补齐后覆盖已有设置。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct GeneralSettings {
    pub launch_at_startup: bool,
    pub continue_in_background: bool,
    pub show_tray_icon: bool,
    pub theme: ThemeMode,
    pub language: Language,
    pub history_retention_days: u16,
}

impl GeneralSettings {
    pub(super) fn validate(&self) -> Result<(), ConfigurationError> {
        if !(1..=3_650).contains(&self.history_retention_days) {
            return Err(ConfigurationError::InvalidInput {
                field: "history_retention_days",
            });
        }
        Ok(())
    }
}
