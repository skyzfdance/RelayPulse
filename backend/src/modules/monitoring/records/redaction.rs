//! 正式探测文件不保存当前站点凭据的直接或 JSON 转义回显；跨网络块也能识别。
use serde_json::Value;
use zeroize::Zeroizing;
pub(super) struct Redactor {
    patterns: Vec<Zeroizing<Vec<u8>>>,
    pending: Zeroizing<Vec<u8>>,
    keep: usize,
}
impl Redactor {
    pub(super) fn new(secret: &str) -> Self {
        let escaped = serde_json::to_string(secret).expect("string serializes");
        let mut patterns = vec![
            Zeroizing::new(secret.as_bytes().to_vec()),
            Zeroizing::new(escaped.as_bytes()[1..escaped.len() - 1].to_vec()),
        ];
        patterns.retain(|p| !p.is_empty());
        patterns.sort_by_key(|p| std::cmp::Reverse(p.len()));
        patterns.dedup();
        let keep = patterns.iter().map(|p| p.len()).max().unwrap_or(1);
        Self {
            patterns,
            pending: Zeroizing::new(vec![]),
            keep,
        }
    }
    pub(super) fn text(&self, value: &str) -> String {
        let mut redactor = Self {
            patterns: self.patterns.clone(),
            pending: Zeroizing::new(vec![]),
            keep: self.keep,
        };
        String::from_utf8(redactor.push(value.as_bytes(), true)).expect("UTF-8 redaction")
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
        self.pending.extend_from_slice(bytes);
        let mut output = vec![];
        let mut position = 0;
        while position < self.pending.len()
            && (final_chunk || self.pending.len() - position >= self.keep)
        {
            if let Some(pattern) = self
                .patterns
                .iter()
                .find(|p| self.pending[position..].starts_with(p))
            {
                output.extend_from_slice(b"[REDACTED]");
                position += pattern.len();
            } else {
                output.push(self.pending[position]);
                position += 1;
            }
        }
        self.pending.drain(..position);
        output
    }
}

/// 除已知站点凭据外，头中独立的代理认证、Cookie、API key 和令牌也不能落盘。
pub(super) fn sensitive_header(name: &str) -> bool {
    let name = name.to_ascii_lowercase().replace(['-', '_'], "");
    sensitive(&name)
        || name.contains("authorization")
        || name.contains("apikey")
        || name.contains("token")
        || name.contains("secret")
        || name.contains("password")
        || name.contains("credential")
        || name == "xauth"
}

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
