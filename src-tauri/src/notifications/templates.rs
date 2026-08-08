use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use unicode_segmentation::UnicodeSegmentation;

use super::{NotificationError, NotificationEventTone, SYSTEM_CHANNEL_ID};

pub const TEMPLATE_SCHEMA_VERSION: u16 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationTemplateMode {
    Default,
    Custom,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationTemplateMarker {
    pub marker_id: String,
    pub symbol: String,
    pub accessible_name: String,
    pub tone: Option<NotificationEventTone>,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationTemplateVariable {
    pub key: String,
    pub label: String,
    pub group: String,
    pub description: String,
    pub example: String,
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationTemplateDescriptor {
    pub event_type: String,
    pub channel_id: String,
    pub template_schema_version: u16,
    pub default_marker_id: String,
    pub default_title_template: String,
    pub default_body_template: String,
    pub variables: Vec<NotificationTemplateVariable>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationTemplateOverride {
    pub event_type: String,
    pub channel_id: String,
    pub mode: NotificationTemplateMode,
    pub marker_id: String,
    pub title_template: String,
    pub body_template: String,
    pub template_schema_version: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationTemplateDraft {
    pub event_type: String,
    pub channel_id: String,
    pub mode: NotificationTemplateMode,
    pub marker_id: String,
    pub title_template: String,
    pub body_template: String,
    pub template_schema_version: u16,
}

impl From<NotificationTemplateDraft> for NotificationTemplateOverride {
    fn from(value: NotificationTemplateDraft) -> Self {
        Self {
            event_type: value.event_type,
            channel_id: value.channel_id,
            mode: value.mode,
            marker_id: value.marker_id,
            title_template: value.title_template,
            body_template: value.body_template,
            template_schema_version: value.template_schema_version,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationTemplateTestResult {
    pub accepted: bool,
    pub rendered_marker: String,
    pub rendered_title: String,
    pub rendered_body: String,
    pub operation_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RenderedNotificationTemplate {
    pub marker: String,
    pub title: String,
    pub body: String,
    pub fallback_error_code: Option<&'static str>,
}

pub fn marker_catalog() -> Vec<NotificationTemplateMarker> {
    vec![
        marker("none", "", "无标记", None),
        marker("red", "🔴", "红色", Some(NotificationEventTone::Critical)),
        marker("orange", "🟠", "橙色", None),
        marker("yellow", "🟡", "黄色", None),
        marker("green", "🟢", "绿色", Some(NotificationEventTone::Positive)),
        marker("blue", "🔵", "蓝色", Some(NotificationEventTone::Neutral)),
        marker("warning", "⚠️", "警告", None),
        marker(
            "failure",
            "❌",
            "失败",
            Some(NotificationEventTone::Critical),
        ),
        marker(
            "success",
            "✅",
            "成功",
            Some(NotificationEventTone::Positive),
        ),
        marker("info", "ℹ️", "信息", Some(NotificationEventTone::Neutral)),
        marker("reminder", "🔔", "提醒", None),
        marker("timer", "⏱️", "计时", None),
        marker("task_completed", "☑️", "任务完成", None),
        marker(
            "red_failure",
            "🔴",
            "红色故障",
            Some(NotificationEventTone::Critical),
        ),
    ]
}

fn marker(
    marker_id: &str,
    symbol: &str,
    accessible_name: &str,
    tone: Option<NotificationEventTone>,
) -> NotificationTemplateMarker {
    NotificationTemplateMarker {
        marker_id: marker_id.to_owned(),
        symbol: symbol.to_owned(),
        accessible_name: accessible_name.to_owned(),
        tone,
        enabled: true,
    }
}

pub fn template_descriptor(
    event_type: &str,
    display_name: &str,
) -> Option<NotificationTemplateDescriptor> {
    let (default_marker_id, default_title_template, default_body_template, event_variables) =
        match event_type {
            "probe.model_failed" => (
                "red_failure",
                "模型探测故障",
                "{{site_name}} · {{model_name}}：{{failure_reason}}，发生时间：{{occurred_at}}",
                vec![
                    variable(
                        "site_name",
                        "站点名称",
                        "事件变量",
                        "发生故障的站点名称",
                        "东京主站",
                    ),
                    variable(
                        "model_name",
                        "模型名称",
                        "事件变量",
                        "发生故障的模型名称",
                        "GPT-5",
                    ),
                    variable(
                        "failure_reason",
                        "故障原因",
                        "事件变量",
                        "经过安全归类的故障原因",
                        "探测请求超时",
                    ),
                ],
            ),
            "probe.model_recovered" => (
                "success",
                "模型探测恢复",
                "{{site_name}} · {{model_name}}：{{recovery_state}}，恢复时间：{{occurred_at}}",
                vec![
                    variable(
                        "site_name",
                        "站点名称",
                        "事件变量",
                        "恢复可用的站点名称",
                        "东京主站",
                    ),
                    variable(
                        "model_name",
                        "模型名称",
                        "事件变量",
                        "恢复可用的模型名称",
                        "GPT-5",
                    ),
                    variable(
                        "recovery_state",
                        "恢复状态",
                        "事件变量",
                        "恢复后的稳定状态",
                        "已恢复为正常",
                    ),
                ],
            ),
            #[cfg(test)]
            "fixture.task_completed" => (
                "task_completed",
                "RelayPulse fixture",
                "{{event_name}}，发生时间：{{occurred_at}}",
                Vec::new(),
            ),
            _ => return None,
        };

    let mut variables = vec![
        variable(
            "occurred_at",
            "发生时间",
            "通用变量",
            "事件发生的本地时间",
            "2026-08-08 14:32:18",
        ),
        variable(
            "event_name",
            "事件名称",
            "通用变量",
            "当前通知事件的显示名称",
            display_name,
        ),
    ];
    variables.extend(event_variables);
    Some(NotificationTemplateDescriptor {
        event_type: event_type.to_owned(),
        channel_id: SYSTEM_CHANNEL_ID.to_owned(),
        template_schema_version: TEMPLATE_SCHEMA_VERSION,
        default_marker_id: default_marker_id.to_owned(),
        default_title_template: default_title_template.to_owned(),
        default_body_template: default_body_template.to_owned(),
        variables,
    })
}

fn variable(
    key: &str,
    label: &str,
    group: &str,
    description: &str,
    example: &str,
) -> NotificationTemplateVariable {
    NotificationTemplateVariable {
        key: key.to_owned(),
        label: label.to_owned(),
        group: group.to_owned(),
        description: description.to_owned(),
        example: example.to_owned(),
        required: true,
    }
}

pub(crate) fn validate_template_override(
    descriptor: &NotificationTemplateDescriptor,
    template_override: &NotificationTemplateOverride,
) -> Result<(), NotificationError> {
    if template_override.event_type != descriptor.event_type
        || template_override.channel_id != descriptor.channel_id
    {
        return Err(NotificationError::TemplateInvalid { field: "template" });
    }
    if template_override.template_schema_version != descriptor.template_schema_version {
        return Err(NotificationError::TemplateSchemaChanged);
    }
    if !marker_catalog()
        .iter()
        .any(|marker| marker.enabled && marker.marker_id == template_override.marker_id)
    {
        return Err(NotificationError::TemplateInvalid { field: "marker_id" });
    }
    let allowed_variables = descriptor
        .variables
        .iter()
        .map(|variable| variable.key.as_str())
        .collect::<HashSet<_>>();
    validate_template_field(
        &template_override.title_template,
        120,
        false,
        &allowed_variables,
        "title_template",
    )?;
    validate_template_field(
        &template_override.body_template,
        500,
        true,
        &allowed_variables,
        "body_template",
    )?;
    Ok(())
}

fn validate_template_field(
    value: &str,
    maximum: usize,
    allow_newlines: bool,
    allowed_variables: &HashSet<&str>,
    field: &'static str,
) -> Result<(), NotificationError> {
    if value.trim().is_empty()
        || value.graphemes(true).count() > maximum
        || value.chars().count() > maximum.saturating_mul(4)
        || value.chars().any(|character| {
            character.is_control() && !(allow_newlines && matches!(character, '\n' | '\r'))
        })
        || (!allow_newlines && value.contains(['\n', '\r']))
    {
        return Err(NotificationError::TemplateInvalid { field });
    }
    parse_template(value, allowed_variables, None).map(|_| ())
}

pub(crate) fn render_notification_template(
    descriptor: &NotificationTemplateDescriptor,
    template_override: Option<&NotificationTemplateOverride>,
    variables: &HashMap<String, String>,
) -> Result<RenderedNotificationTemplate, NotificationError> {
    let mut fallback_error_code = None;
    let selected = match template_override {
        Some(value) if value.mode == NotificationTemplateMode::Custom => {
            match validate_template_override(descriptor, value) {
                Ok(()) => Some(value),
                Err(error) => {
                    fallback_error_code = Some(template_error_code(&error));
                    None
                }
            }
        }
        Some(value) if value.template_schema_version != descriptor.template_schema_version => {
            fallback_error_code = Some("notification_template_schema_changed");
            None
        }
        _ => None,
    };
    if let Some(selected) = selected {
        match render_selected_template(
            descriptor,
            &selected.marker_id,
            &selected.title_template,
            &selected.body_template,
            variables,
        ) {
            Ok((marker, title, body)) => {
                return Ok(RenderedNotificationTemplate {
                    marker,
                    title,
                    body,
                    fallback_error_code,
                });
            }
            Err(error) => fallback_error_code = Some(template_error_code(&error)),
        }
    }
    let (marker, title, body) = render_selected_template(
        descriptor,
        &descriptor.default_marker_id,
        &descriptor.default_title_template,
        &descriptor.default_body_template,
        variables,
    )
    .map_err(|_| NotificationError::TemplateRenderFailed)?;
    Ok(RenderedNotificationTemplate {
        marker,
        title,
        body,
        fallback_error_code,
    })
}

fn render_selected_template(
    descriptor: &NotificationTemplateDescriptor,
    marker_id: &str,
    title_template: &str,
    body_template: &str,
    variables: &HashMap<String, String>,
) -> Result<(String, String, String), NotificationError> {
    let allowed_variables = descriptor
        .variables
        .iter()
        .map(|variable| variable.key.as_str())
        .collect::<HashSet<_>>();
    let marker = marker_catalog()
        .into_iter()
        .find(|marker| marker.enabled && marker.marker_id == marker_id)
        .ok_or(NotificationError::TemplateInvalid { field: "marker_id" })?;
    let rendered_title = parse_template(title_template, &allowed_variables, Some(variables))?;
    let title = if marker.symbol.is_empty() {
        rendered_title
    } else {
        format!("{} {rendered_title}", marker.symbol)
    };
    let body = parse_template(body_template, &allowed_variables, Some(variables))?;
    validate_rendered(&title, 120, false)?;
    validate_rendered(&body, 500, true)?;
    Ok((marker.symbol, title, body))
}

fn template_error_code(error: &NotificationError) -> &'static str {
    match error {
        NotificationError::TemplateSchemaChanged => "notification_template_schema_changed",
        NotificationError::TemplateVariableMissing => "notification_template_variable_missing",
        NotificationError::TemplateInvalid { .. } => "notification_template_invalid",
        NotificationError::TemplateRenderFailed => "notification_template_render_failed",
        _ => "notification_template_render_failed",
    }
}

fn parse_template(
    template: &str,
    allowed_variables: &HashSet<&str>,
    values: Option<&HashMap<String, String>>,
) -> Result<String, NotificationError> {
    let mut rendered = String::with_capacity(template.len());
    let mut remaining = template;
    while !remaining.is_empty() {
        let Some(open) = remaining.find(['{', '}']) else {
            rendered.push_str(remaining);
            break;
        };
        rendered.push_str(&remaining[..open]);
        remaining = &remaining[open..];
        if !remaining.starts_with("{{") {
            return Err(NotificationError::TemplateInvalid { field: "template" });
        }
        let Some(close) = remaining[2..].find("}}") else {
            return Err(NotificationError::TemplateInvalid { field: "template" });
        };
        let key = &remaining[2..close + 2];
        if !valid_variable_key(key) || !allowed_variables.contains(key) {
            return Err(NotificationError::TemplateInvalid { field: "template" });
        }
        if let Some(values) = values {
            let value = values
                .get(key)
                .ok_or(NotificationError::TemplateVariableMissing)?;
            rendered.push_str(value);
        }
        remaining = &remaining[close + 4..];
    }
    Ok(rendered)
}

fn valid_variable_key(value: &str) -> bool {
    let mut characters = value.chars();
    characters
        .next()
        .is_some_and(|character| character.is_ascii_lowercase())
        && characters.all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_'
        })
}

fn validate_rendered(
    value: &str,
    maximum: usize,
    allow_newlines: bool,
) -> Result<(), NotificationError> {
    if value.trim().is_empty()
        || value.graphemes(true).count() > maximum
        || value.chars().count() > maximum.saturating_mul(4)
        || value.chars().any(|character| {
            character.is_control() && !(allow_newlines && matches!(character, '\n' | '\r'))
        })
        || (!allow_newlines && value.contains(['\n', '\r']))
    {
        return Err(NotificationError::TemplateRenderFailed);
    }
    Ok(())
}

pub(crate) fn example_variables(
    descriptor: &NotificationTemplateDescriptor,
) -> HashMap<String, String> {
    descriptor
        .variables
        .iter()
        .map(|variable| (variable.key.clone(), variable.example.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unknown_or_malformed_placeholders() {
        let descriptor =
            template_descriptor("probe.model_failed", "模型探测故障").expect("descriptor exists");
        let mut template_override = NotificationTemplateOverride {
            event_type: descriptor.event_type.clone(),
            channel_id: descriptor.channel_id.clone(),
            mode: NotificationTemplateMode::Custom,
            marker_id: "warning".to_owned(),
            title_template: "{{unknown}}".to_owned(),
            body_template: "{{site_name}}".to_owned(),
            template_schema_version: TEMPLATE_SCHEMA_VERSION,
        };
        assert!(matches!(
            validate_template_override(&descriptor, &template_override),
            Err(NotificationError::TemplateInvalid { .. })
        ));
        template_override.title_template = "{{site_name}".to_owned();
        assert!(matches!(
            validate_template_override(&descriptor, &template_override),
            Err(NotificationError::TemplateInvalid { .. })
        ));
    }

    #[test]
    fn renders_custom_templates_with_authoritative_examples() {
        let descriptor =
            template_descriptor("probe.model_failed", "模型探测故障").expect("descriptor exists");
        let template_override = NotificationTemplateOverride {
            event_type: descriptor.event_type.clone(),
            channel_id: descriptor.channel_id.clone(),
            mode: NotificationTemplateMode::Custom,
            marker_id: "warning".to_owned(),
            title_template: "{{model_name}} 异常".to_owned(),
            body_template: "{{site_name}}：{{failure_reason}}".to_owned(),
            template_schema_version: TEMPLATE_SCHEMA_VERSION,
        };
        let rendered = render_notification_template(
            &descriptor,
            Some(&template_override),
            &example_variables(&descriptor),
        )
        .expect("template renders");
        assert_eq!(rendered.title, "⚠️ GPT-5 异常");
        assert_eq!(rendered.body, "东京主站：探测请求超时");
    }

    #[test]
    fn falls_back_to_the_default_snapshot_when_custom_rendering_is_too_long() {
        let descriptor =
            template_descriptor("probe.model_failed", "模型探测故障").expect("descriptor exists");
        let template_override = NotificationTemplateOverride {
            event_type: descriptor.event_type.clone(),
            channel_id: descriptor.channel_id.clone(),
            mode: NotificationTemplateMode::Custom,
            marker_id: "warning".to_owned(),
            title_template: "自定义故障".to_owned(),
            body_template: "{{model_name}}{{model_name}}{{model_name}}{{model_name}}{{model_name}}"
                .to_owned(),
            template_schema_version: TEMPLATE_SCHEMA_VERSION,
        };
        let mut variables = example_variables(&descriptor);
        variables.insert("model_name".to_owned(), "M".repeat(120));

        let rendered =
            render_notification_template(&descriptor, Some(&template_override), &variables)
                .expect("default template provides a safe fallback");

        assert_eq!(rendered.title, "🔴 模型探测故障");
        assert_eq!(
            rendered.fallback_error_code,
            Some("notification_template_render_failed")
        );
    }

    #[test]
    fn a_template_test_can_detect_a_runtime_length_fallback() {
        let descriptor =
            template_descriptor("probe.model_failed", "模型探测故障").expect("descriptor exists");
        let template_override = NotificationTemplateOverride {
            event_type: descriptor.event_type.clone(),
            channel_id: descriptor.channel_id.clone(),
            mode: NotificationTemplateMode::Custom,
            marker_id: "warning".to_owned(),
            title_template: "a".repeat(120),
            body_template: "{{failure_reason}}".to_owned(),
            template_schema_version: TEMPLATE_SCHEMA_VERSION,
        };
        let rendered = render_notification_template(
            &descriptor,
            Some(&template_override),
            &example_variables(&descriptor),
        )
        .expect("the production renderer falls back safely");

        assert_eq!(
            rendered.fallback_error_code,
            Some("notification_template_render_failed")
        );
    }
}
