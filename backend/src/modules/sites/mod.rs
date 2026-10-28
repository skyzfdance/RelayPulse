//! 站点业务：配置维护、同步、草稿试探、继承与访问协调；协议执行委托公共客户端。
mod access;
mod error;
mod inputs;
mod policy;
mod service;
mod sqlite;
mod sync;
#[cfg(test)]
#[path = "../../../tests/unit/sites/configuration.rs"]
mod tests;
mod types;
mod validation;
use crate::shared::{
    diagnostics::{DiagnosticFailure, Failure},
    persistence::PersistenceError,
};
pub(crate) use access::{SiteAccess, TaskError};
pub(crate) use error::SiteError;
pub(crate) use inputs::{ExecutionSnapshot, SiteExecutionScope};
pub(crate) use service::{SiteMutation, SiteService};
use sqlite::require_model;
// 以下两种导入只会编译其中一种：测试时向整个 crate 开放 SiteStore，供其他模块的测试准备数据；
// 非测试构建仅在 sites 内部使用，外部通过 SiteService 操作，避免绕过任务互斥和同步协调。
#[cfg(test)]
pub(crate) use sqlite::SiteStore;
#[cfg(not(test))]
use sqlite::SiteStore;
pub(crate) use sqlite::SCHEMA;
pub(crate) use types::*;

#[derive(Debug, thiserror::Error)]
pub(crate) enum SiteConfigError {
    #[error("{message}")]
    Invalid {
        field: &'static str,
        message: &'static str,
    },
    #[error("站点或模型不存在，请刷新后重试。")]
    NotFound,
    #[error("站点配置已变化，请刷新后重新检查。")]
    Stale,
    #[error("模型名称已存在。")]
    Duplicate,
    #[error("请先配置站点访问凭据。")]
    MissingCredential,
    #[error("本地站点数据不可用。")]
    Storage,
}
impl From<PersistenceError> for SiteConfigError {
    fn from(_: PersistenceError) -> Self {
        Self::Storage
    }
}
impl From<rusqlite::Error> for SiteConfigError {
    fn from(_: rusqlite::Error) -> Self {
        Self::Storage
    }
}
impl DiagnosticFailure for SiteConfigError {
    fn diagnostic_failure(&self) -> Failure {
        match self {
            Self::Storage => Failure::Storage,
            _ => Failure::Validation,
        }
    }
}
pub(crate) type Result<T> = std::result::Result<T, SiteConfigError>;
pub(crate) fn invalid(field: &'static str, message: &'static str) -> SiteConfigError {
    SiteConfigError::Invalid { field, message }
}

impl From<crate::shared::model_client::InputError> for SiteConfigError {
    fn from(value: crate::shared::model_client::InputError) -> Self {
        Self::Invalid {
            field: value.field,
            message: value.message,
        }
    }
}
