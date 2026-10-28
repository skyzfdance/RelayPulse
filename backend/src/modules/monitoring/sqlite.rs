//! 当前状态持久化与一次正式完成的事务边界；历史 SQL 由 records 提供。
use super::{
    health::{Health, HealthState},
    records::{self, CompletedRecord, RecordStore},
};
use crate::shared::persistence::{Database, Migration, PersistenceError};
use rusqlite::params;
use std::{collections::HashMap, sync::Arc};

pub(crate) const SCHEMA: Migration = Migration {
    name: "site_probe_records",
    sql: include_str!("monitoring.sql"),
};
#[derive(Clone)]
pub(super) struct MonitoringStore {
    database: Arc<Database>,
    workers: tokio::sync::watch::Sender<usize>,
}
impl MonitoringStore {
    pub(super) fn new(database: Arc<Database>) -> Self {
        Self {
            database,
            workers: tokio::sync::watch::channel(0).0,
        }
    }
    pub(super) fn records(&self) -> RecordStore {
        RecordStore::new(self.database.clone())
    }

    /// 等待方被取消时，已经派发的 SQLite 工作仍由租约计数，退出必须等它完成。
    pub(super) async fn blocking<T: Send + 'static>(
        &self,
        work: impl FnOnce() -> T + Send + 'static,
    ) -> Result<T, tokio::task::JoinError> {
        self.workers.send_modify(|count| *count += 1);
        let lease = WorkerLease(self.workers.clone());
        tokio::task::spawn_blocking(move || {
            let _lease = lease;
            work()
        })
        .await
    }

    pub(super) async fn drained(&self) {
        let mut count = self.workers.subscribe();
        while *count.borrow_and_update() != 0 {
            if count.changed().await.is_err() {
                break;
            }
        }
    }

    pub(super) fn states(
        &self,
        model_ids: &[String],
    ) -> Result<HashMap<String, HealthState>, PersistenceError> {
        self.database.with_connection(|connection| {
            let mut statement = connection.prepare(
                "SELECT model_id,health,first_token_ms,duration_ms,failures,successes,finished_at_ms
                 FROM probe_states WHERE model_id=?1",
            )?;
            let mut states = HashMap::new();
            for id in model_ids {
                let rows = statement.query_map([id], |row| {
                    let health = match row.get::<_, String>(1)?.as_str() {
                        "healthy" => Health::Healthy,
                        "warning" => Health::Warning,
                        "error" => Health::Error,
                        "muted" => Health::Muted,
                        _ => return Err(rusqlite::Error::InvalidQuery),
                    };
                    Ok((
                        row.get(0)?,
                        HealthState {
                            health,
                            first_token_ms: row
                                .get::<_, Option<i64>>(2)?
                                .map(read_integer)
                                .transpose()?,
                            duration_ms: Some(read_integer(row.get(3)?)?),
                            failures: row.get(4)?,
                            successes: row.get(5)?,
                            finished_at_ms: read_integer(row.get(6)?)?,
                        },
                    ))
                })?;
                for row in rows {
                    let (id, state) = row?;
                    states.insert(id, state);
                }
            }
            Ok(states)
        })
    }

    pub(super) fn complete(
        &self,
        record: &CompletedRecord,
        state: &HealthState,
        retention_days: u16,
    ) -> Result<(), PersistenceError> {
        self.database.with_connection(|connection| {
            let tx = connection.transaction()?;
            records::insert(&tx, record)?;
            let health = match state.health {
                Health::Healthy=>"healthy", Health::Warning=>"warning",
                Health::Error=>"error", Health::Muted=>"muted",
            };
            tx.execute(
                "INSERT INTO probe_states (model_id,health,first_token_ms,duration_ms,failures,successes,finished_at_ms)
                 VALUES (?1,?2,?3,?4,?5,?6,?7)
                 ON CONFLICT(model_id) DO UPDATE SET health=excluded.health,
                 first_token_ms=excluded.first_token_ms,duration_ms=excluded.duration_ms,
                 failures=excluded.failures,successes=excluded.successes,finished_at_ms=excluded.finished_at_ms",
                params![record.context.model_id,health,state.first_token_ms.map(sql_integer).transpose()?,
                    sql_integer(record.result.duration_ms)?,state.failures,state.successes,sql_integer(state.finished_at_ms)?],
            )?;
            records::prune(&tx, state.finished_at_ms.saturating_sub(u64::from(retention_days)*86_400_000))?;
            tx.commit()
        })
    }
}

struct WorkerLease(tokio::sync::watch::Sender<usize>);
impl Drop for WorkerLease {
    fn drop(&mut self) {
        self.0.send_modify(|count| *count -= 1);
    }
}

fn read_integer(value: i64) -> rusqlite::Result<u64> {
    u64::try_from(value).map_err(|_| rusqlite::Error::InvalidQuery)
}
pub(super) fn sql_integer(value: u64) -> rusqlite::Result<i64> {
    i64::try_from(value).map_err(|_| rusqlite::Error::InvalidQuery)
}
