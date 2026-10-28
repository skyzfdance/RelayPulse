//! 统一执行和记录入口：业务只提供操作标识及错误分类，不暴露业务结果或原始错误。
use super::{Diagnostics, Failure, Outcome};
use std::sync::Arc;

/// 错误解释归所属模块；公共层不匹配模块错误，也不调用 Debug/Display。
pub(crate) trait DiagnosticFailure {
    fn diagnostic_failure(&self) -> Failure;
}

/// 未注入文件资源时仍正常执行业务，供模块独立测试使用。
#[derive(Default)]
pub(crate) struct Recorder(Option<Arc<Diagnostics>>);

impl Recorder {
    pub(crate) fn new(diagnostics: Arc<Diagnostics>) -> Self {
        Self(Some(diagnostics))
    }

    pub(crate) async fn observe_async<T, E: DiagnosticFailure>(
        &self,
        operation: &'static str,
        action: impl std::future::Future<Output = Result<T, E>>,
    ) -> Result<T, E> {
        let result = action.await;
        if let Some(diagnostics) = &self.0 {
            let outcome = match &result {
                Ok(_) => Outcome::Succeeded,
                Err(error) => Outcome::Failed(error.diagnostic_failure()),
            };
            diagnostics.record_best_effort(operation, outcome);
        }
        result
    }

    pub(crate) fn observe<T, E: DiagnosticFailure>(
        &self,
        operation: &'static str,
        action: impl FnOnce() -> Result<T, E>,
    ) -> Result<T, E> {
        let result = action();
        if let Some(diagnostics) = &self.0 {
            let outcome = match &result {
                Ok(_) => Outcome::Succeeded,
                Err(error) => Outcome::Failed(error.diagnostic_failure()),
            };
            diagnostics.record_best_effort(operation, outcome);
        }
        result
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/diagnostics/recorder.rs"]
mod tests;
