#![allow(non_upper_case_globals)]
use relaypulse_operations::ActionDefinition;
pub const AutomaticProbe: ActionDefinition =
    ActionDefinition::new("automatic_probe", "自动探测", "probe");
pub const ModelFailure: ActionDefinition =
    ActionDefinition::new("model_failure", "模型故障", "status_change");
pub const ModelRecovered: ActionDefinition =
    ActionDefinition::new("model_recovered", "模型恢复", "status_change");
pub const ScheduledRecordCleanup: ActionDefinition =
    ActionDefinition::new("scheduled_record_cleanup", "定时清理记录", "cleanup");
pub const ScheduledLogCleanup: ActionDefinition =
    ActionDefinition::new("scheduled_log_cleanup", "定时清理日志", "cleanup");
pub const ManualProbe: ActionDefinition =
    ActionDefinition::new("manual_probe", "手动探测", "probe");
pub const RecordDeleted: ActionDefinition =
    ActionDefinition::new("record_deleted", "删除探测记录", "delete");
pub const RecordsCleared: ActionDefinition =
    ActionDefinition::new("records_cleared", "清理探测记录", "cleanup");
pub const PageLoadFailed: ActionDefinition =
    ActionDefinition::new("page_load_failed", "页面加载失败", "read");
pub const DetailReadFailed: ActionDefinition =
    ActionDefinition::new("detail_read_failed", "详情读取失败", "read");
