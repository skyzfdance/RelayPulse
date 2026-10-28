use super::*;

#[tokio::test]
async fn manual_refresh_performs_real_requests_reports_progress_and_waits_for_all_results() {
    let f = Fixture::new();
    let server = Server::new(vec![
        Response::raw(200, "text/plain", "OK"),
        Response::raw(200, "text/plain", "slow").delayed(Duration::from_millis(1100)),
        Response::raw(503, "text/plain", "unavailable"),
    ]);
    let id = f.site(
        &server.url,
        &["healthy", "slow", "unavailable", "disabled"],
        true,
    );
    let site = f.store.get(&id).unwrap();
    {
        let _write = f.app.panel.changes().begin();
        f.store
            .set_model_enabled(&id, &site.models[3].id, false)
            .unwrap();
        let mut connection = site.connection;
        connection.failure_threshold = 1;
        connection.slow_total_seconds = 1;
        f.store.save_connection(&id, &connection).unwrap();
    }
    let mut general = f.app.configuration.load().unwrap();
    general.continue_in_background = false;
    f.app.configuration.save(general).unwrap();
    let panel = f.app.panel.clone();
    let task = tokio::spawn(async move { panel.refresh().await.unwrap() });
    server.wait_requests(2).await;
    assert!(!task.is_finished());
    let progress = snapshot(&f).await;
    assert_eq!(
        progress["activeProbe"],
        json!({"completedCount":1,"totalCount":3})
    );
    assert!(progress["lastProbe"].is_null());
    assert!(matches!(f.app.panel.refresh().await, Err(PanelError::Busy)));
    let result = serde_json::to_value(task.await.unwrap()).unwrap();
    assert_eq!(result["state"], "ready");
    assert_eq!(result["modelCount"], 3);
    assert_eq!(
        result["sites"][0]["models"]
            .as_array()
            .unwrap()
            .iter()
            .map(|model| model["status"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["healthy", "slow", "unavailable"]
    );
    assert!(result["activeProbe"].is_null());
    assert_eq!(result["lastProbe"]["completedCount"], 3);
    assert_eq!(result["lastProbe"]["totalCount"], 3);
    assert_eq!(result["schedule"]["state"], "disabled");
    assert!(result["sites"][0]["models"][0]["firstTokenMs"].is_null());
    assert!(result["sites"][0]["models"][2]["totalMs"].is_number());
    assert_eq!(server.requests().len(), 3);
    assert_eq!(
        f.database
            .with_connection(|c| c
                .query_row("SELECT count(*) FROM probe_records", [], |row| row
                    .get::<_, i64>(0)))
            .unwrap(),
        3
    );
}

#[tokio::test]
async fn a_conflicting_external_probe_is_not_cancelled_and_errors_do_not_leave_phantom_activity() {
    let f = Fixture::new();
    let server = Server::new(vec![
        Response::raw(200, "text/plain", "OK").delayed(Duration::from_millis(150))
    ]);
    let id = f.site(&server.url, &["m"], true);
    let monitoring = f.app.monitoring.clone();
    let task = tokio::spawn(async move { monitoring.run(id, None).await.unwrap() });
    server.wait_requests(1).await;
    assert!(f.app.panel.refresh().await.is_err());
    assert!(snapshot(&f).await["activeProbe"].is_object());
    task.await.unwrap();
    assert!(snapshot(&f).await["activeProbe"].is_null());
    assert_eq!(server.requests().len(), 1);
}

#[tokio::test]
async fn early_failed_results_stay_visible_while_health_waits_for_the_failure_threshold() {
    let f = Fixture::new();
    let server = Server::new(vec![Response::raw(503, "text/plain", "unavailable"); 3]);
    let id = f.site(&server.url, &["m"], true);
    let model_id = f.store.get(&id).unwrap().models[0].id.clone();
    assert_eq!(snapshot(&f).await["state"], "waiting");
    for failures in 1..3 {
        let result = serde_json::to_value(f.app.panel.refresh().await.unwrap()).unwrap();
        assert_eq!(result["state"], "ready");
        assert_eq!(result["modelCount"], 1);
        assert_eq!(result["sites"][0]["siteId"], id);
        assert_eq!(result["sites"][0]["models"][0]["id"], model_id);
        assert_eq!(result["sites"][0]["models"][0]["status"], "pending");
        assert!(result["sites"][0]["models"][0]["firstTokenMs"].is_null());
        assert!(result["sites"][0]["models"][0]["totalMs"].is_number());
        assert!(result["lastProbe"].is_object());
        let states = f
            .app
            .monitoring
            .states(vec![model_id.clone()])
            .await
            .unwrap();
        assert_eq!(
            states[&model_id].health,
            crate::modules::monitoring::Health::Muted
        );
        assert_eq!(states[&model_id].failures, failures);
    }
    let result = serde_json::to_value(f.app.panel.refresh().await.unwrap()).unwrap();
    assert_eq!(result["sites"][0]["models"][0]["status"], "unavailable");
    assert_eq!(server.requests().len(), 3);
}

#[tokio::test]
async fn automatic_probes_publish_the_same_activity_and_finish_after_the_background_switch_is_disabled(
) {
    let f = Fixture::new();
    let server = Server::new(vec![
        Response::raw(200, "text/plain", "OK").delayed(Duration::from_millis(150))
    ]);
    let id = f.site(&server.url, &["m"], true);
    {
        let _write = f.app.panel.changes().begin();
        let mut connection = f.store.get(&id).unwrap().connection;
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
    }
    let mut changes = f.app.panel.changes().subscribe();
    let lifecycle =
        crate::application::lifecycle::Lifecycle::start(f.app.monitoring.clone()).unwrap();
    server.wait_requests(1).await;
    changes.changed().await.unwrap();
    assert_eq!(
        snapshot(&f).await["activeProbe"],
        json!({"completedCount":0,"totalCount":1})
    );
    let mut general = f.app.configuration.load().unwrap();
    general.continue_in_background = false;
    f.app.configuration.save(general).unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let result = snapshot(&f).await;
            if result["activeProbe"].is_null() && result["lastProbe"].is_object() {
                break;
            }
            changes.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
    let result = snapshot(&f).await;
    assert_eq!(result["state"], "ready");
    assert_eq!(result["schedule"]["state"], "disabled");
    lifecycle.stop().await;
    assert_eq!(server.requests().len(), 1);
}

#[tokio::test]
async fn empty_refresh_does_not_invent_a_completion_and_shutdown_interrupts_snapshot_waiters() {
    let f = Fixture::new();
    let result = f.app.panel.refresh().await.unwrap();
    assert!(result.last_probe.is_none());
    assert!(result.active_probe.is_none());
    let _write = f.app.panel.changes().begin();
    let panel = f.app.panel.clone();
    let reader = tokio::spawn(async move { panel.snapshot().await });
    tokio::task::yield_now().await;
    f.app.monitoring.close();
    assert!(tokio::time::timeout(Duration::from_secs(1), reader)
        .await
        .unwrap()
        .unwrap()
        .is_err());
}

#[tokio::test]
async fn newly_enabled_models_in_a_running_automatic_batch_expand_the_actual_plan() {
    let f = Fixture::new();
    let server = Server::new(vec![
        Response::raw(200, "text/plain", "OK")
            .delayed(Duration::from_millis(400));
        2
    ]);
    let id = f.site(&server.url, &["first", "later"], true);
    let later = f.store.get(&id).unwrap().models[1].id.clone();
    {
        let _write = f.app.panel.changes().begin();
        f.store.set_model_enabled(&id, &later, false).unwrap();
        let mut connection = f.store.get(&id).unwrap().connection;
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
    }
    let lifecycle =
        crate::application::lifecycle::Lifecycle::start(f.app.monitoring.clone()).unwrap();
    server.wait_requests(1).await;
    assert_eq!(snapshot(&f).await["activeProbe"]["totalCount"], 1);
    f.app
        .sites
        .execute(Operation::ModelEnabled(id, later, true))
        .await
        .unwrap();
    server.wait_requests(2).await;
    let result = snapshot(&f).await;
    assert_eq!(
        result["activeProbe"],
        json!({"completedCount":1,"totalCount":2})
    );
    let mut general = f.app.configuration.load().unwrap();
    general.continue_in_background = false;
    f.app.configuration.save(general).unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        while snapshot(&f).await["activeProbe"].is_object() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    let completed = snapshot(&f).await;
    assert_eq!(completed["lastProbe"]["completedCount"], 2);
    assert_eq!(completed["lastProbe"]["totalCount"], 2);
    lifecycle.stop().await;
}
