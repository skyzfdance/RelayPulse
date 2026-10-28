use crate::{commands::error::CommandError, modules::monitoring::RecordError};
impl From<RecordError> for CommandError {
    fn from(error: RecordError) -> Self {
        let message = error.to_string();
        let (code, category, field, retryable) = match error {
            RecordError::Validation { field, .. } => {
                ("probe_records_validation", "validation", Some(field), false)
            }
            RecordError::NotFound => ("probe_record_not_found", "not_found", None, false),
            RecordError::RequestUnavailable => (
                "probe_record_request_unavailable",
                "unavailable",
                None,
                true,
            ),
            RecordError::ResponseUnavailable => (
                "probe_record_response_unavailable",
                "unavailable",
                None,
                true,
            ),
            RecordError::LogUnavailable => {
                ("probe_record_log_unavailable", "unavailable", None, true)
            }
            RecordError::LogOpenFailed => {
                ("probe_record_log_open_failed", "unavailable", None, true)
            }
            RecordError::Unavailable | RecordError::Busy => {
                ("probe_records_unavailable", "unavailable", None, true)
            }
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
