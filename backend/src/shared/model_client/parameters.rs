use super::{types::invalid, InputError, Parameter, ParameterType};
use serde_json::{json, Value};
use std::collections::HashSet;

pub(crate) fn enabled(values: &[Parameter], parameter: &Parameter) -> bool {
    parameter.enabled
        && !values.iter().any(|parent| {
            !parent.enabled
                && !parent.key.is_empty()
                && parameter.key.starts_with(&format!("{}.", parent.key))
        })
}
pub(crate) fn parameters(values: &[Parameter]) -> Result<(), InputError> {
    let mut keys = HashSet::new();
    for value in values.iter().filter(|value| enabled(values, value)) {
        let path: Vec<_> = value.key.split('.').collect();
        if path.iter().any(|s| {
            s.is_empty()
                || *s != s.trim()
                || ["__proto__", "prototype", "constructor"].contains(&s.to_lowercase().as_str())
        }) {
            return Err(invalid("parameters", "参数对象路径无效。"));
        }
        if values.iter().any(|parent| {
            !parent.key.is_empty()
                && value.key.starts_with(&format!("{}.", parent.key))
                && parent.kind != ParameterType::Object
        }) {
            return Err(invalid("parameters", "有子参数的父级须使用 object 类型。"));
        }
        if value.binding.is_some() && value.kind != ParameterType::String {
            return Err(invalid("parameters", "动态模型参数须使用 string 类型。"));
        }
        if !keys.insert(&value.key) {
            return Err(invalid("parameters", "参数字段不能重复。"));
        }
        parameter_value(value)?;
    }
    Ok(())
}
pub(crate) fn parameter_value(value: &Parameter) -> Result<Value, InputError> {
    let bad = || invalid("parameters", "参数值与所选类型不匹配。");
    match value.kind {
        ParameterType::String => Ok(Value::String(value.value.clone())),
        ParameterType::Object => Ok(json!({})),
        ParameterType::Boolean => match value.value.as_str() {
            "true" => Ok(Value::Bool(true)),
            "false" => Ok(Value::Bool(false)),
            _ => Err(bad()),
        },
        ParameterType::Json => serde_json::from_str(&value.value)
            .map(normalize_json_numbers)
            .map_err(|_| bad()),
        ParameterType::Number => {
            // JavaScript Number 接受十进制、指数以及 0x/0o/0b；不接受空值和无穷。
            let text = value.value.trim();
            let number = if let Some((digits, radix)) = [("0x", 16), ("0o", 8), ("0b", 2)]
                .into_iter()
                .find_map(|(prefix, radix)| {
                    text.to_lowercase()
                        .strip_prefix(prefix)
                        .map(|digits| (digits.to_owned(), radix))
                }) {
                if digits.is_empty() {
                    return Err(bad());
                }
                digits
                    .chars()
                    .try_fold(0f64, |n, c| {
                        c.to_digit(radix).map(|d| n * radix as f64 + d as f64)
                    })
                    .ok_or_else(bad)?
            } else {
                text.parse::<f64>().map_err(|_| bad())?
            };
            json_number(number).map(Value::Number).ok_or_else(bad)
        }
    }
}
// Vue 使用 JavaScript Number / JSON.parse；整数不带 .0，大整数采用 IEEE-754 精度。
fn json_number(value: f64) -> Option<serde_json::Number> {
    if value.fract() == 0.0 {
        if value >= i64::MIN as f64 && value < i64::MAX as f64 {
            return Some((value as i64).into());
        }
        if value >= 0.0 && value < u64::MAX as f64 {
            return Some((value as u64).into());
        }
    }
    serde_json::Number::from_f64(value)
}
fn normalize_json_numbers(value: Value) -> Value {
    match value {
        Value::Number(number) => number
            .as_f64()
            .and_then(json_number)
            .map_or(Value::Null, Value::Number),
        Value::Array(values) => {
            Value::Array(values.into_iter().map(normalize_json_numbers).collect())
        }
        Value::Object(values) => Value::Object(
            values
                .into_iter()
                .map(|(key, value)| (key, normalize_json_numbers(value)))
                .collect(),
        ),
        value => value,
    }
}

pub(super) fn append(
    body: &mut Value,
    values: &[Parameter],
    model: &str,
) -> Result<(), InputError> {
    let mut active: Vec<_> = values
        .iter()
        .filter(|value| enabled(values, value))
        .collect();
    active.sort_by_key(|value| value.key.matches('.').count());
    for parameter in active {
        let segments: Vec<_> = parameter.key.split('.').collect();
        let mut target = &mut *body;
        for segment in &segments[..segments.len() - 1] {
            if !target[*segment].is_object() {
                target[*segment] = json!({});
            }
            target = &mut target[*segment];
        }
        target[segments[segments.len() - 1]] = if parameter.binding.is_some() {
            Value::String(model.into())
        } else {
            parameter_value(parameter)?
        };
    }
    Ok(())
}
