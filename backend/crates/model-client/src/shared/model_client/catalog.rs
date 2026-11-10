use super::{endpoint, ClientError};
use serde_json::Value;
use std::collections::HashSet;

pub(super) async fn discover(
    client: &super::ModelClient,
    request: &super::CatalogRequest,
    options: super::ExecutionOptions,
) -> Result<Vec<String>, ClientError> {
    endpoint::target(&request.target)?;
    let mut url = endpoint::endpoint(&request.target.base_url, &request.endpoint, false)?;
    let mut names = Vec::new();
    let mut seen = HashSet::new();
    let mut cursors = HashSet::new();
    // Anthropic 目录支持 after_id；每页和全量均有边界，失败不返回部分目录。
    for _ in 0..100 {
        let response = client
            .transport
            .send(&request.target, url.clone(), None, options)
            .await?;
        if !response.status().is_success() {
            return Err(ClientError::Status(response.status().as_u16()));
        }
        let bytes = client
            .transport
            .read_buffered(response, 2 * 1024 * 1024, options)
            .await?;
        let value: Value = serde_json::from_slice(&bytes).map_err(|_| ClientError::Response)?;
        for name in parse(&value)? {
            if seen.insert(name.clone()) {
                names.push(name);
            }
            if names.len() > 10000 {
                return Err(ClientError::Response);
            }
        }
        if value.get("has_more").and_then(Value::as_bool) != Some(true) {
            return Ok(names);
        }
        let cursor = value["last_id"]
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or(ClientError::Response)?;
        if !cursors.insert(cursor.to_string()) {
            return Err(ClientError::Response);
        }
        let pairs: Vec<_> = url
            .query_pairs()
            .filter(|(key, _)| key != "after_id")
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect();
        url.set_query(None);
        url.query_pairs_mut()
            .extend_pairs(pairs)
            .append_pair("after_id", cursor);
    }
    Err(ClientError::Response)
}
fn parse(value: &Value) -> Result<Vec<String>, ClientError> {
    let items = value
        .as_array()
        .or_else(|| {
            value
                .get("data")
                .or_else(|| value.get("models"))
                .and_then(Value::as_array)
        })
        .ok_or(ClientError::Response)?;
    if items.len() > 10000 {
        return Err(ClientError::Response);
    }
    items
        .iter()
        .map(|item| {
            let name = item
                .as_str()
                .or_else(|| item.get("id").and_then(Value::as_str))
                .or_else(|| item.get("name").and_then(Value::as_str))
                .ok_or(ClientError::Response)?;
            if name.trim().is_empty() || name != name.trim() {
                return Err(ClientError::Response);
            }
            Ok(name.to_string())
        })
        .collect()
}
