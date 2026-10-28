use super::*;
use crate::modules::notifications::validation::{template, test_notification};
use serde_json::json;

#[test]
fn unicode_graphemes_markers_and_newline_boundaries() {
    let mut input = custom();
    input.title_template = "👩🏽‍💻".repeat(118);
    assert!(template(&input).is_ok()); // emoji + 空格占 2，其余 emoji 各占 1。
    input.title_template.push('a');
    assert!(template(&input).is_err());
    input.marker_id = "none".into();
    input.title_template = "e\u{301}".repeat(120);
    input.body_template = "🇨🇳".repeat(500);
    assert!(template(&input).is_ok());
    input.body_template.push('a');
    assert!(template(&input).is_err());
    input.body_template = "正文\r\n第二行".into();
    for illegal in [
        "", "  ", "\t", "\0", "\u{7f}", "\u{85}", "\n", "\r", "\u{2028}", "\u{2029}",
    ] {
        input.title_template = illegal.to_string();
        assert!(template(&input).is_err(), "{illegal:?}");
    }
    input.title_template = "正常".into();
    for illegal in ["a\t", "a\0", "a\u{7f}", "a\u{85}"] {
        input.body_template = illegal.into();
        assert!(template(&input).is_err());
    }
}

#[test]
fn validates_placeholder_syntax_catalog_and_preview_length() {
    let mut input = custom();
    for value in [
        "{{unknown}}",
        "{{Model}}",
        "{{ model }}",
        "{{}}",
        "{{model}",
        "{model}",
        "abc}",
        "{{{model}}}",
        "{{model-name}}",
    ] {
        input.title_template = value.into();
        assert!(template(&input).is_err(), "{value}");
    }
    input.title_template = "{{time}}".repeat(10); // 源模板 80，但示例展开后超长。
    assert!(template(&input).is_err());
    input.title_template = "{{model}}".into();
    input.marker_id = "unknown".into();
    assert!(template(&input).is_err());
    for rule in catalog::RULES {
        assert!(template(&rule.default_template()).is_ok());
    }
    assert!(test_notification(&TestNotification {
        title: "⚠️ {{literal}}".into(),
        body: "已经渲染".into(),
        marker_id: Some("warning".into())
    })
    .is_ok());
}

#[test]
fn json_enums_and_exact_camel_case_contract() {
    for (permission, value) in [
        (NotificationPermission::Unknown, "unknown"),
        (NotificationPermission::NotRequested, "not_requested"),
        (NotificationPermission::Denied, "denied"),
        (NotificationPermission::Granted, "granted"),
        (NotificationPermission::Unsupported, "unsupported"),
    ] {
        assert_eq!(serde_json::to_value(permission).unwrap(), value);
        assert_eq!(
            serde_json::from_value::<NotificationPermission>(json!(value)).unwrap(),
            permission
        );
    }
    for (mode, value) in [
        (TemplateMode::Default, "default"),
        (TemplateMode::Custom, "custom"),
    ] {
        assert_eq!(serde_json::to_value(mode).unwrap(), value);
    }
    assert!(serde_json::from_value::<NotificationPermission>(json!(true)).is_err());
    assert!(serde_json::from_value::<TemplateMode>(json!("invalid")).is_err());
    let input = serde_json::to_value(custom()).unwrap();
    assert_eq!(input.as_object().unwrap().len(), 4);
    for key in ["templateType", "markerId", "titleTemplate", "bodyTemplate"] {
        let mut missing = input.clone();
        missing.as_object_mut().unwrap().remove(key);
        assert!(serde_json::from_value::<TemplateUpdate>(missing).is_err());
    }
    let mut extra = input;
    extra["revision"] = json!(1);
    assert!(serde_json::from_value::<TemplateUpdate>(extra).is_err());
}
