use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCategory {
    Validation,
    NotFound,
    Credential,
    Database,
    Network,
    Upstream,
    Conflict,
    Internal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    InvalidInput,
    NotFound,
    CredentialUnavailable,
    DatabaseUnavailable,
    NetworkUnavailable,
    AuthenticationFailed,
    RateLimited,
    Timeout,
    ProtocolError,
    Cancelled,
    Conflict,
    Internal,
}

impl ErrorCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidInput => "invalid_input",
            Self::NotFound => "not_found",
            Self::CredentialUnavailable => "credential_unavailable",
            Self::DatabaseUnavailable => "database_unavailable",
            Self::NetworkUnavailable => "network_unavailable",
            Self::AuthenticationFailed => "authentication_failed",
            Self::RateLimited => "rate_limited",
            Self::Timeout => "timeout",
            Self::ProtocolError => "protocol_error",
            Self::Cancelled => "cancelled",
            Self::Conflict => "conflict",
            Self::Internal => "internal",
        }
    }

    pub const fn category(self) -> ErrorCategory {
        match self {
            Self::InvalidInput => ErrorCategory::Validation,
            Self::NotFound => ErrorCategory::NotFound,
            Self::CredentialUnavailable => ErrorCategory::Credential,
            Self::DatabaseUnavailable => ErrorCategory::Database,
            Self::NetworkUnavailable | Self::Timeout | Self::Cancelled => ErrorCategory::Network,
            Self::AuthenticationFailed | Self::RateLimited | Self::ProtocolError => {
                ErrorCategory::Upstream
            }
            Self::Conflict => ErrorCategory::Conflict,
            Self::Internal => ErrorCategory::Internal,
        }
    }

    pub const fn retryable(self) -> bool {
        matches!(
            self,
            Self::CredentialUnavailable
                | Self::DatabaseUnavailable
                | Self::NetworkUnavailable
                | Self::RateLimited
                | Self::Timeout
                | Self::Conflict
        )
    }

    pub const fn safe_message(self) -> &'static str {
        match self {
            Self::InvalidInput => "The submitted value is invalid.",
            Self::NotFound => "The requested resource was not found.",
            Self::CredentialUnavailable => "The local credential file is unavailable.",
            Self::DatabaseUnavailable => "The RelayPulse database is unavailable.",
            Self::NetworkUnavailable => "The network request could not be completed.",
            Self::AuthenticationFailed => "The upstream service rejected authentication.",
            Self::RateLimited => "The upstream service rate limited the request.",
            Self::Timeout => "The operation timed out.",
            Self::ProtocolError => "The upstream response did not match the configured protocol.",
            Self::Cancelled => "The operation was cancelled.",
            Self::Conflict => "Another conflicting operation is already running.",
            Self::Internal => "RelayPulse could not complete the operation.",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DomainError {
    code: ErrorCode,
    field: Option<&'static str>,
}

impl DomainError {
    pub const fn invalid(field: &'static str) -> Self {
        Self {
            code: ErrorCode::InvalidInput,
            field: Some(field),
        }
    }

    pub const fn code(&self) -> ErrorCode {
        self.code
    }

    pub const fn field(&self) -> Option<&'static str> {
        self.field
    }
}

impl fmt::Display for DomainError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code.safe_message())
    }
}

impl std::error::Error for DomainError {}

pub type Result<T> = std::result::Result<T, DomainError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_error_metadata_does_not_depend_on_internal_messages() {
        let error = DomainError::invalid("base_url");

        assert_eq!(error.code().as_str(), "invalid_input");
        assert_eq!(error.code().category(), ErrorCategory::Validation);
        assert!(!error.code().retryable());
        assert_eq!(error.field(), Some("base_url"));
        assert_eq!(error.to_string(), "The submitted value is invalid.");
    }
}
