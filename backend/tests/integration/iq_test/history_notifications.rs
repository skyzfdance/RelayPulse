use super::*;
use crate::modules::notifications::{
    NotificationAction, NotificationPermission, TemplateMode, TemplateUpdate,
};
use crate::notification_host::FakePlatform;

fn notifications(f: &Fixture, permission: NotificationPermission) -> Arc<FakePlatform> {
    let platform = Arc::new(FakePlatform::new(permission));
    let mut settings = f.app.notifications.load(platform.as_ref()).unwrap();
    settings.notifications_enabled = true;
    f.app
        .notifications
        .save(settings, platform.as_ref())
        .unwrap();
    assert!(f
        .app
        .iq_test
        .set_completion_observer(Arc::new(
            crate::application::notifications::IqCompletionNotifications::new(
                f.app.notifications.clone(),
                platform.clone(),
                f.app.operation_logs.clone(),
            )
        ))
        .is_ok());
    platform
}
async fn wait_sent(platform: &FakePlatform, count: usize) {
    tokio::time::timeout(Duration::from_secs(2), async {
        while platform.sent.lock().unwrap().len() < count {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn natural_completion_notifies_once_even_for_all_failure_and_abort_never_notifies() {
    let f = Fixture::new();
    let platform = notifications(&f, NotificationPermission::Granted);
    f.app
        .notifications
        .save_template(
            "iq.run_completed",
            TemplateUpdate {
                template_type: TemplateMode::Custom,
                marker_id: "info".into(),
                title_template: "{{model}} done".into(),
                body_template: "{{app}} {{time}}".into(),
            },
        )
        .unwrap();
    let mut stalled = success(Protocol::OpenaiCompatible);
    stalled.header_delay = Duration::from_secs(5);
    let server = Server::new(vec![
        success(Protocol::OpenaiCompatible),
        Response::raw(500, "application/json", "upstream failed"),
        success(Protocol::OpenaiCompatible),
        stalled,
    ]);
    let site = f.site(&server.url, Protocol::OpenaiCompatible, &["model"], true);
    f.available(&site, 0, json!({})).await;
    let q = f.question("all calls can fail").await;
    let run = f.start(std::slice::from_ref(&q)).await;
    let run = f.terminal(&run.id).await;
    assert_eq!(run.status, RunStatus::Completed);
    assert_eq!(run.summary.call_result.value, CallResultValue::AllFailure);
    wait_sent(&platform, 1).await;
    assert!(platform.sent.lock().unwrap()[0].0.contains("IQ 测试 done"));
    assert_eq!(
        *platform.actions.lock().unwrap(),
        vec![Some(NotificationAction {
            target: "iq_run".into(),
            parameters: std::collections::BTreeMap::from([("runId".into(), run.id.clone())])
        })]
    );
    f.app.iq_test.snapshot().await.unwrap();
    f.app.iq_test.run_details(run.id).await.unwrap();
    f.available(&site, 0, json!({})).await;
    let abort = f.start(&[q]).await;
    server.wait_requests(4).await;
    f.app.iq_test.abort_run().await.unwrap();
    assert_eq!(
        f.app.iq_test.run_details(abort.id).await.unwrap().status,
        RunStatus::Aborted
    );
    assert_eq!(platform.sent.lock().unwrap().len(), 1);
    assert_eq!(*platform.requested.lock().unwrap(), 0);
    f.app.iq_test.stop().await;
}

#[tokio::test]
async fn automatic_iq_notifications_obey_permission_global_switch_and_rule_switch_without_requesting_permission(
) {
    for gate in 0..3 {
        let f = Fixture::new();
        let platform = notifications(
            &f,
            if gate == 0 {
                NotificationPermission::Denied
            } else {
                NotificationPermission::Granted
            },
        );
        let mut settings = f.app.notifications.load(platform.as_ref()).unwrap();
        if gate == 1 {
            settings.notifications_enabled = false;
        }
        if gate == 2 {
            settings
                .rules
                .iter_mut()
                .find(|r| r.id == "iq.run_completed")
                .unwrap()
                .status = false;
        }
        f.app
            .notifications
            .save(settings, platform.as_ref())
            .unwrap();
        let server = Server::new(vec![success(Protocol::OpenaiCompatible); 2]);
        let site = f.site(&server.url, Protocol::OpenaiCompatible, &["model"], true);
        f.available(&site, 0, json!({})).await;
        let run = f.start(&[f.question("quiet finish").await]).await;
        assert_eq!(f.terminal(&run.id).await.status, RunStatus::Completed);
        f.app.iq_test.stop().await;
        assert!(platform.sent.lock().unwrap().is_empty());
        assert_eq!(*platform.requested.lock().unwrap(), 0);
    }
}

#[tokio::test]
async fn history_filters_use_frozen_site_and_model_identities_after_sources_are_deleted_and_retention_settings_do_not_purge_iq(
) {
    let f = Fixture::new();
    let server = Server::new(vec![success(Protocol::OpenaiCompatible); 6]);
    let site = f.site(
        &server.url,
        Protocol::OpenaiCompatible,
        &["Zulu", "alpha"],
        true,
    );
    f.available(&site, 0, json!({})).await;
    f.available(&site, 1, json!({})).await;
    let q = f.question("history question").await;
    let old = f.start(std::slice::from_ref(&q)).await;
    let old = f.terminal(&old.id).await;
    // 显式从源重建一份新快照，用来验证历史选项采用最近快照的名称。
    for p in f.app.iq_test.snapshot().await.unwrap().participants {
        f.app.iq_test.remove_participant(p.id).await.unwrap();
    }
    f.database
        .with_connection(|c| c.execute("UPDATE sites SET name='最新站点' WHERE id=?1", [&site.id]))
        .unwrap();
    f.available(&site, 0, json!({})).await;
    let newer = f.start(std::slice::from_ref(&q)).await;
    let newer = f.terminal(&newer.id).await;
    f.app
        .sites
        .execute(crate::application::sites::Operation::Delete(
            site.id.clone(),
        ))
        .await
        .unwrap();
    f.app.iq_test.delete_question(q.id).await.unwrap();
    let options = f.app.iq_test.history_options().await.unwrap();
    assert_eq!(options.len(), 1);
    assert_eq!(options[0].value, site.id);
    assert_eq!(options[0].label, "最新站点");
    let models = options[0].children.as_ref().unwrap();
    assert_eq!(
        models.iter().map(|m| m.label.as_str()).collect::<Vec<_>>(),
        vec!["alpha", "Zulu"]
    );
    assert!(models.iter().all(|m| m.children.is_none()));
    assert_eq!(
        f.app
            .iq_test
            .run_details(old.id.clone())
            .await
            .unwrap()
            .participants[0]
            .candidate
            .site_name,
        "原站点"
    );
    assert_eq!(
        f.app.iq_test.run_details(newer.id).await.unwrap().questions[0].content,
        "history question"
    );
    let mut settings = f.app.configuration.load().unwrap();
    settings.history_retention_days = 1;
    f.app.configuration.save(settings).unwrap();
    f.app.monitoring.clear_probe_records().await.unwrap();
    f.app.iq_test.stop().await;
    let reopened = f.reopen();
    assert_eq!(reopened.iq_test.snapshot().await.unwrap().runs.len(), 2);
    assert!(raw(&f.root(), &old.entries[0], 1).exists());
    reopened.iq_test.stop().await;
}

#[test]
fn notification_extension_preserves_both_original_rule_switches_and_custom_templates() {
    use crate::{
        modules::{configuration, notifications},
        shared::persistence::{test_support::environment, Database},
    };
    let directory = tempfile::tempdir().unwrap();
    let env = environment(directory.path(), "com.relaypulse.app.dev");
    let db = Database::open(env.paths(), &[configuration::SCHEMA, notifications::SCHEMA]).unwrap();
    let template = json!({"templateType":"custom","markerId":"warning","titleTemplate":"{{model}} persisted","bodyTemplate":"{{app}}"}).to_string();
    db.with_connection(|c| {
        c.execute("UPDATE notification_settings SET enabled=1",[])?;
        c.execute("INSERT INTO notification_rules (id,status,custom_template) VALUES ('probe.model_failed',0,?1)",[&template])?;
        Ok(())
    }).unwrap();
    drop(db);
    let db = Arc::new(
        Database::open(env.paths(), &[configuration::SCHEMA, notifications::SCHEMA]).unwrap(),
    );
    let service = notifications::NotificationService::new(
        notifications::SqliteNotificationStore::new(db.clone()),
        crate::notification_test_definitions::catalog(),
    );
    let settings = service
        .load(&FakePlatform::new(NotificationPermission::Granted))
        .unwrap();
    assert!(settings.notifications_enabled);
    assert_eq!(settings.rules.len(), 3);
    assert!(
        !settings
            .rules
            .iter()
            .find(|r| r.id == "probe.model_failed")
            .unwrap()
            .status
    );
    assert!(
        settings
            .rules
            .iter()
            .find(|r| r.id == "probe.model_recovered")
            .unwrap()
            .status
    );
    assert!(
        settings
            .rules
            .iter()
            .find(|r| r.id == "iq.run_completed")
            .unwrap()
            .status
    );
    assert_eq!(
        service
            .template("probe.model_failed")
            .unwrap()
            .title_template,
        "{{model}} persisted"
    );
}
