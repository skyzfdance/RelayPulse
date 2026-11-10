use super::{
    catalog, NotificationError, StoredSettings, TemplateMode, TemplateUpdate, TestNotification,
};
use unicode_segmentation::UnicodeSegmentation;

fn invalid(field: &'static str, message: &'static str) -> NotificationError {
    NotificationError::InvalidInput { field, message }
}

fn text(
    value: &str,
    max: usize,
    newlines: bool,
    field: &'static str,
) -> Result<(), NotificationError> {
    if value.trim().is_empty() {
        return Err(invalid(field, "通知内容不能为空。"));
    }
    if value.graphemes(true).count() > max {
        return Err(invalid(
            field,
            "通知标题最多 120 个书写符号，正文最多 500 个书写符号。",
        ));
    }
    if value.chars().any(|c| {
        (c.is_control() && !(newlines && matches!(c, '\r' | '\n')))
            || (!newlines && matches!(c, '\u{2028}' | '\u{2029}'))
    }) {
        return Err(invalid(field, "通知不能包含控制字符，标题不能包含换行。"));
    }
    Ok(())
}

// 单次扫描：拒绝残缺花括号、非法键名与未知变量。返回仅供长度校验的示例预览。
fn preview(
    value: &str,
    catalog: &catalog::EventCatalog,
    field: &'static str,
) -> Result<String, NotificationError> {
    let variables = &catalog.variables;
    let mut rest = value;
    let mut result = String::new();
    while let Some(index) = rest.find(['{', '}']) {
        result.push_str(&rest[..index]);
        rest = &rest[index..];
        let Some(token) = rest.strip_prefix("{{") else {
            return Err(invalid(field, "变量占位符格式不完整，请使用 {{变量名}}。"));
        };
        let Some(end) = token.find("}}") else {
            return Err(invalid(field, "变量占位符格式不完整，请使用 {{变量名}}。"));
        };
        let key = &token[..end];
        let variable = variables
            .iter()
            .find(|v| v.key == key)
            .ok_or_else(|| invalid(field, "模板包含未知变量或不合法的变量名。"))?;
        result.push_str(variable.example);
        rest = &token[end + 2..];
    }
    result.push_str(rest);
    Ok(result)
}

pub(super) fn template(
    update: &TemplateUpdate,
    catalog: &catalog::EventCatalog,
) -> Result<(), NotificationError> {
    let marker = catalog::markers()
        .into_iter()
        .find(|m| m.marker_id == update.marker_id)
        .ok_or_else(|| invalid("markerId", "标题标记不可用。"))?;
    text(&update.title_template, 120, false, "titleTemplate")?;
    text(&update.body_template, 500, true, "bodyTemplate")?;
    let title = preview(&update.title_template, catalog, "titleTemplate")?;
    preview(&update.body_template, catalog, "bodyTemplate")?;
    let title = if marker.symbol.is_empty() {
        title
    } else {
        format!("{} {title}", marker.symbol)
    };
    text(&title, 120, false, "titleTemplate")
}

pub(super) fn test_notification(notification: &TestNotification) -> Result<(), NotificationError> {
    if notification
        .marker_id
        .as_ref()
        .is_some_and(|id| !catalog::markers().iter().any(|m| m.marker_id == id))
    {
        return Err(invalid("markerId", "标题标记不可用。"));
    }
    // 成品文本允许普通花括号，不再渲染、替换变量或拼接标记。
    text(&notification.title, 120, false, "title")?;
    text(&notification.body, 500, true, "body")
}

pub(super) fn rule_ids<'a>(
    ids: impl Iterator<Item = &'a str>,
    catalog: &catalog::EventCatalog,
) -> Result<(), NotificationError> {
    let ids: Vec<_> = ids.collect();
    let mut unique = std::collections::HashSet::new();
    if ids
        .iter()
        .any(|id| !unique.insert(*id) || catalog.rule(id).is_err())
    {
        return Err(invalid("rules", "规则未知或重复。"));
    }
    Ok(())
}
pub(super) fn stored(settings: &StoredSettings) -> Result<(), NotificationError> {
    let mut ids = std::collections::HashSet::new();
    for rule in &settings.rules {
        if rule.id.is_empty() || !ids.insert(&rule.id) {
            return Err(NotificationError::CorruptSettings);
        }
        if let Some(custom) = &rule.custom {
            if custom.template_type != TemplateMode::Custom {
                return Err(NotificationError::CorruptSettings);
            }
        }
    }
    Ok(())
}

pub(super) fn stored_catalog(
    settings: &StoredSettings,
    catalog: &catalog::EventCatalog,
) -> Result<(), NotificationError> {
    stored(settings)?;
    for rule in &settings.rules {
        if catalog.rule(&rule.id).is_ok() {
            if let Some(custom) = &rule.custom {
                template(custom, catalog).map_err(|_| NotificationError::CorruptSettings)?;
            }
        }
    }
    Ok(())
}
