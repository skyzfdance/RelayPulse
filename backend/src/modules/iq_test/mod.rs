//! IQ 题库、独立执行快照、运行编排和永久证据；不调用正式监测入口。
mod evidence;
mod history;
pub use history::{inspect_evidence_formats, verify_history};
mod replay;
mod runner;
pub use replay::IqEntryOutput;
mod service;
mod sqlite;
mod tasks;
mod types;
mod validation;

pub use service::{CandidateHealth, CompletionObserver, IqTest};
pub use sqlite::SCHEMA;
pub use types::*;

#[derive(Debug, thiserror::Error)]
pub enum IqError {
    #[error("{message}")]
    Validation {
        field: &'static str,
        message: &'static str,
    },
    #[error("IQ 题目、快照或运行记录不存在，请刷新后重试。")]
    NotFound,
    #[error("已有 IQ 测试正在运行，请等待结束或中止。")]
    AlreadyRunning,
    #[error("应用正在退出，IQ 任务已取消。")]
    Closing,
    #[error("IQ 本地数据不可用，请检查存储后重试。")]
    Storage,
    #[error("IQ 证据文件操作失败，已保留现有记录及文件。")]
    Evidence,
    #[error("IQ 记录已删除，隔离文件尚未清理；请重试此次删除。")]
    CleanupPending,
    #[error("IQ 删除尚未提交，隔离目录需要恢复；请重试或重新启动应用。")]
    RecoveryRequired,
    #[error("IQ 记录目录无法由系统打开，请重试。")]
    OpenFailed,
    #[error("IQ 后台任务未能完成，请重试。")]
    Worker,
    #[error("IQ 服务尚未就绪，请检查应用启动状态后重试。")]
    Unavailable,
    #[error(transparent)]
    Source(#[from] crate::modules::sites::SiteConfigError),
}
impl From<crate::shared::persistence::PersistenceError> for IqError {
    fn from(_: crate::shared::persistence::PersistenceError) -> Self {
        Self::Storage
    }
}
impl From<rusqlite::Error> for IqError {
    fn from(_: rusqlite::Error) -> Self {
        Self::Storage
    }
}
impl From<std::io::Error> for IqError {
    fn from(_: std::io::Error) -> Self {
        Self::Evidence
    }
}
impl From<crate::shared::file_output::filesystem::FileError> for IqError {
    fn from(_: crate::shared::file_output::filesystem::FileError) -> Self {
        Self::Evidence
    }
}
type Result<T> = std::result::Result<T, IqError>;
fn invalid(field: &'static str, message: &'static str) -> IqError {
    IqError::Validation { field, message }
}
pub(super) fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(i64::MAX as u128) as u64
}

pub mod notification_definitions;

mod actions;
