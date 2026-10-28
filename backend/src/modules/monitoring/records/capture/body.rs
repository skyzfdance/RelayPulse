//! 记录格式由监测业务选择；客户端仍只交付原始响应字节。
use super::{
    text::{JsonDocument, SseLog},
    write_event,
};
use crate::shared::{
    file_output::{OutputError, OutputSession},
    model_client::ResponseKind,
};
use serde_json::json;

const CHUNK_BYTES: usize = 16 * 1024;
#[derive(Default)]
pub(super) struct BodyWriter {
    format: Format,
    utf8_tail: Vec<u8>,
    binary_file: &'static str,
    binary_bytes: u64,
}
#[derive(Default)]
enum Format {
    #[default]
    Text,
    Json(JsonDocument),
    Sse(SseLog),
    Binary,
}
impl BodyWriter {
    pub(super) fn new(kind: ResponseKind, content_type: &str) -> Self {
        let content_type = content_type.to_ascii_lowercase();
        let media = content_type.split(';').next().unwrap_or("").trim();
        let utf8 = content_type.split(';').skip(1).all(|parameter| {
            let Some((key, value)) = parameter.trim().split_once('=') else {
                return true;
            };
            key.trim() != "charset"
                || matches!(
                    value.trim().trim_matches('"'),
                    "utf-8" | "utf8" | "us-ascii"
                )
        });
        let format = if !utf8 {
            Format::Binary
        } else if matches!(kind, ResponseKind::EventStream) {
            Format::Sse(SseLog::default())
        } else if matches!(kind, ResponseKind::Json) || media.ends_with("+json") {
            Format::Json(JsonDocument::default())
        } else if media.is_empty()
            || media.starts_with("text/")
            || media.ends_with("+xml")
            || matches!(
                media,
                "application/xml"
                    | "application/javascript"
                    | "application/x-ndjson"
                    | "application/jsonl"
            )
        {
            Format::Text
        } else {
            Format::Binary
        };
        Self {
            format,
            binary_file: attachment_name(media),
            ..Default::default()
        }
    }
    pub(super) async fn push(
        &mut self,
        output: &OutputSession,
        bytes: &[u8],
        elapsed: u64,
    ) -> Result<(), OutputError> {
        if matches!(self.format, Format::Binary) {
            return self.binary(output, bytes, elapsed).await;
        }
        // 控制单条日志和解码临时缓存，不限制响应累计大小。
        for part in bytes.chunks(CHUNK_BYTES) {
            if matches!(self.format, Format::Binary) {
                self.binary(output, part, elapsed).await?;
                continue;
            }
            self.utf8_tail.extend_from_slice(part);
            let valid = match std::str::from_utf8(&self.utf8_tail) {
                Ok(_) => self.utf8_tail.len(),
                Err(error) if error.error_len().is_none() => error.valid_up_to(),
                Err(_) => {
                    self.binary_fallback(output, elapsed).await?;
                    continue;
                }
            };
            if valid > 0 {
                let text = String::from_utf8(self.utf8_tail.drain(..valid).collect())
                    .map_err(|_| OutputError)?;
                match &mut self.format {
                    Format::Text => {
                        write_event(
                            output,
                            json!({"event":"text", "elapsedMs":elapsed, "text":text}),
                        )
                        .await?
                    }
                    Format::Json(document) => document.push(output, &text, elapsed).await?,
                    Format::Sse(stream) => stream.push(output, &text, elapsed).await?,
                    Format::Binary => unreachable!("binary handled before decoding"),
                }
            }
        }
        Ok(())
    }
    async fn binary(
        &mut self,
        output: &OutputSession,
        bytes: &[u8],
        elapsed: u64,
    ) -> Result<(), OutputError> {
        if bytes.is_empty() {
            return Ok(());
        }
        output.append(self.binary_file, bytes).await?;
        write_event(
            output,
            json!({"event":"binary", "elapsedMs":elapsed, "file":self.binary_file,
            "offset":self.binary_bytes, "bytes":bytes.len()}),
        )
        .await?;
        self.binary_bytes += bytes.len() as u64;
        Ok(())
    }
    async fn finish_format(
        &mut self,
        output: &OutputSession,
        complete: bool,
        elapsed: u64,
    ) -> Result<(), OutputError> {
        match &mut self.format {
            Format::Json(document) => document.finish(output, complete, elapsed).await,
            Format::Sse(stream) => stream.finish(output, elapsed).await,
            _ => Ok(()),
        }
    }
    async fn binary_fallback(
        &mut self,
        output: &OutputSession,
        elapsed: u64,
    ) -> Result<(), OutputError> {
        self.finish_format(output, false, elapsed).await?;
        self.format = Format::Binary;
        self.binary_file = "response.bin";
        write_event(
            output,
            json!({"event":"encoding_fallback", "elapsedMs":elapsed,
            "reason":"invalid_utf8", "file":self.binary_file}),
        )
        .await?;
        let bytes = std::mem::take(&mut self.utf8_tail);
        self.binary(output, &bytes, elapsed).await
    }
    pub(super) async fn finish(
        &mut self,
        output: &OutputSession,
        complete: bool,
        elapsed: u64,
    ) -> Result<(), OutputError> {
        // 不用有损替换吞掉截断的 UTF-8；以附件保留最后几个无法解码的字节。
        if !self.utf8_tail.is_empty() {
            self.binary_fallback(output, elapsed).await?;
        }
        self.finish_format(output, complete, elapsed).await
    }
}
fn attachment_name(media: &str) -> &'static str {
    match media {
        "image/png" => "response.png",
        "image/jpeg" => "response.jpg",
        "image/gif" => "response.gif",
        "image/webp" => "response.webp",
        "image/avif" => "response.avif",
        "application/pdf" => "response.pdf",
        "audio/mpeg" => "response.mp3",
        "audio/wav" | "audio/x-wav" => "response.wav",
        "audio/ogg" => "response.ogg",
        "video/mp4" => "response.mp4",
        "application/zip" => "response.zip",
        _ => "response.bin",
    }
}
