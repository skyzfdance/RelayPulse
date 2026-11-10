use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationSource {
    System,
    User,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationResult {
    Success,
    Failure,
    Cancelled,
    Blocked,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActionDefinition {
    pub id: &'static str,
    pub label: &'static str,
    pub group: &'static str,
}
impl ActionDefinition {
    pub const fn new(id: &'static str, label: &'static str, group: &'static str) -> Self {
        Self { id, label, group }
    }
    pub fn group(self) -> &'static str {
        self.group
    }
    pub fn label(self) -> &'static str {
        self.label
    }
}
pub type OperationActionGroup = String;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum DetailBehavior {
    Drawer,
    Navigate {
        target: String,
        parameters: std::collections::BTreeMap<String, String>,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotField {
    pub label: String,
    pub value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sensitive: Option<bool>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeField {
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
pub enum OperationLogContent {
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
    Presentation {
        format: String,
        fields: Vec<SnapshotField>,
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
pub struct OperationLogListItem {
    pub id: String,
    pub occurred_at: String,
    pub source: OperationSource,
    pub actor_snapshot: String,
    pub action: String,
    pub action_group: OperationActionGroup,
    pub action_label: String,
    pub object_snapshot: String,
    pub result: OperationResult,
    pub summary: String,
    pub detail_behavior: DetailBehavior,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationLogDetail {
    pub item: OperationLogListItem,
    pub explanation: String,
    pub content: OperationLogContent,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationLogPage {
    pub action_groups: Vec<String>,
    pub items: Vec<OperationLogListItem>,
    pub total: u64,
    pub page: u64,
    pub page_size: u64,
}
#[derive(Clone, Debug)]
pub struct OperationEvent {
    // 原始标识与时间仅供模块内部补记；普通业务事件由记录器统一生成。
    pub id: Option<String>,
    pub occurred_at_ms: Option<i64>,
    pub action: ActionDefinition,
    pub source: OperationSource,
    pub result: OperationResult,
    pub object: String,
    pub summary: String,
    pub explanation: String,
    pub content: OperationLogContent,
    pub detail_behavior: DetailBehavior,
}
impl OperationEvent {
    pub fn new(
        action: ActionDefinition,
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

impl PartialEq<ActionDefinition> for String {
    fn eq(&self, other: &ActionDefinition) -> bool {
        self == other.id
    }
}

impl Serialize for ActionDefinition {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.id)
    }
}

/// Optional, caller-owned legacy decoding. New writes only accept the generic typed contract.
pub trait HistoryDecoder: Send + Sync {
    fn behavior(&self, value: serde_json::Value) -> Result<DetailBehavior, serde_json::Error> {
        serde_json::from_value(value)
    }
    fn content(&self, value: serde_json::Value) -> Result<OperationLogContent, serde_json::Error> {
        serde_json::from_value(value)
    }
}
pub struct GenericHistoryDecoder;
impl HistoryDecoder for GenericHistoryDecoder {}
