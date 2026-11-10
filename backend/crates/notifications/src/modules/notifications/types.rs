use serde::{Deserialize, Serialize};

pub struct NotificationContext<'a> {
    pub values: std::collections::BTreeMap<&'a str, String>,
    pub action: NotificationAction,
}
/// Opaque navigation data interpreted and authorized by the desktop adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationAction {
    pub target: String,
    pub parameters: std::collections::BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationPermission {
    Unknown,
    NotRequested,
    Denied,
    Granted,
    Unsupported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TemplateMode {
    Default,
    Custom,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuleSummary {
    pub id: String,
    pub title: String,
    pub sub_title: String,
    pub status: bool,
    pub template_type: TemplateMode,
    pub channel: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NotificationSettings {
    pub notifications_enabled: bool,
    pub authorization: NotificationPermission,
    pub rules: Vec<RuleSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TemplateUpdate {
    pub template_type: TemplateMode,
    pub marker_id: String,
    pub title_template: String,
    pub body_template: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateDetail {
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
pub struct TestNotification {
    pub title: String,
    pub body: String,
    pub marker_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationMarker {
    pub marker_id: &'static str,
    pub symbol: &'static str,
    pub name: &'static str,
    pub tone: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct NotificationVariable {
    pub key: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    pub example: &'static str,
    pub group: &'static str,
}

/// 只包含持久化配置，不缓存系统授权；默认模板内容来自固定目录。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredRule {
    pub id: String,
    pub status: bool,
    pub custom: Option<TemplateUpdate>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredSettings {
    pub enabled: bool,
    pub rules: Vec<StoredRule>,
}
