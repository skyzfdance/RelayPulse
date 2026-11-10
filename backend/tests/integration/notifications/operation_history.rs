use crate::application::notifications::audit;
use crate::modules::notifications::*;
use crate::notification_host::FakePlatform;
use crate::shared::persistence::{test_support::environment, Database};
use std::sync::Arc;
const DEV: &str = "com.relaypulse.app.dev";
const FAILED: &str = "probe.model_failed";
use crate::modules::operation_logs::*;
use serde_json::json;

#[tokio::test]
async fn rendered_notifications_preserve_trigger_product_source_and_real_send_outcome() {
    let temp = tempfile::tempdir().unwrap();
    let database = Arc::new(
        Database::open(
            environment(temp.path(), DEV).paths(),
            &[
                crate::modules::notifications::SCHEMA,
                crate::modules::operation_logs::SCHEMA,
            ],
        )
        .unwrap(),
    );
    let logs = Arc::new(OperationLogs::new(database.clone()));
    let service = NotificationService::new(
        SqliteNotificationStore::new(database),
        crate::application::notifications::catalog(),
    );
    let platform = FakePlatform::new(NotificationPermission::Granted);
    let context = crate::application::notifications::context(
        "historic-model",
        "https://example.com",
        1_792_972_800_000,
        NotificationAction {
            target: "probe_records".into(),
            parameters: Default::default(),
        },
    );
    let query = || {
        serde_json::from_value(json!({"startDate":null,"endDate":null,"source":[],"result":[],"actions":["notification"],"page":1,"pageSize":100})).unwrap()
    };
    audit::send_rule(
        &service,
        &logs,
        OperationSource::User,
        FAILED,
        &context,
        &platform,
    )
    .unwrap();
    assert_eq!(
        logs.query(query()).await.unwrap().total,
        0,
        "disabled is skipped"
    );
    let mut settings = service.load(&platform).unwrap();
    settings.notifications_enabled = true;
    service.save(settings, &platform).unwrap();
    audit::send_rule(
        &service,
        &logs,
        OperationSource::User,
        FAILED,
        &context,
        &platform,
    )
    .unwrap();
    let page = logs.query(query()).await.unwrap();
    assert_eq!(page.total, 1);
    assert_eq!(page.items[0].source, OperationSource::User);
    let detail = logs.detail(page.items[0].id.clone()).await.unwrap();
    let OperationLogContent::Notification {
        title,
        body,
        send_result,
        ..
    } = detail.content
    else {
        panic!("not notification")
    };
    assert_eq!((title, body), platform.sent.lock().unwrap()[0]);
    assert!(send_result.contains("已接受"));
    *platform.fail.lock().unwrap() = Some("send");
    audit::send_rule(
        &service,
        &logs,
        OperationSource::User,
        FAILED,
        &context,
        &platform,
    )
    .unwrap_err();
    let page = logs.query(query()).await.unwrap();
    assert_eq!(page.total, 2);
    assert!(page
        .items
        .iter()
        .any(|i| i.result == OperationResult::Failure));
    *platform.fail.lock().unwrap() = None;
    *platform.permission.lock().unwrap() = NotificationPermission::Denied;
    audit::send_rule(
        &service,
        &logs,
        OperationSource::User,
        FAILED,
        &context,
        &platform,
    )
    .unwrap();
    assert_eq!(
        logs.query(query()).await.unwrap().total,
        2,
        "permission skipped"
    );
    audit::send_test(
        &service,
        &logs,
        TestNotification {
            title: "test".into(),
            body: "product".into(),
            marker_id: None,
        },
        &platform,
    )
    .unwrap_err();
    assert!(logs
        .query(query())
        .await
        .unwrap()
        .items
        .iter()
        .any(|i| i.result == OperationResult::Blocked));
}

#[tokio::test]
async fn same_capability_read_has_audit_only_in_its_page_use_case() {
    let f = crate::panel_test_support::Fixture::new();
    let platform = FakePlatform::new(NotificationPermission::Granted);
    *platform.fail.lock().unwrap() = Some("permission_query");
    let count = || {
        f.database
            .with_connection(|c| {
                c.query_row("SELECT count(*) FROM operation_logs", [], |r| {
                    r.get::<_, i64>(0)
                })
            })
            .unwrap()
    };
    let before = count();
    assert!(f.app.notifications.load(&platform).is_err());
    assert_eq!(count(), before, "background reads have no page semantics");
    assert!(audit::load(&f.app.notifications, &f.app.operation_logs, &platform).is_err());
    assert_eq!(
        count(),
        before + 1,
        "page failure is recorded once by its owner"
    );
    assert!(f.app.notifications.template("unknown.event").is_err());
    assert_eq!(count(), before + 1);
    assert!(audit::template(&f.app.notifications, &f.app.operation_logs, "unknown.event").is_err());
    assert_eq!(count(), before + 2);
}
