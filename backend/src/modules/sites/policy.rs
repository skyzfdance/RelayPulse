//! 默认值仅在创建配置时载入；旧 JSON 在读取时转换为完整参数表。
use super::{
    CustomRequest, Parameter, ParameterType, ProbePolicy, Protocol, Result, SiteConfigError,
};
use crate::shared::model_client::parameter_value;
use serde::Deserialize;
use serde_json::{json, Value};

impl ProbePolicy {
    pub(crate) fn for_protocol(protocol: Protocol) -> Self {
        Self {
            version: 2,
            parameters: flatten(default_body(protocol, "只回复 OK", 8)),
        }
    }
}
impl Default for ProbePolicy {
    fn default() -> Self {
        Self::for_protocol(Protocol::OpenaiCompatible)
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LegacyPolicy {
    prompt: String,
    max_tokens: u16,
    parameters: Vec<Parameter>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyCustom {
    endpoint: String,
    parameters: Vec<Parameter>,
}

pub(super) fn decode_policy(text: &str, protocol: Protocol) -> Result<ProbePolicy> {
    let value: Value = serde_json::from_str(text).map_err(|_| SiteConfigError::Storage)?;
    if value.get("version").is_some() {
        return serde_json::from_value(value).map_err(|_| SiteConfigError::Storage);
    }
    let legacy: LegacyPolicy =
        serde_json::from_value(value).map_err(|_| SiteConfigError::Storage)?;
    let mut body = default_body(protocol, &legacy.prompt, legacy.max_tokens);
    append_legacy(&mut body, &legacy.parameters)?;
    Ok(ProbePolicy {
        version: 2,
        parameters: flatten(body),
    })
}
pub(super) fn decode_custom(text: &str) -> Result<CustomRequest> {
    let value: Value = serde_json::from_str(text).map_err(|_| SiteConfigError::Storage)?;
    if value.get("version").is_some() {
        return serde_json::from_value(value).map_err(|_| SiteConfigError::Storage);
    }
    let legacy: LegacyCustom =
        serde_json::from_value(value).map_err(|_| SiteConfigError::Storage)?;
    let mut body = json!({"model":""});
    append_legacy(&mut body, &legacy.parameters)?;
    Ok(CustomRequest {
        version: 2,
        endpoint: legacy.endpoint,
        parameters: flatten(body),
    })
}

fn default_body(protocol: Protocol, prompt: &str, max_tokens: u16) -> Value {
    match protocol {
        Protocol::OpenaiCompatible => {
            json!({"model":"", "messages":[{"role":"user","content":prompt}], "stream":true, "max_tokens":max_tokens, "reasoning_effort":"low"})
        }
        Protocol::OpenaiResponses => {
            json!({"model":"", "input":prompt, "stream":true, "max_output_tokens":max_tokens, "reasoning":{"effort":"low"}})
        }
        Protocol::Anthropic => {
            json!({"model":"", "messages":[{"role":"user","content":prompt}], "stream":true, "max_tokens":max_tokens})
        }
        Protocol::CustomHttp => json!({"model":""}),
    }
}

// 旧的父级 JSON 参数允许覆盖已有对象，转换时保留原先的顺序与最终请求。
fn append_legacy(body: &mut Value, parameters: &[Parameter]) -> Result<()> {
    for parameter in parameters {
        let path: Vec<_> = parameter.key.split('.').collect();
        let mut target = &mut *body;
        for segment in &path[..path.len() - 1] {
            if !target[*segment].is_object() {
                target[*segment] = json!({});
            }
            target = &mut target[*segment];
        }
        target[path[path.len() - 1]] = parameter_value(parameter)?;
    }
    Ok(())
}

fn flatten(body: Value) -> Vec<Parameter> {
    fn append(parameters: &mut Vec<Parameter>, key: String, value: Value) {
        if let Value::Object(children) = value {
            // JSON 对象里的点号等字面键不能转换为路径，保留整个 JSON 值。
            if children.keys().any(|child| {
                child.is_empty()
                    || child != child.trim()
                    || child.contains('.')
                    || ["__proto__", "prototype", "constructor"]
                        .contains(&child.to_lowercase().as_str())
            }) {
                parameters.push(Parameter::new(
                    key,
                    Value::Object(children).to_string(),
                    ParameterType::Json,
                ));
                return;
            }
            parameters.push(Parameter::new(&key, "{}", ParameterType::Object));
            for (child, value) in children {
                append(parameters, format!("{key}.{child}"), value);
            }
            return;
        }
        let (kind, value) = match value {
            Value::String(value) => (ParameterType::String, value),
            Value::Number(value) => (ParameterType::Number, value.to_string()),
            Value::Bool(value) => (ParameterType::Boolean, value.to_string()),
            value => (ParameterType::Json, value.to_string()),
        };
        let mut parameter = if key == "model" {
            Parameter::model()
        } else {
            Parameter::new(&key, "", kind)
        };
        parameter.value = value;
        parameters.push(parameter);
    }
    let mut parameters = vec![];
    if let Value::Object(mut fields) = body {
        for key in [
            "model",
            "input",
            "messages",
            "stream",
            "max_output_tokens",
            "max_tokens",
            "reasoning",
            "reasoning_effort",
        ] {
            if let Some(value) = fields.remove(key) {
                append(&mut parameters, key.into(), value);
            }
        }
        for (key, value) in fields {
            append(&mut parameters, key, value);
        }
    }
    parameters
}
