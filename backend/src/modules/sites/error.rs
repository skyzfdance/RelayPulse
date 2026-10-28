use super::{SiteConfigError, TaskError};
use crate::shared::{
    diagnostics::{DiagnosticFailure, Failure},
    model_client::ClientError,
    persistence::PersistenceError,
};
#[derive(Debug, thiserror::Error)]
pub(crate) enum SiteError {
    #[error(transparent)]
    Config(#[from] SiteConfigError),
    #[error(transparent)]
    Http(ClientError),
    #[error(transparent)]
    Task(#[from] TaskError),
    #[error("站点存储操作失败，请重试。")]
    Storage,
    #[error("本次探测的记录文件写入失败。")]
    Capture,
    #[error("站点任务执行失败，请重试。")]
    Worker,
}
impl From<PersistenceError> for SiteError {
    fn from(_: PersistenceError) -> Self {
        Self::Storage
    }
}
impl DiagnosticFailure for SiteError {
    fn diagnostic_failure(&self) -> Failure {
        match self {
            Self::Config(e) => e.diagnostic_failure(),
            Self::Storage | Self::Capture => Failure::Storage,
            Self::Http(_) => Failure::Network,
            Self::Task(_) => Failure::Conflict,
            Self::Worker => Failure::Platform,
        }
    }
}
impl From<ClientError> for SiteError {
    fn from(error: ClientError) -> Self {
        match error {
            ClientError::Capture => Self::Capture,
            ClientError::Invalid(error) => Self::Config(error.into()),
            ClientError::Cancelled => Self::Task(TaskError::Closing),
            error => Self::Http(error),
        }
    }
}
