//! 仅解码 IPC 的类型、必填字段及封装；业务取值范围由 monitoring 校验。
use crate::{
    commands::error::CommandError,
    modules::monitoring::{ProbeRecordsQuery, RecordError, TokenStatsQuery},
};
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};
use tauri::ipc::{InvokeBody, Request};
fn invalid(field: &'static str) -> CommandError {
    RecordError::Validation {
        field,
        message: "探测记录请求字段缺失、类型错误或含未知字段。",
    }
    .into()
}
fn object(request: Request<'_>, field: &'static str) -> Result<Map<String, Value>, CommandError> {
    match request.body() {
        InvokeBody::Json(Value::Object(object)) => Ok(object.clone()),
        _ => Err(invalid(field)),
    }
}
fn take<T: DeserializeOwned>(
    map: &mut Map<String, Value>,
    key: &str,
    field: &'static str,
) -> Result<T, CommandError> {
    serde_json::from_value(map.remove(key).ok_or_else(|| invalid(field))?)
        .map_err(|_| invalid(field))
}
fn query(request: Request<'_>, allowed: &[&str]) -> Result<Map<String, Value>, CommandError> {
    let mut body = object(request, "query")?;
    let query: Map<String, Value> = take(&mut body, "query", "query")?;
    if !body.is_empty() || query.keys().any(|key| !allowed.contains(&key.as_str())) {
        return Err(invalid("query"));
    }
    Ok(query)
}
pub(super) fn records(request: Request<'_>) -> Result<ProbeRecordsQuery, CommandError> {
    let mut q = query(
        request,
        &["range", "siteId", "modelName", "result", "page", "pageSize"],
    )?;
    Ok(ProbeRecordsQuery {
        range: take(&mut q, "range", "query.range")?,
        site_id: take(&mut q, "siteId", "query.siteId")?,
        model_name: take(&mut q, "modelName", "query.modelName")?,
        result: take(&mut q, "result", "query.result")?,
        page: take(&mut q, "page", "query.page")?,
        page_size: take(&mut q, "pageSize", "query.pageSize")?,
    })
}
pub(super) fn tokens(request: Request<'_>) -> Result<TokenStatsQuery, CommandError> {
    let mut q = query(request, &["range", "siteId", "page", "pageSize"])?;
    Ok(TokenStatsQuery {
        range: take(&mut q, "range", "query.range")?,
        site_id: take(&mut q, "siteId", "query.siteId")?,
        page: take(&mut q, "page", "query.page")?,
        page_size: take(&mut q, "pageSize", "query.pageSize")?,
    })
}
pub(super) fn record_id(request: Request<'_>) -> Result<String, CommandError> {
    let mut body = object(request, "recordId")?;
    let id = take(&mut body, "recordId", "recordId")?;
    if !body.is_empty() {
        return Err(invalid("recordId"));
    }
    Ok(id)
}
pub(super) fn no_args(request: Request<'_>) -> Result<(), CommandError> {
    match request.body() {
        InvokeBody::Json(Value::Null) => Ok(()),
        InvokeBody::Json(Value::Object(object)) if object.is_empty() => Ok(()),
        _ => Err(invalid("request")),
    }
}
