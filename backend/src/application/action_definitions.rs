use crate::modules::operation_logs::ActionDefinition;
#[allow(non_upper_case_globals)]
pub(crate) mod actions {
    use super::ActionDefinition;
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
    pub use crate::application::notifications::actions::{
        NotificationPermissionRequested, NotificationSettingsUpdated, NotificationTemplateUpdated,
        TestNotification,
    };
    pub const LogsCleared: ActionDefinition =
        ActionDefinition::new("logs_cleared", "清理日志", "cleanup");
    pub const DiagnosticsExported: ActionDefinition =
        ActionDefinition::new("diagnostics_exported", "导出诊断包", "export");
    pub const PageLoadFailed: ActionDefinition =
        ActionDefinition::new("page_load_failed", "页面加载失败", "read");
    pub const DetailReadFailed: ActionDefinition =
        ActionDefinition::new("detail_read_failed", "详情读取失败", "read");
}
