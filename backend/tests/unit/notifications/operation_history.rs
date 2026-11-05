use super::*;
use crate::modules::operation_logs::*;
use serde_json::json;

#[tokio::test]
async fn rendered_notifications_preserve_trigger_product_source_and_real_send_outcome() {
    let temp = tempfile::tempdir().unwrap();
    let database = Arc::new(
        Database::open(
            environment(temp.path(), DEV).paths(),
            &[
                configuration::SCHEMA,
                crate::modules::notifications::SCHEMA,
                crate::modules::operation_logs::SCHEMA,
            ],
        )
        .unwrap(),
    );
    let logs = Arc::new(OperationLogs::new(database.clone()));
    let service = NotificationService::new(SqliteNotificationStore::new(database))
        .with_operation_logs(logs.clone());
    let platform = FakePlatform::new(NotificationPermission::Granted);
    let context = NotificationContext {
        source: OperationSource::User,
        model: "historic-model",
        endpoint: "https://example.com",
        finished_at_ms: 1_792_972_800_000,
    };
    let query = || {
        serde_json::from_value(json!({"startDate":null,"endDate":null,"source":[],"result":[],"actions":["notification"],"page":1,"pageSize":100})).unwrap()
    };
    service.send_rule(FAILED, &context, &platform).unwrap();
    assert_eq!(
        logs.query(query()).await.unwrap().total,
        0,
        "disabled is skipped"
    );
    let mut settings = service.load(&platform).unwrap();
    settings.notifications_enabled = true;
    service.save(settings, &platform).unwrap();
    service.send_rule(FAILED, &context, &platform).unwrap();
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
    service.send_rule(FAILED, &context, &platform).unwrap_err();
    let page = logs.query(query()).await.unwrap();
    assert_eq!(page.total, 2);
    assert!(page
        .items
        .iter()
        .any(|i| i.result == OperationResult::Failure));
    *platform.fail.lock().unwrap() = None;
    *platform.permission.lock().unwrap() = NotificationPermission::Denied;
    service.send_rule(FAILED, &context, &platform).unwrap();
    assert_eq!(
        logs.query(query()).await.unwrap().total,
        2,
        "permission skipped"
    );
    service
        .send_test(
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
