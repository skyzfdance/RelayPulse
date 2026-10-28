use serde::{Deserialize, Serialize};

pub(crate) struct NotificationContext<'a> {
    pub model: &'a str,
    pub endpoint: &'a str,
    pub finished_at_ms: u64,
}

/// 由通知用例指定的固定点击动作，不接受前端传入的路由或脚本。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NotificationAction {
    OpenProbeRecords,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum NotificationPermission {
    Unknown,
    NotRequested,
    Denied,
    Granted,
    Unsupported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum TemplateMode {
    Default,
    Custom,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct RuleSummary {
    pub id: String,
    pub title: String,
    pub sub_title: String,
    pub status: bool,
    pub template_type: TemplateMode,
    pub channel: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct NotificationSettings {
    pub notifications_enabled: bool,
    pub authorization: NotificationPermission,
    pub rules: Vec<RuleSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct TemplateUpdate {
    pub template_type: TemplateMode,
    pub marker_id: String,
    pub title_template: String,
    pub body_template: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TemplateDetail {
    #[serde(flatten)]
    pub summary: RuleSummary,
    pub marker_id: String,
    pub title_template: String,
    pub body_template: String,
    pub default_marker_id: String,
    pub default_title_template: String,
    pub default_body_template: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct TestNotification {
    pub title: String,
    pub body: String,
    pub marker_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NotificationMarker {
    pub marker_id: &'static str,
    pub symbol: &'static str,
    pub name: &'static str,
    pub tone: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct NotificationVariable {
    pub key: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    pub example: &'static str,
    pub group: &'static str,
}

/// 只包含持久化配置，不缓存系统授权；默认模板内容来自固定目录。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StoredRule {
    pub id: String,
    pub status: bool,
    pub custom: Option<TemplateUpdate>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StoredSettings {
    pub enabled: bool,
    pub rules: Vec<StoredRule>,
}
