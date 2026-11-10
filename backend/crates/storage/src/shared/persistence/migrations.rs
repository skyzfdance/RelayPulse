use super::PersistenceError;
use rusqlite::{Connection, TransactionBehavior};

/// Complete schema fragment owned by one capability. This is a baseline, never an upgrade step.
#[derive(Clone, Copy)]
pub struct Migration {
    pub name: &'static str,
    pub sql: &'static str,
}
pub const FORMAT_VERSION: i64 = 3;
const APPLICATION_ID: i64 = 0x52504c53;

pub fn schema_objects(connection: &Connection) -> rusqlite::Result<Vec<(String, String)>> {
    connection.prepare("SELECT name,sql FROM sqlite_schema WHERE sql IS NOT NULL AND name NOT LIKE 'sqlite_%' ORDER BY name")?
        .query_map([], |r| Ok((r.get(0)?,r.get(1)?)))?.collect()
}

/// Validate before opening for writes. Matching migration names alone is insufficient.
pub fn validate(connection: &Connection, baseline: &[Migration]) -> Result<(), PersistenceError> {
    let version: i64 = connection.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    let app: i64 = connection.query_row("PRAGMA application_id", [], |r| r.get(0))?;
    if version != FORMAT_VERSION || app != APPLICATION_ID {
        return Err(PersistenceError::UnsupportedSchema(
            "incompatible database format; use the explicit offline converter into a new directory",
        ));
    }
    let expected = Connection::open_in_memory()?;
    initialize(&expected, baseline)?;
    let manifest = |c: &Connection| -> rusqlite::Result<Vec<(i64, String, String)>> {
        c.prepare("SELECT position,name,sql FROM schema_migrations ORDER BY position")?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect()
    };
    if manifest(connection)? != manifest(&expected)?
        || schema_objects(connection)? != schema_objects(&expected)?
    {
        return Err(PersistenceError::UnsupportedSchema(
            "schema content differs from the complete baseline; automatic migration is disabled",
        ));
    }
    Ok(())
}
fn initialize(connection: &Connection, baseline: &[Migration]) -> rusqlite::Result<()> {
    connection.execute_batch("CREATE TABLE schema_migrations(position INTEGER PRIMARY KEY CHECK(position>=0),name TEXT NOT NULL UNIQUE,sql TEXT NOT NULL) STRICT;")?;
    for (position, fragment) in baseline.iter().enumerate() {
        connection.execute_batch(fragment.sql)?;
        connection.execute(
            "INSERT INTO schema_migrations VALUES (?1,?2,?3)",
            rusqlite::params![position as i64, fragment.name, fragment.sql],
        )?;
    }
    connection.pragma_update(None, "user_version", FORMAT_VERSION)?;
    connection.pragma_update(None, "application_id", APPLICATION_ID)?;
    Ok(())
}
pub(super) fn apply(
    connection: &mut Connection,
    baseline: &[Migration],
) -> Result<(), PersistenceError> {
    if !schema_objects(connection)?.is_empty() {
        return validate(connection, baseline);
    }
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    initialize(&transaction, baseline)?;
    transaction.commit()?;
    Ok(())
}
