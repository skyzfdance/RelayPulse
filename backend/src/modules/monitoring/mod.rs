//! 正式监测业务：手动/周期探测、健康状态和正式记录。
mod activity;
mod health;
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
pub(crate) use activity::ProbeActivity;
pub(crate) use health::{next_due_at, summarize, Health, HealthState};
pub(crate) use records::{
    OpenProbeRecordLogResult, ProbeRecordDetail, ProbeRecordListItem, ProbeRecordOptions,
    ProbeRecordRequestContent, ProbeRecordResponseContent, ProbeRecordsQuery, RecordError,
    RecordsPage, TokenStatsPage, TokenStatsQuery,
};
pub(crate) use service::{Monitoring, MonitoringError};
pub(crate) use sqlite::SCHEMA;
pub(crate) use transitions::{HealthChange, HealthObserver, HealthTransition};
