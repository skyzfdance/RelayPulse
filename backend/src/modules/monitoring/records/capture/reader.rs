//! 逐行读取已有 JSONL。仅失败响应需要正文投影，正常响应不构造合并流。
use super::super::{
    dto::{ProbeRecordResponseChunk, RecordHeader},
    redaction,
};
use super::CaptureStore;
use crate::shared::file_output::{filesystem, OutputError};
use serde::{de::DeserializeOwned, Deserialize};
use serde_json::{value::RawValue, Value};
use std::{
    fs::File,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
};
type Result<T> = std::result::Result<T, OutputError>;

#[derive(Default)]
pub(in crate::modules::monitoring) struct CapturedRecord {
    pub method: Option<String>,
    pub request_headers: Option<Vec<RecordHeader>>,
    pub request_body: Option<String>,
    pub response_started: bool,
    pub response_kind: Option<String>,
    pub response_headers: Option<Vec<RecordHeader>>,
    pub chunks: Option<Vec<ProbeRecordResponseChunk>>,
    pub received_bytes: Option<u64>,
    pub body_complete: Option<bool>,
    pub finished: bool,
    pub response_body: Option<String>,
    pub path: PathBuf,
}
impl CaptureStore {
    pub(in crate::modules::monitoring) fn read(
        &self,
        id: &str,
        body: bool,
    ) -> Result<CapturedRecord> {
        self.output.inspect(id, |path| read(path, id, body))
    }
    pub(in crate::modules::monitoring) fn delete(&self, id: String) -> Result<()> {
        self.output.remove_ids(&[id])
    }
    pub(in crate::modules::monitoring) fn clear(&self) -> Result<()> {
        self.output.remove_all()
    }
}
fn read(directory: &Path, id: &str, include_body: bool) -> Result<CapturedRecord> {
    let path = directory.join("response.jsonl");
    filesystem::regular_file(&path)?.ok_or(OutputError)?;
    let input = File::open(&path)?;
    let mut result = CapturedRecord {
        path,
        ..Default::default()
    };
    let mut chunks = Vec::new();
    let mut body = String::new();
    let mut body_saved = false;
    let mut binary = false;
    let mut seen = false;
    let mut request_started = false;
    let mut fragment: Option<(Option<u64>, u64)> = None;
    for line in BufReader::new(input).lines() {
        let line = line?;
        let event: Value = serde_json::from_str(&line).map_err(|_| OutputError)?;
        let name = event
            .get("event")
            .and_then(Value::as_str)
            .ok_or(OutputError)?;
        seen |= matches!(
            name,
            "record_created" | "request_started" | "response_started" | "request_finished"
        );
        match name {
            "record_created" => {
                if let Some(saved_id) = optional::<String>(&event, "recordId")? {
                    if saved_id != id {
                        return Err(OutputError);
                    }
                }
                if optional::<u64>(&event, "formatVersion")?.is_some_and(|v| !(1..=2).contains(&v))
                {
                    return Err(OutputError);
                }
            }
            "request_started" => {
                if request_started {
                    return Err(OutputError);
                }
                request_started = true;
                result.method = optional(&event, "method")?;
                result.request_headers = headers(&event)?;
                // RawValue 保留文件中的字段顺序、空白和字符串内容；不重新排版请求快照。
                #[derive(Deserialize)]
                struct RequestBody<'a> {
                    #[serde(borrow)]
                    body: Option<&'a RawValue>,
                }
                let saved: RequestBody<'_> =
                    serde_json::from_str(&line).map_err(|_| OutputError)?;
                result.request_body = saved.body.map(|raw| display_raw(raw.get())).transpose()?;
            }
            "response_started" => {
                if result.response_started {
                    return Err(OutputError);
                }
                result.response_started = true;
                result.response_kind = optional(&event, "kind")?;
                if result
                    .response_kind
                    .as_deref()
                    .is_some_and(|kind| !matches!(kind, "event_stream" | "json" | "other"))
                {
                    return Err(OutputError);
                }
                optional::<u16>(&event, "statusCode")?;
                result.response_headers = headers(&event)?;
            }
            "response_chunk" => {
                if result.finished {
                    return Err(OutputError);
                }
                chunks.push(ProbeRecordResponseChunk {
                    index: chunks.len() as u64 + 1,
                    elapsed_ms: optional(&event, "elapsedMs")?,
                    bytes: optional(&event, "receivedBytes")?,
                });
                if let Some(total) = optional(&event, "receivedTotal")? {
                    result.received_bytes = Some(total);
                }
            }
            "request_finished" => {
                if result.finished || fragment.is_some() {
                    return Err(OutputError);
                }
                result.finished = true;
                result.body_complete = optional(&event, "bodyComplete")?;
                if let Some(total) = optional(&event, "receivedTotal")? {
                    result.received_bytes = Some(total);
                }
            }
            "text" | "json_fragment" | "sse_fragment" => {
                let text = event
                    .get("text")
                    .and_then(Value::as_str)
                    .ok_or(OutputError)?;
                if name != "text" {
                    let event_id = if name == "sse_fragment" {
                        Some(optional::<u64>(&event, "eventId")?.ok_or(OutputError)?)
                    } else {
                        None
                    };
                    let index = optional::<u64>(&event, "index")?.ok_or(OutputError)?;
                    if fragment.unwrap_or((event_id, 0)) != (event_id, index) {
                        return Err(OutputError);
                    }
                    fragment = if optional::<bool>(&event, "last")?.ok_or(OutputError)? {
                        None
                    } else {
                        Some((event_id, index + 1))
                    };
                }
                body_saved = true;
                if include_body {
                    body.push_str(text);
                }
            }
            "json" => {
                #[derive(Deserialize)]
                struct JsonBody<'a> {
                    #[serde(borrow)]
                    value: &'a RawValue,
                }
                let saved: JsonBody<'_> = serde_json::from_str(&line).map_err(|_| OutputError)?;
                body_saved = true;
                if include_body {
                    body.push_str(saved.value.get());
                }
            }
            "sse" => {
                body_saved = true;
                let text = sse(&event)?;
                if include_body {
                    body.push_str(&text);
                }
            }
            "binary" => {
                binary = true;
                let name = event
                    .get("file")
                    .and_then(Value::as_str)
                    .ok_or(OutputError)?;
                if name.is_empty()
                    || name.starts_with('.')
                    || !name
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_'))
                {
                    return Err(OutputError);
                }
                let meta = filesystem::regular_file(&directory.join(name))?.ok_or(OutputError)?;
                let offset = optional::<u64>(&event, "offset")?.ok_or(OutputError)?;
                let bytes = optional::<u64>(&event, "bytes")?.ok_or(OutputError)?;
                if offset.checked_add(bytes).is_none_or(|end| end > meta.len()) {
                    return Err(OutputError);
                }
            }
            "request_context" | "encoding_fallback" => {}
            // 未知事件可来自后续兼容扩展，但仍要求每一行是有效 JSON 事件。
            _ => {}
        }
    }
    if !seen {
        return Err(OutputError);
    }
    if !chunks.is_empty()
        || (result.response_started && result.finished && result.received_bytes == Some(0))
    {
        result.chunks = Some(chunks);
    }
    if include_body
        && !binary
        && (body_saved || (result.response_started && result.received_bytes == Some(0)))
    {
        result.response_body = Some(body);
    }
    Ok(result)
}
fn optional<T: DeserializeOwned>(event: &Value, key: &str) -> Result<Option<T>> {
    event
        .get(key)
        .filter(|v| !v.is_null())
        .map(|v| serde_json::from_value(v.clone()).map_err(|_| OutputError))
        .transpose()
}
fn headers(event: &Value) -> Result<Option<Vec<RecordHeader>>> {
    let mut headers = optional::<Vec<RecordHeader>>(event, "headers")?;
    if let Some(headers) = &mut headers {
        for header in headers {
            if redaction::sensitive_header(&header.name) {
                header.value = "[REDACTED]".into();
            }
        }
    }
    Ok(headers)
}
fn display_raw(raw: &str) -> Result<String> {
    if raw.starts_with('"') {
        serde_json::from_str(raw).map_err(|_| OutputError)
    } else {
        Ok(raw.into())
    }
}
fn sse(event: &Value) -> Result<String> {
    let mut text = String::new();
    for comment in optional::<Vec<String>>(event, "comments")?.unwrap_or_default() {
        text.push_str(&format!(": {comment}\n"));
    }
    if let Some(kind) = optional::<String>(event, "eventType")? {
        text.push_str(&format!("event: {kind}\n"));
    }
    if let Some(id) = optional::<String>(event, "id")? {
        text.push_str(&format!("id: {id}\n"));
    }
    for field in optional::<Vec<RecordHeader>>(event, "fields")?.unwrap_or_default() {
        text.push_str(&format!("{}: {}\n", field.name, field.value));
    }
    if let Some(data) = event.get("data").filter(|v| !v.is_null()) {
        let data = match data {
            Value::String(text) => text.clone(),
            _ => data.to_string(),
        };
        for line in data.split('\n') {
            text.push_str(&format!("data: {line}\n"));
        }
    }
    if optional::<bool>(event, "terminated")? == Some(true) {
        text.push('\n');
    }
    Ok(text)
}
