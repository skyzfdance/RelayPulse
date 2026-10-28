//! 小文档/事件直接提供结构化值；大内容退化为有序文本片段，不截断或全量缓存。
use super::write_event;
use crate::shared::file_output::{OutputError, OutputSession};
use serde_json::{json, Value};

pub(super) const FRAME_BYTES: usize = 64 * 1024;
#[derive(Default)]
pub(super) struct JsonDocument {
    buffer: String,
    fragmented: bool,
    index: u64,
}
impl JsonDocument {
    pub(super) async fn push(
        &mut self,
        output: &OutputSession,
        text: &str,
        elapsed: u64,
    ) -> Result<(), OutputError> {
        if self.buffer.len() + text.len() > FRAME_BYTES {
            self.fragmented = true;
            self.fragment(output, false, false, elapsed).await?;
        }
        self.buffer.push_str(text);
        if self.fragmented {
            self.fragment(output, false, false, elapsed).await?;
        }
        Ok(())
    }
    async fn fragment(
        &mut self,
        output: &OutputSession,
        last: bool,
        complete: bool,
        elapsed: u64,
    ) -> Result<(), OutputError> {
        let text = std::mem::take(&mut self.buffer);
        write_event(
            output,
            json!({"event":"json_fragment", "elapsedMs":elapsed,
            "index":self.index, "text":text, "last":last, "complete":complete}),
        )
        .await?;
        self.index += 1;
        Ok(())
    }
    pub(super) async fn finish(
        &mut self,
        output: &OutputSession,
        complete: bool,
        elapsed: u64,
    ) -> Result<(), OutputError> {
        if self.fragmented {
            return self.fragment(output, true, complete, elapsed).await;
        }
        if self.buffer.is_empty() {
            return Ok(());
        }
        if let Ok(value) = serde_json::from_str::<Value>(&self.buffer) {
            write_event(
                output,
                json!({"event":"json", "elapsedMs":elapsed, "value":value, "complete":complete}),
            )
            .await?;
            self.buffer.clear();
            Ok(())
        } else {
            self.fragment(output, true, complete, elapsed).await
        }
    }
}

#[derive(Default)]
pub(super) struct SseLog {
    buffer: String,
    fragmented: bool,
    index: u64,
    event_id: u64,
    line_has_content: bool,
    skip_lf: bool,
}
impl SseLog {
    pub(super) async fn push(
        &mut self,
        output: &OutputSession,
        text: &str,
        elapsed: u64,
    ) -> Result<(), OutputError> {
        for mut segment in text.split_inclusive(['\r', '\n']) {
            if self.skip_lf {
                self.skip_lf = false;
                if let Some(rest) = segment.strip_prefix('\n') {
                    segment = rest;
                }
            }
            if segment.is_empty() {
                continue;
            }
            let terminated = segment.ends_with(['\r', '\n']);
            let content = &segment[..segment.len() - usize::from(terminated)];
            if !content.is_empty() {
                self.line_has_content = true;
            }
            self.append(output, content, elapsed).await?;
            if terminated {
                self.skip_lf = segment.ends_with('\r');
                self.append(output, "\n", elapsed).await?;
                if !self.line_has_content {
                    self.emit(output, true, elapsed).await?;
                }
                self.line_has_content = false;
            }
        }
        Ok(())
    }
    async fn append(
        &mut self,
        output: &OutputSession,
        mut text: &str,
        elapsed: u64,
    ) -> Result<(), OutputError> {
        while !text.is_empty() {
            let mut take = (FRAME_BYTES - self.buffer.len()).min(text.len());
            while !text.is_char_boundary(take) {
                take -= 1;
            }
            if take == 0 {
                self.fragmented = true;
                self.fragment(output, false, false, elapsed).await?;
                continue;
            }
            self.buffer.push_str(&text[..take]);
            text = &text[take..];
        }
        Ok(())
    }
    async fn fragment(
        &mut self,
        output: &OutputSession,
        last: bool,
        terminated: bool,
        elapsed: u64,
    ) -> Result<(), OutputError> {
        let text = std::mem::take(&mut self.buffer);
        write_event(
            output,
            json!({"event":"sse_fragment", "elapsedMs":elapsed, "eventId":self.event_id,
            "index":self.index, "text":text, "last":last, "terminated":terminated}),
        )
        .await?;
        self.index += 1;
        Ok(())
    }
    async fn emit(
        &mut self,
        output: &OutputSession,
        terminated: bool,
        elapsed: u64,
    ) -> Result<(), OutputError> {
        if self.fragmented {
            self.fragment(output, true, terminated, elapsed).await?;
        } else if !self.buffer.trim().is_empty() {
            let value = sse_event(&self.buffer, self.event_id, terminated, elapsed);
            write_event(output, value).await?;
        }
        self.buffer.clear();
        self.fragmented = false;
        self.index = 0;
        self.event_id += 1;
        Ok(())
    }
    pub(super) async fn finish(
        &mut self,
        output: &OutputSession,
        elapsed: u64,
    ) -> Result<(), OutputError> {
        if !self.buffer.is_empty() || self.fragmented {
            self.emit(output, false, elapsed).await?;
        }
        Ok(())
    }
}
fn sse_event(text: &str, event_id: u64, terminated: bool, elapsed: u64) -> Value {
    let mut data = vec![];
    let mut event_type = "message";
    let mut id = None;
    let mut comments = vec![];
    let mut fields = vec![];
    for line in text.lines().filter(|line| !line.is_empty()) {
        if let Some(comment) = line.strip_prefix(':') {
            comments.push(comment.strip_prefix(' ').unwrap_or(comment));
            continue;
        }
        let (name, value) = line.split_once(':').unwrap_or((line, ""));
        let value = value.strip_prefix(' ').unwrap_or(value);
        match name {
            "data" => data.push(value),
            "event" => event_type = value,
            "id" => id = Some(value),
            _ => fields.push(json!({"name":name, "value":value})),
        }
    }
    let data = if data.is_empty() {
        Value::Null
    } else {
        let text = data.join("\n");
        serde_json::from_str(&text).unwrap_or(Value::String(text))
    };
    json!({"event":"sse", "elapsedMs":elapsed, "eventId":event_id, "terminated":terminated,
        "eventType":event_type, "id":id, "data":data, "comments":comments, "fields":fields})
}
