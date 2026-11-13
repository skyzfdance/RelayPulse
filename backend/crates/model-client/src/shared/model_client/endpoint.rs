use super::{types::invalid, InputError};
use reqwest::{header::HeaderValue, Url};

pub fn base_url(value: &str) -> Result<Url, InputError> {
    if value.is_empty() || value != value.trim() || value.contains(['\r', '\n', '\t']) {
        return Err(invalid(
            "baseUrl",
            "Base URL 不能为空、首尾空格或包含换行。",
        ));
    }
    let url = Url::parse(value).map_err(|_| invalid("baseUrl", "请输入有效的 Base URL。"))?;
    validate_url(&url)?;
    Ok(url)
}
fn validate_url(url: &Url) -> Result<(), InputError> {
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(invalid("endpoint", "请输入不含凭据的 HTTP 或 HTTPS 地址。"));
    }
    Ok(())
}

/// 只连接原始字符串，不补斜杠、不解析相对路径，也不替换 Base URL。
pub fn endpoint(base: &str, value: &str, custom: bool) -> Result<Url, InputError> {
    if value.trim().is_empty() || value != value.trim() || value.contains(['\r', '\n', '\t']) {
        return Err(invalid("endpoint", "端点不能为空、首尾空格或包含换行。"));
    }
    let parsed_base = base_url(base)?;
    let address = format!("{base}{value}");
    let url = Url::parse(&address).map_err(|_| invalid("endpoint", "端点格式无效。"))?;
    validate_url(&url)?;
    // URL 解析器会消解 ../、反斜杠等输入；拒绝这些地址，避免发送悄悄改写的请求。
    if url.as_str() != address {
        return Err(invalid(
            "endpoint",
            "拼接地址需要 URL 规范化，请直接修正配置；程序不会自动补全或纠正。",
        ));
    }
    if custom && url.origin() != parsed_base.origin() {
        return Err(invalid("endpoint", "端点必须与站点 Base URL 同源。"));
    }
    Ok(url)
}

pub fn credential(value: &str) -> Result<(), InputError> {
    if value.trim().is_empty() || HeaderValue::from_str(value).is_err() {
        return Err(invalid("secret", "请输入有效的访问凭据。"));
    }
    Ok(())
}

pub(super) fn target(value: &super::RequestTarget) -> Result<(), InputError> {
    credential(&value.secret)?;
    base_url(&value.base_url)?;
    headers(&value.headers)
}

pub fn headers(values: &[super::RequestHeader]) -> Result<(), InputError> {
    let mut names = std::collections::HashSet::new();
    for header in values {
        if header.key.is_empty()
            || header.key != header.key.trim()
            || reqwest::header::HeaderName::from_bytes(header.key.as_bytes()).is_err()
            || !names.insert(header.key.to_ascii_lowercase())
        {
            return Err(invalid("headers", "请求头名称无效或重复。"));
        }
        if header.value != header.value.trim()
            || header.value.contains(['\r', '\n'])
            || HeaderValue::from_str(&header.value).is_err()
        {
            return Err(invalid("headers", "请求头值格式无效。"));
        }
    }
    Ok(())
}
