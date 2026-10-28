use super::{
    NotificationError, NotificationMarker, NotificationVariable, TemplateMode, TemplateUpdate,
};

pub(super) struct RuleDefinition {
    pub id: &'static str,
    pub title: &'static str,
    pub sub_title: &'static str,
    pub marker: &'static str,
    pub title_template: &'static str,
    pub body_template: &'static str,
}

impl RuleDefinition {
    pub fn default_template(&self) -> TemplateUpdate {
        TemplateUpdate {
            template_type: TemplateMode::Default,
            marker_id: self.marker.into(),
            title_template: self.title_template.into(),
            body_template: self.body_template.into(),
        }
    }
}

pub(super) const RULES: &[RuleDefinition] = &[
    RuleDefinition {
        id: "probe.model_failed",
        title: "模型探测故障",
        sub_title: "模型达到稳定故障判定时发送系统通知",
        marker: "warning",
        title_template: "{{model}} 探测故障",
        body_template: "{{model}} 在 {{endpoint}} 连续探测失败，已判定为稳定故障（{{time}}）。",
    },
    RuleDefinition {
        id: "probe.model_recovered",
        title: "模型探测恢复",
        sub_title: "模型从稳定故障恢复时发送系统通知",
        marker: "success",
        title_template: "{{model}} 已恢复",
        body_template: "{{model}} 在 {{endpoint}} 恢复正常响应（{{time}}）。",
    },
];

pub(super) fn rule(id: &str) -> Result<&'static RuleDefinition, NotificationError> {
    RULES
        .iter()
        .find(|rule| rule.id == id)
        .ok_or(NotificationError::RuleNotFound)
}

pub(crate) fn markers() -> Vec<NotificationMarker> {
    [
        ("none", "", "无标记", "neutral"),
        ("info", "ℹ️", "信息", "info"),
        ("success", "✅", "成功", "positive"),
        ("warning", "⚠️", "警告", "warning"),
        ("error", "❌", "错误", "critical"),
        ("bell", "🔔", "提醒", "info"),
        ("fire", "🔥", "严重", "critical"),
    ]
    .into_iter()
    .map(|(marker_id, symbol, name, tone)| NotificationMarker {
        marker_id,
        symbol,
        name,
        tone,
    })
    .collect()
}

pub(crate) fn variables() -> Vec<NotificationVariable> {
    [
        (
            "time",
            "时间",
            "事件发生时间",
            "2026-10-11 14:30",
            "通用变量",
        ),
        ("app", "应用名", "应用名称", "RelayPulse", "通用变量"),
        ("model", "模型", "相关模型名", "gpt-4o", "事件变量"),
        (
            "endpoint",
            "端点",
            "探测目标端点",
            "api.openai.com",
            "事件变量",
        ),
    ]
    .into_iter()
    .map(
        |(key, label, description, example, group)| NotificationVariable {
            key,
            label,
            description,
            example,
            group,
        },
    )
    .collect()
}
