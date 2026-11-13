use super::*;
use crate::modules::operation_logs::OperationSource;
use crate::test_actions::actions as OperationAction;
async fn history(f: &Fixture) -> crate::modules::operation_logs::OperationLogPage {
    f.operation_logs.query(serde_json::from_value(json!({"startDate":null,"endDate":null,"source":[],"result":[],"actions":[],"page":1,"pageSize":100})).unwrap()).await.unwrap()
}
#[tokio::test]
async fn user_mutations_preserve_names_and_redacted_configuration_after_delete() {
    let f = Fixture::new();
    let id = f.site("https://example.com", &["before"]);
    let model = f.store.get(&id).unwrap().models[0].id.clone();
    f.execute(Operation::Connection(
        id.clone(),
        sites::Connection {
            models_path: "/models".into(),
            probe_path: "/probe".into(),
            headers: vec![crate::shared::model_client::RequestHeader {
                key: "X-Trace".into(),
                value: "Bearer super-private".into(),
            }],
            ..sites::Connection::for_protocol(Protocol::CustomHttp)
        },
    ))
    .await;
    f.execute(Operation::Save(
        id.clone(),
        SiteDraft {
            name: "renamed".into(),
            protocol: Protocol::CustomHttp,
            base_url: "https://example.com".into(),
            credential_action: CredentialAction::Keep,
            secret: String::new(),
        },
    ))
    .await;
    f.execute(Operation::DeleteModel(id.clone(), model)).await;
    f.execute(Operation::Delete(id)).await;
    let page = history(&f).await;
    assert_eq!(page.total, 4);
    assert!(page.items.iter().all(|i| i.source == OperationSource::User));
    let changed = page
        .items
        .iter()
        .find(|i| i.action == OperationAction::SiteUpdated && i.object_snapshot == "local")
        .unwrap();
    let detail = f.operation_logs.detail(changed.id.clone()).await.unwrap();
    let text = serde_json::to_string(&detail).unwrap();
    assert!(text.contains("local"));
    assert!(!text.contains("super-private"));
    assert!(!text.contains("secret-only-in-vault"));
    let removed = page
        .items
        .iter()
        .find(|i| i.action == OperationAction::ModelDeleted)
        .unwrap();
    assert_eq!(removed.object_snapshot, "renamed / before");
}
#[tokio::test]
async fn read_success_is_silent_and_log_database_failure_does_not_undo_mutation() {
    let f = Fixture::new();
    let id = f.site("https://example.com", &[]);
    f.execute(Operation::List).await;
    f.execute(Operation::Get(id.clone())).await;
    assert_eq!(history(&f).await.total, 0);
    f.database
        .with_connection(|c| c.execute_batch("DROP TABLE operation_logs"))
        .unwrap();
    f.execute(Operation::Enabled(id.clone(), true)).await;
    assert!(f.store.get(&id).unwrap().enabled);
}

#[tokio::test]
async fn changes_compare_committed_fields_and_credential_replacement_is_explicit() {
    let f = Fixture::new();
    let id = f.site("https://example.com", &["model"]);
    f.execute(Operation::Save(
        id,
        SiteDraft {
            name: "renamed".into(),
            protocol: Protocol::CustomHttp,
            base_url: "https://example.com".into(),
            credential_action: CredentialAction::Replace,
            secret: "replacement-only-in-vault".into(),
        },
    ))
    .await;
    let page = history(&f).await;
    let detail = f
        .operation_logs
        .detail(page.items[0].id.clone())
        .await
        .unwrap();
    let content = serde_json::to_value(detail.content).unwrap();
    let fields = content["changes"].as_array().unwrap();
    assert!(fields.contains(&json!({"field":"name","before":"local","after":"renamed"})));
    assert!(fields
        .contains(&json!({"field":"凭据","before":"已配置","after":"已更换","sensitive":true})));
    assert!(!fields
        .iter()
        .any(|field| field["field"].as_str().unwrap().starts_with("保存结果")));
    assert!(!serde_json::to_string(&content)
        .unwrap()
        .contains("replacement-only-in-vault"));
}

#[tokio::test]
async fn creation_contains_saved_defaults_and_deletion_contains_only_removed_model_snapshot() {
    let f = Fixture::new();
    let server = Server::new(vec![Response::json(json!(["first", "second"]))]);
    let id = f
        .execute(Operation::Create(CreateSite {
            name: "new site".into(),
            protocol: Protocol::CustomHttp,
            base_url: server.url.clone(),
            models_path: "/models".into(),
            probe_path: "/probe".into(),
            secret: "creation-only-in-vault".into(),
            enabled: false,
        }))
        .await
        .as_str()
        .unwrap()
        .to_owned();
    let created = history(&f).await.items.remove(0);
    let detail = f.operation_logs.detail(created.id).await.unwrap();
    let text = serde_json::to_string(&detail.content).unwrap();
    assert!(text.contains("保存结果.connection.intervalSeconds"));
    assert!(text.contains("保存结果.models[0].name"));
    assert!(!text.contains("creation-only-in-vault"));
    let model = f.store.get(&id).unwrap().models[0].id.clone();
    f.execute(Operation::DeleteModel(id, model)).await;
    let page = history(&f).await;
    let removed = page
        .items
        .iter()
        .find(|item| item.action == OperationAction::ModelDeleted)
        .unwrap();
    let detail = f.operation_logs.detail(removed.id.clone()).await.unwrap();
    let content = serde_json::to_value(detail.content).unwrap();
    assert!(content["fields"]
        .as_array()
        .unwrap()
        .contains(&json!({"label":"name","value":"first"})));
    assert!(!serde_json::to_string(&content).unwrap().contains("second"));
}

#[tokio::test]
async fn committed_snapshot_and_history_finish_under_lease_after_waiter_abort() {
    let f = Fixture::new();
    let id = f.site("https://example.com", &["model"]);
    let audit = f
        .service
        .prepare_audit(&Operation::Enabled(id.clone(), true));
    let observed = audit.observer(crate::modules::operation_logs::OperationRecorder::new(
        f.operation_logs.clone(),
    ));
    let (entered, entered_rx) = tokio::sync::oneshot::channel();
    let (release, release_rx) = std::sync::mpsc::channel();
    let sites = f.sites.clone();
    let target = id.clone();
    let task = tokio::spawn(async move {
        sites
            .mutate_observed(
                target,
                SiteMutation::Enabled(true),
                Box::new(move |committed| {
                    let _ = entered.send(());
                    release_rx.recv().unwrap();
                    observed(committed);
                }),
            )
            .await
    });
    entered_rx.await.unwrap();
    task.abort();
    let _ = task.await;
    assert!(matches!(
        f.sites
            .mutate(id.clone(), SiteMutation::Enabled(false))
            .await,
        Err(SiteError::Task(sites::TaskError::Busy))
    ));
    release.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(2), f.sites.drained())
        .await
        .unwrap();
    f.sites
        .mutate(id, SiteMutation::Enabled(false))
        .await
        .unwrap();
    let page = history(&f).await;
    assert_eq!(page.total, 1);
    let detail = f
        .operation_logs
        .detail(page.items[0].id.clone())
        .await
        .unwrap();
    let content = serde_json::to_value(detail.content).unwrap();
    assert!(content["changes"]
        .as_array()
        .unwrap()
        .contains(&json!({"field":"enabled","before":"false","after":"true"})));
}

#[tokio::test]
async fn blocked_attempt_keeps_target_identity_and_redacted_requested_configuration() {
    let f = Fixture::new();
    let id = f.site("https://example.com", &["model"]);
    let _scope = f.sites.begin_execution(id.clone()).await.unwrap();
    assert!(f
        .service
        .execute(Operation::Connection(
            id.clone(),
            sites::Connection {
                models_path: "/models".into(),
                probe_path: "/probe".into(),
                interval_seconds: 420,
                headers: vec![crate::shared::model_client::RequestHeader {
                    key: "X-Trace".into(),
                    value: "private-header".into()
                }],
                ..sites::Connection::for_protocol(Protocol::CustomHttp)
            }
        ))
        .await
        .is_err());
    let page = history(&f).await;
    assert_eq!(page.total, 1);
    assert_eq!(page.items[0].object_snapshot, format!("站点 {id}"));
    assert_eq!(
        page.items[0].result,
        crate::modules::operation_logs::OperationResult::Blocked
    );
    let detail = f
        .operation_logs
        .detail(page.items[0].id.clone())
        .await
        .unwrap();
    assert!(detail.explanation.contains("420"));
    assert!(!serde_json::to_string(&detail)
        .unwrap()
        .contains("private-header"));
}
