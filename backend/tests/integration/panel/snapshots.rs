use super::*;

#[tokio::test]
async fn saved_theme_is_in_the_initial_snapshot_and_each_revision_without_probing() {
    use crate::modules::configuration::ThemeMode;
    let f = Fixture::new();
    let mut changes = f.app.panel.changes().subscribe();
    let initial = f.app.panel.snapshot().await.unwrap();
    assert_eq!(initial.theme, ThemeMode::System);
    let mut revision = initial.revision;
    for theme in [ThemeMode::Light, ThemeMode::Dark, ThemeMode::System] {
        let mut settings = f.app.configuration.load().unwrap();
        settings.theme = theme;
        f.app.configuration.save(settings).unwrap();
        changes.changed().await.unwrap();
        let next = f.app.panel.snapshot().await.unwrap();
        assert_eq!(next.theme, theme);
        assert!(next.revision > revision);
        assert!(next.active_probe.is_none());
        revision = next.revision;
    }
}

#[tokio::test]
async fn snapshot_reads_real_configuration_without_probing_and_notifies_on_scope_and_schedule_changes(
) {
    let f = Fixture::new();
    let server = Server::new(vec![]);
    let first = f.site(&server.url, &["first", "second"], true);
    let second = f.site(&server.url, &["third"], true);
    f.site(&server.url, &["hidden"], false);
    let before = snapshot(&f).await;
    assert_eq!(
        (
            before["state"].clone(),
            before["siteCount"].clone(),
            before["modelCount"].clone()
        ),
        (json!("waiting"), json!(2), json!(3))
    );
    assert_eq!(before["schedule"]["state"], "scheduled");
    assert!(server.requests().is_empty());

    let mut changes = f.app.panel.changes().subscribe();
    f.app
        .sites
        .execute(Operation::Enabled(first.clone(), false))
        .await
        .unwrap();
    changes.changed().await.unwrap();
    let smaller = snapshot(&f).await;
    assert_eq!(
        (smaller["siteCount"].clone(), smaller["modelCount"].clone()),
        (json!(1), json!(1))
    );
    assert!(smaller["revision"].as_u64() > before["revision"].as_u64());
    let mut general = f.app.configuration.load().unwrap();
    general.continue_in_background = false;
    f.app.configuration.save(general).unwrap();
    changes.changed().await.unwrap();
    assert_eq!(snapshot(&f).await["schedule"], json!({"state":"disabled"}));
    f.app
        .sites
        .execute(Operation::Delete(second))
        .await
        .unwrap();
    assert_eq!(snapshot(&f).await["modelCount"], 0);
    assert!(server.requests().is_empty());
}

#[tokio::test]
async fn query_waits_for_real_writes_and_never_publishes_a_mixed_snapshot() {
    let f = Fixture::new();
    let server = Server::new(vec![]);
    let id = f.site(&server.url, &["m"], true);
    let before = snapshot(&f).await;
    let write = f.app.panel.changes().begin();
    let panel = f.app.panel.clone();
    let reading = tokio::spawn(async move { panel.snapshot().await.unwrap() });
    tokio::task::yield_now().await;
    assert!(!reading.is_finished());
    f.store.set_enabled(&id, false).unwrap();
    let mut general = f.app.configuration.load().unwrap();
    general.continue_in_background = false;
    f.app.configuration.save(general).unwrap();
    drop(write);
    let after = reading.await.unwrap();
    assert!(after.revision > before["revision"].as_u64().unwrap());
    assert_eq!(after.site_count, 0);
    assert_eq!(
        serde_json::to_value(after).unwrap()["schedule"]["state"],
        "disabled"
    );
}

#[tokio::test]
async fn completed_state_and_actual_due_time_survive_reopen_without_fabricating_a_batch() {
    let f = Fixture::new();
    let server = Server::new(vec![Response::raw(200, "text/plain", "OK")]);
    f.site(&server.url, &["m"], true);
    let result = f.app.panel.refresh().await.unwrap();
    assert_eq!(
        serde_json::to_value(&result).unwrap()["lastProbe"]["totalCount"],
        1
    );
    let env = crate::shared::persistence::test_support::environment(
        f.directory.path(),
        "com.relaypulse.app.dev",
    );
    let diagnostics = Arc::new(crate::shared::diagnostics::Diagnostics::new(&env));
    let reopened = crate::application::composition::compose(env, diagnostics).unwrap();
    let after = reopened.panel.snapshot().await.unwrap();
    assert_eq!(after.sites, result.sites);
    assert_eq!(after.schedule, result.schedule);
    assert!(after.last_probe.is_some());
    assert!(after.active_probe.is_none());
    assert_eq!(server.requests().len(), 1);
}

#[tokio::test]
async fn completed_pending_results_survive_reopen_without_another_probe() {
    let f = Fixture::new();
    let server = Server::new(vec![Response::raw(503, "text/plain", "unavailable")]);
    f.site(&server.url, &["m"], true);
    let result = f.app.panel.refresh().await.unwrap();
    let json = serde_json::to_value(&result).unwrap();
    assert_eq!(json["state"], "ready");
    assert_eq!(json["sites"][0]["models"][0]["status"], "pending");

    let env = crate::shared::persistence::test_support::environment(
        f.directory.path(),
        "com.relaypulse.app.dev",
    );
    let diagnostics = Arc::new(crate::shared::diagnostics::Diagnostics::new(&env));
    let reopened = crate::application::composition::compose(env, diagnostics).unwrap();
    let after = reopened.panel.snapshot().await.unwrap();
    assert_eq!(after.state, result.state);
    assert_eq!(after.sites, result.sites);
    assert!(after.last_probe.is_some());
    assert!(after.active_probe.is_none());
    assert_eq!(server.requests().len(), 1);
}

#[tokio::test]
async fn actual_batch_counts_survive_a_persisted_completion_in_the_same_millisecond() {
    let f = Fixture::new();
    let server = Server::new(vec![Response::raw(200, "text/plain", "OK"); 2]);
    f.site(&server.url, &["first", "second"], true);
    let result = f.app.panel.refresh().await.unwrap();
    let completed = result.last_probe.as_ref().unwrap();
    let finished_at_ms = time::OffsetDateTime::parse(
        &completed.completed_at,
        &time::format_description::well_known::Rfc3339,
    )
    .unwrap()
    .unix_timestamp_nanos()
        / 1_000_000;
    {
        let _write = f.app.panel.changes().begin();
        f.database
            .with_connection(|c| {
                c.execute(
                    "UPDATE probe_states SET finished_at_ms=?1",
                    [i64::try_from(finished_at_ms).unwrap()],
                )
            })
            .unwrap();
    }
    let snapshot = f.app.panel.snapshot().await.unwrap();
    assert_eq!(snapshot.last_probe, result.last_probe);
    assert_eq!(snapshot.last_probe.unwrap().completed_count, 2);
}
