use super::*;
use crate::shared::persistence::{test_support::environment, Database};
use serde_json::json;
use std::sync::Arc;

fn setup(root: &std::path::Path) -> (Arc<Database>, Arc<OperationLogs>) {
    let env = environment(root, "com.relaypulse.app.dev");
    let database = Arc::new(Database::open(env.paths(), &[SCHEMA]).unwrap());
    (database.clone(), Arc::new(OperationLogs::new(database)))
}
fn query() -> OperationLogQuery {
    OperationLogQuery {
        start_date: None,
        end_date: None,
        source: vec![],
        result: vec![],
        actions: vec![],
        page: 1,
        page_size: 10,
    }
}
fn event(
    id: &str,
    at: i64,
    source: OperationSource,
    result: OperationResult,
    action: OperationAction,
) -> OperationEvent {
    let mut event = OperationEvent::new(
        action,
        source,
        result,
        "原站点",
        "配置已保存。",
        OperationLogContent::Snapshot {
            fields: snapshot_fields(
                &json!({"name":"原站点","secret":"private-secret","baseUrl":"https://user:pass@example.com/v1?key=private-key"}),
            ),
        },
    );
    event.id = Some(id.into());
    event.occurred_at_ms = Some(at);
    event
}
#[tokio::test]
async fn named_migration_history_reopen_snapshots_and_lazy_detail() {
    let temp = tempfile::tempdir().unwrap();
    let (database, logs) = setup(temp.path());
    logs.record(event(
        "one",
        1_700_000_000_000,
        OperationSource::User,
        OperationResult::Success,
        OperationAction::SiteCreated,
    ))
    .unwrap();
    assert_eq!(
        database
            .with_connection(|c| c.query_row(
                "SELECT name FROM schema_migrations WHERE position=0",
                [],
                |r| r.get::<_, String>(0)
            ))
            .unwrap(),
        "operation_history"
    );
    // Stable replay does not create a duplicate or replace the original snapshot.
    let mut replay = event(
        "one",
        1_700_000_000_000,
        OperationSource::User,
        OperationResult::Success,
        OperationAction::SiteCreated,
    );
    replay.object = "已修改名称".into();
    logs.record(replay).unwrap();
    let page = logs.query(query()).await.unwrap();
    assert_eq!(page.total, 1);
    let value = serde_json::to_value(&page.items[0]).unwrap();
    assert!(value.get("content").is_none());
    drop(logs);
    drop(database);
    let (_, logs) = setup(temp.path());
    let detail = logs.detail("one".into()).await.unwrap();
    assert_eq!(detail.item.object_snapshot, "原站点");
    let text = serde_json::to_string(&detail).unwrap();
    for secret in ["private-secret", "private-key", "user:pass"] {
        assert!(!text.contains(secret), "{text}");
    }
    assert!(text.contains("已配置"));
    assert!(matches!(
        logs.detail("missing".into()).await,
        Err(OperationLogError::NotFound)
    ));
}
#[tokio::test]
async fn filters_count_before_pagination_stable_order_and_page_clamping() {
    let temp = tempfile::tempdir().unwrap();
    let (_, logs) = setup(temp.path());
    for i in 0..25 {
        logs.record(event(
            &format!("event-{i:03}"),
            1_700_000_000_000,
            if i % 2 == 0 {
                OperationSource::User
            } else {
                OperationSource::System
            },
            if i % 3 == 0 {
                OperationResult::Failure
            } else {
                OperationResult::Success
            },
            if i % 2 == 0 {
                OperationAction::SiteStatusChanged
            } else {
                OperationAction::AutomaticProbe
            },
        ))
        .unwrap();
    }
    let page = logs.query(query()).await.unwrap();
    assert_eq!(page.total, 25);
    assert_eq!(page.items.len(), 10);
    assert_eq!(page.items[0].id, "event-000");
    assert_eq!(page.items[9].id, "event-009");
    let mut q = query();
    q.page = 100;
    let last = logs.query(q).await.unwrap();
    assert_eq!(last.page, 3);
    assert_eq!(last.items.len(), 5);
    let mut q = query();
    q.source = vec![OperationSource::User, OperationSource::System];
    q.actions = vec![OperationActionGroup::Update];
    q.result = vec![OperationResult::Failure, OperationResult::Cancelled];
    let filtered = logs.query(q).await.unwrap();
    assert_eq!(filtered.total, 5);
    assert_eq!(filtered.items[0].action_group, OperationActionGroup::Update);
    let mut q = query();
    q.result = vec![OperationResult::Blocked];
    q.page = 99;
    let empty = logs.query(q).await.unwrap();
    assert_eq!(empty.total, 0);
    assert_eq!(empty.page, 1);
}
#[tokio::test]
async fn local_calendar_dates_include_the_entire_boundary_days() {
    let temp = tempfile::tempdir().unwrap();
    let (database, logs) = setup(temp.path());
    // Ask SQLite for the device-local boundaries rather than assume this test host's TZ.
    let midnight: i64 = database
        .with_connection(|c| {
            c.query_row(
                "SELECT CAST(strftime('%s','2026-04-08 00:00:00','utc') AS INTEGER)*1000",
                [],
                |r| r.get(0),
            )
        })
        .unwrap();
    for (id, at) in [
        ("before", midnight - 1),
        ("first", midnight),
        ("last", midnight + 86_400_000 - 1),
        ("after", midnight + 86_400_000),
    ] {
        logs.record(event(
            id,
            at,
            OperationSource::User,
            OperationResult::Success,
            OperationAction::SiteUpdated,
        ))
        .unwrap();
    }
    let mut q = query();
    q.start_date = Some("2026-04-08".into());
    q.end_date = q.start_date.clone();
    let page = logs.query(q).await.unwrap();
    assert_eq!(
        page.items.iter().map(|i| i.id.as_str()).collect::<Vec<_>>(),
        ["last", "first"]
    );
}
#[test]
fn validation_rejects_invalid_dates_pagination_and_excess_filters() {
    for (start, end) in [
        ("2026-02-29", "2026-03-01"),
        ("0000-01-01", "2026-03-01"),
        ("2026-4-08", "2026-04-09"),
        ("2026-04-10", "2026-04-09"),
        ("éééé-1", "2026-04-09"),
    ] {
        let mut q = query();
        q.start_date = Some(start.into());
        q.end_date = Some(end.into());
        assert!(q.validate().is_err());
    }
    for (page, size) in [(0, 10), (9_007_199_254_740_992, 10), (1, 70), (1, 11)] {
        let mut q = query();
        q.page = page;
        q.page_size = size;
        assert!(q.validate().is_err());
    }
    assert!(query::log_id("  ").is_err());
    assert!(query::log_id(&"x".repeat(201)).is_err());
}
#[test]
fn credential_redaction_preserves_token_metrics_and_handles_multiple_urls() {
    let fields = snapshot_fields(
        &json!({"firstTokenMs":3,"totalTokens":12,"credentialConfigured":false,"secret":"hidden","headers":[{"name":"x-note","value":"hidden"}],"url":"https://name:pass@example.com/path?token=secret"}),
    );
    assert_eq!(
        fields
            .iter()
            .find(|f| f.label == "firstTokenMs")
            .unwrap()
            .value,
        "3"
    );
    assert_eq!(
        fields
            .iter()
            .find(|f| f.label == "totalTokens")
            .unwrap()
            .value,
        "12"
    );
    assert_eq!(
        fields
            .iter()
            .find(|f| f.label == "credentialConfigured")
            .unwrap()
            .value,
        "未配置"
    );
    let text = privacy::text(
        r#"{"a":"https://a:b@one.test/?secret=one","b":"http://c:d@two.test/?secret=two"}"#,
    );
    for credential in ["a:b@", "c:d@", "secret=one", "secret=two"] {
        assert!(!text.contains(credential));
    }
    assert!(text.contains("one.test"));
    assert!(text.contains("two.test"));
}
#[test]
fn failures_are_isolated_no_recursive_writes_and_database_is_released() {
    let temp = tempfile::tempdir().unwrap();
    let (database, logs) = setup(temp.path());
    database
        .with_connection(|c| c.execute_batch("DROP TABLE operation_logs"))
        .unwrap();
    logs.record_best_effort(event(
        "failed",
        1,
        OperationSource::User,
        OperationResult::Failure,
        OperationAction::SiteCreated,
    ));
    assert!(logs
        .record(event(
            "failed2",
            2,
            OperationSource::User,
            OperationResult::Failure,
            OperationAction::SiteCreated
        ))
        .is_err());
    let weak = Arc::downgrade(&database);
    drop(database);
    drop(logs);
    assert!(weak.upgrade().is_none());
}
#[tokio::test]
async fn front_end_failure_bridge_keeps_independent_attempts_and_only_records_user_failures() {
    let temp = tempfile::tempdir().unwrap();
    let (_, logs) = setup(temp.path());
    logs.report_read_failure(ReadFailureTarget::Route, ReadFailureKind::Page);
    logs.report_read_failure(ReadFailureTarget::Route, ReadFailureKind::Page);
    logs.report_read_failure(ReadFailureTarget::SiteDetail, ReadFailureKind::Detail);
    let page = logs.query(query()).await.unwrap();
    assert_eq!(page.total, 3);
    assert!(page.items.iter().all(
        |item| item.result == OperationResult::Failure && item.source == OperationSource::User
    ));
}
#[test]
fn parameter_values_are_redacted_before_they_are_flattened_and_approved_states_survive() {
    let fields = snapshot_fields(
        &json!({"parameters":[{"key":"api_key","value":"parameter-secret"},{"name":"Authorization","value":"Bearer private"},{"key":"provider_payload","value":"{\"password\":\"nested-secret\"}"}],"requestParameters":[{"key":"api-key","value":"custom-secret"},{"key":"max_tokens","value":"42"}],"凭据":"已更换"}),
    );
    let text = serde_json::to_string(&fields).unwrap();
    for secret in [
        "parameter-secret",
        "Bearer private",
        "nested-secret",
        "custom-secret",
    ] {
        assert!(!text.contains(secret), "{text}");
    }
    assert_eq!(
        fields.iter().find(|f| f.label == "凭据").unwrap().value,
        "已更换"
    );
    assert_eq!(
        fields
            .iter()
            .find(|f| f.label == "requestParameters[1].value")
            .unwrap()
            .value,
        "42"
    );
    assert_eq!(privacy::text("https://example.com"), "https://example.com");
}
#[test]
fn content_discriminants_and_fields_match_the_frontend_contract() {
    let variants = [
        (
            OperationLogContent::Snapshot { fields: vec![] },
            "snapshot",
            vec!["kind", "fields"],
        ),
        (
            OperationLogContent::Changes { changes: vec![] },
            "changes",
            vec!["kind", "changes"],
        ),
        (
            OperationLogContent::Deletion {
                summary: "deleted".into(),
                fields: vec![SnapshotField {
                    label: "凭据".into(),
                    value: "已配置".into(),
                    sensitive: Some(true),
                }],
            },
            "deletion",
            vec!["kind", "summary", "fields"],
        ),
        (
            OperationLogContent::Notification {
                trigger: "trigger".into(),
                title: "title".into(),
                body: "body".into(),
                send_result: "sent".into(),
            },
            "notification",
            vec!["kind", "trigger", "title", "body", "sendResult"],
        ),
        (
            OperationLogContent::Cleanup {
                target: "target".into(),
                scope: "scope".into(),
                count: 2,
                space: None,
            },
            "cleanup",
            vec!["kind", "target", "scope", "count"],
        ),
        (
            OperationLogContent::Export {
                filename: "relaypulse-diagnostics.json".into(),
                file_size: "10 B".into(),
                outcome: "已取消".into(),
            },
            "export",
            vec!["kind", "filename", "fileSize", "outcome"],
        ),
        (
            OperationLogContent::Failure {
                activity: "activity".into(),
                target: "target".into(),
                reason: "reason".into(),
            },
            "failure",
            vec!["kind", "activity", "target", "reason"],
        ),
        (
            OperationLogContent::ProbeFailure {
                reason: "no records".into(),
            },
            "probe_failure",
            vec!["kind", "reason"],
        ),
    ];
    for (content, kind, mut keys) in variants {
        let value = serde_json::to_value(content).unwrap();
        let mut actual = value
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>();
        keys.sort();
        actual.sort();
        assert_eq!(actual, keys);
        assert_eq!(value["kind"], kind);
        if kind == "deletion" {
            assert!(value["fields"][0].get("sensitive").is_none());
        }
    }
}
#[test]
fn appending_operation_history_preserves_an_existing_user_database() {
    use crate::modules::{configuration, monitoring, notifications, sites};
    let temp = tempfile::tempdir().unwrap();
    let env = environment(temp.path(), "com.relaypulse.app.dev");
    let existing = [
        configuration::SCHEMA,
        notifications::SCHEMA,
        sites::SCHEMA,
        monitoring::SCHEMA,
    ];
    let database = Database::open(env.paths(), &existing).unwrap();
    database.with_connection(|c|{c.execute("UPDATE general_settings SET history_retention_days=90,theme='dark' WHERE singleton=1",[])?;Ok(())}).unwrap();
    drop(database);
    let database = Database::open(
        env.paths(),
        &[existing[0], existing[1], existing[2], existing[3], SCHEMA],
    )
    .unwrap();
    assert_eq!(
        database
            .with_connection(|c| c.query_row(
                "SELECT history_retention_days,theme FROM general_settings WHERE singleton=1",
                [],
                |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
            ))
            .unwrap(),
        (90, "dark".into())
    );
    assert_eq!(
        database
            .with_connection(|c| c.query_row(
                "SELECT name FROM schema_migrations WHERE position=4",
                [],
                |r| r.get::<_, String>(0)
            ))
            .unwrap(),
        "operation_history"
    );
}
#[cfg(unix)]
#[tokio::test]
async fn historical_daylight_saving_uses_each_timestamp_offset() {
    const CHILD: &str = "RELAYPULSE_OPERATION_TZ_CHILD";
    if std::env::var_os(CHILD).is_none() {
        // Isolate TZ in a fresh process so parallel tests never observe a changed environment.
        let result=std::process::Command::new(std::env::current_exe().unwrap()).args(["--exact","modules::operation_logs::tests::historical_daylight_saving_uses_each_timestamp_offset","--nocapture"]).env("TZ","America/New_York").env(CHILD,"1").status().unwrap();
        assert!(result.success());
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let (_, logs) = setup(temp.path());
    for (date, start, end) in [
        ("2026-03-08", "2026-03-08T05:00:00Z", "2026-03-09T04:00:00Z"),
        ("2026-11-01", "2026-11-01T04:00:00Z", "2026-11-02T05:00:00Z"),
    ] {
        let millis = |s| {
            time::OffsetDateTime::parse(s, &time::format_description::well_known::Rfc3339)
                .unwrap()
                .unix_timestamp_nanos()
                .div_euclid(1_000_000) as i64
        };
        let start = millis(start);
        let end = millis(end);
        for (suffix, at) in [
            ("before", start - 1),
            ("first", start),
            ("last", end - 1),
            ("after", end),
        ] {
            logs.record(event(
                &format!("{date}-{suffix}"),
                at,
                OperationSource::System,
                OperationResult::Success,
                OperationAction::AutomaticProbe,
            ))
            .unwrap();
        }
        let mut q = query();
        q.start_date = Some(date.into());
        q.end_date = q.start_date.clone();
        let page = logs.query(q).await.unwrap();
        assert_eq!(page.total, 2, "{date}");
        assert_eq!(page.items[0].id, format!("{date}-last"));
        assert_eq!(page.items[1].id, format!("{date}-first"));
    }
}
#[test]
fn protected_diffs_skip_unchanged_values_and_describe_replacement_without_secrets() {
    let before = json!({"headers":[{"key":"Authorization","value":"old-secret"}],"credentialConfigured":true,"name":"before"});
    let unchanged = changes(&before, &before);
    assert!(unchanged.is_empty());
    let after = json!({"headers":[{"key":"Authorization","value":"new-secret"}],"credentialConfigured":true,"name":"after"});
    let changed = changes(&before, &after);
    assert_eq!(changed.len(), 2);
    let headers = changed
        .iter()
        .find(|field| field.field == "headers")
        .unwrap();
    assert_eq!(headers.before, "已配置");
    assert_eq!(headers.after, "已更换");
    let text = serde_json::to_string(&changed).unwrap();
    assert!(!text.contains("old-secret"));
    assert!(!text.contains("new-secret"));
}
