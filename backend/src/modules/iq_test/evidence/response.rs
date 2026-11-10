//! IQ 响应过程按 JSONL 记录；原始响应可从字节块无损还原。
use super::*;
use std::io::{BufRead, BufReader};

pub(in crate::modules::iq_test) fn supported(meta: &Value) -> bool {
    matches!(meta["formatVersion"].as_u64(), Some(1 | 2))
}

pub(in crate::modules::iq_test) fn path(
    directory: &Path,
    attempt: u32,
    meta: &Value,
) -> Result<PathBuf> {
    let name = match meta["formatVersion"].as_u64() {
        Some(1) => format!("stream.{attempt}.sse"),
        Some(2) => format!("response.{attempt}.jsonl"),
        _ => return Err(IqError::Evidence),
    };
    Ok(directory.join(name))
}

pub(super) fn event(writer: &mut FileWriter, value: Value) -> Result<()> {
    let mut bytes = serde_json::to_vec(&value).map_err(|_| IqError::Evidence)?;
    bytes.push(b'\n');
    writer.write_all(&bytes)?;
    Ok(())
}

pub(super) fn chunk(writer: &mut FileWriter, bytes: &[u8], elapsed: u64) -> Result<()> {
    for part in bytes.chunks(16 * 1024) {
        // UTF-8 在网络块边界被拆开或上游返回非文本时，保留原字节，避免有损替换。
        let value = match std::str::from_utf8(part) {
            Ok(text) => json!({"event":"response_chunk", "elapsedMs":elapsed, "text":text}),
            Err(_) => json!({"event":"response_chunk", "elapsedMs":elapsed, "bytes":part}),
        };
        event(writer, value)?;
    }
    Ok(())
}

/// 第二项表示 JSONL 是否包含完整行；异常退出留下的不完整末行不阻止读取之前的内容。
pub(in crate::modules::iq_test) fn read(path: &Path) -> Result<(Vec<u8>, bool)> {
    filesystem::regular_file(path)?.ok_or(IqError::Evidence)?;
    if path.extension().is_some_and(|extension| extension == "sse") {
        return Ok((fs::read(path)?, true));
    }
    let mut reader = BufReader::new(fs::File::open(path)?);
    let mut body = Vec::new();
    let mut line = Vec::new();
    loop {
        line.clear();
        if reader.read_until(b'\n', &mut line)? == 0 {
            return Ok((body, true));
        }
        if !line.ends_with(b"\n") {
            return Ok((body, false));
        }
        let value: Value = serde_json::from_slice(&line).map_err(|_| IqError::Evidence)?;
        if value["event"] == "response_chunk" {
            match (value.get("text"), value.get("bytes")) {
                (Some(Value::String(text)), None) => body.extend_from_slice(text.as_bytes()),
                (None, Some(bytes)) => body.extend(
                    serde_json::from_value::<Vec<u8>>(bytes.clone())
                        .map_err(|_| IqError::Evidence)?,
                ),
                _ => return Err(IqError::Evidence),
            }
        }
    }
}
