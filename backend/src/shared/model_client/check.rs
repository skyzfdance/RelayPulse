use super::{
    capture, request::PreparedRequest, stream, CaptureEvent, CaptureSink, CheckFailure,
    CheckResult, ClientError, ExecutionOptions, Protocol, RequestTarget, ResponseKind,
    ResponseMetadata,
};
use futures_util::StreamExt;
use std::time::Instant;

// 只限制整体 JSON 解析缓存；可选记录仍先收到原始数据块。
const MAX_BUFFERED_JSON_BYTES: usize = 16 * 1024 * 1024;
impl super::ModelClient {
    pub(super) async fn perform_check(
        &self,
        config: &RequestTarget,
        request: PreparedRequest,
        options: ExecutionOptions,
        capture: &mut Option<&mut dyn CaptureSink>,
        start: Instant,
        metadata: &mut ResponseMetadata,
    ) -> Result<CheckResult, ClientError> {
        let mut status = None;
        let mut first = None;
        let result = async {
            let http = self
                .transport
                .request(config, request.url, Some(&request.body), options)?;
            capture::emit(
                capture,
                CaptureEvent::Started {
                    protocol: config.protocol,
                    method: "POST",
                    url: http.url().as_str(),
                    body: &request.body,
                    headers: http.headers(),
                },
            )
            .await?;
            let response = self.transport.execute(http).await?;
            let code = response.status().as_u16();
            status = Some(code);
            metadata.request_id = ["x-request-id", "request-id", "anthropic-request-id"]
                .iter()
                .find_map(|name| {
                    response
                        .headers()
                        .get(*name)
                        .and_then(|value| value.to_str().ok())
                })
                .filter(|value| !value.is_empty())
                .map(str::to_owned);
            let successful = response.status().is_success();
            let raw_content_type = response
                .headers()
                .get("content-type")
                .and_then(|value| value.to_str().ok())
                .unwrap_or("");
            let content_type = raw_content_type.split(';').next().unwrap_or("").trim();
            let sse = content_type.eq_ignore_ascii_case("text/event-stream");
            let kind = if sse {
                ResponseKind::EventStream
            } else if content_type.eq_ignore_ascii_case("application/json") {
                ResponseKind::Json
            } else {
                ResponseKind::Other
            };
            metadata.kind = Some(kind);
            capture::emit(
                capture,
                CaptureEvent::Response {
                    status: code,
                    kind,
                    content_type: raw_content_type,
                    headers: response.headers(),
                    elapsed_ms: elapsed(start),
                },
            )
            .await?;
            if !successful && capture.is_none() {
                return Err(ClientError::Status(code));
            }
            let parse = successful && !request.custom;
            let mut parsing_failed = parse
                && !sse
                && response
                    .content_length()
                    .is_some_and(|n| n > MAX_BUFFERED_JSON_BYTES as u64);
            if parsing_failed && capture.is_none() {
                return Err(ClientError::Response);
            }
            let mut content = stream::Content::default();
            let mut decoder = stream::Sse::default();
            let mut body = Vec::new();
            let mut bytes = response.bytes_stream();
            while let Some(chunk) = tokio::time::timeout(options.idle_timeout, bytes.next())
                .await
                .map_err(|_| ClientError::Timeout)?
            {
                let chunk = chunk.map_err(ClientError::transport)?;
                let received_ms = elapsed(start);
                metadata.received_bytes =
                    metadata.received_bytes.saturating_add(chunk.len() as u64);
                // 先交付原始字节，再进行解析。等待写入形成背压，不产生无界待写队列。
                capture::emit(
                    capture,
                    CaptureEvent::Chunk {
                        bytes: &chunk,
                        elapsed_ms: received_ms,
                    },
                )
                .await?;
                if !parse || parsing_failed {
                    continue;
                }
                let parsed = if sse {
                    let parsed = observe_events(
                        &mut decoder,
                        &mut content,
                        &mut first,
                        config.protocol,
                        &chunk,
                        received_ms,
                    );
                    metadata.usage = content.usage.clone();
                    parsed
                } else if body.len().saturating_add(chunk.len()) > MAX_BUFFERED_JSON_BYTES {
                    Err(ClientError::Response)
                } else {
                    body.extend_from_slice(&chunk);
                    Ok(())
                };
                if let Err(error) = parsed {
                    if capture.is_none() {
                        return Err(error);
                    }
                    // 解析失败不伪造成功；记录开启时继续接收，保留后续未知事件与错误正文。
                    parsing_failed = true;
                }
            }
            metadata.body_complete = true;
            if !successful {
                return Err(ClientError::Status(code));
            }
            if request.custom {
                return Ok(());
            }
            if parsing_failed {
                return Err(ClientError::Response);
            }
            if sse {
                if !decoder.complete() {
                    return Err(ClientError::Response);
                }
            } else {
                let value = serde_json::from_slice(&body).map_err(|_| ClientError::Response)?;
                content.observe(&value, config.protocol);
                metadata.usage = content.usage.clone();
            }
            if content.success() {
                Ok(())
            } else {
                Err(ClientError::Response)
            }
        }
        .await;
        if matches!(result, Err(ClientError::Capture)) {
            return Err(ClientError::Capture);
        }
        let failure = result.err().map(|error| match error {
            ClientError::Timeout => CheckFailure::Timeout,
            ClientError::Network => CheckFailure::Network,
            ClientError::Status(_) => CheckFailure::Http,
            _ => CheckFailure::InvalidResponse,
        });
        Ok(CheckResult {
            success: failure.is_none(),
            status_code: status,
            duration_ms: elapsed(start),
            first_token_ms: first,
            failure,
            metadata: metadata.clone(),
        })
    }
}
fn observe_events(
    decoder: &mut stream::Sse,
    content: &mut stream::Content,
    first: &mut Option<u64>,
    protocol: Protocol,
    bytes: &[u8],
    received_ms: u64,
) -> Result<(), ClientError> {
    for event in decoder.push(bytes)? {
        if event == "[DONE]" && protocol == Protocol::OpenaiCompatible {
            content.ended = true;
            continue;
        }
        let value = serde_json::from_str(&event).map_err(|_| ClientError::Response)?;
        if content.observe(&value, protocol) && first.is_none() {
            *first = Some(received_ms);
        }
    }
    Ok(())
}
pub(super) fn elapsed(start: Instant) -> u64 {
    start.elapsed().as_millis().min(u64::MAX as u128) as u64
}
