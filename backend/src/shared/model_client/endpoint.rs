use super::{types::invalid, InputError};
use reqwest::{header::HeaderValue, Url};

pub(crate) fn base_url(value: &str) -> Result<Url, InputError> {
    let url =
        Url::parse(value.trim()).map_err(|_| invalid("baseUrl", "请输入有效的 Base URL。"))?;
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

/// 高级端点遵循 new URL(endpoint, baseUrl)；自定义端点遵循前端追加路径并校验同源。
pub(crate) fn endpoint(base: &str, value: &str, custom: bool) -> Result<Url, InputError> {
    if value.trim().is_empty()
        || value != value.trim()
        || value.contains(['\r', '\n'])
        || (custom && value.contains('\t'))
    {
        return Err(invalid("endpoint", "端点不能为空、首尾空格或包含换行。"));
    }
    let base = base_url(base)?;
    let resolution_base = if custom && !value.starts_with('/') {
        Url::parse(&format!("{}/", base.as_str().trim_end_matches('/')))
            .map_err(|_| invalid("endpoint", "探测端点格式无效。"))?
    } else {
        base.clone()
    };
    let url = resolution_base
        .join(value)
        .map_err(|_| invalid("endpoint", "端点格式无效。"))?;
    validate_url(&url)?;
    if custom && url.origin() != base.origin() {
        return Err(invalid("endpoint", "端点必须与站点 Base URL 同源。"));
    }
    Ok(url)
}

pub(crate) fn credential(value: &str) -> Result<(), InputError> {
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

pub(crate) fn headers(values: &[super::RequestHeader]) -> Result<(), InputError> {
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
