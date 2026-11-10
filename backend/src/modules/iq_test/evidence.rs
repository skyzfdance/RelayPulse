//! IQ 完整响应体、逐次元信息与用户删除隔离。此处不实现保留期或容量清理。
mod artifacts;
pub(super) mod response;
use super::tasks::BlockingActivity;
use super::*;
use crate::shared::{
    file_output::filesystem::{self, FileWriter},
    model_client::{
        CaptureError, CaptureEvent, CaptureOutcome, CaptureSink, CheckFailure, OutputKind,
        TokenUsage,
    },
};
use futures_util::future::BoxFuture;
use serde_json::{json, Value};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::sync::Mutex;

#[derive(Clone)]
pub(super) struct Evidence {
    root: PathBuf,
    activity: Arc<BlockingActivity>,
}
impl Evidence {
    pub(super) fn root(&self) -> &Path {
        &self.root
    }
    pub(super) fn new(root: PathBuf) -> Self {
        Self {
            root,
            activity: Arc::default(),
        }
    }
    pub(super) async fn drained(&self) {
        self.activity.drained().await;
    }
    pub(super) fn validate_formats(&self) -> Result<()> {
        if !filesystem::directory(&self.root, false)? {
            return Ok(());
        }
        let mut files = Vec::new();
        let mut directories = Vec::new();
        collect(&self.root, &mut files, &mut directories, 0)?;
        for file in files {
            if file
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("meta.") && n.ends_with(".json"))
            {
                let meta: Value =
                    serde_json::from_slice(&fs::read(file)?).map_err(|_| IqError::Evidence)?;
                if !response::supported(&meta) {
                    return Err(IqError::Evidence);
                }
            }
        }
        Ok(())
    }
    pub(super) fn prepare(&self) -> Result<()> {
        filesystem::directory(&self.root, true)?;
        for name in ["runs", "trash"] {
            filesystem::directory(&self.root.join(name), true)?;
        }
        Ok(())
    }
    fn base(&self, group: &str, id: &str) -> Result<PathBuf> {
        validation::identifier(id, "id")?;
        self.prepare()?;
        Ok(self.root.join(group).join(id))
    }
    fn entry_directory(&self, run: &str, entry: &str, create: bool) -> Result<PathBuf> {
        validation::identifier(entry, "entryId")?;
        let path = self.base("runs", run)?;
        if !filesystem::directory(&path, create)? {
            return Err(IqError::NotFound);
        }
        let entries = path.join("entries");
        if !filesystem::directory(&entries, create)? {
            return Err(IqError::NotFound);
        }
        let path = entries.join(entry);
        if !filesystem::directory(&path, create)? {
            return Err(IqError::NotFound);
        }
        Ok(path)
    }
    pub(super) async fn start_attempt(
        &self,
        run: &IqRun,
        entry: &IqEntry,
        sanitize: &super::service::Sanitizer,
        retry_max: u32,
    ) -> Result<AttemptCapture> {
        let evidence = self.clone();
        let entry = entry.clone();
        let run_id = run.id.clone();
        let participant = sanitize.value(&json!(run.participants[entry.participant_index]));
        let question = sanitize.value(&json!(run.questions[entry.question_index]));
        let held = self.activity.enter();
        tokio::task::spawn_blocking(move || {
            let _held = held;
            let path = evidence.entry_directory(&run_id, &entry.id, true)?;
            let raw_path = path.join(format!("response.{}.jsonl", entry.attempt));
            let meta_path = path.join(format!("meta.{}.json", entry.attempt));
            // 每个 attempt 独占创建，不覆盖旧尝试；开始时就有可恢复的 running meta。
            if filesystem::regular_file(&meta_path)?.is_some() { return Err(IqError::Evidence); }
            let stream = FileWriter::create_new(&raw_path)?;
            let meta = json!({"formatVersion":2,"runId":run_id,"entryId":entry.id,"attempt":entry.attempt,
                "snapshotId":participant["snapshotId"],"participant":participant,"question":question,
                "startedAtMs":entry.started_at_ms,"finishedAtMs":null,"status":"running",
                "firstTokenMs":null,"totalTimeMs":null,"usage":TokenUsage::default(),
                "streamBytes":0,"bodyComplete":false,"statusCode":null,"errorCode":null,"outputFiles":[],"retryMax":retry_max});
            write_meta(&meta_path, &meta)?;
            Ok(AttemptCapture { activity: evidence.activity.clone(), inner: Arc::new(Mutex::new(AttemptFiles {
                stream, answer: String::new(), received_bytes: 0, raw_path, meta_path, meta, streaming: false, facts: AttemptFacts { has_raw_stream: true, ..Default::default() },
            })) })
        }).await.map_err(|_| IqError::Worker)?
    }
    pub(super) fn raw_path(&self, entry: &IqEntry) -> Result<PathBuf> {
        self.raw_attempt_path(entry, None)
    }
    pub(super) fn raw_attempt_path(
        &self,
        entry: &IqEntry,
        attempt: Option<u32>,
    ) -> Result<PathBuf> {
        let attempt = attempt.unwrap_or(entry.attempt);
        if attempt == 0 || attempt > entry.attempt {
            return Err(invalid("attempt", "尝试次数不存在。"));
        }
        if !entry.status.terminal() || entry.attempt == 0 || !entry.has_raw_stream {
            return Err(invalid("entryId", "该调用尚无可打开的终态原始流。"));
        }
        let directory = self.entry_directory(&entry.run_id, &entry.id, false)?;
        let meta_path = directory.join(format!("meta.{attempt}.json"));
        filesystem::regular_file(&meta_path)?.ok_or(IqError::Evidence)?;
        let meta: Value =
            serde_json::from_slice(&fs::read(meta_path)?).map_err(|_| IqError::Evidence)?;
        let path = response::path(&directory, attempt, &meta)?;
        filesystem::regular_file(&path)?.ok_or(IqError::Evidence)?;
        Ok(path)
    }
    pub(super) fn trash_ids(&self) -> Result<Vec<String>> {
        self.prepare()?;
        let mut ids = Vec::new();
        for entry in fs::read_dir(self.root.join("trash"))? {
            let entry = entry?;
            let id = entry
                .file_name()
                .into_string()
                .map_err(|_| IqError::RecoveryRequired)?;
            validation::identifier(&id, "id")?;
            filesystem::directory(&entry.path(), false)?;
            ids.push(id);
        }
        ids.sort();
        Ok(ids)
    }
    pub(super) fn reconcile(&self, id: &str, retained: bool) -> Result<()> {
        let isolated = self.base("trash", id)?;
        if !filesystem::directory(&isolated, false)? {
            return Ok(());
        }
        if retained {
            self.restore(id)
        } else {
            self.cleanup(id)
        }
    }
    pub(super) fn isolate(&self, id: &str) -> Result<()> {
        let source = self.base("runs", id)?;
        let target = self.base("trash", id)?;
        if filesystem::directory(&target, false)? {
            return Err(IqError::RecoveryRequired);
        }
        if filesystem::directory(&source, false)? {
            fs::rename(source, target)?;
            self.sync_roots()?;
        }
        Ok(())
    }
    pub(super) fn restore(&self, id: &str) -> Result<()> {
        let source = self.base("trash", id)?;
        if !filesystem::directory(&source, false)? {
            return Ok(());
        }
        let target = self.base("runs", id)?;
        if filesystem::directory(&target, false)? {
            return Err(IqError::RecoveryRequired);
        }
        fs::rename(source, target).map_err(|_| IqError::RecoveryRequired)?;
        self.sync_roots().map_err(|_| IqError::RecoveryRequired)
    }
    fn sync_roots(&self) -> Result<()> {
        #[cfg(unix)]
        for name in ["runs", "trash"] {
            fs::File::open(self.root.join(name))?.sync_all()?;
        }
        Ok(())
    }
    pub(super) fn cleanup(&self, id: &str) -> Result<()> {
        let path = self.base("trash", id)?;
        if !filesystem::directory(&path, false)? {
            return Ok(());
        }
        let mut files = Vec::new();
        let mut directories = Vec::new();
        // 全量检查后再删除；不跟随链接，不跨出用户授权的单场隔离目录。
        collect(&path, &mut files, &mut directories, 0).map_err(|_| IqError::CleanupPending)?;
        for file in files {
            fs::remove_file(file).map_err(|_| IqError::CleanupPending)?;
        }
        for directory in directories {
            fs::remove_dir(directory).map_err(|_| IqError::CleanupPending)?;
        }
        self.sync_roots().map_err(|_| IqError::CleanupPending)
    }
    pub(super) fn recover_run(&self, run: &mut IqRun) -> Result<bool> {
        let mut changed = false;
        for entry in &mut run.entries {
            if entry.attempt == 0 {
                continue;
            }
            let directory = match self.entry_directory(&run.id, &entry.id, false) {
                Ok(directory) => directory,
                // 本地写盘可能在建立目录前失败；不能据此生成虚构的调用文件。
                Err(IqError::NotFound) => continue,
                Err(error) => return Err(error),
            };
            // 不只检查末次：进程可能在重试切换前退出，所有 running meta 都要收敛。
            for attempt in 1..=entry.attempt {
                let path = directory.join(format!("meta.{attempt}.json"));
                if filesystem::regular_file(&path)?.is_none() {
                    continue;
                }
                let mut meta: Value =
                    serde_json::from_slice(&fs::read(&path)?).map_err(|_| IqError::Evidence)?;
                if !response::supported(&meta) {
                    return Err(IqError::Evidence);
                }
                if meta["status"] == "running" {
                    let raw = response::path(&directory, attempt, &meta)?;
                    let size = if filesystem::regular_file(&raw)?.is_some() {
                        response::read(&raw)?.0.len() as u64
                    } else {
                        0
                    };
                    meta["status"] = json!("aborted");
                    meta["errorCode"] = json!("interrupted");
                    meta["finishedAtMs"] = json!(run.finished_at_ms);
                    meta["bodyComplete"] = json!(false);
                    meta["streamBytes"] = json!(size);
                    write_meta(&path, &meta)?;
                }
                if attempt == entry.attempt {
                    let has_raw =
                        filesystem::regular_file(&response::path(&directory, attempt, &meta)?)?
                            .is_some();
                    if entry.has_raw_stream != has_raw {
                        entry.has_raw_stream = has_raw;
                        changed = true;
                    }
                    // 若进程恰好在 meta 完成后、DB 更新前退出，保留已经落盘的指标。
                    for (field, value) in [
                        (&mut entry.first_token_ms, &meta["firstTokenMs"]),
                        (&mut entry.total_time_ms, &meta["totalTimeMs"]),
                        (&mut entry.input_tokens, &meta["usage"]["inputTokens"]),
                        (&mut entry.output_tokens, &meta["usage"]["outputTokens"]),
                    ] {
                        if field.is_none() {
                            if let Some(value) = value.as_u64().filter(|v| *v <= i64::MAX as u64) {
                                *field = Some(value);
                                changed = true;
                            }
                        }
                    }
                }
            }
        }
        Ok(changed)
    }
}
fn collect(
    path: &Path,
    files: &mut Vec<PathBuf>,
    directories: &mut Vec<PathBuf>,
    depth: usize,
) -> Result<()> {
    if depth > 32 || !filesystem::directory(path, false)? {
        return Err(IqError::Evidence);
    }
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let meta = fs::symlink_metadata(entry.path())?;
        if meta.file_type().is_symlink() {
            return Err(IqError::Evidence);
        }
        if meta.is_dir() {
            collect(&entry.path(), files, directories, depth + 1)?;
        } else {
            filesystem::regular_file(&entry.path())?.ok_or(IqError::Evidence)?;
            files.push(entry.path());
        }
    }
    directories.push(path.into());
    Ok(())
}
fn write_meta(path: &Path, meta: &Value) -> Result<()> {
    filesystem::regular_file(path)?;
    let parent = path.parent().ok_or(IqError::Evidence)?;
    let temporary = parent.join(format!(".meta-{}.tmp", uuid::Uuid::new_v4()));
    let mut writer = FileWriter::create_new(&temporary)?;
    writer.write_all(&serde_json::to_vec_pretty(meta).map_err(|_| IqError::Evidence)?)?;
    writer.sync_all()?;
    drop(writer);
    fs::rename(temporary, path)?;
    #[cfg(unix)]
    fs::File::open(parent)?.sync_all()?;
    Ok(())
}
#[derive(Clone, Default)]
pub(super) struct AttemptFacts {
    pub has_raw_stream: bool,
    pub first_token_ms: Option<u64>,
    pub duration_ms: Option<u64>,
    pub usage: TokenUsage,
}
struct AttemptFiles {
    stream: FileWriter,
    answer: String,
    received_bytes: u64,
    raw_path: PathBuf,
    meta_path: PathBuf,
    meta: Value,
    facts: AttemptFacts,
    streaming: bool,
}
pub(super) struct AttemptCapture {
    inner: Arc<Mutex<AttemptFiles>>,
    activity: Arc<BlockingActivity>,
}
impl AttemptCapture {
    pub(super) async fn facts(&self) -> AttemptFacts {
        self.inner.lock().await.facts.clone()
    }
}
enum OwnedEvent {
    Response(u16, bool, String, u64),
    Chunk(Vec<u8>, u64),
    Output(OutputKind, String, u64),
    Finished(CaptureOutcome, bool, u64, TokenUsage),
}
impl CaptureSink for AttemptCapture {
    fn record<'a>(
        &'a mut self,
        event: CaptureEvent<'a>,
    ) -> BoxFuture<'a, std::result::Result<(), CaptureError>> {
        Box::pin(async move {
            // 不复制或序列化 Started 中的完整请求、URL 和请求头。
            let event = match event {
                CaptureEvent::Started { .. } => return Ok(()),
                CaptureEvent::Response {
                    status,
                    kind,
                    content_type,
                    elapsed_ms,
                    ..
                } => OwnedEvent::Response(
                    status,
                    kind == crate::shared::model_client::ResponseKind::EventStream,
                    content_type.to_owned(),
                    elapsed_ms,
                ),
                CaptureEvent::Chunk { bytes, elapsed_ms } => {
                    OwnedEvent::Chunk(bytes.to_vec(), elapsed_ms)
                }
                CaptureEvent::Output {
                    kind,
                    text,
                    elapsed_ms,
                } => OwnedEvent::Output(kind, text.to_owned(), elapsed_ms),
                CaptureEvent::Finished {
                    outcome,
                    body_complete,
                    elapsed_ms,
                    metadata,
                } => {
                    OwnedEvent::Finished(outcome, body_complete, elapsed_ms, metadata.usage.clone())
                }
            };
            let mut files = self.inner.clone().lock_owned().await;
            let held = self.activity.enter();
            tokio::task::spawn_blocking(move || -> Result<()> {
                let _held = held;
                match event {
                    OwnedEvent::Response(status, streaming, content_type, elapsed) => {
                        files.meta["statusCode"] = json!(status);
                        files.streaming = streaming;
                        response::event(
                            &mut files.stream,
                            json!({"event":"response_started",
                            "statusCode":status,"contentType":content_type,"elapsedMs":elapsed}),
                        )?;
                    }
                    OwnedEvent::Chunk(bytes, elapsed) => {
                        response::chunk(&mut files.stream, &bytes, elapsed)?;
                        files.received_bytes += bytes.len() as u64;
                        // 原始响应不改写、不截断，包括心跳、未知事件和上游自己的回显。
                    }
                    OwnedEvent::Output(kind, text, elapsed) => {
                        if files.streaming {
                            files.facts.first_token_ms.get_or_insert(elapsed);
                        }
                        if kind == OutputKind::Text {
                            files.answer.push_str(&text);
                        }
                    }
                    OwnedEvent::Finished(outcome, complete, elapsed, usage) => {
                        files.facts.duration_ms = Some(elapsed);
                        files.facts.usage = usage.clone();
                        files.meta["firstTokenMs"] = json!(files.facts.first_token_ms);
                        files.meta["totalTimeMs"] = json!(elapsed);
                        files.meta["usage"] = json!(usage);
                        files.meta["streamBytes"] = json!(files.received_bytes);
                        files.meta["bodyComplete"] =
                            json!(complete && !matches!(outcome, CaptureOutcome::WriteFailed));
                        files.meta["finishedAtMs"] = json!(now_ms());
                        let (status, error) = match outcome {
                            CaptureOutcome::Succeeded => ("completed", None),
                            CaptureOutcome::Cancelled => ("aborted", Some("cancelled")),
                            CaptureOutcome::Failed(CheckFailure::Timeout) => {
                                ("timeout", Some("output_timeout"))
                            }
                            CaptureOutcome::Failed(CheckFailure::Http) => {
                                ("error", Some("upstream_http"))
                            }
                            CaptureOutcome::Failed(CheckFailure::Network) => {
                                ("error", Some("network_error"))
                            }
                            CaptureOutcome::WriteFailed => ("error", Some("evidence_write_failed")),
                            _ => ("error", Some("invalid_response")),
                        };
                        files.meta["status"] = json!(status);
                        files.meta["errorCode"] = json!(error);
                        let received = files.received_bytes;
                        response::event(
                            &mut files.stream,
                            json!({"event":"request_finished",
                            "outcome":outcome,"bodyComplete":complete,"elapsedMs":elapsed,
                            "receivedBytes":received,"usage":usage}),
                        )?;
                        // meta 尽量先落地。若流文件同步失败，仍不把响应标为完整。
                        let saved = files
                            .stream
                            .sync_all()
                            .map_err(IqError::from)
                            .and_then(|()| {
                                artifacts::write(
                                    files.raw_path.parent().ok_or(IqError::Evidence)?,
                                    files.meta["attempt"].as_u64().ok_or(IqError::Evidence)? as u32,
                                    &files.answer,
                                )
                            });
                        let names = match saved {
                            Ok(names) => names,
                            Err(_) => {
                                files.meta["status"] = json!("error");
                                files.meta["errorCode"] = json!("evidence_write_failed");
                                files.meta["bodyComplete"] = json!(false);
                                write_meta(&files.meta_path, &files.meta)?;
                                return Err(IqError::Evidence);
                            }
                        };
                        files.meta["outputFiles"] = json!(names);
                        write_meta(&files.meta_path, &files.meta)?;
                    }
                }
                Ok(())
            })
            .await
            .map_err(|_| CaptureError)?
            .map_err(|_| CaptureError)
        })
    }
}
