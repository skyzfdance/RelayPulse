use super::{catalog, NotificationContext, NotificationError, TemplateUpdate};
use unicode_segmentation::UnicodeSegmentation;

fn clean(value: &str) -> String {
    value
        .chars()
        .map(|c| {
            if c.is_control() || matches!(c, '\u{2028}' | '\u{2029}') {
                ' '
            } else {
                c
            }
        })
        .collect()
}

fn limit(value: String, maximum: usize) -> String {
    if value.graphemes(true).count() <= maximum {
        return value;
    }
    format!(
        "{}…",
        value.graphemes(true).take(maximum - 1).collect::<String>()
    )
}

pub(super) fn render(
    template: &TemplateUpdate,
    context: &NotificationContext<'_>,
) -> Result<(String, String), NotificationError> {
    let render_text = |template: &str| -> Result<String, NotificationError> {
        let mut rest = template;
        let mut output = String::new();
        // 模板已由配置校验；只扫描源模板一次，变量值中的花括号不会再次执行替换。
        while let Some(start) = rest.find("{{") {
            output.push_str(&rest[..start]);
            let token = &rest[start + 2..];
            let Some(end) = token.find("}}") else {
                break;
            };
            let value = context
                .values
                .get(&token[..end])
                .ok_or(NotificationError::CorruptSettings)?;
            output.push_str(&clean(value));
            rest = &token[end + 2..];
        }
        output.push_str(rest);
        Ok(output)
    };
    let symbol = catalog::markers()
        .into_iter()
        .find(|marker| marker.marker_id == template.marker_id)
        .ok_or(NotificationError::CorruptSettings)?
        .symbol;
    let title = render_text(&template.title_template)?;
    let title = if symbol.is_empty() {
        title
    } else {
        format!("{symbol} {title}")
    };
    Ok((
        limit(title, 120),
        limit(render_text(&template.body_template)?, 500),
    ))
}
