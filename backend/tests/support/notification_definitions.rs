#![allow(dead_code)]
use crate::modules::notifications::*;
pub fn catalog() -> EventCatalog {
    EventCatalog::new(
        vec![
            RuleDefinition {
                id: "iq.run_completed",
                title: "智商测试完成",
                sub_title: "整场测试结束后通知一次，包含有失败结果的测试",
                marker: "info",
                title_template: "智商测试已完成",
                body_template: "整场测试已完成，点击查看详情。",
            },
            RuleDefinition {
                id: "probe.model_failed",
                title: "模型探测故障",
                sub_title: "模型达到稳定故障判定时发送系统通知",
                marker: "warning",
                title_template: "{{model}} 探测故障",
                body_template:
                    "{{model}} 在 {{endpoint}} 连续探测失败，已判定为稳定故障（{{time}}）。",
            },
            RuleDefinition {
                id: "probe.model_recovered",
                title: "模型探测恢复",
                sub_title: "模型从稳定故障恢复时发送系统通知",
                marker: "success",
                title_template: "{{model}} 已恢复",
                body_template: "{{model}} 在 {{endpoint}} 恢复正常响应（{{time}}）。",
            },
        ],
        vec![
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
        ],
    )
    .unwrap()
}
pub fn context<'a>(
    model: &str,
    endpoint: &str,
    finished_at_ms: u64,
    action: crate::modules::notifications::NotificationAction,
) -> crate::modules::notifications::NotificationContext<'a> {
    let timestamp = time::OffsetDateTime::from_unix_timestamp((finished_at_ms / 1000) as i64)
        .unwrap_or(time::OffsetDateTime::UNIX_EPOCH)
        .to_offset(time::UtcOffset::current_local_offset().unwrap_or(time::UtcOffset::UTC));
    crate::modules::notifications::NotificationContext {
        action,
        values: std::collections::BTreeMap::from([
            ("model", model.into()),
            ("endpoint", endpoint.into()),
            ("app", "RelayPulse".into()),
            (
                "time",
                format!(
                    "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
                    timestamp.year(),
                    u8::from(timestamp.month()),
                    timestamp.day(),
                    timestamp.hour(),
                    timestamp.minute(),
                    timestamp.second()
                ),
            ),
        ]),
    }
}
