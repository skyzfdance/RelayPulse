use super::*;
use crate::{
    modules::{
        monitoring::Health,
        notifications::{
            tests::FakePlatform, NotificationPermission, TemplateMode, TemplateUpdate,
        },
    },
    panel_test_support::Fixture,
    shared::model_client::test_support::{Response, Server},
};
use std::time::Duration;

fn connect(f: &Fixture) -> Arc<FakePlatform> {
    let platform = Arc::new(FakePlatform::new(NotificationPermission::Granted));
    let mut settings = f.app.notifications.load(platform.as_ref()).unwrap();
    settings.notifications_enabled = true;
    f.app
        .notifications
        .save(settings, platform.as_ref())
        .unwrap();
    assert!(f
        .app
        .monitoring
        .set_health_observer(Arc::new(HealthNotifications::new(
            f.app.notifications.clone(),
            platform.clone(),
        )))
        .is_ok());
    platform
}

#[tokio::test]
async fn committed_health_edges_notify_once_after_failure_and_recovery_thresholds() {
    let f = Fixture::new();
    let platform = connect(&f);
    f.app
        .notifications
        .save_template(
            "probe.model_failed",
            TemplateUpdate {
                template_type: TemplateMode::Custom,
                marker_id: "warning".into(),
                title_template: "{{app}} / {{model}}".into(),
                body_template: "{{endpoint}} @ {{time}}".into(),
            },
        )
        .unwrap();
    let server = Server::new(vec![
        Response::raw(200, "text/plain", "OK"),
        Response::raw(503, "text/plain", "unavailable"),
        Response::raw(503, "text/plain", "unavailable"),
        Response::raw(503, "text/plain", "unavailable"),
        Response::raw(200, "text/plain", "OK"),
        Response::raw(200, "text/plain", "OK").delayed(Duration::from_millis(1100)),
        Response::raw(200, "text/plain", "OK"),
    ]);
    let id = f.site(&server.url, &["model {{app}}"], true);
    let site = f.store.get(&id).unwrap();
    let mut connection = site.connection;
    connection.failure_threshold = 2;
    connection.recovery_threshold = 2;
    connection.slow_total_seconds = 1;
    f.store.save_connection(&id, &connection).unwrap();
    for (index, expected) in [0, 0, 1, 1, 1, 2, 2].into_iter().enumerate() {
        f.app.monitoring.run(id.clone(), None).await.unwrap();
        assert_eq!(
            platform.sent.lock().unwrap().len(),
            expected,
            "probe {index}"
        );
        if index == 5 {
            assert_eq!(
                f.app
                    .monitoring
                    .states(vec![site.models[0].id.clone()])
                    .await
                    .unwrap()[&site.models[0].id]
                    .health,
                Health::Warning
            );
        }
    }
    let sent = platform.sent.lock().unwrap();
    assert_eq!(sent[0].0, "⚠️ RelayPulse / model {{app}}");
    assert!(sent[0].1.starts_with("127.0.0.1 @ "));
    assert!(!sent[0].1.contains("{{time}}"));
    assert_eq!(sent[1].0, "✅ model {{app}} 已恢复");
    assert_eq!(server.requests().len(), 7);
}

#[tokio::test]
async fn failed_notification_does_not_fail_or_repeat_the_committed_probe() {
    let f = Fixture::new();
    let platform = connect(&f);
    *platform.fail.lock().unwrap() = Some("send");
    let server = Server::new(vec![Response::raw(503, "text/plain", "failed"); 2]);
    let id = f.site(&server.url, &["model"], true);
    let site = f.store.get(&id).unwrap();
    let mut connection = site.connection;
    connection.failure_threshold = 1;
    f.store.save_connection(&id, &connection).unwrap();
    f.app.monitoring.run(id.clone(), None).await.unwrap();
    *platform.fail.lock().unwrap() = None;
    f.app.monitoring.run(id, None).await.unwrap();
    assert!(platform.sent.lock().unwrap().is_empty());
    assert_eq!(
        f.app
            .monitoring
            .states(vec![site.models[0].id.clone()])
            .await
            .unwrap()[&site.models[0].id]
            .health,
        Health::Error
    );
    assert_eq!(
        f.database
            .with_connection(|c| c
                .query_row("SELECT count(*) FROM probe_records", [], |row| row
                    .get::<_, i64>(0)))
            .unwrap(),
        2
    );
}

#[tokio::test]
async fn rolled_back_probe_never_sends_a_notification() {
    let f = Fixture::new();
    let platform = connect(&f);
    let server = Server::new(vec![Response::raw(503, "text/plain", "failed")]);
    let id = f.site(&server.url, &["model"], true);
    let mut connection = f.store.get(&id).unwrap().connection;
    connection.failure_threshold = 1;
    f.store.save_connection(&id, &connection).unwrap();
    f.database.with_connection(|c| c.execute_batch("CREATE TRIGGER reject_health BEFORE INSERT ON probe_states BEGIN SELECT RAISE(ABORT, 'fixture'); END;")).unwrap();
    assert!(f.app.monitoring.run(id, None).await.is_err());
    assert!(platform.sent.lock().unwrap().is_empty());
    assert_eq!(
        f.database
            .with_connection(|c| c
                .query_row("SELECT count(*) FROM probe_records", [], |row| row
                    .get::<_, i64>(0)))
            .unwrap(),
        0
    );
}

#[tokio::test]
async fn scheduled_probes_use_the_same_notification_path() {
    let f = Fixture::new();
    let platform = connect(&f);
    let server = Server::new(vec![
        Response::raw(503, "text/plain", "failed").delayed(Duration::from_millis(100))
    ]);
    let id = f.site(&server.url, &["scheduled-model"], true);
    let mut connection = f.store.get(&id).unwrap().connection;
    connection.failure_threshold = 1;
    connection.interval_seconds = 1;
    f.store.save_connection(&id, &connection).unwrap();
    f.database
        .with_connection(|c| {
            c.execute(
                "UPDATE site_models SET created_at_ms=created_at_ms-2000",
                [],
            )
        })
        .unwrap();
    let lifecycle =
        crate::application::lifecycle::Lifecycle::start(f.app.monitoring.clone()).unwrap();
    server.wait_requests(1).await;
    let mut general = f.app.configuration.load().unwrap();
    general.continue_in_background = false;
    f.app.configuration.save(general).unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        while platform.sent.lock().unwrap().is_empty() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    lifecycle.stop().await;
    assert_eq!(platform.sent.lock().unwrap().len(), 1);
    assert_eq!(server.requests().len(), 1);
}
