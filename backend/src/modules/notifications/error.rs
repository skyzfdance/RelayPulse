use super::NotificationPermission;
use crate::shared::{
    diagnostics::{DiagnosticFailure, Failure},
    persistence::PersistenceError,
};

#[derive(Debug, thiserror::Error)]
pub(crate) enum NotificationError {
    #[error("invalid notification field {field}: {message}")]
    InvalidInput {
        field: &'static str,
        message: &'static str,
    },
    #[error("notification rule not found")]
    RuleNotFound,
    #[error("invalid stored notification configuration")]
    CorruptSettings,
    #[error("notification storage failed: {0}")]
    Storage(#[from] PersistenceError),
    #[error("notifications are disabled")]
    Disabled,
    #[error("notification permission: {0:?}")]
    Permission(NotificationPermission),
    #[error("native notification requires an application bundle")]
    ApplicationIdentity,
    #[error("notification platform operation failed: {0}")]
    Platform(&'static str),
}

impl DiagnosticFailure for NotificationError {
    fn diagnostic_failure(&self) -> Failure {
        match self {
            Self::InvalidInput { .. } | Self::RuleNotFound => Failure::Validation,
            Self::CorruptSettings | Self::Storage(_) => Failure::Storage,
            Self::Permission(_) => Failure::Permission,
            Self::Disabled => Failure::Disabled,
            Self::ApplicationIdentity | Self::Platform(_) => Failure::Platform,
        }
    }
}
