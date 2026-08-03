mod config;
mod credentials;
mod probes;
#[cfg(test)]
pub(crate) mod test_support;

use rusqlite::{Connection, OptionalExtension, Transaction};

use crate::domain::{EntityId, ProbeStatus, TimestampMs};

use super::{Database, DatabaseError, Result};

#[derive(Clone)]
pub struct Repository {
    database: Database,
}

impl Repository {
    pub fn new(database: Database) -> Self {
        Self { database }
    }

    pub fn database(&self) -> &Database {
        &self.database
    }

    pub async fn revision(&self) -> Result<u64> {
        self.database.call(current_revision).await
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeCursor {
    pub started_at: TimestampMs,
    pub id: EntityId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbePageRequest {
    pub site_id: Option<EntityId>,
    pub model_id: Option<EntityId>,
    pub status: Option<ProbeStatus>,
    pub before: Option<ProbeCursor>,
    pub limit: u16,
}

impl Default for ProbePageRequest {
    fn default() -> Self {
        Self {
            site_id: None,
            model_id: None,
            status: None,
            before: None,
            limit: 20,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbePage {
    pub records: Vec<crate::domain::ProbeResult>,
    pub next_cursor: Option<ProbeCursor>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CleanupResult {
    pub deleted: u64,
    pub revision: u64,
}

fn current_revision(connection: &mut Connection) -> Result<u64> {
    let value = connection.query_row(
        "SELECT revision FROM app_state WHERE singleton = 1",
        [],
        |row| row.get::<_, i64>(0),
    )?;
    sqlite_u64(value, "revision")
}

fn bump_revision(transaction: &Transaction<'_>) -> Result<u64> {
    transaction.execute(
        "UPDATE app_state SET revision = revision + 1 WHERE singleton = 1",
        [],
    )?;
    let value = transaction.query_row(
        "SELECT revision FROM app_state WHERE singleton = 1",
        [],
        |row| row.get::<_, i64>(0),
    )?;
    sqlite_u64(value, "revision")
}

fn unchanged_revision(transaction: &Transaction<'_>) -> Result<u64> {
    let value = transaction.query_row(
        "SELECT revision FROM app_state WHERE singleton = 1",
        [],
        |row| row.get::<_, i64>(0),
    )?;
    sqlite_u64(value, "revision")
}

fn sqlite_i64(value: u64, field: &'static str) -> Result<i64> {
    i64::try_from(value).map_err(|_| DatabaseError::InvalidInput { field })
}

fn optional_sqlite_i64(value: Option<u64>, field: &'static str) -> Result<Option<i64>> {
    value.map(|value| sqlite_i64(value, field)).transpose()
}

fn sqlite_u64(value: i64, field: &'static str) -> Result<u64> {
    u64::try_from(value).map_err(|_| DatabaseError::InvalidInput { field })
}

fn require_changed(changed: usize, entity: &'static str) -> Result<()> {
    if changed == 0 {
        return Err(DatabaseError::NotFound { entity });
    }
    Ok(())
}

fn entity_exists(
    transaction: &Transaction<'_>,
    table: &'static str,
    id: &EntityId,
) -> Result<bool> {
    let sql = match table {
        "sites" => "SELECT 1 FROM sites WHERE id = ?1",
        "models" => "SELECT 1 FROM models WHERE id = ?1",
        _ => return Err(DatabaseError::InvalidInput { field: "table" }),
    };
    Ok(transaction
        .query_row(sql, [id], |_| Ok(()))
        .optional()?
        .is_some())
}
