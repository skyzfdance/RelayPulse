use crate::{
    commands::error::CommandError,
    modules::{iq_test::IqError, sites::SiteConfigError},
};
impl From<IqError> for CommandError {
    fn from(error: IqError) -> Self {
        let message = error.to_string();
        let (code, category, field, retryable) = match error {
            IqError::Validation { field, .. } => {
                ("iq_validation", "validation", Some(field), false)
            }
            IqError::Source(SiteConfigError::Invalid { field, .. }) => {
                ("iq_validation", "validation", Some(field), false)
            }
            IqError::NotFound | IqError::Source(SiteConfigError::NotFound) => {
                ("iq_not_found", "not_found", None, false)
            }
            IqError::AlreadyRunning => ("AlreadyRunning", "conflict", None, false),
            IqError::Closing => ("iq_cancelled", "cancelled", None, false),
            IqError::Storage | IqError::Source(SiteConfigError::Storage) => {
                ("database_unavailable", "storage", None, true)
            }
            IqError::Evidence => ("iq_evidence_failed", "storage", None, true),
            IqError::CleanupPending => ("iq_cleanup_pending", "storage", None, true),
            IqError::RecoveryRequired => ("iq_recovery_required", "storage", None, true),
            IqError::Source(SiteConfigError::MissingCredential) => (
                "site_credential_missing",
                "validation",
                Some("input.modelKey"),
                false,
            ),
            IqError::Source(_) => ("iq_validation", "validation", Some("input.modelKey"), false),
            IqError::OpenFailed => ("iq_raw_open_failed", "unavailable", None, true),
            IqError::Worker | IqError::Unavailable => ("iq_unavailable", "unavailable", None, true),
        };
        Self {
            code,
            category,
            message,
            field,
            retryable,
            operation_id: None,
        }
    }
}
