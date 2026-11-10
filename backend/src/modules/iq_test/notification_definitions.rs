use relaypulse_notifications::{NotificationVariable, RuleDefinition};
pub const RULES: &[RuleDefinition] = &[RuleDefinition {
    id: "iq.run_completed",
    title: "智商测试完成",
    sub_title: "整场测试结束后通知一次，包含有失败结果的测试",
    marker: "info",
    title_template: "智商测试已完成",
    body_template: "整场测试已完成，点击查看详情。",
}];
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
