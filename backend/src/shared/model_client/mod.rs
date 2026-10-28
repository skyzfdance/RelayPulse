//! 无数据库、业务实体或自动审计的模型协议客户端。
mod cancellation;
mod capture;
mod catalog;
mod check;
mod endpoint;
mod http;
mod parameters;
mod request;
mod stream;
#[cfg(test)]
#[path = "../../../tests/support/model_client.rs"]
pub(crate) mod test_support;
#[cfg(test)]
#[path = "../../../tests/unit/model_client/mod.rs"]
mod tests;
mod types;
mod usage;
mod user_agent;
pub(crate) use cancellation::Cancellation;
pub(crate) use capture::{CaptureError, CaptureEvent, CaptureOutcome, CaptureSink, ResponseKind};
pub(crate) use endpoint::{base_url, credential, endpoint, headers as validate_headers};
pub(crate) use parameters::{enabled as parameter_enabled, parameter_value, parameters};
pub(crate) use types::*;

#[derive(Clone)]
pub(crate) struct ModelClient {
    transport: http::Transport,
}
impl Default for ModelClient {
    fn default() -> Self {
        Self {
            transport: http::Transport::new(),
        }
    }
}
impl ModelClient {
    pub(crate) async fn check_once(
        &self,
        request: &CheckRequest,
        options: ExecutionOptions,
        cancellation: &Cancellation,
        mut capture: Option<&mut dyn CaptureSink>,
    ) -> Result<CheckResult, ClientError> {
        let prepared = request::prepare(request)?;
        let start = std::time::Instant::now();
        let mut metadata = ResponseMetadata::default();
        let result = tokio::select! {
            biased;
            _ = cancellation.cancelled() => Err(ClientError::Cancelled),
            result = self.perform_check(&request.target, prepared, options, &mut capture, start, &mut metadata) => result,
        };
        // 显式取消仍通知写入器；若整个调用被丢弃，文件实现保留 partial 标记兜底。
        capture::emit(
            &mut capture,
            CaptureEvent::Finished {
                outcome: CaptureOutcome::from_result(&result),
                body_complete: metadata.body_complete,
                elapsed_ms: check::elapsed(start),
                metadata: &metadata,
            },
        )
        .await?;
        result
    }
    pub(crate) async fn list_models(
        &self,
        request: &CatalogRequest,
        options: ExecutionOptions,
        cancellation: &Cancellation,
    ) -> Result<Vec<String>, ClientError> {
        tokio::select! {
            biased;
            _ = cancellation.cancelled() => Err(ClientError::Cancelled),
            result = tokio::time::timeout(options.total_timeout, catalog::discover(self, request, options)) =>
                result.map_err(|_| ClientError::Timeout)?,
        }
    }
}
