//! 启动阶段失败必须阻止就绪，且不创建错误环境资源或覆盖已有数据。
use super::support::{connection, defaults, ready, DEV};
use crate::{
    shared::{
        environment::EnvironmentContext,
        persistence::{test_support::environment, PersistenceError},
    },
    startup::{initialize, StartupError, StartupState},
};
use rusqlite::Connection;
use std::fs;

#[test]
fn environment_failure_creates_no_resources() {
    let temp = tempfile::tempdir().unwrap();
    let invalid = EnvironmentContext::resolve("unknown", temp.path(), &temp.path().join("unknown"));
    assert!(matches!(
        initialize(invalid).0,
        StartupState::Blocked(StartupError::Environment(_))
    ));
    assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 0);
}

#[test]
fn reports_resource_creation_and_database_open_failures() {
    let temp = tempfile::tempdir().unwrap();
    let env = environment(temp.path(), DEV);
    fs::write(env.paths().data_root(), b"occupied").unwrap();
    assert!(matches!(
        initialize(Ok(env)).0,
        StartupState::Blocked(StartupError::Persistence(_))
    ));
    let temp = tempfile::tempdir().unwrap();
    let env = environment(temp.path(), DEV);
    fs::create_dir_all(env.paths().data_root()).unwrap();
    let path = env.paths().data_root().join("relaypulse.sqlite3");
    fs::write(&path, b"invalid database").unwrap();
    assert!(matches!(
        initialize(Ok(env)).0,
        StartupState::Blocked(StartupError::Persistence(_))
    ));
    assert_eq!(fs::read(path).unwrap(), b"invalid database");
}

#[test]
fn old_schema_is_reported_without_resetting_the_database() {
    let temp = tempfile::tempdir().unwrap();
    let env = environment(temp.path(), DEV);
    fs::create_dir_all(env.paths().data_root()).unwrap();
    let path = env.paths().data_root().join("relaypulse.sqlite3");
    let connection = Connection::open(&path).unwrap();
    connection.execute_batch("CREATE TABLE general_settings (tray_action TEXT); INSERT INTO general_settings VALUES ('panel'); PRAGMA user_version = 7;").unwrap();
    drop(connection);
    let before = fs::read(&path).unwrap();
    assert!(matches!(
        initialize(Ok(env)).0,
        StartupState::Blocked(StartupError::Persistence(
            PersistenceError::UnsupportedSchema(_)
        ))
    ));
    assert_eq!(fs::read(path).unwrap(), before);
}

#[test]
fn missing_or_invalid_settings_block_readiness() {
    for change in [
        "DELETE FROM general_settings",
        "PRAGMA ignore_check_constraints = ON; UPDATE general_settings SET history_retention_days = 0",
        "PRAGMA ignore_check_constraints = ON; UPDATE general_settings SET theme = 'unknown'",
        "PRAGMA ignore_check_constraints = ON; UPDATE general_settings SET language = 'en-US'",
    ] {
        let temp = tempfile::tempdir().unwrap();
        let app = ready(environment(temp.path(), DEV));
        let connection = connection(&app);
        connection.execute_batch(change).unwrap();
        drop(connection);
        drop(app);
        assert!(matches!(initialize(Ok(environment(temp.path(), DEV))).0, StartupState::Blocked(StartupError::Configuration(_))));
        let connection = Connection::open(temp.path().join(DEV).join("relaypulse.sqlite3")).unwrap();
        // 修复仅由测试夹具显式执行，启动过程不自动回填。
        connection.execute_batch("BEGIN EXCLUSIVE; DELETE FROM general_settings; INSERT INTO general_settings (singleton) VALUES (1); COMMIT;").unwrap();
        drop(connection);
        assert_eq!(ready(environment(temp.path(), DEV)).configuration.load().unwrap(), defaults());
    }
}
