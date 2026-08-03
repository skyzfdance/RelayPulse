mod database;
mod migrations;
pub(crate) mod repository;
mod sql_types;

pub use database::Database;
pub use migrations::{schema_version, LATEST_SCHEMA_VERSION};
pub use repository::{CleanupResult, ProbeCursor, ProbePage, ProbePageRequest, Repository};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum DatabaseError {
    #[error("database worker is unavailable")]
    WorkerUnavailable,
    #[error("database worker stopped before returning a result")]
    WorkerStopped,
    #[error("database schema version {found} is newer than supported version {supported}")]
    SchemaTooNew { found: u32, supported: u32 },
    #[error("database operation failed")]
    Sqlite(#[source] rusqlite::Error),
    #[error("database migration failed")]
    Migration(#[source] rusqlite_migration::Error),
    #[error("database worker thread panicked")]
    WorkerPanicked,
    #[error("invalid persistence input for {field}")]
    InvalidInput { field: &'static str },
    #[error("persisted {entity} was not found")]
    NotFound { entity: &'static str },
}

impl From<rusqlite::Error> for DatabaseError {
    fn from(value: rusqlite::Error) -> Self {
        Self::Sqlite(value)
    }
}

impl From<rusqlite_migration::Error> for DatabaseError {
    fn from(value: rusqlite_migration::Error) -> Self {
        Self::Migration(value)
    }
}

pub type Result<T> = std::result::Result<T, DatabaseError>;
