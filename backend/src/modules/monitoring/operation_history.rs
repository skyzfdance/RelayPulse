//! Probe presentation and legacy decoding belong to monitoring, not the history SDK.
use relaypulse_operations::{DetailBehavior, HistoryDecoder, OperationLogContent, SnapshotField};
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;

pub const FAILURE_FORMAT: &str = "monitoring.probe_failure";
pub fn failure(reason: impl Into<String>) -> OperationLogContent {
    OperationLogContent::Presentation {
        format: FAILURE_FORMAT.into(),
        fields: vec![SnapshotField {
            label: "reason".into(),
            value: reason.into(),
            sensitive: None,
        }],
    }
}
pub fn records(record_ids: Vec<String>, availability: &str) -> DetailBehavior {
    DetailBehavior::Navigate {
        target: "probe_records".into(),
        parameters: BTreeMap::from([
            (
                "recordIds".into(),
                serde_json::to_string(&record_ids).expect("string list"),
            ),
            ("availability".into(), availability.into()),
        ]),
    }
}

pub struct ProbeHistoryDecoder;
impl HistoryDecoder for ProbeHistoryDecoder {
    fn behavior(&self, value: Value) -> Result<DetailBehavior, serde_json::Error> {
        if value["kind"] == "probe_records" {
            #[derive(Deserialize)]
            #[serde(rename_all = "snake_case")]
            enum Availability {
                Available,
                Expired,
                None,
            }
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct Legacy {
                record_ids: Vec<String>,
                availability: Availability,
            }
            let old: Legacy = serde_json::from_value(value)?;
            Ok(records(
                old.record_ids,
                match old.availability {
                    Availability::Available => "available",
                    Availability::Expired => "expired",
                    Availability::None => "none",
                },
            ))
        } else {
            serde_json::from_value(value)
        }
    }
    fn content(&self, value: Value) -> Result<OperationLogContent, serde_json::Error> {
        if value["kind"] == "probe_failure" {
            #[derive(Deserialize)]
            struct Legacy {
                reason: String,
            }
            Ok(failure(serde_json::from_value::<Legacy>(value)?.reason))
        } else {
            serde_json::from_value(value)
        }
    }
}
