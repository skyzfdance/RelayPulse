//! 用真实本地 HTTP 验证自动探测的三层开关，以及停止派发和取消请求的区别。
use super::*;

fn background(f: &Fixture, enabled: bool) {
    let mut settings = f.general.load().unwrap();
    settings.continue_in_background = enabled;
    f.general.save(settings).unwrap();
}

fn make_due(f: &Fixture, id: &str) {
    let mut connection = f.store.get(id).unwrap().connection;
    connection.interval_seconds = 1;
    f.store.save_connection(id, &connection).unwrap();
    f.database
        .with_connection(|c| {
            c.execute(
                "UPDATE site_models SET created_at_ms=created_at_ms-2000 WHERE site_id=?1",
                [id],
            )?;
            Ok(())
        })
        .unwrap();
}

async fn settled(f: &Fixture, count: i64) {
    tokio::time::timeout(Duration::from_secs(3), async {
        while f.count() != count {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        f.monitoring.drained().await;
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn automatic_requests_require_all_three_switches_in_every_combination() {
    for general in [false, true] {
        for site in [false, true] {
            for model in [false, true] {
                let f = Fixture::new();
                let server = Server::new(vec![Response::raw(200, "text/plain", "OK")]);
                let id = f.site(&server.url, &["m"]);
                let model_id = f.store.get(&id).unwrap().models[0].id.clone();
                f.store.set_enabled(&id, site).unwrap();
                f.store.set_model_enabled(&id, &model_id, model).unwrap();
                background(&f, general);
                make_due(&f, &id);
                let lifecycle = Lifecycle::start(f.monitoring.clone()).unwrap();

                if general && site && model {
                    server.wait_requests(1).await;
                    settled(&f, 1).await;
                } else {
                    tokio::time::sleep(Duration::from_millis(550)).await;
                    assert_eq!(
                        server.requests().len(),
                        0,
                        "switches: {general}/{site}/{model}"
                    );
                    assert_eq!(f.count(), 0);
                    if !general && !site && !model {
                        // 自动条件不约束显式单模型手动探测。
                        f.execute(Operation::Run(id, Some(model_id))).await;
                        assert_eq!(f.count(), 1);
                        assert_eq!(server.requests().len(), 1);
                    }
                }
                lifecycle.stop().await;
            }
        }
    }
}

#[tokio::test]
async fn enabled_models_from_hidden_sites_do_not_enable_another_sites_automatic_probes() {
    let f = Fixture::new();
    let hidden = Server::new(vec![]);
    let visible = Server::new(vec![Response::raw(200, "text/plain", "OK")]);
    let hidden_id = f.site(&hidden.url, &["hidden-model"]);
    let visible_id = f.site(&visible.url, &["visible-model"]);
    f.store.set_enabled(&visible_id, true).unwrap();
    let model = f.store.get(&visible_id).unwrap().models[0].id.clone();
    f.store
        .set_model_enabled(&visible_id, &model, false)
        .unwrap();
    make_due(&f, &hidden_id);
    make_due(&f, &visible_id);
    let lifecycle = Lifecycle::start(f.monitoring.clone()).unwrap();
    tokio::time::sleep(Duration::from_millis(550)).await;
    assert!(hidden.requests().is_empty());
    assert!(visible.requests().is_empty());

    f.execute(Operation::ModelEnabled(visible_id, model, true))
        .await;
    visible.wait_requests(1).await;
    settled(&f, 1).await;
    assert!(hidden.requests().is_empty());
    lifecycle.stop().await;
}

#[derive(Clone, Copy, Debug)]
enum Switch {
    General,
    Site,
    Models,
}

async fn toggle(f: &Fixture, id: &str, switch: Switch, enabled: bool) {
    match switch {
        Switch::General => background(f, enabled),
        Switch::Site => {
            f.execute(Operation::Enabled(id.into(), enabled)).await;
        }
        Switch::Models => {
            for model in f.store.get(id).unwrap().models {
                f.execute(Operation::ModelEnabled(id.into(), model.id, enabled))
                    .await;
            }
        }
    }
}

#[tokio::test]
async fn disabling_any_switch_drains_the_current_request_and_stops_new_dispatch_until_reenabled() {
    for switch in [Switch::General, Switch::Site, Switch::Models] {
        let f = Fixture::new();
        let response = Response::raw(200, "text/plain", "complete response")
            .delayed(Duration::from_millis(400));
        let server = Server::new(vec![response.clone(), response]);
        let id = f.site(&server.url, &["first", "pending"]);
        f.store.set_enabled(&id, true).unwrap();
        make_due(&f, &id);
        let lifecycle = Lifecycle::start(f.monitoring.clone()).unwrap();
        server.wait_requests(1).await;

        // 请求配置和目录结构仍被执行作用域保护，只有启停开关可并行保存。
        for operation in [
            Operation::Connection(id.clone(), f.store.get(&id).unwrap().connection),
            Operation::DeleteModel(id.clone(), f.store.get(&id).unwrap().models[1].id.clone()),
            Operation::Sync(id.clone(), vec!["first".into(), "pending".into()]),
        ] {
            assert!(matches!(
                f.service.execute(operation).await,
                Err(SiteError::Task(TaskError::Busy))
            ));
        }
        toggle(&f, &id, switch, false).await;
        settled(&f, 1).await;
        tokio::time::sleep(Duration::from_millis(550)).await;
        assert_eq!(server.requests().len(), 1, "disabled {switch:?}");
        assert_eq!(f.count(), 1);

        let record_id: String = f.database.with_connection(|c| {
            c.query_row(
                "SELECT id FROM probe_records WHERE run_kind='scheduled' AND success=1 AND body_complete=1",
                [], |r| r.get(0),
            )
        }).unwrap();
        let env = environment(f._directory.path(), "com.relaypulse.app.dev");
        let path = env
            .paths()
            .data_root()
            .join("probe-records")
            .join(record_id)
            .join("response.jsonl");
        let log = std::fs::read_to_string(path).unwrap();
        assert!(log.contains("complete response"));
        assert!(log.contains("request_finished"));

        // 恢复条件后由同一个生命周期重新派发，不需要重启应用或创建第二个调度器。
        toggle(&f, &id, switch, true).await;
        server.wait_requests(2).await;
        toggle(&f, &id, switch, false).await;
        settled(&f, 2).await;
        lifecycle.stop().await;
        assert_eq!(server.requests().len(), 2);
    }
}

#[tokio::test]
async fn lifecycle_schedules_real_probes_at_seconds_interval_and_shutdown_cancels_and_drains() {
    let f = Fixture::new();
    let server = Server::new(vec![
        Response::raw(200, "text/plain", "OK"),
        Response::raw(200, "text/plain", "slow").delayed(Duration::from_secs(1)),
    ]);
    let id = f.site(&server.url, &["m"]);
    f.store.set_enabled(&id, true).unwrap();
    let site = f.store.get(&id).unwrap();
    let mut connection = site.connection;
    connection.interval_seconds = 1;
    f.store.save_connection(&id, &connection).unwrap();
    f.database
        .with_connection(|c| {
            c.execute_batch("UPDATE site_models SET created_at_ms=created_at_ms-2000")
        })
        .unwrap();
    let lifecycle = Lifecycle::start(f.monitoring.clone()).unwrap();
    assert!(Lifecycle::start(f.monitoring.clone()).is_err());
    server.wait_requests(1).await;
    tokio::time::timeout(Duration::from_secs(2), async {
        while f.count() != 1 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        f.monitoring
            .states(site.models.iter().map(|m| m.id.clone()).collect())
            .await
            .unwrap()[&site.models[0].id]
            .health,
        Health::Healthy
    );
    server.wait_requests(2).await;
    tokio::time::timeout(Duration::from_secs(2), lifecycle.stop())
        .await
        .unwrap();
    assert_eq!(f.count(), 1);
    assert!(matches!(
        f.service.execute(Operation::Run(id, None)).await,
        Err(SiteError::Task(TaskError::Closing))
    ));
}
