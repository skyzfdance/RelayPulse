use crate::{
    commands::error::CommandError,
    modules::operation_logs::{
        OperationLogError, OperationLogQuery, ReadFailureKind, ReadFailureTarget,
    },
};
use serde::{de::DeserializeOwned, Deserialize};
use serde_json::{Map, Value};
use tauri::ipc::{InvokeBody, Request};
fn invalid(field: &'static str) -> CommandError {
    OperationLogError::Validation {
        field,
        message: "操作记录请求字段缺失、类型错误或含未知字段。",
    }
    .into()
}
fn object(request: Request<'_>, field: &'static str) -> Result<Map<String, Value>, CommandError> {
    match request.body() {
        InvokeBody::Json(Value::Object(body)) => Ok(body.clone()),
        _ => Err(invalid(field)),
    }
}
fn take<T: DeserializeOwned>(
    body: &mut Map<String, Value>,
    key: &str,
    field: &'static str,
) -> Result<T, CommandError> {
    serde_json::from_value(body.remove(key).ok_or_else(|| invalid(field))?)
        .map_err(|_| invalid(field))
}
pub(super) fn query(request: Request<'_>) -> Result<OperationLogQuery, CommandError> {
    let mut body = object(request, "query")?;
    let mut q: Map<String, Value> = take(&mut body, "query", "query")?;
    if !body.is_empty()
        || q.keys().any(|key| {
            ![
                "startDate",
                "endDate",
                "source",
                "result",
                "actions",
                "page",
                "pageSize",
            ]
            .contains(&key.as_str())
        })
    {
        return Err(invalid("query"));
    }
    Ok(OperationLogQuery {
        start_date: take(&mut q, "startDate", "query.startDate")?,
        end_date: take(&mut q, "endDate", "query.endDate")?,
        source: take(&mut q, "source", "query.source")?,
        result: take(&mut q, "result", "query.result")?,
        actions: take(&mut q, "actions", "query.actions")?,
        page: take(&mut q, "page", "query.page")?,
        page_size: take(&mut q, "pageSize", "query.pageSize")?,
    })
}
pub(super) fn log_id(request: Request<'_>) -> Result<String, CommandError> {
    let mut body = object(request, "logId")?;
    let id = take(&mut body, "logId", "logId")?;
    if !body.is_empty() {
        return Err(invalid("logId"));
    }
    Ok(id)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ReadFailure {
    pub target: ReadFailureTarget,
    pub kind: ReadFailureKind,
}
pub(super) fn read_failure(request: Request<'_>) -> Result<ReadFailure, CommandError> {
    let mut body = object(request, "input")?;
    let input = take(&mut body, "input", "input")?;
    if !body.is_empty() {
        return Err(invalid("input"));
    }
    Ok(input)
}
