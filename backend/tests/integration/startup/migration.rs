use super::*;
use crate::{legacy_data, shared::persistence::test_support::environment, startup};
use std::{fs, path::Path};

#[test]
fn old_database_prompts_without_writes_and_one_click_preserves_backup_and_settings() {
    let temp = tempfile::tempdir().unwrap();
    let env = environment(temp.path(), "com.relaypulse.app");
    let root = env.paths().data_root().to_owned();
    legacy_data::create(&root);
    let original = fs::read(root.join("relaypulse.sqlite3")).unwrap();
    let (state, diagnostics) = startup::initialize(Ok(env));
    assert!(matches!(
        state,
        StartupState::Blocked(StartupError::Persistence(_))
    ));
    assert!(diagnostics.0.is_none());
    let recovery = StartupRecovery::new(Some(root.clone()), &state);
    assert!(matches!(
        recovery.status().status,
        StartupPhase::MigrationRequired
    ));
    assert_eq!(fs::read(root.join("relaypulse.sqlite3")).unwrap(), original);
    assert!(!recovery.can_restart());

    let outcome = recovery.migrate().unwrap();
    assert!(recovery.can_restart());
    assert!(matches!(
        recovery.status().status,
        StartupPhase::MigrationCompleted
    ));
    let backup = Path::new(&outcome.backup_directory);
    assert_eq!(
        fs::read(backup.join("relaypulse.sqlite3")).unwrap(),
        original
    );
    assert_eq!(
        fs::read(root.join("logs/retained.jsonl")).unwrap(),
        b"original diagnostic bytes\n"
    );
    let (reopened, _) = startup::initialize(Ok(environment(temp.path(), "com.relaypulse.app")));
    let StartupState::Ready(app) = reopened else {
        panic!("converted application must start")
    };
    let settings = app.configuration.load().unwrap();
    assert_eq!(settings.history_retention_days, 77);
    assert!(matches!(
        settings.theme,
        crate::modules::configuration::ThemeMode::Dark
    ));
    assert_eq!(
        recovery.migrate().unwrap().backup_directory,
        outcome.backup_directory
    );
}

#[test]
fn unknown_database_never_offers_or_attempts_an_upgrade() {
    let temp = tempfile::tempdir().unwrap();
    let env = environment(temp.path(), "com.relaypulse.app.dev");
    let root = env.paths().data_root().to_owned();
    legacy_data::create(&root);
    rusqlite::Connection::open(root.join("relaypulse.sqlite3"))
        .unwrap()
        .execute_batch("CREATE TABLE unknown(value TEXT);")
        .unwrap();
    let before = fs::read(root.join("relaypulse.sqlite3")).unwrap();
    let state = startup::initialize(Ok(env)).0;
    let recovery = StartupRecovery::new(Some(root.clone()), &state);
    assert!(matches!(recovery.status().status, StartupPhase::Blocked));
    assert!(recovery.migrate().is_err());
    assert_eq!(fs::read(root.join("relaypulse.sqlite3")).unwrap(), before);
    assert!(!recovery.can_restart());
}

#[test]
fn failed_evidence_validation_retains_original_and_does_not_activate_partial_result() {
    let temp = tempfile::tempdir().unwrap();
    let env = environment(temp.path(), "com.relaypulse.app.dev");
    let root = env.paths().data_root().to_owned();
    legacy_data::create(&root);
    let evidence = root.join("probe-records/broken");
    fs::create_dir_all(&evidence).unwrap();
    fs::write(
        evidence.join("response.jsonl"),
        "{\"event\":\"record_created\",\"formatVersion\":999}\n",
    )
    .unwrap();
    let before = fs::read(root.join("relaypulse.sqlite3")).unwrap();
    let state = startup::initialize(Ok(env)).0;
    let recovery = StartupRecovery::new(Some(root.clone()), &state);
    assert!(recovery.migrate().unwrap_err().message.contains("校验失败"));
    assert_eq!(fs::read(root.join("relaypulse.sqlite3")).unwrap(), before);
    assert!(evidence.join("response.jsonl").exists());
    assert!(!recovery.can_restart());
    assert!(!recovery.is_busy());
}

#[test]
fn migration_is_unavailable_for_ready_or_environment_blocked_applications() {
    let temp = tempfile::tempdir().unwrap();
    let env = environment(temp.path(), "com.relaypulse.app.dev");
    let root = env.paths().data_root().to_owned();
    let state = startup::initialize(Ok(env)).0;
    let recovery = StartupRecovery::new(Some(root), &state);
    assert!(matches!(recovery.status().status, StartupPhase::Ready));
    assert!(recovery.migrate().is_err());
    let blocked = startup::initialize(Err(
        crate::shared::environment::EnvironmentError::DataDirectoryMismatch,
    ))
    .0;
    let recovery = StartupRecovery::new(None, &blocked);
    assert!(matches!(recovery.status().status, StartupPhase::Blocked));
    assert!(recovery.migrate().is_err());
}

#[test]
fn a_second_request_is_rejected_while_migration_is_running() {
    let temp = tempfile::tempdir().unwrap();
    let state = startup::initialize(Err(
        crate::shared::environment::EnvironmentError::DataDirectoryMismatch,
    ))
    .0;
    let recovery = StartupRecovery::new(Some(temp.path().to_owned()), &state);
    recovery.busy.store(true, Ordering::SeqCst);
    assert_eq!(
        recovery.migrate().unwrap_err().code,
        "startup_migration_busy"
    );
    assert!(matches!(recovery.status().status, StartupPhase::Migrating));
}
