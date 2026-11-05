use super::*;
use crate::{
    modules::operation_logs::{OperationLogQuery, OperationSource},
    shared::persistence::test_support::environment,
    startup::{initialize, StartupState},
};
use serde_json::{json, Value};
use std::{fs, sync::Arc};

fn setup(root: &Path) -> (Arc<Diagnostics>, Arc<OperationLogs>) {
    let (state, diagnostics) = initialize(Ok(environment(root, "com.relaypulse.app.dev")));
    let StartupState::Ready(application) = state else {
        panic!("not ready");
    };
    (diagnostics.0.unwrap(), application.operation_logs)
}
fn query() -> OperationLogQuery {
    serde_json::from_value(json!({"startDate":null,"endDate":null,"source":["user"],"result":[],"actions":[],"page":1,"pageSize":100})).unwrap()
}
async fn details(logs: &Arc<OperationLogs>) -> Vec<Value> {
    let mut details = Vec::new();
    for item in logs.query(query()).await.unwrap().items {
        details.push(serde_json::to_value(logs.detail(item.id).await.unwrap()).unwrap());
    }
    details
}

#[tokio::test]
async fn cleanup_uses_atomic_shared_facts_and_export_uses_actual_written_bytes() {
    let temp = tempfile::tempdir().unwrap();
    let (diagnostics, logs) = setup(temp.path());
    fs::write(
        diagnostics.log_directory().unwrap().join("relaypulse.log"),
        b"legacy log",
    )
    .unwrap();
    let before = statistics(&diagnostics, Some(&logs)).unwrap();
    assert!(before.total_bytes > 10);
    assert_eq!(before.file_count, 2);
    assert_eq!(
        logs.query(query()).await.unwrap().total,
        0,
        "successful reads do not write history"
    );
    assert_eq!(
        clear(&diagnostics, Some(&logs)).unwrap(),
        LogStatistics::default()
    );
    let values = details(&logs).await;
    assert_eq!(values.len(), 1);
    assert_eq!(values[0]["content"]["count"], before.file_count);
    assert_eq!(
        values[0]["content"]["space"],
        format!("{} B", before.total_bytes)
    );
    assert_eq!(
        values[0]["item"]["source"],
        serde_json::to_value(OperationSource::User).unwrap()
    );
    let destination = temp.path().join("renamed.json");
    assert!(export(&diagnostics, Some(&logs), || Ok(Some(destination.clone()))).unwrap());
    let size = fs::metadata(&destination).unwrap().len();
    let values = details(&logs).await;
    let exported = values
        .iter()
        .find(|detail| detail["item"]["action"] == "diagnostics_exported")
        .unwrap();
    assert_eq!(exported["content"]["filename"], "renamed.json");
    assert_eq!(exported["content"]["fileSize"], format!("{size} B"));
    assert_eq!(exported["item"]["result"], "success");
    assert!(!serde_json::to_string(&values)
        .unwrap()
        .contains(temp.path().to_str().unwrap()));
}

#[tokio::test]
async fn host_cancel_and_failure_are_classified_once_while_successful_directory_read_is_silent() {
    let temp = tempfile::tempdir().unwrap();
    let (diagnostics, logs) = setup(temp.path());
    let before = diagnostics.statistics().unwrap();
    assert!(matches!(
        export(&diagnostics, Some(&logs), || Ok(None)),
        Err(DiagnosticsOperationError::Cancelled)
    ));
    assert_eq!(diagnostics.statistics().unwrap(), before);
    assert!(matches!(
        export(&diagnostics, Some(&logs), || Err(())),
        Err(DiagnosticsOperationError::SaveDialog)
    ));
    let mut opened = false;
    open_directory(&diagnostics, Some(&logs), |path| {
        opened = path.is_dir();
        Ok(())
    })
    .unwrap();
    assert!(opened);
    assert!(matches!(
        open_directory(&diagnostics, Some(&logs), |_| Err(())),
        Err(DiagnosticsOperationError::OpenDirectory)
    ));
    let values = details(&logs).await;
    assert_eq!(values.len(), 3);
    assert_eq!(
        values
            .iter()
            .filter(|detail| detail["item"]["result"] == "cancelled")
            .count(),
        1
    );
    assert_eq!(
        values
            .iter()
            .filter(|detail| detail["item"]["result"] == "failure")
            .count(),
        2
    );
    assert!(values
        .iter()
        .any(|detail| detail["item"]["action"] == "detail_read_failed"));
}

#[tokio::test]
async fn shared_filesystem_failure_is_audited_without_raw_error_or_path() {
    let temp = tempfile::tempdir().unwrap();
    let (diagnostics, logs) = setup(temp.path());
    let directory = diagnostics.log_directory().unwrap();
    diagnostics.clear().unwrap();
    fs::remove_dir(&directory).unwrap();
    fs::write(&directory, b"directory conflict").unwrap();
    assert!(matches!(
        statistics(&diagnostics, Some(&logs)),
        Err(DiagnosticsOperationError::Diagnostics(_))
    ));
    assert!(matches!(
        clear(&diagnostics, Some(&logs)),
        Err(DiagnosticsOperationError::Diagnostics(_))
    ));
    let values = details(&logs).await;
    assert_eq!(values.len(), 2);
    assert!(values
        .iter()
        .all(|detail| detail["item"]["result"] == "failure"));
    let serialized = serde_json::to_string(&values).unwrap();
    assert!(!serialized.contains(temp.path().to_str().unwrap()));
    assert!(!serialized.contains("directory conflict"));
}

#[test]
fn diagnostics_operations_remain_available_without_a_business_database() {
    let temp = tempfile::tempdir().unwrap();
    let diagnostics = Diagnostics::new(&environment(temp.path(), "com.relaypulse.app.dev"));
    diagnostics
        .record("startup", crate::shared::diagnostics::Outcome::Started)
        .unwrap();
    assert!(statistics(&diagnostics, None).unwrap().total_bytes > 0);
    let destination = temp.path().join("without-database.json");
    assert!(export(&diagnostics, None, || Ok(Some(destination.clone()))).unwrap());
    assert!(destination.is_file());
    assert_eq!(clear(&diagnostics, None).unwrap(), LogStatistics::default());
}
