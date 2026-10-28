use super::*;
#[tokio::test]
async fn leases_reject_duplicates_global_conflicts_and_drain_on_close() {
    let gate = std::sync::Arc::new(SiteAccess::default());
    let lease = gate.acquire(Some("one")).unwrap();
    assert!(matches!(gate.acquire(Some("one")), Err(TaskError::Busy)));
    assert!(matches!(gate.acquire(None), Err(TaskError::Busy)));
    let second = gate.acquire(Some("two")).unwrap();
    drop(second);
    let clone = lease.clone();
    drop(lease);
    assert!(gate.acquire(Some("one")).is_err());
    drop(clone);
    let global = gate.acquire(None).unwrap();
    assert!(gate.acquire(Some("two")).is_err());
    gate.close();
    drop(global);
    gate.drained().await;
    assert!(matches!(gate.acquire(Some("one")), Err(TaskError::Closing)));
}

#[tokio::test]
async fn database_worker_keeps_lease_even_if_waiting_future_is_cancelled() {
    let gate = std::sync::Arc::new(SiteAccess::default());
    let lease = gate.acquire(Some("site")).unwrap();
    let (sending, entered) = tokio::sync::oneshot::channel();
    let (release, wait) = std::sync::mpsc::channel();
    let task = tokio::spawn(async move {
        lease
            .blocking(move || {
                let _ = sending.send(());
                wait.recv().unwrap();
            })
            .await
    });
    entered.await.unwrap();
    task.abort();
    let _ = task.await;
    assert!(matches!(gate.acquire(Some("site")), Err(TaskError::Busy)));
    release.send(()).unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(2), gate.drained())
        .await
        .unwrap();
    assert!(gate.acquire(Some("site")).is_ok());
}

#[tokio::test]
async fn only_switches_can_overlap_scheduled_execution_and_both_leases_are_tracked() {
    let gate = Arc::new(SiteAccess::default());
    let scheduled = gate.acquire_scheduled("site").unwrap();
    let switch = gate.acquire_switch("site").unwrap();
    assert!(matches!(gate.acquire(Some("site")), Err(TaskError::Busy)));
    assert!(matches!(
        gate.acquire_scheduled("site"),
        Err(TaskError::Busy)
    ));
    assert!(matches!(gate.acquire_switch("site"), Err(TaskError::Busy)));
    assert!(matches!(gate.acquire(None), Err(TaskError::Busy)));

    drop(switch);
    assert!(matches!(gate.acquire(Some("site")), Err(TaskError::Busy)));
    let switch = gate.acquire_switch("site").unwrap();
    drop(scheduled);
    assert!(matches!(gate.acquire(Some("site")), Err(TaskError::Busy)));
    assert!(matches!(
        gate.acquire_scheduled("site"),
        Err(TaskError::Busy)
    ));

    gate.close();
    assert!(matches!(
        gate.acquire_switch("site"),
        Err(TaskError::Closing)
    ));
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(10), gate.drained())
            .await
            .is_err()
    );
    drop(switch);
    gate.drained().await;

    let gate = Arc::new(SiteAccess::default());
    let _manual = gate.acquire(Some("site")).unwrap();
    assert!(matches!(gate.acquire_switch("site"), Err(TaskError::Busy)));
}
