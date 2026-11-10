use relaypulse_notifications::{NotificationVariable, RuleDefinition};
pub const RULES: &[RuleDefinition] = &[
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
pub const VARIABLES: &[NotificationVariable] = &[
    NotificationVariable {
        key: "model",
        label: "模型",
        description: "模型",
        example: "gpt-4o",
        group: "事件变量",
    },
    NotificationVariable {
        key: "endpoint",
        label: "端点",
        description: "端点",
        example: "api.openai.com",
        group: "事件变量",
    },
    NotificationVariable {
        key: "time",
        label: "时间",
        description: "时间",
        example: "2026-10-11 14:30",
        group: "事件变量",
    },
    NotificationVariable {
        key: "app",
        label: "应用名",
        description: "应用名",
        example: "RelayPulse",
        group: "事件变量",
    },
];
