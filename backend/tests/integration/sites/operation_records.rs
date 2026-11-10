use super::*;
use crate::modules::{monitoring::RecordError, operation_logs::OperationLogQuery};
use crate::test_actions::actions as OperationAction;

fn operation_query(action: relaypulse_operations::ActionDefinition) -> OperationLogQuery {
    OperationLogQuery {
        start_date: None,
        end_date: None,
        source: vec![],
        result: vec![],
        actions: vec![action.group().into()],
        page: 1,
        page_size: 100,
    }
}
async fn records(f: &Fixture) -> Vec<String> {
    f.database
        .with_connection(|c| {
            c.prepare("SELECT id FROM probe_records ORDER BY rowid")?
                .query_map([], |row| row.get(0))?
                .collect()
        })
        .unwrap()
}
#[tokio::test]
async fn exact_associations_survive_partial_task_failure_and_report_cleanup_and_deletion() {
    let f = Fixture::new();
    let server = Server::new(vec![Response::raw(200, "text/plain", "OK"); 2]);
    let site = f.site(&server.url, &["first", "second"]);
    f.database
        .with_connection(|c| {
            c.execute_batch(
                "CREATE TRIGGER reject_second BEFORE INSERT ON probe_records
        WHEN NEW.model_name='second' BEGIN SELECT RAISE(ABORT,'fixture'); END;",
            )
        })
        .unwrap();
    assert!(f.monitoring.run(site.clone(), None).await.is_err());
    let ids = records(&f).await;
    assert_eq!(ids.len(), 1);
    let lookup = serde_json::to_value(
        f.monitoring
            .get_probe_records_by_ids(vec![ids[0].clone(), "missing".into(), ids[0].clone()])
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(lookup["items"].as_array().unwrap().len(), 1);
    assert_eq!(lookup["items"][0]["id"], ids[0]);
    assert_eq!(lookup["missingIds"], json!(["missing"]));
    f.monitoring.get_probe_record_options().await.unwrap();
    f.monitoring
        .get_probe_record_detail(ids[0].clone())
        .await
        .unwrap();
    let reads = f
        .operation_logs
        .query(OperationLogQuery {
            actions: vec![String::from("read")],
            ..operation_query(OperationAction::ManualProbe)
        })
        .await
        .unwrap();
    assert!(reads.items.is_empty());
    let logs = operation_logs(&f, OperationAction::ManualProbe).await;
    let event = &logs[0];
    assert_eq!(event["source"], "user");
    assert_eq!(event["result"], "failure");
    assert_eq!(event["objectSnapshot"], "local");
    assert_eq!(event["detailBehavior"]["recordIds"], json!(ids));
    let serialized = serde_json::to_string(&logs).unwrap();
    assert!(!serialized.contains("secret-only-in-vault"));
    assert!(!serialized.contains("fixture"));
    f.monitoring
        .delete_probe_record(ids[0].clone())
        .await
        .unwrap();
    let missing = f
        .monitoring
        .get_probe_records_by_ids(ids.clone())
        .await
        .unwrap();
    assert!(missing.items.is_empty());
    assert_eq!(missing.missing_ids, ids);
    assert_eq!(
        operation_logs(&f, OperationAction::ManualProbe).await[0],
        *event
    );
    // 再次记录后清理全部数据；历史的关联依旧以原始 ID 保留。
    f.database
        .with_connection(|c| c.execute_batch("DROP TRIGGER reject_second;"))
        .unwrap();
    let server = Server::new(vec![Response::raw(200, "text/plain", "OK"); 2]);
    let mut connection = f.store.get(&site).unwrap().connection;
    connection.interval_seconds = 300;
    f.store.save_connection(&site, &connection).unwrap();
    f.store
        .save_basic(
            &site,
            &SiteDraft {
                name: "renamed".into(),
                protocol: Protocol::CustomHttp,
                base_url: server.url.clone(),
                credential_action: CredentialAction::Keep,
                secret: String::new(),
            },
        )
        .unwrap();
    f.monitoring.run(site.clone(), None).await.unwrap();
    let ids = records(&f).await;
    assert_eq!(ids.len(), 2);
    f.monitoring.clear_probe_records().await.unwrap();
    let lookup = f
        .monitoring
        .get_probe_records_by_ids(ids.clone())
        .await
        .unwrap();
    assert!(lookup.items.is_empty());
    assert_eq!(lookup.missing_ids, ids);
    assert_eq!(
        operation_logs(&f, OperationAction::ManualProbe).await.len(),
        2
    );
}

#[tokio::test]
async fn failures_without_records_use_drawer_and_failed_models_still_complete_the_task() {
    let f = Fixture::new();
    let server = Server::new(vec![Response::raw(503, "text/plain", "failed")]);
    let site = f.site(&server.url, &["model"]);
    f.monitoring.run(site.clone(), None).await.unwrap();
    let log = operation_logs(&f, OperationAction::ManualProbe)
        .await
        .remove(0);
    assert_eq!(log["result"], "success");
    let ids = records(&f).await;
    assert_eq!(log["detailBehavior"]["recordIds"], json!(ids));
    let lookup = f.monitoring.get_probe_records_by_ids(ids).await.unwrap();
    assert_eq!(
        serde_json::to_value(lookup).unwrap()["items"][0]["result"],
        "failure"
    );
    assert!(f.monitoring.run("missing-site".into(), None).await.is_err());
    let logs = operation_logs(&f, OperationAction::ManualProbe).await;
    let failed = logs
        .iter()
        .find(|event| event["result"] == "failure")
        .unwrap();
    assert_eq!(failed["detailBehavior"], json!({"kind":"drawer"}));
    let detail = serde_json::to_value(
        f.operation_logs
            .detail(failed["id"].as_str().unwrap().into())
            .await
            .unwrap(),
    )
    .unwrap();
    let detail = crate::application::operation_history::wire(detail);
    assert_eq!(detail["content"]["kind"], "probe_failure");
}

#[tokio::test]
async fn association_limits_validate_before_storage_and_expiry_is_explicit() {
    let f = Fixture::new();
    for ids in [
        vec![],
        vec![" ".into()],
        vec!["x".repeat(201)],
        vec!["x".into(); 101],
        vec!["a\nb".into()],
    ] {
        assert!(matches!(
            f.monitoring.get_probe_records_by_ids(ids).await,
            Err(RecordError::Validation {
                field: "recordIds",
                ..
            })
        ));
    }
    let server = Server::new(vec![Response::raw(200, "text/plain", "OK"); 2]);
    let site = f.site(&server.url, &["model"]);
    f.monitoring.run(site.clone(), None).await.unwrap();
    let ids = records(&f).await;
    f.database
        .with_connection(|c| {
            c.execute(
                "UPDATE probe_records SET finished_at_ms=0,started_at_ms=0",
                [],
            )
        })
        .unwrap();
    let lookup = f
        .monitoring
        .get_probe_records_by_ids(ids.clone())
        .await
        .unwrap();
    assert!(lookup.items.is_empty());
    assert_eq!(lookup.missing_ids, ids);
    f.monitoring.cleanup_records().await;
    let cleanup = operation_logs(&f, OperationAction::ScheduledRecordCleanup).await;
    assert_eq!(cleanup.len(), 1);
    assert_eq!(cleanup[0]["source"], "user");
    f.monitoring.cleanup_records().await;
    assert_eq!(
        operation_logs(&f, OperationAction::ScheduledRecordCleanup)
            .await
            .len(),
        1
    );
    f.monitoring.run(site.clone(), None).await.unwrap();
    let ids = records(&f).await;
    assert_eq!(ids.len(), 1);
    f.store.delete(&site).unwrap();
    let lookup = f
        .monitoring
        .get_probe_records_by_ids(ids.clone())
        .await
        .unwrap();
    assert!(lookup.items.is_empty());
    assert_eq!(lookup.missing_ids, ids);
    assert_eq!(
        operation_logs(&f, OperationAction::ManualProbe).await.len(),
        2
    );
}

#[tokio::test]
async fn cancelled_tasks_keep_already_committed_ids_and_release_resources() {
    let f = Fixture::new();
    let server = Server::new(vec![
        Response::raw(200, "text/plain", "OK"),
        Response::raw(200, "text/plain", "OK").delayed(Duration::from_secs(2)),
    ]);
    let site = f.site(&server.url, &["first", "second"]);
    let monitoring = f.monitoring.clone();
    let task = tokio::spawn(async move { monitoring.run(site, None).await });
    server.wait_requests(2).await;
    f.monitoring.close();
    assert!(task.await.unwrap().is_err());
    tokio::time::timeout(Duration::from_secs(2), f.monitoring.drained())
        .await
        .unwrap();
    let ids = records(&f).await;
    assert_eq!(ids.len(), 1);
    let log = operation_logs(&f, OperationAction::ManualProbe)
        .await
        .remove(0);
    assert_eq!(log["result"], "cancelled");
    assert_eq!(log["detailBehavior"]["recordIds"], json!(ids));
}

async fn operation_logs(
    f: &Fixture,
    action: relaypulse_operations::ActionDefinition,
) -> Vec<Value> {
    let logs = f
        .operation_logs
        .query(operation_query(action))
        .await
        .unwrap();
    // Actual monitoring writes persist only the generic SDK payload, before wire adaptation.
    for item in &logs.items {
        let (behavior, content): (String, String) = f
            .database
            .with_connection(|c| {
                c.query_row(
                    "SELECT detail_behavior,content FROM operation_logs WHERE id=?1",
                    [&item.id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
            })
            .unwrap();
        assert_ne!(
            serde_json::from_str::<Value>(&behavior).unwrap()["kind"],
            "probe_records"
        );
        assert_ne!(
            serde_json::from_str::<Value>(&content).unwrap()["kind"],
            "probe_failure"
        );
    }
    crate::application::operation_history::wire(serde_json::to_value(logs).unwrap())["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|event| event["action"] == serde_json::to_value(action).unwrap())
        .cloned()
        .collect()
}
