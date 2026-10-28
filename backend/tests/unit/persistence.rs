use super::{test_support::environment, *};
use std::fs;

const DEV: &str = "com.relaypulse.app.dev";
// 基础设施测试只使用测试 SQL，不依赖真实业务模块。
const EXAMPLE: Migration = Migration {
    name: "example",
    sql: "CREATE TABLE example (value TEXT NOT NULL); INSERT INTO example VALUES ('initial');",
};

#[test]
fn named_steps_run_once_and_preserve_data_on_reopen() {
    let temp = tempfile::tempdir().unwrap();
    let env = environment(temp.path(), DEV);
    let database = Database::open(env.paths(), &[EXAMPLE]).unwrap();
    database
        .with_connection(|connection| {
            assert!(connection.query_row("PRAGMA foreign_keys", [], |row| row.get::<_, bool>(0))?);
            connection.execute("UPDATE example SET value = 'saved'", [])?;
            Ok(())
        })
        .unwrap();
    drop(database);
    let database = Database::open(env.paths(), &[EXAMPLE]).unwrap();
    database
        .with_connection(|connection| {
            assert_eq!(
                connection.query_row("SELECT value FROM example", [], |row| row
                    .get::<_, String>(0))?,
                "saved"
            );
            assert_eq!(
                connection.query_row("SELECT name FROM schema_migrations", [], |row| row
                    .get::<_, String>(0))?,
                "example"
            );
            Ok(())
        })
        .unwrap();
}

#[test]
fn new_named_step_is_recorded_and_failure_rolls_back_its_changes() {
    let temp = tempfile::tempdir().unwrap();
    let env = environment(temp.path(), DEV);
    drop(Database::open(env.paths(), &[EXAMPLE]).unwrap());
    let failed = Migration { name: "example_notes", sql: "CREATE TABLE notes (value TEXT); UPDATE example SET value = 'changed'; INSERT INTO missing VALUES (1);" };
    assert!(Database::open(env.paths(), &[EXAMPLE, failed]).is_err());
    let database = Database::open(env.paths(), &[EXAMPLE]).unwrap();
    database
        .with_connection(|connection| {
            assert_eq!(
                connection.query_row("SELECT value FROM example", [], |row| row
                    .get::<_, String>(0))?,
                "initial"
            );
            assert_eq!(
                connection.query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| row
                    .get::<_, i64>(0))?,
                1
            );
            assert_eq!(
                connection.query_row(
                    "SELECT COUNT(*) FROM sqlite_schema WHERE name = 'notes'",
                    [],
                    |row| row.get::<_, i64>(0)
                )?,
                0
            );
            Ok(())
        })
        .unwrap();
    drop(database);
    let next = Migration {
        name: "example_notes",
        sql: "CREATE TABLE notes (value TEXT);",
    };
    drop(Database::open(env.paths(), &[EXAMPLE, next]).unwrap());
    assert!(matches!(
        Database::open(env.paths(), &[EXAMPLE]),
        Err(PersistenceError::UnsupportedSchema(_))
    ));
    assert!(matches!(
        Database::open(env.paths(), &[next, EXAMPLE]),
        Err(PersistenceError::UnsupportedSchema(_))
    ));
}

#[test]
fn failed_first_initialization_rolls_back_schema_and_history() {
    let temp = tempfile::tempdir().unwrap();
    let env = environment(temp.path(), DEV);
    let failed = Migration {
        name: "broken",
        sql: "CREATE TABLE example(value TEXT); INSERT INTO missing VALUES (1);",
    };
    assert!(Database::open(env.paths(), &[failed]).is_err());
    // 失败后整个文件仍可按空库重新初始化，不残留半张表或执行记录。
    drop(Database::open(env.paths(), &[EXAMPLE]).unwrap());
}

#[test]
fn untracked_and_corrupt_databases_are_not_replaced() {
    let temp = tempfile::tempdir().unwrap();
    let env = environment(temp.path(), DEV);
    fs::create_dir_all(env.paths().data_root()).unwrap();
    let path = env.paths().data_root().join("relaypulse.sqlite3");
    let connection = Connection::open(&path).unwrap();
    connection
        .execute_batch("CREATE TABLE existing(value TEXT); INSERT INTO existing VALUES ('keep');")
        .unwrap();
    drop(connection);
    let before = fs::read(&path).unwrap();
    assert!(matches!(
        Database::open(env.paths(), &[EXAMPLE]),
        Err(PersistenceError::UnsupportedSchema(_))
    ));
    assert_eq!(fs::read(&path).unwrap(), before);
    fs::write(&path, b"not a SQLite database").unwrap();
    assert!(Database::open(env.paths(), &[EXAMPLE]).is_err());
    assert_eq!(fs::read(&path).unwrap(), b"not a SQLite database");
}

#[cfg(unix)]
#[test]
fn database_and_sidecar_links_cannot_cross_environment_boundaries() {
    use std::os::unix::fs::symlink;
    for name in [
        "relaypulse.sqlite3",
        "relaypulse.sqlite3-wal",
        "relaypulse.sqlite3-shm",
        "relaypulse.sqlite3-journal",
    ] {
        let temp = tempfile::tempdir().unwrap();
        let dev = environment(temp.path(), DEV);
        let prod = environment(temp.path(), "com.relaypulse.app");
        fs::create_dir_all(dev.paths().data_root()).unwrap();
        fs::create_dir_all(prod.paths().data_root()).unwrap();
        let target = prod.paths().data_root().join(name);
        fs::write(&target, b"untouched").unwrap();
        symlink(&target, dev.paths().data_root().join(name)).unwrap();
        assert!(matches!(
            Database::open(dev.paths(), &[EXAMPLE]),
            Err(PersistenceError::PathConflict(_))
        ));
        assert_eq!(fs::read(target).unwrap(), b"untouched");
    }
}

#[cfg(unix)]
#[test]
fn database_hard_links_are_rejected_before_opening() {
    let temp = tempfile::tempdir().unwrap();
    let dev = environment(temp.path(), DEV);
    let prod = environment(temp.path(), "com.relaypulse.app");
    drop(Database::open(prod.paths(), &[EXAMPLE]).unwrap());
    fs::create_dir_all(dev.paths().data_root()).unwrap();
    let target = prod.paths().data_root().join("relaypulse.sqlite3");
    let before = fs::read(&target).unwrap();
    fs::hard_link(&target, dev.paths().data_root().join("relaypulse.sqlite3")).unwrap();
    assert!(matches!(
        Database::open(dev.paths(), &[EXAMPLE]),
        Err(PersistenceError::PathConflict(_))
    ));
    assert_eq!(fs::read(target).unwrap(), before);
}
