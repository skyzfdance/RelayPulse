use super::*;
use crate::shared::persistence::Database;
use std::{sync::Arc, time::Duration};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn abandoned_database_reader_still_holds_its_shutdown_lease() {
    let temp = tempfile::tempdir().unwrap();
    let database = Arc::new(
        Database::open_directory(&temp.path().canonicalize().unwrap(), &[SCHEMA]).unwrap(),
    );
    let logs = Arc::new(OperationLogs::new(database.clone()));
    let (held_tx, held_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let blocker = std::thread::spawn(move || {
        database
            .with_connection(|_| {
                held_tx.send(()).unwrap();
                release_rx.recv().unwrap();
                Ok(())
            })
            .unwrap()
    });
    held_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    let waiter = {
        let logs = logs.clone();
        tokio::spawn(async move { logs.detail("missing".into()).await })
    };
    tokio::time::sleep(Duration::from_millis(30)).await;
    waiter.abort();
    logs.close();
    assert!(
        tokio::time::timeout(Duration::from_millis(30), logs.drained())
            .await
            .is_err()
    );
    release_tx.send(()).unwrap();
    blocker.join().unwrap();
    tokio::time::timeout(Duration::from_secs(2), logs.drained())
        .await
        .unwrap();
    assert!(logs.detail("missing".into()).await.is_err());
}
