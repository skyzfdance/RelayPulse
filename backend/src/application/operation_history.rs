use crate::application::action_definitions::actions as OperationAction;
use crate::modules::operation_logs::*;
use serde::{Deserialize, Serialize};
pub(crate) fn report_read_failure(
    logs: &OperationLogs,
    target: ReadFailureTarget,
    kind: ReadFailureKind,
) {
    // 前端在确认本次可见读取失败后上报一次；独立重试不能按页面和时间窗口合并。
    let action = match kind {
        ReadFailureKind::Page => OperationAction::PageLoadFailed,
        ReadFailureKind::Detail => OperationAction::DetailReadFailed,
    };
    logs.record_best_effort(OperationEvent::new(
        action,
        OperationSource::User,
        OperationResult::Failure,
        target.label(),
        "页面内容暂时无法读取。",
        OperationLogContent::Failure {
            activity: action.label().into(),
            target: target.label().into(),
            reason: "前端加载或通信失败，请重试。".into(),
        },
    ));
}
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReadFailureTarget {
    Route,
    SiteList,
    SiteDetail,
    ProbeRecords,
    ProbeRecordDetail,
    NotificationSettings,
    NotificationTemplate,
    GeneralSettings,
    Panel,
}
impl ReadFailureTarget {
    fn label(self) -> &'static str {
        match self {
            Self::Route => "应用页面",
            Self::SiteList => "站点列表",
            Self::SiteDetail => "站点详情",
            Self::ProbeRecords => "探测记录",
            Self::ProbeRecordDetail => "探测记录详情",
            Self::NotificationSettings => "通知设置",
            Self::NotificationTemplate => "通知模板",
            Self::GeneralSettings => "通用设置",
            Self::Panel => "监测面板",
        }
    }
}
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReadFailureKind {
    Page,
    Detail,
}

/// Keep the established IPC DTO while persistence consumes the generic SDK contract.
pub(crate) fn wire(mut value: serde_json::Value) -> serde_json::Value {
    fn item(value: &mut serde_json::Value) {
        let behavior = &value["detailBehavior"];
        if behavior["kind"] == "navigate" && behavior["target"] == "probe_records" {
            if let (Some(ids), Some(availability)) = (
                behavior["parameters"]["recordIds"].as_str(),
                behavior["parameters"]["availability"].as_str(),
            ) {
                if let Ok(ids) = serde_json::from_str::<Vec<String>>(ids) {
                    value["detailBehavior"] = serde_json::json!({"kind":"probe_records","recordIds":ids,"availability":availability});
                }
            }
        }
    }
    if let Some(items) = value
        .get_mut("items")
        .and_then(serde_json::Value::as_array_mut)
    {
        for entry in items {
            item(entry);
        }
    }
    if value.get("item").is_some() {
        item(&mut value["item"]);
    }
    if value["content"]["kind"] == "presentation"
        && value["content"]["format"]
            == crate::modules::monitoring::operation_history::FAILURE_FORMAT
    {
        if let Some(reason) = value["content"]["fields"]
            .as_array()
            .and_then(|f| f.iter().find(|f| f["label"] == "reason"))
            .and_then(|f| f["value"].as_str())
        {
            value["content"] = serde_json::json!({"kind":"probe_failure","reason":reason});
        }
    }
    value
}
