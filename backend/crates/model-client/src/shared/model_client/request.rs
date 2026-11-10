use super::{endpoint, parameters, CheckBody, CheckRequest, EndpointMode, InputError};
use reqwest::Url;
use serde_json::{json, Value};

pub(super) struct PreparedRequest {
    pub url: Url,
    pub body: Value,
    pub custom: bool,
}
pub(super) fn prepare(request: &CheckRequest) -> Result<PreparedRequest, InputError> {
    endpoint::target(&request.target)?;
    let url = endpoint::endpoint(
        &request.target.base_url,
        &request.endpoint,
        matches!(request.endpoint_mode, EndpointMode::SameOriginAppend),
    )?;
    let (values, custom) = match &request.body {
        CheckBody::Custom { parameters } => (parameters, true),
        CheckBody::Standard { parameters } => (parameters, false),
    };
    parameters::parameters(values)?;
    let mut body = json!({});
    parameters::append(&mut body, values, &request.model)?;
    Ok(PreparedRequest { url, body, custom })
}
