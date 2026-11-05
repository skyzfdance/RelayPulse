use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum OperationSource {
    System,
    User,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum OperationResult {
    Success,
    Failure,
    Cancelled,
    Blocked,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum OperationActionGroup {
    Probe,
    Create,
    Update,
    Delete,
    Sync,
    Cleanup,
    Notification,
    Authorization,
    Export,
    Startup,
    Read,
    StatusChange,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum OperationAction {
    AutomaticProbe,
    ModelFailure,
    ModelRecovered,
    AutomaticNotification,
    ScheduledRecordCleanup,
    ScheduledLogCleanup,
    ApplicationReady,
    ApplicationBlocked,
    SiteCreated,
    SiteUpdated,
    SiteDeleted,
    SiteReordered,
    SiteStatusChanged,
    ModelCreated,
    ModelDeleted,
    ModelStatusChanged,
    ModelsSynchronized,
    ProbeConfigSaved,
    TestDraftSaved,
    ManualProbe,
    GeneralSettingsUpdated,
    NotificationSettingsUpdated,
    NotificationTemplateUpdated,
    TestNotification,
    NotificationPermissionRequested,
    RecordDeleted,
    RecordsCleared,
    LogsCleared,
    DiagnosticsExported,
    PageLoadFailed,
    DetailReadFailed,
}
impl OperationAction {
    pub(crate) fn group(self) -> OperationActionGroup {
        use OperationAction::*;
        use OperationActionGroup as G;
        match self {
            AutomaticProbe | ManualProbe => G::Probe,
            ModelFailure | ModelRecovered => G::StatusChange,
            AutomaticNotification | TestNotification => G::Notification,
            ScheduledRecordCleanup | ScheduledLogCleanup | RecordsCleared | LogsCleared => {
                G::Cleanup
            }
            ApplicationReady | ApplicationBlocked => G::Startup,
            SiteCreated | ModelCreated => G::Create,
            SiteDeleted | ModelDeleted | RecordDeleted => G::Delete,
            ModelsSynchronized => G::Sync,
            NotificationPermissionRequested => G::Authorization,
            DiagnosticsExported => G::Export,
            PageLoadFailed | DetailReadFailed => G::Read,
            _ => G::Update,
        }
    }
    pub(crate) fn label(self) -> &'static str {
        use OperationAction::*;
        match self {
            AutomaticProbe => "自动探测",
            ModelFailure => "模型故障",
            ModelRecovered => "模型恢复",
            AutomaticNotification => "自动通知",
            ScheduledRecordCleanup => "定时清理记录",
            ScheduledLogCleanup => "定时清理日志",
            ApplicationReady => "应用就绪",
            ApplicationBlocked => "启动受阻",
            SiteCreated => "新增站点",
            SiteUpdated => "修改站点",
            SiteDeleted => "删除站点",
            SiteReordered => "调整站点排序",
            SiteStatusChanged => "调整站点状态",
            ModelCreated => "新增模型",
            ModelDeleted => "删除模型",
            ModelStatusChanged => "调整模型状态",
            ModelsSynchronized => "同步模型",
            ProbeConfigSaved => "保存探测配置",
            TestDraftSaved => "保存测试草稿",
            ManualProbe => "手动探测",
            GeneralSettingsUpdated => "修改通用设置",
            NotificationSettingsUpdated => "修改通知设置",
            NotificationTemplateUpdated => "修改通知模板",
            TestNotification => "测试通知",
            NotificationPermissionRequested => "请求通知授权",
            RecordDeleted => "删除探测记录",
            RecordsCleared => "清理探测记录",
            LogsCleared => "清理日志",
            DiagnosticsExported => "导出诊断包",
            PageLoadFailed => "页面加载失败",
            DetailReadFailed => "详情读取失败",
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RecordAvailability {
    Available,
    Expired,
    None,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub(crate) enum DetailBehavior {
    Drawer,
    ProbeRecords {
        record_ids: Vec<String>,
        availability: RecordAvailability,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SnapshotField {
    pub label: String,
    pub value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sensitive: Option<bool>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ChangeField {
    pub field: String,
    pub before: String,
    pub after: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sensitive: Option<bool>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub(crate) enum OperationLogContent {
    Snapshot {
        fields: Vec<SnapshotField>,
    },
    Changes {
        changes: Vec<ChangeField>,
    },
    Deletion {
        summary: String,
        #[serde(serialize_with = "deletion_fields")]
        fields: Vec<SnapshotField>,
    },
    Notification {
        trigger: String,
        title: String,
        body: String,
        send_result: String,
    },
    Cleanup {
        target: String,
        scope: String,
        count: u64,
        #[serde(skip_serializing_if = "Option::is_none")]
        space: Option<String>,
    },
    Export {
        filename: String,
        file_size: String,
        outcome: String,
    },
    Failure {
        activity: String,
        target: String,
        reason: String,
    },
    ProbeFailure {
        reason: String,
    },
}
fn deletion_fields<S: serde::Serializer>(
    fields: &[SnapshotField],
    serializer: S,
) -> Result<S::Ok, S::Error> {
    fields
        .iter()
        .map(|field| serde_json::json!({"label":field.label,"value":field.value}))
        .collect::<Vec<_>>()
        .serialize(serializer)
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OperationLogListItem {
    pub id: String,
    pub occurred_at: String,
    pub source: OperationSource,
    pub actor_snapshot: String,
    pub action: OperationAction,
    pub action_group: OperationActionGroup,
    pub action_label: String,
    pub object_snapshot: String,
    pub result: OperationResult,
    pub summary: String,
    pub detail_behavior: DetailBehavior,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OperationLogDetail {
    pub item: OperationLogListItem,
    pub explanation: String,
    pub content: OperationLogContent,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OperationLogPage {
    pub items: Vec<OperationLogListItem>,
    pub total: u64,
    pub page: u64,
    pub page_size: u64,
}
#[derive(Clone, Debug)]
pub(crate) struct OperationEvent {
    // 原始标识与时间仅供模块内部补记；普通业务事件由记录器统一生成。
    pub(super) id: Option<String>,
    pub(super) occurred_at_ms: Option<i64>,
    pub action: OperationAction,
    pub source: OperationSource,
    pub result: OperationResult,
    pub object: String,
    pub summary: String,
    pub explanation: String,
    pub content: OperationLogContent,
    pub detail_behavior: DetailBehavior,
}
impl OperationEvent {
    pub(crate) fn new(
        action: OperationAction,
        source: OperationSource,
        result: OperationResult,
        object: impl Into<String>,
        summary: impl Into<String>,
        content: OperationLogContent,
    ) -> Self {
        let summary = summary.into();
        Self {
            id: None,
            occurred_at_ms: None,
            action,
            source,
            result,
            object: object.into(),
            explanation: summary.clone(),
            summary,
            content,
            detail_behavior: DetailBehavior::Drawer,
        }
    }
}
pub(super) fn enum_string(value: impl Serialize) -> String {
    serde_json::to_value(value)
        .expect("enum serializes")
        .as_str()
        .expect("unit enum is string")
        .to_owned()
}
