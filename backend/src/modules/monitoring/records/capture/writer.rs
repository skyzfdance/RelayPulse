use super::super::{redaction, RecordContext};
use super::body;
use crate::modules::monitoring::health;
use crate::shared::{
    file_output::{OutputError, OutputSession, OutputStore},
    model_client::{CaptureError, CaptureEvent, CaptureSink},
};
use futures_util::future::BoxFuture;
use serde_json::{json, Value};
use std::{collections::HashSet, future::Future, path::PathBuf};

#[derive(Clone)]
pub(in crate::modules::monitoring) struct CaptureStore {
    pub(super) output: OutputStore,
}
pub(in crate::modules::monitoring) struct FileCapture {
    output: OutputSession,
    redactor: redaction::Redactor,
    received: u64,
    processed: u64,
    body: body::BodyWriter,
}
impl CaptureStore {
    pub(in crate::modules::monitoring) fn new(root: PathBuf) -> Self {
        Self {
            output: OutputStore::new(root),
        }
    }
    pub(in crate::modules::monitoring) async fn start(
        &self,
        id: String,
        site: &str,
        model: &str,
        secret: &str,
    ) -> Result<FileCapture, OutputError> {
        let output = self.output.start(id.clone()).await?;
        let capture = FileCapture {
            output,
            redactor: redaction::Redactor::new(secret),
            received: 0,
            processed: 0,
            body: body::BodyWriter::default(),
        };
        capture
            .event(
                json!({"event":"record_created", "recordId":id, "siteId":site, "modelId":model,
            "startedAtMs":health::now_ms(), "formatVersion":2, "credentialRedaction":true}),
            )
            .await?;
        Ok(capture)
    }
    pub(in crate::modules::monitoring) async fn cleanup(
        &self,
        retained: impl Future<Output = Result<HashSet<String>, OutputError>>,
        retention_days: u16,
    ) -> Result<(), OutputError> {
        // 先枚举非活动文件，再轮询保留 ID 查询，避免把刚提交的新记录误认成孤儿。
        // 查询由 monitoring 跟踪；此适配器只负责文件，不另起无人等待的数据库 worker。
        let entries = self.output.entries().await?;
        let retained = retained.await?;
        let cutoff = health::now_ms().saturating_sub(u64::from(retention_days) * 86_400_000);
        for entry in entries {
            if (entry.published && !retained.contains(&entry.id))
                || (!entry.published && entry.modified_ms < cutoff)
            {
                self.output.remove(entry).await?;
            }
        }
        Ok(())
    }
    pub(in crate::modules::monitoring) fn close(&self) {
        self.output.close();
    }
    pub(in crate::modules::monitoring) async fn drained(&self) {
        self.output.drained().await;
    }
}
impl FileCapture {
    async fn event(&self, event: Value) -> Result<(), OutputError> {
        write_event(&self.output, event).await
    }
    pub(in crate::modules::monitoring) async fn context(
        &self,
        context: &RecordContext,
    ) -> Result<(), OutputError> {
        self.event(json!({"event":"request_context", "context":context}))
            .await
    }
    pub(in crate::modules::monitoring) async fn publish(&self) -> Result<(), OutputError> {
        self.output.publish().await
    }
}
impl CaptureSink for FileCapture {
    fn record<'a>(
        &'a mut self,
        event: CaptureEvent<'a>,
    ) -> BoxFuture<'a, Result<(), CaptureError>> {
        Box::pin(async move {
            let event = match event {
                CaptureEvent::Started {
                    protocol,
                    method,
                    url,
                    body,
                    headers,
                } => {
                    let url = reqwest::Url::parse(url).map_err(|_| CaptureError)?;
                    json!({"event":"request_started", "protocol":protocol,
                        "method":method,"endpoint":self.redactor.endpoint(&url),"body":self.redactor.value(body),
                        "headers":self.redactor.headers(headers)})
                }
                CaptureEvent::Response {
                    status,
                    kind,
                    content_type,
                    headers,
                    elapsed_ms,
                } => {
                    self.body = body::BodyWriter::new(kind, content_type);
                    json!({"event":"response_started", "statusCode":status, "kind":kind,
                        "contentType":self.redactor.text(content_type), "elapsedMs":elapsed_ms,
                        "headers":self.redactor.headers(headers)})
                }
                CaptureEvent::Chunk { bytes, elapsed_ms } => {
                    let output = self.redactor.push(bytes, false);
                    self.received += bytes.len() as u64;
                    self.processed += output.len() as u64;
                    self.event(json!({"event":"response_chunk", "elapsedMs":elapsed_ms,
                        "receivedBytes":bytes.len(), "receivedTotal":self.received}))
                        .await
                        .map_err(|_| CaptureError)?;
                    self.body
                        .push(&self.output, &output, elapsed_ms)
                        .await
                        .map_err(|_| CaptureError)?;
                    return Ok(());
                }
                CaptureEvent::Finished {
                    outcome,
                    body_complete,
                    elapsed_ms,
                    metadata,
                } => {
                    let tail = self.redactor.push(&[], true);
                    self.body
                        .push(&self.output, &tail, elapsed_ms)
                        .await
                        .map_err(|_| CaptureError)?;
                    self.body
                        .finish(&self.output, body_complete, elapsed_ms)
                        .await
                        .map_err(|_| CaptureError)?;
                    self.processed += tail.len() as u64;
                    json!({"event":"request_finished", "outcome":outcome, "bodyComplete":body_complete,
                        "elapsedMs":elapsed_ms, "receivedTotal":self.received, "processedBytes":self.processed,
                        "metadata":self.redactor.value(&serde_json::to_value(metadata).map_err(|_| CaptureError)?)})
                }
            };
            self.event(event).await.map_err(|_| CaptureError)
        })
    }
}

pub(super) async fn write_event(output: &OutputSession, event: Value) -> Result<(), OutputError> {
    let mut bytes = serde_json::to_vec(&event).map_err(|_| OutputError)?;
    bytes.push(b'\n');
    output.append("response.jsonl", &bytes).await
}
