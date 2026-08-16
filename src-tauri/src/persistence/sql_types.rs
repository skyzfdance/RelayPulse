use std::fmt;

use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ToSql, ToSqlOutput, ValueRef};

use crate::domain::{
    AuthPreset, CredentialAction, CredentialOperationStatus, DiscoveryStatus, EntityId, HttpMethod,
    LifecycleState, ModelSource, ParameterType, ProbeMode, ProbeRunKind, ProbeStatus, ResponseMode,
    SiteProtocol, ThemeMode, TimestampMs, TrayAction,
};

#[derive(Debug)]
struct InvalidPersistedValue(&'static str);

impl fmt::Display for InvalidPersistedValue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid persisted value for {}", self.0)
    }
}

impl std::error::Error for InvalidPersistedValue {}

fn invalid(field: &'static str) -> FromSqlError {
    FromSqlError::Other(Box::new(InvalidPersistedValue(field)))
}

impl ToSql for EntityId {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::Borrowed(ValueRef::Text(
            self.as_str().as_bytes(),
        )))
    }
}

impl FromSql for EntityId {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        let value = value.as_str()?;
        Self::parse(value).map_err(|_| invalid("id"))
    }
}

impl ToSql for TimestampMs {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(self.as_i64().into())
    }
}

impl FromSql for TimestampMs {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        Self::new(value.as_i64()?).map_err(|_| invalid("timestamp"))
    }
}

macro_rules! sqlite_enum {
    ($type:ty, $field:literal, { $($variant:path => $value:literal),+ $(,)? }) => {
        impl ToSql for $type {
            fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
                let value = match self {
                    $($variant => $value,)+
                };
                Ok(ToSqlOutput::Borrowed(ValueRef::Text(value.as_bytes())))
            }
        }

        impl FromSql for $type {
            fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
                match value.as_str()? {
                    $($value => Ok($variant),)+
                    _ => Err(invalid($field)),
                }
            }
        }
    };
}

sqlite_enum!(SiteProtocol, "site.protocol", {
    SiteProtocol::OpenAiCompatible => "openai-compatible",
    SiteProtocol::Anthropic => "anthropic",
    SiteProtocol::OpenAiResponses => "openai-responses",
    SiteProtocol::CustomHttp => "custom-http",
});

sqlite_enum!(HttpMethod, "http_method", {
    HttpMethod::Get => "GET",
    HttpMethod::Post => "POST",
});

sqlite_enum!(AuthPreset, "site.auth_preset", {
    AuthPreset::Bearer => "bearer",
    AuthPreset::XApiKey => "x-api-key",
    AuthPreset::ApiKey => "api-key",
    AuthPreset::None => "none",
});

sqlite_enum!(LifecycleState, "site.lifecycle_state", {
    LifecycleState::Ready => "ready",
    LifecycleState::CredentialPending => "credential_pending",
    LifecycleState::CleanupPending => "cleanup_pending",
    LifecycleState::DeletePending => "delete_pending",
});

sqlite_enum!(ModelSource, "model.source", {
    ModelSource::Discovered => "discovered",
    ModelSource::Manual => "manual",
});

sqlite_enum!(DiscoveryStatus, "model.discovery_status", {
    DiscoveryStatus::Available => "available",
    DiscoveryStatus::Missing => "missing",
});

sqlite_enum!(ProbeMode, "probe_policy.mode", {
    ProbeMode::Inherit => "inherit",
    ProbeMode::DisabledThinking => "disabled-thinking",
    ProbeMode::MinimumReasoning => "minimum-reasoning",
});

sqlite_enum!(ParameterType, "probe_parameter.value_type", {
    ParameterType::String => "string",
    ParameterType::Boolean => "boolean",
    ParameterType::Number => "number",
    ParameterType::Json => "json",
});

sqlite_enum!(ProbeRunKind, "probe_result.run_kind", {
    ProbeRunKind::Scheduled => "scheduled",
    ProbeRunKind::Manual => "manual",
    ProbeRunKind::Trial => "trial",
});

sqlite_enum!(ProbeStatus, "probe_status", {
    ProbeStatus::Healthy => "healthy",
    ProbeStatus::Slow => "slow",
    ProbeStatus::Probing => "probing",
    ProbeStatus::RateLimited => "rate_limited",
    ProbeStatus::AuthError => "auth_error",
    ProbeStatus::ModelUnavailable => "model_unavailable",
    ProbeStatus::Timeout => "timeout",
    ProbeStatus::ProtocolError => "protocol_error",
    ProbeStatus::ValidationFailed => "validation_failed",
    ProbeStatus::UpstreamError => "upstream_error",
    ProbeStatus::NetworkError => "network_error",
    ProbeStatus::Cancelled => "cancelled",
    ProbeStatus::Unknown => "unknown",
});

sqlite_enum!(ResponseMode, "probe_capture.response_mode", {
    ResponseMode::Stream => "stream",
    ResponseMode::Synchronous => "synchronous",
});

sqlite_enum!(ThemeMode, "general_settings.theme", {
    ThemeMode::System => "system",
    ThemeMode::Light => "light",
    ThemeMode::Dark => "dark",
});

sqlite_enum!(TrayAction, "general_settings.tray_action", {
    TrayAction::Panel => "panel",
    TrayAction::Disabled => "disabled",
});

sqlite_enum!(CredentialAction, "credential_operation.action", {
    CredentialAction::Create => "create",
    CredentialAction::Replace => "replace",
    CredentialAction::Clear => "clear",
    CredentialAction::Delete => "delete",
});

sqlite_enum!(CredentialOperationStatus, "credential_operation.status", {
    CredentialOperationStatus::IntentRecorded => "intent_recorded",
    CredentialOperationStatus::CredentialWritten => "credential_written",
    CredentialOperationStatus::ConfigCommitted => "config_committed",
    CredentialOperationStatus::CleanupPending => "cleanup_pending",
    CredentialOperationStatus::RecoveryFailed => "recovery_failed",
});
