use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationHistoryQueryDto {
    pub cursor: Option<String>,
    pub limit: u64,
    pub started_after: Option<String>,
    pub started_before: Option<String>,
    pub category: Option<String>,
    pub operation_query: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationHistoryPageDto {
    pub items: Vec<OperationRecordSummaryDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    pub dataset_version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationRecordSummaryDto {
    pub record_ref: String,
    pub occurred_at: OperationOccurredAtDto,
    pub category: OperationCategory,
    pub action_label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationRecordDetailsDto {
    pub record_ref: String,
    pub occurred_at: OperationOccurredAtDto,
    pub category: OperationCategory,
    pub trigger: OperationTrigger,
    pub action_label: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
    pub detail: OperationRecordSpecificDetailsDto,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exception: Option<OperationExceptionDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationOccurredAtDto {
    pub value: String,
    pub precision: OccurredAtPrecision,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub qualifier: Option<OccurredAtQualifier>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OccurredAtPrecision {
    Exact,
    Approximate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OccurredAtQualifier {
    About,
    After,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationCategory {
    AppLifecycle,
    ConfigurationChange,
    DataCollection,
    DiagnosticsMaintenance,
}

impl OperationCategory {
    pub(super) fn parse(value: &str) -> Option<Self> {
        match value {
            "app_lifecycle" => Some(Self::AppLifecycle),
            "configuration_change" => Some(Self::ConfigurationChange),
            "data_collection" => Some(Self::DataCollection),
            "diagnostics_maintenance" => Some(Self::DiagnosticsMaintenance),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationTrigger {
    UserAction,
    Automatic,
    SystemStartup,
    SystemRecovery,
    SystemShutdown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DataCollectionSource {
    IntelligenceLadder,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum OperationRecordSpecificDetailsDto {
    ConfigurationChange {
        changes: Vec<ConfigurationChangeDto>,
    },
    AppLifecycle {
        event: LifecycleEvent,
        #[serde(skip_serializing_if = "Option::is_none")]
        launch_method: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        app_version: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        restored_site_count: Option<u64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        restored_model_count: Option<u64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        background_tasks_started: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        settings_saved: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        window_state_saved: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        background_tasks_stopped: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        previous_session_started_at: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        last_completed_action: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        failure_source: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        crash_report_found: Option<bool>,
    },
    DataCollection {
        source: DataCollectionSource,
        #[serde(skip_serializing_if = "Option::is_none")]
        entry_count: Option<u64>,
    },
    DiagnosticsMaintenance {
        #[serde(flatten)]
        details: DiagnosticsDetailsDto,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleEvent {
    Started,
    Exited,
    AbnormalExit,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "action",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum DiagnosticsDetailsDto {
    Export {
        included_categories: Vec<String>,
        file_count: u64,
        bundle_size_label: String,
        location_label: String,
        request_bodies_excluded: bool,
    },
    Cleanup {
        before_size_label: String,
        file_count: u64,
        after_size_label: String,
        settings_preserved: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigurationChangeDto {
    pub field_label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationExceptionDto {
    pub reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub safe_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_step: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recommended_action: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationHistoryQueryError {
    InvalidQuery,
    CursorExpired,
    QueryBusy,
    NotFound,
    BackendUnavailable,
}

#[derive(Debug, Clone)]
pub(super) struct ProjectedRecord {
    pub occurred_at_unix_nanos: i128,
    pub details: OperationRecordDetailsDto,
}

impl ProjectedRecord {
    pub(super) fn summary(&self) -> OperationRecordSummaryDto {
        OperationRecordSummaryDto {
            record_ref: self.details.record_ref.clone(),
            occurred_at: self.details.occurred_at.clone(),
            category: self.details.category,
            action_label: self.details.action_label.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{json, to_value};

    use super::*;

    #[test]
    fn diagnostics_details_serialize_as_the_flat_typescript_union() {
        let details = OperationRecordSpecificDetailsDto::DiagnosticsMaintenance {
            details: DiagnosticsDetailsDto::Export {
                included_categories: vec!["结构化诊断日志".to_owned()],
                file_count: 2,
                bundle_size_label: "1.0 KB".to_owned(),
                location_label: "用户选择的位置".to_owned(),
                request_bodies_excluded: true,
            },
        };

        assert_eq!(
            to_value(details).expect("details serialize"),
            json!({
                "kind": "diagnostics_maintenance",
                "action": "export",
                "includedCategories": ["结构化诊断日志"],
                "fileCount": 2,
                "bundleSizeLabel": "1.0 KB",
                "locationLabel": "用户选择的位置",
                "requestBodiesExcluded": true,
            })
        );
    }

    #[test]
    fn optional_fields_are_omitted_instead_of_serialized_as_null() {
        let occurred_at = OperationOccurredAtDto {
            value: "2026-08-08T08:00:00Z".to_owned(),
            precision: OccurredAtPrecision::Exact,
            qualifier: None,
        };

        assert_eq!(
            to_value(occurred_at).expect("timestamp serializes"),
            json!({ "value": "2026-08-08T08:00:00Z", "precision": "exact" })
        );
    }
}
