use super::*;
use crate::{
    legacy_data,
    shared::persistence::test_support::environment,
    startup::{self, StartupState},
};

fn fixture() -> (tempfile::TempDir, PathBuf, Paths, SwitchJournal) {
    let temp = tempfile::tempdir().unwrap();
    let env = environment(temp.path(), "com.relaypulse.app.dev");
    let root = env.paths().data_root().to_owned();
    legacy_data::create(&root);
    let journal = SwitchJournal {
        format_version: 1,
        id: Uuid::new_v4(),
    };
    let paths = paths(&root, journal.id).unwrap();
    data_transfer::convert_offline(&root, &paths.converted).unwrap();
    write_journal(&root, &paths, &journal).unwrap();
    (temp, root, paths, journal)
}

#[test]
fn startup_restores_original_if_interrupted_between_renames() {
    let (temp, root, paths, _) = fixture();
    let original = fs::read(root.join("relaypulse.sqlite3")).unwrap();
    fs::rename(&root, &paths.backup).unwrap();
    let state = startup::initialize(Ok(environment(temp.path(), "com.relaypulse.app.dev"))).0;
    assert!(matches!(
        state,
        StartupState::Blocked(crate::startup::StartupError::Persistence(_))
    ));
    assert_eq!(fs::read(root.join("relaypulse.sqlite3")).unwrap(), original);
    assert!(paths.converted.exists());
    assert!(!paths.journal.exists());
}

#[test]
fn startup_recognizes_completed_replacement_and_keeps_old_backup() {
    let (temp, root, paths, _) = fixture();
    let original = fs::read(root.join("relaypulse.sqlite3")).unwrap();
    fs::rename(&root, &paths.backup).unwrap();
    fs::rename(&paths.converted, &root).unwrap();
    let state = startup::initialize(Ok(environment(temp.path(), "com.relaypulse.app.dev"))).0;
    assert!(matches!(state, StartupState::Ready(_)));
    assert_eq!(
        fs::read(paths.backup.join("relaypulse.sqlite3")).unwrap(),
        original
    );
    assert!(!paths.journal.exists());
}

#[test]
fn abandoned_switch_before_first_rename_preserves_original() {
    let (_temp, root, paths, _) = fixture();
    let original = fs::read(root.join("relaypulse.sqlite3")).unwrap();
    restore_interrupted(&root).unwrap();
    assert_eq!(fs::read(root.join("relaypulse.sqlite3")).unwrap(), original);
    assert!(!paths.journal.exists());
    assert!(paths.converted.exists());
}

#[test]
fn failed_second_rename_rolls_back_and_retains_converted_copy() {
    let (_temp, root, paths, _) = fixture();
    let original = fs::read(root.join("relaypulse.sqlite3")).unwrap();
    let result = activate(&root, &paths, |source, destination| {
        if source == paths.converted {
            Err(std::io::ErrorKind::PermissionDenied.into())
        } else {
            fs::rename(source, destination)
        }
    });
    assert!(result.unwrap_err().message.contains("已恢复旧数据"));
    assert_eq!(fs::read(root.join("relaypulse.sqlite3")).unwrap(), original);
    assert!(paths.converted.exists());
    assert!(!paths.journal.exists());
}

#[test]
fn failed_rollback_is_recovered_by_next_startup() {
    let (_temp, root, paths, _) = fixture();
    let original = fs::read(root.join("relaypulse.sqlite3")).unwrap();
    let result = activate(&root, &paths, |source, destination| {
        if source == root {
            fs::rename(source, destination)
        } else {
            Err(std::io::ErrorKind::PermissionDenied.into())
        }
    });
    assert!(result.is_err());
    assert!(paths.journal.exists());
    assert!(!root.exists());
    restore_interrupted(&root).unwrap();
    assert_eq!(fs::read(root.join("relaypulse.sqlite3")).unwrap(), original);
}

#[test]
fn corrupt_journal_and_ambiguous_folders_block_without_initializing_empty_data() {
    let (temp, root, paths, _) = fixture();
    fs::rename(&root, &paths.backup).unwrap();
    fs::write(&paths.journal, b"{broken").unwrap();
    let state = startup::initialize(Ok(environment(temp.path(), "com.relaypulse.app.dev"))).0;
    assert!(matches!(
        state,
        StartupState::Blocked(crate::startup::StartupError::DataMigration(_))
    ));
    assert!(!root.exists());
    assert!(paths.backup.join("relaypulse.sqlite3").exists());
}

#[test]
fn existing_backup_cannot_be_overwritten() {
    let (_temp, root, paths, _) = fixture();
    fs::create_dir(&paths.backup).unwrap();
    fs::write(paths.backup.join("retained"), b"existing backup").unwrap();
    assert!(activate(&root, &paths, |source, destination| fs::rename(
        source,
        destination
    ))
    .is_err());
    assert_eq!(
        fs::read(paths.backup.join("retained")).unwrap(),
        b"existing backup"
    );
    assert!(root.join("relaypulse.sqlite3").exists());
}

#[test]
fn missing_replacement_database_cannot_be_treated_as_a_fresh_install() {
    let (temp, root, paths, _) = fixture();
    fs::rename(&root, &paths.backup).unwrap();
    fs::rename(&paths.converted, &root).unwrap();
    fs::remove_file(root.join("relaypulse.sqlite3")).unwrap();
    let state = startup::initialize(Ok(environment(temp.path(), "com.relaypulse.app.dev"))).0;
    assert!(matches!(
        state,
        StartupState::Blocked(crate::startup::StartupError::DataMigration(_))
    ));
    assert!(!root.join("relaypulse.sqlite3").exists());
    assert!(paths.backup.join("relaypulse.sqlite3").exists());
    assert!(paths.journal.exists());
}
