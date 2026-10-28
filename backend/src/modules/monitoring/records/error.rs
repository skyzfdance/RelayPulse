use crate::shared::diagnostics::{DiagnosticFailure, Failure};

#[derive(Debug, thiserror::Error)]
pub(crate) enum RecordError {
    #[error("{message}")]
    Validation {
        field: &'static str,
        message: &'static str,
    },
    #[error("这条探测记录已不存在或已过期。")]
    NotFound,
    #[error("探测记录暂时不可用，请稍后重试。")]
    Unavailable,
    #[error("正式探测或记录操作正在进行，请稍后重试。")]
    Busy,
    #[error("无法读取这条记录的请求内容。")]
    RequestUnavailable,
    #[error("无法读取这条记录的响应日志。")]
    ResponseUnavailable,
    #[error("这条记录的日志目录不可用。")]
    LogUnavailable,
    #[error("系统无法打开这条记录的日志目录。")]
    LogOpenFailed,
}
pub(super) type Result<T> = std::result::Result<T, RecordError>;
impl DiagnosticFailure for RecordError {
    fn diagnostic_failure(&self) -> Failure {
        match self {
            Self::Validation { .. } | Self::NotFound => Failure::Validation,
            Self::Busy => Failure::Conflict,
            Self::LogOpenFailed => Failure::Platform,
            _ => Failure::Storage,
        }
    }
}
