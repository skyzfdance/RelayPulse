//! 独立业务操作历史。快照先脱敏再同步落库；写入失败不改变业务结果，也不记录自身错误。
mod privacy;
mod query;
mod service;
mod sqlite;
mod startup;
mod types;

pub(crate) use privacy::{changes, snapshot_fields};
pub(crate) use query::OperationLogQuery;
pub(crate) use service::{OperationLogs, OperationRecorder, ReadFailureKind, ReadFailureTarget};
pub(crate) use sqlite::SCHEMA;
pub(crate) use startup::{defer_startup_failure, StartupFailureStage};
pub(crate) use types::*;

#[derive(Debug, thiserror::Error)]
pub(crate) enum OperationLogError {
    #[error("{message}")]
    Validation {
        field: &'static str,
        message: &'static str,
    },
    #[error("这条操作记录已不存在或已过期。")]
    NotFound,
    #[error("暂时无法读取操作记录，请重试。")]
    Unavailable,
}

#[cfg(test)]
#[path = "../../../tests/unit/operation_logs/history.rs"]
mod tests;
