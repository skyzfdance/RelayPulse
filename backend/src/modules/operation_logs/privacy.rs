use super::{ChangeField, OperationEvent, OperationLogContent, SnapshotField};
use serde::Serialize;
use serde_json::Value;

fn sensitive(label: &str) -> bool {
    let label = label.to_ascii_lowercase();
    let normalized = label.replace(['-', '_', ' '], "");
    [
        "secret",
        "password",
        "cookie",
        "authorization",
        "credential",
        "header",
        "api_key",
        "apikey",
        "accesstoken",
        "refreshtoken",
        "access_token",
        "refresh_token",
        "密钥",
        "凭据",
        "请求头",
        "密码",
    ]
    .iter()
    .any(|part| label.contains(part))
        || [
            "apikey",
            "accesskey",
            "accesstoken",
            "refreshtoken",
            "authtoken",
            "sessiontoken",
        ]
        .iter()
        .any(|part| normalized.contains(part))
        || label.split(['.', '[', ']']).any(|part| part == "token")
}
fn permission(label: &str, value: &Value) -> bool {
    label.ends_with("authorization")
        && value.as_str().is_some_and(|value| {
            [
                "unknown",
                "not_requested",
                "denied",
                "granted",
                "unsupported",
            ]
            .contains(&value)
        })
}
fn status(value: &Value) -> String {
    if let Some(value) = value
        .as_str()
        .filter(|value| ["未配置", "已配置", "已更换"].contains(value))
    {
        return value.into();
    }
    if value.is_null()
        || value == &Value::Bool(false)
        || value.as_str().is_some_and(str::is_empty)
        || value.as_array().is_some_and(Vec::is_empty)
    {
        "未配置".into()
    } else {
        "已配置".into()
    }
}
/// URLs in snapshots never retain userinfo, credential query parameters or fragments.
pub(super) fn text(value: &str) -> String {
    let mut remaining = value;
    let mut output = String::new();
    while let Some(start) = remaining
        .to_ascii_lowercase()
        .find("https://")
        .into_iter()
        .chain(remaining.to_ascii_lowercase().find("http://"))
        .min()
    {
        let (prefix, suffix) = remaining.split_at(start);
        output.push_str(prefix);
        let url_end = suffix
            .find([
                '\"', '\'', '<', '>', ' ', '\n', '\r', '\t', '）', ')', '}', ']',
            ])
            .unwrap_or(suffix.len());
        let (raw, trailing) = suffix.split_at(url_end);
        match reqwest::Url::parse(raw) {
            Ok(mut url) => {
                let credentials = !url.username().is_empty() || url.password().is_some();
                let query_credentials = url.query_pairs().any(|(key, _)| {
                    sensitive(&key) || matches!(key.to_ascii_lowercase().as_str(), "key" | "auth")
                });
                if credentials || query_credentials || url.fragment().is_some() {
                    let _ = url.set_username("");
                    let _ = url.set_password(None);
                    if query_credentials {
                        url.set_query(None);
                    }
                    url.set_fragment(None);
                    output.push_str(url.as_str());
                } else {
                    // Preserve the exact rendered notification product when no credential
                    // removal is needed (URL normalization would change its spelling).
                    output.push_str(raw);
                }
            }
            Err(_) => output.push_str("[已隐藏地址]"),
        }
        remaining = trailing;
    }
    output.push_str(remaining);
    output
}
fn flatten(label: &str, value: &Value, fields: &mut Vec<SnapshotField>) {
    if sensitive(label) && !permission(label, value) {
        fields.push(SnapshotField {
            label: label.into(),
            value: status(value),
            sensitive: Some(true),
        });
    } else if let Value::Object(object) = value {
        let protected = object
            .get("key")
            .or_else(|| object.get("name"))
            .and_then(Value::as_str)
            .is_some_and(sensitive);
        for (key, value) in object {
            let path = if label.is_empty() {
                key.clone()
            } else {
                format!("{label}.{key}")
            };
            if key == "value" && protected {
                fields.push(SnapshotField {
                    label: path,
                    value: status(value),
                    sensitive: Some(true),
                });
            } else {
                flatten(&path, value, fields);
            }
        }
    } else if let Value::Array(values) = value {
        for (i, value) in values.iter().enumerate() {
            flatten(&format!("{label}[{i}]"), value, fields);
        }
        if values.is_empty() {
            fields.push(SnapshotField {
                label: label.into(),
                value: "[]".into(),
                sensitive: None,
            });
        }
    } else {
        fields.push(SnapshotField {
            label: label.into(),
            value: safe_text(
                &value
                    .as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| value.to_string()),
            ),
            sensitive: None,
        });
    }
}
pub(crate) fn snapshot_fields(value: &impl Serialize) -> Vec<SnapshotField> {
    let mut fields = Vec::new();
    if let Ok(value) = serde_json::to_value(value) {
        flatten("", &value, &mut fields);
    }
    fields
}
pub(crate) fn changes(before: &impl Serialize, after: &impl Serialize) -> Vec<ChangeField> {
    // Compare original values only while building the diff, before persistence. An
    // unchanged protected field must not appear as an edit; a changed protected value
    // is represented by state rather than either original value.
    fn original_values(
        value: &Value,
        label: &str,
        values: &mut std::collections::HashMap<String, Value>,
    ) {
        values.insert(label.into(), value.clone());
        match value {
            Value::Object(object) => {
                for (key, value) in object {
                    original_values(
                        value,
                        &if label.is_empty() {
                            key.clone()
                        } else {
                            format!("{label}.{key}")
                        },
                        values,
                    );
                }
            }
            Value::Array(items) => {
                for (i, value) in items.iter().enumerate() {
                    original_values(value, &format!("{label}[{i}]"), values);
                }
            }
            _ => {}
        }
    }
    let mut original_before = std::collections::HashMap::new();
    let mut original_after = std::collections::HashMap::new();
    if let Ok(value) = serde_json::to_value(before) {
        original_values(&value, "", &mut original_before);
    }
    if let Ok(value) = serde_json::to_value(after) {
        original_values(&value, "", &mut original_after);
    }
    let old = snapshot_fields(before);
    let new = snapshot_fields(after);
    let mut labels = old
        .iter()
        .chain(new.iter())
        .map(|f| f.label.clone())
        .collect::<Vec<_>>();
    labels.sort();
    labels.dedup();
    labels
        .into_iter()
        .filter_map(|field| {
            let before = old.iter().find(|f| f.label == field);
            let after = new.iter().find(|f| f.label == field);
            let sensitive = before.or(after).and_then(|f| f.sensitive);
            let before = before
                .map(|f| f.value.clone())
                .unwrap_or_else(|| "未配置".into());
            let mut after = after
                .map(|f| f.value.clone())
                .unwrap_or_else(|| "未配置".into());
            let protected_change = sensitive == Some(true)
                && original_before.get(&field) != original_after.get(&field);
            if protected_change && before == after && after == "已配置" {
                after = "已更换".into();
            }
            (before != after || protected_change).then_some(ChangeField {
                field,
                before,
                after,
                sensitive,
            })
        })
        .collect()
}
fn safe_text(value: &str) -> String {
    fn sanitize(value: &mut Value) {
        match value {
            Value::Object(object) => {
                let protected = object
                    .get("key")
                    .or_else(|| object.get("name"))
                    .and_then(Value::as_str)
                    .is_some_and(sensitive);
                for (key, value) in object {
                    if (sensitive(key) && !permission(key, value)) || (key == "value" && protected)
                    {
                        *value = Value::String(status(value));
                    } else {
                        sanitize(value);
                    }
                }
            }
            Value::Array(values) => values.iter_mut().for_each(sanitize),
            Value::String(value) => *value = safe_text(value),
            _ => {}
        }
    }
    if value.starts_with(['{', '[']) {
        if let Ok(mut parsed) = serde_json::from_str::<Value>(value) {
            sanitize(&mut parsed);
            if let Ok(clean) = serde_json::to_string(&parsed) {
                return clean;
            }
        }
    }
    text(value)
}
pub(super) fn sanitize(event: &mut OperationEvent) {
    event.object = text(&event.object);
    event.summary = text(&event.summary);
    event.explanation = text(&event.explanation);
    match &mut event.content {
        OperationLogContent::Snapshot { fields } | OperationLogContent::Deletion { fields, .. } => {
            for field in fields {
                if field.sensitive == Some(true)
                    || (sensitive(&field.label)
                        && !permission(&field.label, &Value::String(field.value.clone())))
                {
                    field.value = if ["未配置", "已配置", "已更换"].contains(&field.value.as_str())
                    {
                        field.value.clone()
                    } else {
                        "已配置".into()
                    };
                    field.sensitive = Some(true);
                } else {
                    field.value = safe_text(&field.value);
                }
            }
        }
        OperationLogContent::Changes { changes } => {
            for field in changes {
                if field.sensitive == Some(true)
                    || (sensitive(&field.field)
                        && !permission(&field.field, &Value::String(field.after.clone()))
                        && !permission(&field.field, &Value::String(field.before.clone())))
                {
                    for value in [&mut field.before, &mut field.after] {
                        if !["未配置", "已配置", "已更换"].contains(&value.as_str()) {
                            *value = "已配置".into();
                        }
                    }
                    field.sensitive = Some(true);
                } else {
                    field.before = safe_text(&field.before);
                    field.after = safe_text(&field.after);
                }
            }
        }
        _ => {}
    }
    // Apply URL sanitation to every string including notification products and failure reasons.
    fn clean(value: &mut Value) {
        match value {
            Value::String(value) => *value = text(value),
            Value::Array(values) => values.iter_mut().for_each(clean),
            Value::Object(values) => values.values_mut().for_each(clean),
            _ => {}
        }
    }
    if let Ok(mut content) = serde_json::to_value(&event.content) {
        clean(&mut content);
        if let Ok(content) = serde_json::from_value(content) {
            event.content = content;
        }
    }
}
