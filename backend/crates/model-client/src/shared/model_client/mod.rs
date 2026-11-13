//! 无数据库、业务实体或自动审计的模型协议客户端。
mod cancellation;
mod capture;
mod catalog;
mod check;
mod endpoint;
mod generation;
mod http;
mod parameters;
mod request;
mod response_output;
mod stream;
#[cfg(any(test, feature = "test-support"))]
#[path = "../../../../../tests/support/model_client.rs"]
pub mod test_support;
#[cfg(test)]
#[path = "../../../../../tests/unit/model_client/mod.rs"]
mod tests;
mod types;
mod usage;
mod user_agent;
pub use cancellation::Cancellation;
pub use capture::{
    CaptureError, CaptureEvent, CaptureOutcome, CaptureSink, OutputKind, ResponseKind,
};
pub use endpoint::{
    base_url, credential, endpoint, header_parameters, headers as validate_headers,
};
pub use generation::{
    decode_generation_output, validate_parameters as validate_generation_parameters,
    DecodedGenerationOutput, GenerationOutputPart,
};
pub use parameters::{enabled as parameter_enabled, parameter_value, parameters};
pub use types::*;

#[derive(Clone)]
pub struct ModelClient {
    transport: http::Transport,
}
impl Default for ModelClient {
    fn default() -> Self {
        Self::new(
            &relaypulse_runtime::ApplicationInfo::new(
                "relaypulse-model-client",
                env!("CARGO_PKG_VERSION"),
            )
            .expect("SDK metadata"),
        )
    }
}
impl ModelClient {
    pub fn new(application: &relaypulse_runtime::ApplicationInfo) -> Self {
        Self {
            transport: http::Transport::new(application),
        }
    }
    pub async fn check_once(
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
    pub async fn list_models(
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
