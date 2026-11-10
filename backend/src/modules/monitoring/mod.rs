//! 正式监测业务：手动/周期探测、健康状态和正式记录。
mod activity;
mod health;
mod operation_events;
mod records;
mod scheduler;
mod service;
mod sqlite;
#[cfg(test)]
#[path = "../../../tests/unit/monitoring/persistence.rs"]
mod sqlite_tests;
#[cfg(test)]
#[path = "../../../tests/unit/monitoring/health.rs"]
mod tests;
mod transitions;
pub use activity::ProbeActivity;
pub use health::{next_due_at, summarize, Health, HealthState};
pub use records::{
    OpenProbeRecordLogResult, ProbeRecordAssociationLookup, ProbeRecordDetail, ProbeRecordListItem,
    ProbeRecordOperation, ProbeRecordOptions, ProbeRecordRequestContent,
    ProbeRecordResponseContent, ProbeRecordsQuery, RecordError, RecordsPage, TokenStatsPage,
    TokenStatsQuery,
};
pub use service::{Monitoring, MonitoringError};
pub use sqlite::SCHEMA;
pub use transitions::{HealthChange, HealthObserver, HealthTransition};

pub mod notification_definitions;

mod actions;

pub use records::verify_capture_history;

pub use records::inspect_capture_formats;

pub mod operation_history;
