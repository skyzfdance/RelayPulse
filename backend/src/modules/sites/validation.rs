use super::*;
use reqwest::Url;
use std::collections::HashSet;

pub(crate) fn name(value: &str) -> Result<String> {
    let value = value.trim();
    if value.is_empty() {
        return Err(invalid("name", "名称不能为空。"));
    }
    Ok(value.into())
}

pub(crate) fn base_url(value: &str) -> Result<Url> {
    crate::shared::model_client::base_url(value).map_err(Into::into)
}
pub(crate) fn endpoint(base: &str, value: &str, custom: bool) -> Result<Url> {
    crate::shared::model_client::endpoint(base, value, custom).map_err(Into::into)
}
pub(crate) fn credential(value: &str) -> Result<()> {
    crate::shared::model_client::credential(value).map_err(Into::into)
}

pub(crate) fn connection(value: &Connection, base: &str) -> Result<()> {
    endpoint(base, &value.models_path, false)?;
    endpoint(base, &value.probe_path, false)?;
    for (number, max, field) in [
        (value.interval_seconds, 86400, "intervalSeconds"),
        (
            value.slow_first_token_seconds.into(),
            100,
            "slowFirstTokenSeconds",
        ),
        (value.slow_total_seconds.into(), 500, "slowTotalSeconds"),
        (value.failure_threshold.into(), 10, "failureThreshold"),
        (value.recovery_threshold.into(), 10, "recoveryThreshold"),
    ] {
        if number == 0 || number > max {
            return Err(invalid(field, "请输入范围内的整数。"));
        }
    }
    crate::shared::model_client::validate_headers(&value.headers)?;
    for header in &value.headers {
        let key = header.key.to_ascii_lowercase();
        if [
            "authorization",
            "x-api-key",
            "cookie",
            "proxy-authorization",
        ]
        .contains(&key.as_str())
        {
            return Err(invalid("headers", "访问凭据请使用专用凭据字段。"));
        }
    }
    Ok(())
}

pub(crate) fn policy(value: &ProbePolicy, _protocol: Protocol) -> Result<()> {
    if value.version != 2 {
        return Err(invalid("policy", "请求参数配置版本无效。"));
    }
    parameters(&value.parameters)
}
pub(crate) fn custom(value: &CustomRequest, base: &str) -> Result<()> {
    if value.version != 2 {
        return Err(invalid("customRequest", "请求参数配置版本无效。"));
    }
    endpoint(base, &value.endpoint, true)?;
    parameters(&value.parameters)
}
pub(crate) fn parameters(values: &[Parameter]) -> Result<()> {
    crate::shared::model_client::parameters(values).map_err(Into::into)
}
pub(crate) fn names_match(models: &[Model], supplied: &[String]) -> Result<()> {
    let names: HashSet<_> = supplied.iter().map(String::as_str).collect();
    if names.len() != supplied.len()
        || names.len() != models.len()
        || models.iter().any(|m| !names.contains(m.name.as_str()))
    {
        return Err(SiteConfigError::Stale);
    }
    Ok(())
}
