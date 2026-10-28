use super::*;

#[tokio::test]
async fn repeated_and_conflicting_requests_are_rejected_and_abort_releases_lease() {
    let f = Fixture::new();
    let server = Server::new(vec![
        Response::raw(200, "text/plain", "OK").delayed(Duration::from_secs(1))
    ]);
    let id = f.site(&server.url, &["m"]);
    let model = f.store.get(&id).unwrap().models[0].id.clone();
    let service = f.service.clone();
    let target = id.clone();
    let model_id = model.clone();
    let task = tokio::spawn(async move {
        service
            .execute(Operation::Test(
                target,
                model_id,
                CustomRequest {
                    version: 2,
                    endpoint: "/draft".into(),
                    parameters: vec![],
                },
            ))
            .await
    });
    server.wait_requests(1).await;
    for op in [
        Operation::Run(id.clone(), None),
        Operation::Sync(id.clone(), vec!["m".into()]),
        Operation::Delete(id.clone()),
        Operation::AddModel(id.clone(), "new".into()),
        Operation::Enabled(id.clone(), true),
        Operation::ModelEnabled(id.clone(), model.clone(), false),
        Operation::Policy(id.clone(), None, Some(ProbePolicy::default()), None),
    ] {
        assert!(matches!(
            f.service.execute(op).await,
            Err(SiteError::Task(TaskError::Busy))
        ));
    }
    task.abort();
    let _ = task.await;
    f.execute(Operation::AddModel(id.clone(), "new".into()))
        .await;
    assert_eq!(f.count(), 0);
    assert_eq!(server.requests().len(), 1);
}
#[tokio::test]
async fn public_services_share_one_access_guard_without_the_command_facade() {
    let f = Fixture::new();
    let server = Server::new(vec![
        Response::raw(200, "text/plain", "slow").delayed(Duration::from_secs(1))
    ]);
    let id = f.site(&server.url, &["m"]);
    let service = f.monitoring.clone();
    let target = id.clone();
    let task = tokio::spawn(async move { service.run(target, None).await });
    server.wait_requests(1).await;
    assert!(matches!(
        f.sites
            .mutate(id.clone(), SiteMutation::Enabled(true))
            .await,
        Err(SiteError::Task(TaskError::Busy))
    ));
    assert!(matches!(
        f.sites.check_sync(id.clone(), vec!["m".into()]).await,
        Err(SiteError::Task(TaskError::Busy))
    ));
    // 一致只读快照不取得长期站点锁。
    assert_eq!(
        f.sites
            .execution_snapshot(id.clone())
            .await
            .unwrap()
            .site
            .id,
        id
    );
    task.abort();
    let _ = task.await;
    f.sites
        .mutate(id, SiteMutation::Enabled(true))
        .await
        .unwrap();
    assert_eq!(f.count(), 0);
}
#[tokio::test]
async fn public_execution_scope_keeps_transaction_protection_after_caller_abort() {
    let f = Fixture::new();
    let id = f.site("http://127.0.0.1:1", &["m"]);
    let scope = f.sites.begin_execution(id.clone()).await.unwrap();
    let (sending, entered) = tokio::sync::oneshot::channel();
    let (release, wait) = std::sync::mpsc::channel();
    let db = f.database.clone();
    let task = tokio::spawn(async move {
        scope
            .blocking(move || {
                db.with_connection(|c| {
                    let tx = c.transaction()?;
                    let _ = sending.send(());
                    wait.recv().unwrap();
                    tx.commit()
                })
            })
            .await
    });
    entered.await.unwrap();
    task.abort();
    let _ = task.await;
    assert!(matches!(
        f.sites
            .mutate(id.clone(), SiteMutation::Enabled(true))
            .await,
        Err(SiteError::Task(TaskError::Busy))
    ));
    release.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(2), f.sites.drained())
        .await
        .unwrap();
    f.sites
        .mutate(id, SiteMutation::Enabled(true))
        .await
        .unwrap();
}
