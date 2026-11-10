use super::{types::invalid, InputError};
use reqwest::{header::HeaderValue, Url};

pub fn base_url(value: &str) -> Result<Url, InputError> {
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

/// 端点路径追加到 Base URL，保留其路径前缀；自定义端点另校验同源。
pub fn endpoint(base: &str, value: &str, custom: bool) -> Result<Url, InputError> {
    if value.trim().is_empty()
        || value != value.trim()
        || value.contains(['\r', '\n'])
        || (custom && value.contains('\t'))
    {
        return Err(invalid("endpoint", "端点不能为空、首尾空格或包含换行。"));
    }
    let base = base_url(base)?;
    let mut resolution_base = base.clone();
    resolution_base.set_path(&format!("{}/", base.path().trim_end_matches('/')));
    // 完整地址继续按原规则处理；普通路径只归一化连接处的斜杠，不覆盖基址路径。
    let value = if value.starts_with("//") {
        value
    } else {
        value.trim_start_matches('/')
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
