//! 验证启动登记的业务表、schema 历史及配置表约束。
use super::support::{connection, defaults, ready, DEV};
use crate::shared::persistence::test_support::environment;

#[test]
fn creates_only_migrated_modules_and_named_schema_history() {
    let temp = tempfile::tempdir().unwrap();
    let app = ready(environment(temp.path(), DEV));
    let connection = connection(&app);
    let mut statement = connection
        .prepare("SELECT name FROM sqlite_schema WHERE type = 'table' ORDER BY name")
        .unwrap();
    let tables = statement
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    assert_eq!(
        tables,
        [
            "general_settings",
            "notification_rules",
            "notification_settings",
            "probe_records",
            "probe_states",
            "schema_migrations",
            "site_credentials",
            "site_models",
            "sites"
        ]
    );
    assert_eq!(
        connection
            .query_row(
                "SELECT name FROM schema_migrations WHERE position = 0",
                [],
                |row| row.get::<_, String>(0)
            )
            .unwrap(),
        "configuration"
    );
    let mut statement = connection
        .prepare("PRAGMA table_info(general_settings)")
        .unwrap();
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    assert_eq!(
        columns,
        [
            "singleton",
            "launch_at_startup",
            "continue_in_background",
            "show_tray_icon",
            "theme",
            "language",
            "history_retention_days"
        ]
    );
    for assignment in [
        "show_tray_icon = 2",
        "theme = 'blue'",
        "language = 'en-US'",
        "history_retention_days = 0",
        "history_retention_days = 3651",
    ] {
        assert!(connection
            .execute(&format!("UPDATE general_settings SET {assignment}"), [])
            .is_err());
    }
    assert_eq!(app.configuration.load().unwrap(), defaults());
    let schema_steps = connection
        .prepare("SELECT name FROM schema_migrations ORDER BY position")
        .unwrap()
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    assert_eq!(
        schema_steps,
        [
            "configuration",
            "notification_configuration",
            "site_configuration",
            "site_probe_records",
        ]
    );
    let record_columns = connection
        .prepare("PRAGMA table_info(probe_records)")
        .unwrap()
        .query_map([], |row| row.get::<_, String>(1))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    for field in [
        "site_name",
        "model_name",
        "started_at_ms",
        "request_endpoint",
        "policy_json",
        "result_kind",
        "input_tokens",
        "output_tokens",
        "total_tokens",
        "request_id",
        "body_complete",
        "capture_enabled",
    ] {
        assert!(
            record_columns.iter().any(|name| name == field),
            "missing {field}"
        );
    }
}
