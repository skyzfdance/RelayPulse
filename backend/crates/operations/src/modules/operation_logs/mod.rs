//! Generic operation history: supplied action definitions and immutable display snapshots.
mod privacy;
mod query;
mod service;
mod sqlite;
mod types;
pub use privacy::{changes, snapshot_fields};
pub use query::OperationLogQuery;
pub use service::{OperationLogs, OperationRecorder};
pub use sqlite::SCHEMA;
pub use types::*;
#[derive(Debug, thiserror::Error)]
pub enum OperationLogError {
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
#[path = "../../../../../tests/unit/operation_logs/history.rs"]
mod tests;

#[cfg(test)]
#[path = "../../../../../tests/unit/operation_logs/lifecycle.rs"]
mod lifecycle_tests;
