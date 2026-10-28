use super::super::redaction;
use super::writer::FileCapture;
use super::*;
use crate::shared::model_client::{CaptureEvent, CaptureSink, ResponseMetadata};
use crate::shared::model_client::{CaptureOutcome, Protocol, ResponseKind};
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;

#[tokio::test]
async fn headers_preserve_collected_duplicates_and_redact_named_marked_and_echoed_secrets() {
    use reqwest::header::{HeaderMap, HeaderValue};
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("captures");
    let store = CaptureStore::new(root.clone());
    let mut capture = store
        .start("record".into(), "site", "model", "site-secret")
        .await
        .unwrap();
    let mut headers = HeaderMap::new();
    headers.append("x-repeat", HeaderValue::from_static("one"));
    headers.append("x-repeat", HeaderValue::from_static("two"));
    headers.insert(
        "authorization",
        HeaderValue::from_static("Bearer independent-auth"),
    );
    headers.insert(
        "proxy-authorization",
        HeaderValue::from_static("Basic independent-proxy"),
    );
    headers.insert(
        "x-custom-token",
        HeaderValue::from_static("independent-token"),
    );
    headers.insert(
        "x-echo",
        HeaderValue::from_static("before-site-secret-after"),
    );
    let mut sensitive = HeaderValue::from_static("marked-value");
    sensitive.set_sensitive(true);
    headers.insert("x-marked", sensitive);
    capture
        .record(CaptureEvent::Started {
            protocol: Protocol::CustomHttp,
            method: "POST",
            url: "https://example.test/probe",
            body: &json!({"model":"m"}),
            headers: &headers,
        })
        .await
        .unwrap();
    let mut response = HeaderMap::new();
    response.append("set-cookie", HeaderValue::from_static("a=first-cookie"));
    response.append("set-cookie", HeaderValue::from_static("b=second-cookie"));
    response.insert("x-unknown", HeaderValue::from_static("kept"));
    capture
        .record(CaptureEvent::Response {
            status: 503,
            kind: ResponseKind::Other,
            content_type: "text/plain",
            headers: &response,
            elapsed_ms: 0,
        })
        .await
        .unwrap();
    finish(&mut capture, true).await;
    drop(capture);
    let events = read(&root);
    let started = events
        .iter()
        .find(|e| e["event"] == "request_started")
        .unwrap();
    assert_eq!(
        started["headers"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|h| h["name"] == "x-repeat")
            .cloned()
            .collect::<Vec<_>>(),
        vec![
            json!({"name":"x-repeat","value":"one"}),
            json!({"name":"x-repeat","value":"two"})
        ]
    );
    let text = serde_json::to_string(&events).unwrap();
    for secret in [
        "site-secret",
        "independent-auth",
        "independent-proxy",
        "independent-token",
        "marked-value",
        "first-cookie",
        "second-cookie",
    ] {
        assert!(!text.contains(secret));
    }
    assert!(text.contains("before-[REDACTED]-after"));
    let saved = store.read("record", false).unwrap();
    assert_eq!(saved.request_headers.unwrap().len(), headers.len());
    assert_eq!(
        saved
            .response_headers
            .unwrap()
            .iter()
            .filter(|h| h.name == "set-cookie")
            .map(|h| h.value.as_str())
            .collect::<Vec<_>>(),
        ["[REDACTED]", "[REDACTED]"]
    );
}

#[test]
fn credential_redaction_handles_every_split_and_json_escaping_without_changing_other_bytes() {
    let secret = "key-\"\\\n-end";
    let escaped = serde_json::to_string(secret).unwrap();
    let input = format!("prefix:{secret}:{}:suffix", &escaped[1..escaped.len() - 1]).into_bytes();
    for split in 0..=input.len() {
        let mut redactor = redaction::Redactor::new(secret);
        let mut output = redactor.push(&input[..split], false);
        output.extend(redactor.push(&input[split..], false));
        output.extend(redactor.push(&[], true));
        assert_eq!(output, b"prefix:[REDACTED]:[REDACTED]:suffix");
    }
    let bytes = b"\0\xffdata\xfe";
    assert_eq!(redaction::Redactor::new("").push(bytes, true), bytes);
}
async fn recording(
    kind: ResponseKind,
    content_type: &str,
    secret: &str,
) -> (tempfile::TempDir, PathBuf, FileCapture) {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("captures");
    let store = CaptureStore::new(root.clone());
    let mut capture = store
        .start("record".into(), "site", "model", secret)
        .await
        .unwrap();
    capture
        .record(CaptureEvent::Started {
            protocol: Protocol::CustomHttp,
            method: "POST",
            url: "https://example.test/probe",
            body: &json!({"model":"remote-name"}),
            headers: &Default::default(),
        })
        .await
        .unwrap();
    capture
        .record(CaptureEvent::Response {
            status: 200,
            kind,
            content_type,
            headers: &Default::default(),
            elapsed_ms: 2,
        })
        .await
        .unwrap();
    (temp, root, capture)
}
async fn chunk(capture: &mut FileCapture, bytes: &[u8], elapsed: u64) {
    capture
        .record(CaptureEvent::Chunk {
            bytes,
            elapsed_ms: elapsed,
        })
        .await
        .unwrap();
}
async fn finish(capture: &mut FileCapture, complete: bool) {
    capture
        .record(CaptureEvent::Finished {
            outcome: if complete {
                CaptureOutcome::Succeeded
            } else {
                CaptureOutcome::Cancelled
            },
            body_complete: complete,
            elapsed_ms: 100,
            metadata: &ResponseMetadata {
                body_complete: complete,
                ..Default::default()
            },
        })
        .await
        .unwrap();
    capture.publish().await.unwrap();
}
fn read(root: &std::path::Path) -> Vec<Value> {
    fs::read_to_string(root.join("record/response.jsonl"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

#[tokio::test]
async fn cleanup_snapshots_files_before_querying_history_and_preserves_active_output() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("captures");
    let store = CaptureStore::new(root.clone());
    for id in ["kept", "orphan"] {
        let capture = store.start(id.into(), "site", "model", "").await.unwrap();
        capture.publish().await.unwrap();
    }
    let active = store
        .start("active".into(), "site", "model", "")
        .await
        .unwrap();
    let retained = async {
        // 查询 future 开始时，文件枚举应已结束；此后出现的目录留给下一轮判断。
        let late = store
            .start("late".into(), "site", "model", "")
            .await
            .unwrap();
        late.publish().await.unwrap();
        Ok(std::collections::HashSet::from(["kept".to_owned()]))
    };
    store.cleanup(retained, 30).await.unwrap();
    assert!(root.join("kept/response.jsonl").is_file());
    assert!(root.join("late/response.jsonl").is_file());
    assert!(root.join("active.partial/response.jsonl").is_file());
    assert!(!root.join("orphan").exists());
    assert!(store
        .cleanup(async { Err(crate::shared::file_output::OutputError) }, 30)
        .await
        .is_err());
    assert!(root.join("kept/response.jsonl").is_file());
    assert!(root.join("late/response.jsonl").is_file());
    drop(active);
    store.close();
    store.drained().await;
}

#[tokio::test]
async fn utf8_text_is_readable_and_credentials_stay_redacted_across_every_split() {
    let bytes = "中文🦀:private-key:结束".as_bytes();
    for split in 0..=bytes.len() {
        let (_temp, root, mut capture) = recording(
            ResponseKind::Other,
            "text/plain; charset=utf-8",
            "private-key",
        )
        .await;
        chunk(&mut capture, &bytes[..split], 3).await;
        chunk(&mut capture, &bytes[split..], 4).await;
        finish(&mut capture, true).await;
        let entries = read(&root);
        let text: String = entries
            .iter()
            .filter(|e| e["event"] == "text")
            .map(|e| e["text"].as_str().unwrap())
            .collect();
        assert_eq!(text, "中文🦀:[REDACTED]:结束");
        assert_eq!(entries[0]["recordId"], "record");
        assert_eq!(entries[0]["formatVersion"], 2);
        assert_eq!(entries.last().unwrap()["processedBytes"], text.len());
        assert!(!root.join("record/response.bin").exists());
        assert!(!fs::read_to_string(root.join("record/response.jsonl"))
            .unwrap()
            .contains("private-key"));
    }
}

#[tokio::test]
async fn sse_events_keep_tools_multiline_data_comments_unknown_fields_and_errors() {
    let input = concat!(
        ": 心跳\r\nevent: tool_call\r\nid: call-1\r\nx-future: kept\r\n",
        "data: {\"tool\":\"搜索\",\r\ndata: \"arguments\":{\"key\":\"private-key\"}}\r\n\r\n",
        "event: unknown\ndata: not-json\n\n",
        "data: [DONE]\n\n",
        "data: {\"unfinished\":true}",
    );
    let (_temp, root, mut capture) = recording(
        ResponseKind::EventStream,
        "text/event-stream",
        "private-key",
    )
    .await;
    for byte in input.as_bytes() {
        chunk(&mut capture, std::slice::from_ref(byte), 5).await;
    }
    finish(&mut capture, false).await;
    let entries = read(&root);
    let events: Vec<_> = entries.iter().filter(|e| e["event"] == "sse").collect();
    assert_eq!(events.len(), 4);
    assert_eq!(events[0]["eventType"], "tool_call");
    assert_eq!(events[0]["id"], "call-1");
    assert_eq!(
        events[0]["data"],
        json!({"tool":"搜索", "arguments":{"key":"[REDACTED]"}})
    );
    assert_eq!(events[0]["comments"], json!(["心跳"]));
    assert_eq!(
        events[0]["fields"],
        json!([{"name":"x-future","value":"kept"}])
    );
    assert_eq!(events[1]["data"], "not-json");
    assert_eq!(events[2]["data"], "[DONE]");
    assert_eq!(events[3]["data"], json!({"unfinished":true}));
    assert_eq!(events[3]["terminated"], false);
    assert_eq!(entries.last().unwrap()["bodyComplete"], false);
    assert_eq!(entries.last().unwrap()["outcome"]["status"], "cancelled");
    assert!(!root.join("record/response.bin").exists());
}

#[tokio::test]
async fn json_is_structured_but_large_json_and_sse_are_fragmented_without_truncation() {
    let (_temp, root, mut capture) =
        recording(ResponseKind::Json, "application/json", "key-to-hide").await;
    chunk(&mut capture, b"{\"message\":\"hello\",", 3).await;
    chunk(&mut capture, b"\"secret\":\"key-to-hide\"}", 4).await;
    finish(&mut capture, true).await;
    let entries = read(&root);
    assert_eq!(
        entries.iter().find(|e| e["event"] == "json").unwrap()["value"],
        json!({"message":"hello","secret":"[REDACTED]"})
    );

    let payload = format!("{{\"content\":\"{}\"}}", "中文".repeat(text::FRAME_BYTES));
    for sse in [false, true] {
        let (kind, mime) = if sse {
            (ResponseKind::EventStream, "text/event-stream")
        } else {
            (ResponseKind::Json, "application/json")
        };
        let (_temp, root, mut capture) = recording(kind, mime, "").await;
        let input = if sse {
            format!("data: {payload}\n\n")
        } else {
            payload.clone()
        };
        // 单个巨大网络块也不能变成同样巨大的常驻解析缓存。
        chunk(&mut capture, input.as_bytes(), 3).await;
        if sse {
            chunk(&mut capture, b"data: [DONE]\n\n", 4).await;
        }
        finish(&mut capture, true).await;
        let entries = read(&root);
        let event = if sse { "sse_fragment" } else { "json_fragment" };
        let fragments: Vec<_> = entries.iter().filter(|e| e["event"] == event).collect();
        assert!(fragments.len() > 1);
        let restored: String = fragments
            .iter()
            .enumerate()
            .map(|(index, entry)| {
                assert_eq!(entry["index"], index);
                let text = entry["text"].as_str().unwrap();
                assert!(text.len() <= text::FRAME_BYTES);
                text
            })
            .collect();
        assert_eq!(restored, input);
        assert_eq!(fragments.last().unwrap()["last"], true);
        if sse {
            assert!(entries
                .iter()
                .any(|e| e["event"] == "sse" && e["data"] == "[DONE]"));
        }
        assert!(!root.join("record/response.bin").exists());
    }
}

#[tokio::test]
async fn binary_mime_uses_an_attachment_with_offsets_and_redacted_metadata() {
    let (_temp, root, mut capture) = recording(
        ResponseKind::Other,
        "image/png; detail=private-key",
        "private-key",
    )
    .await;
    let bytes = b"\x89PNG\0\xffdata";
    chunk(&mut capture, &bytes[..4], 3).await;
    chunk(&mut capture, &bytes[4..], 4).await;
    finish(&mut capture, true).await;
    let entries = read(&root);
    assert_eq!(fs::read(root.join("record/response.png")).unwrap(), bytes);
    assert_eq!(
        entries
            .iter()
            .find(|e| e["event"] == "response_started")
            .unwrap()["contentType"],
        "image/png; detail=[REDACTED]"
    );
    let mut offset = 0;
    for entry in entries.iter().filter(|e| e["event"] == "binary") {
        assert_eq!(entry["file"], "response.png");
        assert_eq!(entry["offset"], offset);
        offset += entry["bytes"].as_u64().unwrap();
    }
    assert_eq!(offset, bytes.len() as u64);
    assert!(!root.join("record/response.bin").exists());
}

#[tokio::test]
async fn invalid_and_incomplete_utf8_are_preserved_as_attachments_without_lossy_text() {
    for tail in [b"\xffremaining".as_slice(), b"\xe4\xbd".as_slice()] {
        let (_temp, root, mut capture) = recording(ResponseKind::Other, "text/plain", "").await;
        chunk(&mut capture, b"already readable", 3).await;
        let pending = fs::read_to_string(root.join("record.partial/response.jsonl")).unwrap();
        assert!(pending.contains("already readable")); // EOF 前已增量落盘。
        chunk(&mut capture, tail, 4).await;
        finish(&mut capture, false).await;
        let entries = read(&root);
        assert_eq!(fs::read(root.join("record/response.bin")).unwrap(), tail);
        assert!(entries.iter().any(|e| e["event"] == "encoding_fallback"));
        assert!(entries
            .iter()
            .any(|e| e["event"] == "text" && e["text"] == "already readable"));
        assert_eq!(entries.last().unwrap()["bodyComplete"], false);
        assert!(!fs::read_to_string(root.join("record/response.jsonl"))
            .unwrap()
            .contains('\u{fffd}'));
    }
}
