//! IPC JSON 结构解码；不依赖业务错误、字段白名单或操作审计。
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};
use tauri::ipc::{InvokeBody, Request};

pub(super) enum DecodeError {
    Body,
    Fields,
}

pub(super) fn decode<T: DeserializeOwned>(request: Request<'_>) -> Result<T, DecodeError> {
    let InvokeBody::Json(value) = request.body() else {
        return Err(DecodeError::Body);
    };
    serde_json::from_value(value.clone()).map_err(|_| DecodeError::Fields)
}

pub(super) fn object(request: Request<'_>) -> Result<Map<String, Value>, DecodeError> {
    match request.body() {
        InvokeBody::Json(Value::Object(fields)) => Ok(fields.clone()),
        _ => Err(DecodeError::Body),
    }
}

pub(super) fn take<T: DeserializeOwned>(
    fields: &mut Map<String, Value>,
    key: &str,
) -> Result<T, DecodeError> {
    serde_json::from_value(fields.remove(key).ok_or(DecodeError::Fields)?)
        .map_err(|_| DecodeError::Fields)
}

pub(super) fn no_args(request: Request<'_>) -> Result<(), DecodeError> {
    match request.body() {
        InvokeBody::Json(Value::Null) => Ok(()),
        InvokeBody::Json(Value::Object(fields)) if fields.is_empty() => Ok(()),
        _ => Err(DecodeError::Fields),
    }
}
