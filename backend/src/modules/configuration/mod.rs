//! 通用设置的类型、校验和读写用例。SQL 和建表定义留在模块内部。
mod service;
mod sqlite;
mod types;

pub(crate) use service::{ConfigurationError, ConfigurationService};
pub(crate) use sqlite::{SqliteSettingsStore, SCHEMA};
pub(crate) use types::{GeneralSettings, Language, ThemeMode};

use std::error::Error;

type StorageError = Box<dyn Error + Send + Sync>;

/// 保存应原子覆盖整份设置，返回该次提交的结果，不能读到另一次并发保存。
pub(crate) trait SettingsStore: Send + Sync {
    fn load(&self) -> Result<GeneralSettings, StorageError>;
    fn save(&self, settings: &GeneralSettings) -> Result<GeneralSettings, StorageError>;
}

#[cfg(test)]
#[path = "../../../tests/unit/configuration.rs"]
mod tests;
