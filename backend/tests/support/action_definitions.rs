#![allow(dead_code)]
use relaypulse_operations::ActionDefinition;
#[allow(non_upper_case_globals)]
pub mod actions {
    use super::ActionDefinition;
    pub const AutomaticProbe: ActionDefinition =
        ActionDefinition::new("automatic_probe", "自动探测", "probe");
    pub const ModelFailure: ActionDefinition =
        ActionDefinition::new("model_failure", "模型故障", "status_change");
    pub const ModelRecovered: ActionDefinition =
        ActionDefinition::new("model_recovered", "模型恢复", "status_change");
    pub const AutomaticNotification: ActionDefinition =
        ActionDefinition::new("automatic_notification", "自动通知", "notification");
    pub const ScheduledRecordCleanup: ActionDefinition =
        ActionDefinition::new("scheduled_record_cleanup", "定时清理记录", "cleanup");
    pub const ScheduledLogCleanup: ActionDefinition =
        ActionDefinition::new("scheduled_log_cleanup", "定时清理日志", "cleanup");
    pub const ApplicationReady: ActionDefinition =
        ActionDefinition::new("application_ready", "应用就绪", "startup");
    pub const ApplicationBlocked: ActionDefinition =
        ActionDefinition::new("application_blocked", "启动受阻", "startup");
    pub const SiteCreated: ActionDefinition =
        ActionDefinition::new("site_created", "新增站点", "create");
    pub const SiteUpdated: ActionDefinition =
        ActionDefinition::new("site_updated", "修改站点", "update");
    pub const SiteDeleted: ActionDefinition =
        ActionDefinition::new("site_deleted", "删除站点", "delete");
    pub const SiteReordered: ActionDefinition =
        ActionDefinition::new("site_reordered", "调整站点排序", "update");
    pub const SiteStatusChanged: ActionDefinition =
        ActionDefinition::new("site_status_changed", "调整站点状态", "update");
    pub const ModelCreated: ActionDefinition =
        ActionDefinition::new("model_created", "新增模型", "create");
    pub const ModelDeleted: ActionDefinition =
        ActionDefinition::new("model_deleted", "删除模型", "delete");
    pub const ModelStatusChanged: ActionDefinition =
        ActionDefinition::new("model_status_changed", "调整模型状态", "update");
    pub const ModelsSynchronized: ActionDefinition =
        ActionDefinition::new("models_synchronized", "同步模型", "sync");
    pub const ProbeConfigSaved: ActionDefinition =
        ActionDefinition::new("probe_config_saved", "保存探测配置", "update");
    pub const TestDraftSaved: ActionDefinition =
        ActionDefinition::new("test_draft_saved", "保存测试草稿", "update");
    pub const ManualProbe: ActionDefinition =
        ActionDefinition::new("manual_probe", "手动探测", "probe");
    pub const GeneralSettingsUpdated: ActionDefinition =
        ActionDefinition::new("general_settings_updated", "修改通用设置", "update");
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
    pub const RecordDeleted: ActionDefinition =
        ActionDefinition::new("record_deleted", "删除探测记录", "delete");
    pub const RecordsCleared: ActionDefinition =
        ActionDefinition::new("records_cleared", "清理探测记录", "cleanup");
    pub const LogsCleared: ActionDefinition =
        ActionDefinition::new("logs_cleared", "清理日志", "cleanup");
    pub const DiagnosticsExported: ActionDefinition =
        ActionDefinition::new("diagnostics_exported", "导出诊断包", "export");
    pub const PageLoadFailed: ActionDefinition =
        ActionDefinition::new("page_load_failed", "页面加载失败", "read");
    pub const DetailReadFailed: ActionDefinition =
        ActionDefinition::new("detail_read_failed", "详情读取失败", "read");
    pub const IqQuestionCreated: ActionDefinition =
        ActionDefinition::new("iq_question_created", "新建 IQ 题目", "create");
    pub const IqQuestionUpdated: ActionDefinition =
        ActionDefinition::new("iq_question_updated", "修改 IQ 题目", "update");
    pub const IqQuestionDeleted: ActionDefinition =
        ActionDefinition::new("iq_question_deleted", "删除 IQ 题目", "delete");
    pub const IqRunStarted: ActionDefinition =
        ActionDefinition::new("iq_run_started", "开始 IQ 测试", "create");
    pub const IqRunAborted: ActionDefinition =
        ActionDefinition::new("iq_run_aborted", "中止 IQ 测试", "status_change");
    pub const IqRunDeleted: ActionDefinition =
        ActionDefinition::new("iq_run_deleted", "删除 IQ 测试历史", "delete");
}
