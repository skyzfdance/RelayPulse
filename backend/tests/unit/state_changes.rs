use super::*;

#[tokio::test]
async fn concurrent_writers_invalidate_reads_until_the_last_real_worker_finishes() {
    let changes = Arc::new(StateChanges::default());
    let mut observer = changes.subscribe();
    let first = changes.begin();
    let second = changes.begin();
    observer.changed().await.unwrap();
    assert_eq!(
        changes.version(),
        StateVersion {
            revision: 2,
            writers: 2
        }
    );
    drop(first);
    assert_eq!(
        changes.version(),
        StateVersion {
            revision: 3,
            writers: 1
        }
    );
    let worker = std::thread::spawn(move || drop(second));
    worker.join().unwrap();
    assert_eq!(
        changes.version(),
        StateVersion {
            revision: 4,
            writers: 0
        }
    );
    observer.changed().await.unwrap();
}
