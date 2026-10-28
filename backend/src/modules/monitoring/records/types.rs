use super::redaction::{self, Redactor};
use crate::{
    modules::sites::{Model, Site},
    shared::model_client::{
        self, CheckBody, CheckRequest, CheckResult, EndpointMode, InputError, Parameter,
        ParameterType, Protocol,
    },
};
use serde::Serialize;
use serde_json::{json, Value};

/// 执行作用域内冻结的脱敏展示信息；不包含凭据、请求头或请求正文。
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(in crate::modules::monitoring) struct RecordContext {
    pub id: String,
    pub site_id: String,
    pub model_id: String,
    pub site_name: String,
    pub model_name: String,
    pub protocol: Protocol,
    pub run_kind: &'static str,
    pub started_at_ms: u64,
    pub method: &'static str,
    pub endpoint: String,
    pub policy: Value,
    pub slow_first_token_ms: u64,
    pub slow_total_ms: u64,
    pub capture_enabled: bool,
}
impl RecordContext {
    pub(in crate::modules::monitoring) fn new(
        site: &Site,
        model: &Model,
        request: &CheckRequest,
        scheduled: bool,
        capture_enabled: bool,
    ) -> Result<Self, InputError> {
        let redactor = Redactor::new(&request.target.secret);
        let url = model_client::endpoint(
            &request.target.base_url,
            &request.endpoint,
            matches!(request.endpoint_mode, EndpointMode::SameOriginAppend),
        )?;
        let parameters = match &request.body {
            CheckBody::Standard { parameters } | CheckBody::Custom { parameters } => parameters,
        };
        let source = if model.custom_request.is_some() {
            "custom_request"
        } else if model.policy.is_some() {
            "model_policy"
        } else {
            "site_default"
        };
        Ok(Self {
            id: uuid::Uuid::new_v4().to_string(),
            site_id: site.id.clone(),
            model_id: model.id.clone(),
            site_name: redactor.text(&site.name),
            model_name: redactor.text(&model.name),
            protocol: site.protocol,
            run_kind: if scheduled { "scheduled" } else { "manual" },
            started_at_ms: super::super::health::now_ms(),
            method: "POST",
            endpoint: redactor.endpoint(&url),
            policy: json!({"source":source,
                "parameters":parameters.iter().filter(|parameter| model_client::parameter_enabled(parameters, parameter)).map(|parameter| policy_parameter(parameter, &redactor)).collect::<Vec<_>>()}),
            slow_first_token_ms: u64::from(site.connection.slow_first_token_seconds) * 1000,
            slow_total_ms: u64::from(site.connection.slow_total_seconds) * 1000,
            capture_enabled,
        })
    }
}

fn policy_parameter(parameter: &Parameter, redactor: &Redactor) -> Value {
    let value = if redaction::sensitive(&parameter.key) {
        "[REDACTED]".to_owned()
    } else if parameter.kind == ParameterType::Json {
        serde_json::from_str::<Value>(&parameter.value)
            .map(|value| redactor.value(&value).to_string())
            .unwrap_or_else(|_| redactor.text(&parameter.value))
    } else {
        redactor.text(&parameter.value)
    };
    json!({"key":redactor.text(&parameter.key), "value":value, "type":parameter.kind})
}

pub(in crate::modules::monitoring) struct CompletedRecord {
    pub context: RecordContext,
    pub result: CheckResult,
    pub outcome: &'static str,
    pub finished_at_ms: u64,
}
impl CompletedRecord {
    pub(in crate::modules::monitoring) fn new(
        context: RecordContext,
        mut result: CheckResult,
        secret: &str,
        finished_at_ms: u64,
    ) -> Self {
        result.metadata.request_id = result
            .metadata
            .request_id
            .map(|id| Redactor::new(secret).text(&id));
        let outcome = if !result.success {
            "failed"
        } else if super::super::health::is_slow(
            &result,
            context.slow_first_token_ms,
            context.slow_total_ms,
        ) {
            "slow"
        } else {
            "succeeded"
        };
        Self {
            context,
            result,
            outcome,
            finished_at_ms,
        }
    }
}
