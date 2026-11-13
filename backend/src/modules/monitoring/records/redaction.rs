//! 正式探测文件不保存当前站点凭据的直接或 JSON 转义回显；跨网络块也能识别。
use crate::shared::text_redaction::TextRedactor;
use serde_json::Value;
pub(super) struct Redactor {
    text: TextRedactor,
}
impl Redactor {
    pub(super) fn new(secret: &str) -> Self {
        Self {
            text: TextRedactor::default().extended([secret]),
        }
    }
    pub(super) fn text(&self, value: &str) -> String {
        self.text.text(value)
    }
    pub(super) fn include_headers(&mut self, headers: &reqwest::header::HeaderMap) {
        self.text = self
            .text
            .extended(headers.iter().filter_map(|(name, value)| {
                (value.is_sensitive() || sensitive_header(name.as_str()))
                    .then(|| value.to_str().ok())
                    .flatten()
            }));
    }
    /// 请求快照还屏蔽常见凭据字段；响应正文仍按已知站点凭据做跨块脱敏。
    pub(super) fn value(&self, value: &Value) -> Value {
        match value {
            Value::Object(fields) => Value::Object(
                fields
                    .iter()
                    .map(|(key, value)| {
                        let value = if sensitive(key) {
                            Value::String("[REDACTED]".into())
                        } else {
                            self.value(value)
                        };
                        (self.text(key), value)
                    })
                    .collect(),
            ),
            Value::Array(values) => {
                Value::Array(values.iter().map(|value| self.value(value)).collect())
            }
            Value::String(text) => Value::String(self.text(text)),
            _ => value.clone(),
        }
    }
    pub(super) fn endpoint(&self, url: &reqwest::Url) -> String {
        let mut url = url.clone();
        url.set_query(None);
        url.set_fragment(None);
        self.text(url.as_str())
    }
    pub(super) fn headers(&self, headers: &reqwest::header::HeaderMap) -> Value {
        Value::Array(
            headers
                .iter()
                .map(|(name, value)| {
                    let name = name.as_str();
                    let value = if value.is_sensitive() || sensitive_header(name) {
                        "[REDACTED]".into()
                    } else {
                        self.text(&String::from_utf8_lossy(value.as_bytes()))
                    };
                    serde_json::json!({"name":self.text(name), "value":value})
                })
                .collect(),
        )
    }
    pub(super) fn push(&mut self, bytes: &[u8], final_chunk: bool) -> Vec<u8> {
        self.text.push(bytes, final_chunk)
    }
}

pub(super) use crate::modules::sites::sensitive_request_header as sensitive_header;

pub(super) fn sensitive(key: &str) -> bool {
    let key: String = key
        .rsplit('.')
        .next()
        .unwrap_or(key)
        .chars()
        .filter(|c| !matches!(c, '_' | '-'))
        .flat_map(char::to_lowercase)
        .collect();
    matches!(
        key.as_str(),
        "authorization"
            | "apikey"
            | "xapikey"
            | "accesstoken"
            | "refreshtoken"
            | "token"
            | "password"
            | "secret"
            | "credential"
            | "credentials"
            | "cookie"
            | "setcookie"
    )
}
