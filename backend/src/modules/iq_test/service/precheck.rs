use super::*;
use crate::shared::model_client::{
    CaptureError, CaptureEvent, CaptureSink, ClientError, GenerationFailure, GenerationOptions,
    GenerationRequest, ResponseKind,
};
use futures_util::future::BoxFuture;
use zeroize::Zeroizing;

const PRECHECK_TIMEOUT: Duration = Duration::from_secs(60);
const MAX_ERROR_BODY_BYTES: usize = 64 * 1024;
const MAX_MESSAGE_CHARS: usize = 512;

impl IqTest {
    pub(super) async fn check_snapshot(
        &self,
        participant: &ParticipantExecution,
        cancellation: &Cancellation,
    ) -> IqPrecheck {
        // 与正式答题使用同一请求构造及冻结参数，仅验证消息由 IQ 业务层单独提供。
        let request = GenerationRequest {
            target: participant.connection.target.clone(),
            model: participant.connection.source.model_name.clone(),
            endpoint: participant.connection.endpoint.clone(),
            prompt: "只回复 OK".into(),
            parameters: participant.parameters.clone(),
        };
        let mut response = PrecheckResponse::default();
        let result = tokio::time::timeout(
            PRECHECK_TIMEOUT,
            self.client.generate_once(
                &request,
                GenerationOptions {
                    output_timeout: Some(PRECHECK_TIMEOUT),
                },
                cancellation,
                &mut response,
            ),
        )
        .await;
        let fallback: String = match result {
            Ok(Ok(result)) => match result.failure {
                None => {
                    return IqPrecheck {
                        status: PrecheckStatus::Available,
                        message: "当前独立执行快照预检通过。".into(),
                        checked_at_ms: Some(now_ms()),
                    };
                }
                Some(GenerationFailure::Cancelled) => return unavailable("预检已取消。"),
                Some(GenerationFailure::Http) => {
                    "上游拒绝当前快照的预检请求，未提供可识别的错误说明。".into()
                }
                Some(GenerationFailure::Network) => "无法完成预检网络请求。".into(),
                Some(GenerationFailure::Timeout) => "预检等待有效输出超时。".into(),
                Some(GenerationFailure::InvalidResponse) => {
                    "预检响应无有效输出、未正常结束或格式无效。".into()
                }
            },
            Ok(Err(ClientError::Cancelled)) => return unavailable("预检已取消。"),
            Ok(Err(error)) => error.to_string(),
            Err(_) => "预检超过 60 秒，未能完成。".into(),
        };
        unavailable(&response.message(&participant.sanitize, &fallback))
    }
}

/// 预检只在内存中保留有界响应以提取错误说明，不记录请求、响应头或任何文件。
#[derive(Default)]
struct PrecheckResponse {
    status: Option<u16>,
    kind: Option<ResponseKind>,
    plain_text: bool,
    complete: bool,
    overflow: bool,
    body: Zeroizing<Vec<u8>>,
}
impl CaptureSink for PrecheckResponse {
    fn record<'a>(
        &'a mut self,
        event: CaptureEvent<'a>,
    ) -> BoxFuture<'a, std::result::Result<(), CaptureError>> {
        Box::pin(async move {
            match event {
                CaptureEvent::Response {
                    status,
                    kind,
                    content_type,
                    ..
                } => {
                    self.status = Some(status);
                    self.kind = Some(kind);
                    self.plain_text = content_type
                        .split(';')
                        .next()
                        .is_some_and(|mime| mime.trim().eq_ignore_ascii_case("text/plain"));
                }
                CaptureEvent::Chunk { bytes, .. } => {
                    let remaining = MAX_ERROR_BODY_BYTES.saturating_sub(self.body.len());
                    self.body
                        .extend_from_slice(&bytes[..bytes.len().min(remaining)]);
                    self.overflow |= bytes.len() > remaining;
                }
                CaptureEvent::Finished { body_complete, .. } => self.complete = body_complete,
                _ => {}
            }
            Ok(())
        })
    }
}
impl PrecheckResponse {
    fn message(&self, sanitize: &Sanitizer, fallback: &str) -> String {
        let reason = self.reason().filter(|message| !message.trim().is_empty());
        let reason = reason.as_deref().unwrap_or(fallback);
        let message = match self.status {
            Some(code) => format!("预检失败（HTTP {code}）：{reason}"),
            None => reason.into(),
        };
        // 先对完整已提取说明脱敏，再压缩、截断，避免截断后留下部分凭据。
        let redacted = sanitize.text(&message);
        let normalized = redacted.split_whitespace().collect::<Vec<_>>().join(" ");
        let mut chars = normalized.chars().filter(|c| !c.is_control());
        let mut message: String = chars.by_ref().take(MAX_MESSAGE_CHARS - 1).collect();
        if chars.next().is_some() {
            message.push('…');
        }
        message
    }
    fn reason(&self) -> Option<String> {
        // 缓冲被截断时不把可能只收到一部分的秘密当成摘要返回。
        if self.overflow {
            return None;
        }
        let text = std::str::from_utf8(&self.body).ok()?;
        if let Ok(value) = serde_json::from_str::<Value>(text) {
            return error_reason(&value).map(str::to_owned);
        }
        if self.kind == Some(ResponseKind::EventStream) {
            return text.lines().find_map(|line| {
                let value: Value = serde_json::from_str(line.strip_prefix("data:")?.trim()).ok()?;
                error_reason(&value).map(str::to_owned)
            });
        }
        // 只取明确的纯文本错误首行，HTML、成功正文和不完整响应不直接展示。
        if self.plain_text
            && self.complete
            && self.status.is_some_and(|code| !(200..300).contains(&code))
        {
            return text
                .lines()
                .find(|line| !line.trim().is_empty())
                .map(str::to_owned);
        }
        None
    }
}
fn error_reason(value: &Value) -> Option<&str> {
    [
        "/error/message",
        "/response/error/message",
        "/error/description",
        "/error",
        "/message",
        "/detail",
        "/error_description",
    ]
    .iter()
    .find_map(|path| {
        value
            .pointer(path)
            .and_then(Value::as_str)
            .filter(|s| !s.trim().is_empty())
    })
}
pub(super) fn unavailable(message: &str) -> IqPrecheck {
    IqPrecheck {
        status: PrecheckStatus::Unavailable,
        message: message.into(),
        checked_at_ms: Some(now_ms()),
    }
}
