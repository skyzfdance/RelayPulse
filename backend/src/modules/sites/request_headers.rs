//! 站点探针策略请求头的业务约束。
use super::{Parameter, RequestHeader, Result};

pub(crate) fn configured_request_headers(values: &[Parameter]) -> Result<Vec<RequestHeader>> {
    let headers = crate::shared::model_client::header_parameters(values)?;
    for header in &headers {
        let key = header.key.to_ascii_lowercase();
        if [
            "authorization",
            "x-api-key",
            "cookie",
            "proxy-authorization",
        ]
        .contains(&key.as_str())
        {
            return Err(super::invalid("headers", "访问凭据请使用专用凭据字段。"));
        }
    }
    Ok(headers)
}
