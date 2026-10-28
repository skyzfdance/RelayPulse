//! 每次调用显式传入的记录接口；客户端不认识文件路径、业务 ID 或保留策略。
use super::{CheckFailure, CheckResult, ClientError, Protocol, ResponseMetadata};
use futures_util::future::BoxFuture;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ResponseKind {
    EventStream,
    Json,
    Other,
}

pub(crate) enum CaptureEvent<'a> {
    Started {
        protocol: Protocol,
        method: &'static str,
        url: &'a str,
        body: &'a serde_json::Value,
        headers: &'a reqwest::header::HeaderMap,
    },
    Response {
        status: u16,
        kind: ResponseKind,
        content_type: &'a str,
        headers: &'a reqwest::header::HeaderMap,
        elapsed_ms: u64,
    },
    Chunk {
        bytes: &'a [u8],
        elapsed_ms: u64,
    },
    Finished {
        outcome: CaptureOutcome,
        body_complete: bool,
        elapsed_ms: u64,
        metadata: &'a ResponseMetadata,
    },
}
#[derive(Clone, Copy, Serialize)]
#[serde(tag = "status", content = "failure", rename_all = "snake_case")]
pub(crate) enum CaptureOutcome {
    Succeeded,
    Failed(CheckFailure),
    Cancelled,
    Invalid,
    WriteFailed,
}
impl CaptureOutcome {
    pub(super) fn from_result(result: &Result<CheckResult, ClientError>) -> Self {
        match result {
            Ok(result) => result.failure.map_or(Self::Succeeded, Self::Failed),
            Err(ClientError::Cancelled) => Self::Cancelled,
            Err(ClientError::Capture) => Self::WriteFailed,
            Err(_) => Self::Invalid,
        }
    }
}
#[derive(Debug)]
pub(crate) struct CaptureError;

pub(crate) trait CaptureSink: Send {
    fn record<'a>(&'a mut self, event: CaptureEvent<'a>)
        -> BoxFuture<'a, Result<(), CaptureError>>;
}
pub(super) async fn emit(
    sink: &mut Option<&mut dyn CaptureSink>,
    event: CaptureEvent<'_>,
) -> Result<(), ClientError> {
    if let Some(sink) = sink.as_deref_mut() {
        sink.record(event).await.map_err(|_| ClientError::Capture)?;
    }
    Ok(())
}
