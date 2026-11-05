use super::*;
use crate::{
    shared::persistence::test_support::environment,
    startup::{initialize, StartupState},
};
use serde_json::json;

#[tokio::test]
async fn blocked_database_preserves_bounded_marker_and_replays_once_after_repair() {
    let temp = tempfile::tempdir().unwrap();
    let env = environment(temp.path(), "com.relaypulse.app.dev");
    let root = env.paths().data_root().to_owned();
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("relaypulse.sqlite3"), b"broken db").unwrap();
    assert!(matches!(initialize(Ok(env)).0, StartupState::Blocked(_)));
    let path = marker(&root);
    let data = std::fs::read(&path).unwrap();
    assert!(data.len() < 1024);
    let pending: Pending = serde_json::from_slice(&data).unwrap();
    assert!(matches!(pending.stage, StartupFailureStage::Persistence));
    std::fs::remove_file(root.join("relaypulse.sqlite3")).unwrap();
    let StartupState::Ready(app) =
        initialize(Ok(environment(temp.path(), "com.relaypulse.app.dev"))).0
    else {
        panic!("not ready")
    };
    let query = || {
        serde_json::from_value(json!({"startDate":null,"endDate":null,"source":[],"result":["blocked"],"actions":["startup"],"page":1,"pageSize":10})).unwrap()
    };
    let page = app.operation_logs.query(query()).await.unwrap();
    assert_eq!(page.total, 1);
    assert_eq!(page.items[0].id, pending.id);
    assert!(!path.exists());
    // Simulate crash after successful insertion but before deleting the marker.
    std::fs::write(&path, data).unwrap();
    replay(app.environment.paths(), &app.operation_logs);
    assert_eq!(app.operation_logs.query(query()).await.unwrap().total, 1);
    assert!(!path.exists());
}

#[test]
fn invalid_or_unwritable_marker_does_not_change_startup_result() {
    let temp = tempfile::tempdir().unwrap();
    let env = environment(temp.path(), "com.relaypulse.app.dev");
    std::fs::create_dir_all(env.paths().data_root()).unwrap();
    std::fs::write(marker(env.paths().data_root()), vec![0; 2048]).unwrap();
    assert!(matches!(initialize(Ok(env)).0, StartupState::Ready(_)));
}
