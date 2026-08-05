use rusqlite::Connection;
use rusqlite_migration::{Migrations, M};

use super::{DatabaseError, Result};

pub const LATEST_SCHEMA_VERSION: u32 = 2;

fn migrations() -> Migrations<'static> {
    Migrations::new(vec![
        M::up(include_str!("migrations/v1.sql")).foreign_key_check(),
        M::up(include_str!("migrations/v2.sql")).foreign_key_check(),
    ])
}

pub fn schema_version(connection: &Connection) -> Result<u32> {
    let version = connection.query_row("PRAGMA user_version", [], |row| row.get::<_, u32>(0))?;
    Ok(version)
}

pub(super) fn migrate_to_latest(connection: &mut Connection) -> Result<()> {
    let found = schema_version(connection)?;
    if found > LATEST_SCHEMA_VERSION {
        return Err(DatabaseError::SchemaTooNew {
            found,
            supported: LATEST_SCHEMA_VERSION,
        });
    }

    let migrations = migrations();
    migrations.validate()?;
    migrations.to_latest(connection)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use rusqlite::params;
    use rusqlite_migration::SchemaVersion;

    use super::*;
    use crate::persistence::Database;

    const EXPECTED_TABLES: &[&str] = &[
        "app_state",
        "credential_operations",
        "custom_probe_parameters",
        "general_settings",
        "latest_model_status",
        "models",
        "probe_captures",
        "probe_policies",
        "probe_results",
        "site_headers",
        "sites",
    ];

    #[test]
    fn migration_definition_is_valid() {
        assert_eq!(migrations().validate(), Ok(()));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn migrates_a_new_database_to_the_latest_schema() {
        let database = Database::open_in_memory().expect("database opens");
        let (version, tables, site_columns, probe_columns) = database
            .call(|connection| {
                let version = schema_version(connection)?;
                let mut statement = connection.prepare(
                    "SELECT name FROM sqlite_schema \
                     WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
                )?;
                let tables = statement
                    .query_map([], |row| row.get::<_, String>(0))?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                let site_columns = table_columns(connection, "sites")?;
                let probe_columns = table_columns(connection, "probe_results")?;
                Ok((version, tables, site_columns, probe_columns))
            })
            .await
            .expect("schema can be inspected");

        assert_eq!(version, LATEST_SCHEMA_VERSION);
        assert_eq!(
            tables,
            EXPECTED_TABLES
                .iter()
                .map(|table| (*table).to_owned())
                .collect::<Vec<_>>()
        );
        assert!(!site_columns.contains(&"first_token_timeout_ms".to_owned()));
        assert!(!site_columns.contains(&"total_timeout_ms".to_owned()));
        assert!(site_columns.contains(&"slow_first_token_ms".to_owned()));
        assert!(site_columns.contains(&"slow_total_ms".to_owned()));
        assert!(probe_columns.contains(&"request_method".to_owned()));
        assert!(probe_columns.contains(&"endpoint".to_owned()));
        assert!(probe_columns.contains(&"probe_mode".to_owned()));
    }

    fn table_columns(connection: &Connection, table: &str) -> rusqlite::Result<Vec<String>> {
        let mut statement = connection.prepare(&format!("PRAGMA table_info({table})"))?;
        let columns = statement
            .query_map([], |row| row.get::<_, String>(1))?
            .collect();
        columns
    }

    #[tokio::test(flavor = "current_thread")]
    async fn fresh_schema_enables_foreign_keys_and_seeds_settings() {
        let database = Database::open_in_memory().expect("database opens");
        let (foreign_keys, settings) = database
            .call(|connection| {
                let foreign_keys =
                    connection.query_row("PRAGMA foreign_keys", [], |row| row.get::<_, bool>(0))?;
                let settings = connection.query_row(
                    "SELECT launch_at_startup, continue_in_background, tray_action, theme, \
                            history_retention_days, diagnostic_capture_enabled \
                     FROM general_settings WHERE singleton = 1",
                    [],
                    |row| {
                        Ok((
                            row.get::<_, bool>(0)?,
                            row.get::<_, bool>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, String>(3)?,
                            row.get::<_, u16>(4)?,
                            row.get::<_, bool>(5)?,
                        ))
                    },
                )?;
                Ok((foreign_keys, settings))
            })
            .await
            .expect("schema defaults can be read");

        assert!(foreign_keys);
        assert_eq!(
            settings,
            (
                false,
                true,
                "panel".to_owned(),
                "system".to_owned(),
                30,
                true
            )
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn fresh_schema_accepts_all_supported_site_protocols() {
        let database = Database::open_in_memory().expect("database opens");
        let protocols = database
            .call(|connection| {
                for (sort_order, protocol) in [
                    "openai-compatible",
                    "anthropic",
                    "openai-responses",
                    "custom-http",
                ]
                .into_iter()
                .enumerate()
                {
                    let sort_order = i64::try_from(sort_order).expect("fixture order is valid");
                    connection.execute(
                        "INSERT INTO sites (
                           id, name, protocol, base_url, sort_order, created_at_ms, updated_at_ms
                         ) VALUES (?1, ?1, ?2, 'https://example.test/v1', ?3, 0, 0)",
                        params![format!("site-{sort_order}"), protocol, sort_order],
                    )?;
                }
                let mut statement =
                    connection.prepare("SELECT protocol FROM sites ORDER BY sort_order")?;
                let protocols = statement
                    .query_map([], |row| row.get::<_, String>(0))?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                Ok(protocols)
            })
            .await
            .expect("all protocol variants are persisted");

        assert_eq!(
            protocols,
            vec![
                "openai-compatible".to_owned(),
                "anthropic".to_owned(),
                "openai-responses".to_owned(),
                "custom-http".to_owned(),
            ]
        );
    }

    #[test]
    fn upgrades_the_original_v1_tray_constraint_without_losing_settings() {
        let mut connection = Connection::open_in_memory().expect("database opens");
        connection
            .execute_batch(
                "CREATE TABLE general_settings (
                   singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
                   launch_at_startup INTEGER NOT NULL DEFAULT 0 CHECK (launch_at_startup IN (0, 1)),
                   continue_in_background INTEGER NOT NULL DEFAULT 1 CHECK (continue_in_background IN (0, 1)),
                   tray_action TEXT NOT NULL DEFAULT 'panel' CHECK (tray_action = 'panel'),
                   theme TEXT NOT NULL DEFAULT 'system' CHECK (theme IN ('system', 'light', 'dark')),
                   language TEXT NOT NULL DEFAULT 'zh-CN' CHECK (language = 'zh-CN'),
                   history_retention_days INTEGER NOT NULL DEFAULT 30 CHECK (history_retention_days BETWEEN 1 AND 3650),
                   diagnostic_capture_enabled INTEGER NOT NULL DEFAULT 1 CHECK (diagnostic_capture_enabled IN (0, 1)),
                   updated_at_ms INTEGER NOT NULL CHECK (updated_at_ms >= 0)
                 ) STRICT;
                 INSERT INTO general_settings (
                   singleton, launch_at_startup, continue_in_background, tray_action, theme,
                   language, history_retention_days, diagnostic_capture_enabled, updated_at_ms
                 ) VALUES (1, 1, 0, 'panel', 'dark', 'zh-CN', 90, 0, 1234);
                 PRAGMA user_version = 1;",
            )
            .expect("legacy schema is created");

        migrate_to_latest(&mut connection).expect("legacy schema migrates");
        connection
            .execute(
                "UPDATE general_settings SET tray_action = 'disabled' WHERE singleton = 1",
                [],
            )
            .expect("disabled tray action is accepted");

        let settings = connection
            .query_row(
                "SELECT launch_at_startup, continue_in_background, tray_action, theme,
                        history_retention_days, diagnostic_capture_enabled, updated_at_ms
                 FROM general_settings WHERE singleton = 1",
                [],
                |row| {
                    Ok((
                        row.get::<_, bool>(0)?,
                        row.get::<_, bool>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, u16>(4)?,
                        row.get::<_, bool>(5)?,
                        row.get::<_, i64>(6)?,
                    ))
                },
            )
            .expect("migrated settings are readable");

        assert_eq!(schema_version(&connection).expect("version is readable"), 2);
        assert_eq!(
            settings,
            (
                true,
                false,
                "disabled".to_owned(),
                "dark".to_owned(),
                90,
                false,
                1234,
            )
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn rejects_cross_site_model_relations() {
        let database = Database::open_in_memory().expect("database opens");
        let policy_result = database
            .call(|connection| {
                insert_site(connection, "site-a", 0)?;
                insert_site(connection, "site-b", 1)?;
                insert_model(connection, "model-a", "site-a", 0)?;

                Ok(connection.execute(
                    "INSERT INTO probe_policies (
                       id, site_id, model_id, mode, max_output_tokens, prompt,
                       created_at_ms, updated_at_ms
                     ) VALUES (?1, ?2, ?3, 'inherit', 8, 'OK', 0, 0)",
                    params!["policy-a", "site-b", "model-a"],
                ))
            })
            .await
            .expect("database call succeeds");

        assert!(policy_result.is_err());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn deleting_a_site_cascades_owned_data_but_preserves_saga_records() {
        let database = Database::open_in_memory().expect("database opens");
        let remaining = database
            .call(|connection| {
                insert_site(connection, "site-a", 0)?;
                insert_model(connection, "model-a", "site-a", 0)?;
                connection.execute(
                    "INSERT INTO probe_policies (
                       id, site_id, model_id, mode, max_output_tokens, prompt,
                       created_at_ms, updated_at_ms
                     ) VALUES ('policy-a', 'site-a', 'model-a', 'inherit', 8, 'OK', 0, 0)",
                    [],
                )?;
                connection.execute(
                    "INSERT INTO custom_probe_parameters (
                       id, policy_id, parameter_key, value_type, value_json, sort_order
                     ) VALUES ('parameter-a', 'policy-a', 'temperature', 'number', '0', 0)",
                    [],
                )?;
                connection.execute(
                    "INSERT INTO site_headers (
                       id, site_id, header_name, value_template, sort_order
                     ) VALUES ('header-a', 'site-a', 'X-Test', 'value', 0)",
                    [],
                )?;
                connection.execute(
                    "INSERT INTO probe_results (
                       id, site_id, model_id, run_kind, started_at_ms, status, revision
                     ) VALUES ('result-a', 'site-a', 'model-a', 'manual', 0, 'healthy', 1)",
                    [],
                )?;
                connection.execute(
                    "INSERT INTO probe_captures (probe_result_id) VALUES ('result-a')",
                    [],
                )?;
                connection.execute(
                    "INSERT INTO latest_model_status (
                       model_id, probe_result_id, status, revision, updated_at_ms
                     ) VALUES ('model-a', 'result-a', 'healthy', 1, 0)",
                    [],
                )?;
                connection.execute(
                    "INSERT INTO credential_operations (
                       id, site_id, action, status, created_at_ms, updated_at_ms
                     ) VALUES ('operation-a', 'site-a', 'delete', 'cleanup_pending', 0, 0)",
                    [],
                )?;

                connection.execute("DELETE FROM sites WHERE id = 'site-a'", [])?;

                let owned_rows = connection.query_row(
                    "SELECT
                       (SELECT COUNT(*) FROM sites) +
                       (SELECT COUNT(*) FROM models) +
                       (SELECT COUNT(*) FROM probe_policies) +
                       (SELECT COUNT(*) FROM custom_probe_parameters) +
                       (SELECT COUNT(*) FROM site_headers) +
                       (SELECT COUNT(*) FROM probe_results) +
                       (SELECT COUNT(*) FROM probe_captures) +
                       (SELECT COUNT(*) FROM latest_model_status)",
                    [],
                    |row| row.get::<_, i64>(0),
                )?;
                let saga_rows = connection.query_row(
                    "SELECT COUNT(*) FROM credential_operations",
                    [],
                    |row| row.get::<_, i64>(0),
                )?;
                Ok((owned_rows, saga_rows))
            })
            .await
            .expect("cascade operation succeeds");

        assert_eq!(remaining, (0, 1));
    }

    #[test]
    fn a_failed_migration_rolls_back_and_the_file_reopens_cleanly() {
        let directory = tempfile::tempdir().expect("tempdir created");
        let path = directory.path().join("migration-failure.sqlite3");
        let mut connection = Connection::open(&path).expect("database opens");
        connection
            .execute_batch(
                "CREATE TABLE existing_data (id INTEGER PRIMARY KEY, value TEXT NOT NULL); \
                 INSERT INTO existing_data (id, value) VALUES (1, 'before');",
            )
            .expect("fixture is created");
        let migrations = Migrations::new(vec![M::up(
            "CREATE TABLE should_roll_back (id INTEGER PRIMARY KEY); \
             UPDATE existing_data SET value = 'during' WHERE id = 1; \
             INSERT INTO missing_table (id) VALUES (1);",
        )]);

        assert!(migrations.to_latest(&mut connection).is_err());
        drop(connection);

        let connection = Connection::open(&path).expect("database reopens");
        let table_count = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_schema WHERE type = 'table' AND name = 'should_roll_back'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .expect("schema can be inspected");
        assert_eq!(table_count, 0);
        let value = connection
            .query_row("SELECT value FROM existing_data WHERE id = 1", [], |row| {
                row.get::<_, String>(0)
            })
            .expect("original data can be read");
        assert_eq!(value, "before");
        assert_eq!(
            migrations.current_version(&connection),
            Ok(SchemaVersion::NoneSet)
        );
    }

    #[test]
    fn rejects_a_schema_newer_than_the_application() {
        let mut connection = Connection::open_in_memory().expect("database opens");
        connection
            .pragma_update(None, "user_version", LATEST_SCHEMA_VERSION + 1)
            .expect("user_version is set");

        assert!(matches!(
            migrate_to_latest(&mut connection),
            Err(DatabaseError::SchemaTooNew {
                found,
                supported
            }) if found == LATEST_SCHEMA_VERSION + 1 && supported == LATEST_SCHEMA_VERSION
        ));
    }

    #[test]
    fn a_corrupt_database_is_not_replaced() {
        let directory = tempfile::tempdir().expect("tempdir created");
        let path = directory.path().join("relaypulse.sqlite3");
        let original = b"not-a-sqlite-database";
        fs::write(&path, original).expect("corrupt fixture is written");

        assert!(Database::open(&path).is_err());
        assert_eq!(fs::read(&path).expect("fixture can be read"), original);
    }

    fn insert_site(connection: &Connection, id: &str, sort_order: u32) -> Result<()> {
        connection.execute(
            "INSERT INTO sites (
               id, name, protocol, base_url, sort_order, created_at_ms, updated_at_ms
             ) VALUES (?1, ?1, 'openai-compatible', 'https://example.test/v1', ?2, 0, 0)",
            params![id, sort_order],
        )?;
        Ok(())
    }

    fn insert_model(
        connection: &Connection,
        id: &str,
        site_id: &str,
        sort_order: u32,
    ) -> Result<()> {
        connection.execute(
            "INSERT INTO models (
               id, site_id, model_key, source, sort_order, created_at_ms, updated_at_ms
             ) VALUES (?1, ?2, ?1, 'manual', ?3, 0, 0)",
            params![id, site_id, sort_order],
        )?;
        Ok(())
    }
}
