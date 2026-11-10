//! Frozen pre-IQ data shared by startup upgrade and registered-command tests.
use rusqlite::Connection;
use std::{fs, path::Path};

pub(crate) fn create(root: &Path) {
    fs::create_dir_all(root).unwrap();
    let connection = Connection::open(root.join("relaypulse.sqlite3")).unwrap();
    connection.execute_batch("CREATE TABLE schema_migrations (position INTEGER PRIMARY KEY CHECK (position >= 0), name TEXT NOT NULL UNIQUE) STRICT;").unwrap();
    for (position, (name, sql)) in [
        (
            "configuration",
            include_str!("../../src/application/data_transfer/legacy/configuration.sql"),
        ),
        (
            "notification_configuration",
            include_str!("../../src/application/data_transfer/legacy/notifications.sql"),
        ),
        (
            "site_configuration",
            include_str!("../../src/application/data_transfer/legacy/sites.sql"),
        ),
        (
            "site_probe_records",
            include_str!("../../src/application/data_transfer/legacy/monitoring.sql"),
        ),
        (
            "operation_history",
            include_str!("../../src/application/data_transfer/legacy/operation_logs.sql"),
        ),
    ]
    .into_iter()
    .enumerate()
    {
        connection.execute_batch(sql).unwrap();
        connection
            .execute(
                "INSERT INTO schema_migrations VALUES (?1, ?2)",
                rusqlite::params![position as i64, name],
            )
            .unwrap();
    }
    connection
        .execute(
            "UPDATE general_settings SET theme='dark',history_retention_days=77",
            [],
        )
        .unwrap();
    fs::create_dir(root.join("logs")).unwrap();
    fs::write(
        root.join("logs/retained.jsonl"),
        b"original diagnostic bytes\n",
    )
    .unwrap();
}
