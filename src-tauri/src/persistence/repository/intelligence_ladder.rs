use std::collections::HashSet;

use rusqlite::params;

use crate::{
    domain::TimestampMs,
    intelligence_ladder::{
        IntelligenceAutoFetchSettings, IntelligenceRankingEntry, IntelligenceSafeError,
    },
};

use super::{bump_revision, sqlite_u64, DatabaseError, Repository, Result};

#[derive(Debug, Clone, PartialEq)]
pub struct PersistedIntelligenceLadderState {
    pub revision: u64,
    pub entries: Vec<IntelligenceRankingEntry>,
    pub fetched_at: Option<TimestampMs>,
    pub last_error: Option<IntelligenceSafeError>,
    pub settings: IntelligenceAutoFetchSettings,
}

impl Repository {
    pub async fn intelligence_ladder_state(&self) -> Result<PersistedIntelligenceLadderState> {
        self.database()
            .call(|connection| {
                let revision = connection.query_row(
                    "SELECT revision FROM app_state WHERE singleton = 1",
                    [],
                    |row| row.get::<_, i64>(0),
                )?;
                let (fetched_at_ms, last_error_code, last_error_message) = connection.query_row(
                    "SELECT fetched_at_ms, last_error_code, last_error_message
                     FROM intelligence_ladder_snapshot WHERE singleton = 1",
                    [],
                    |row| {
                        Ok((
                            row.get::<_, Option<i64>>(0)?,
                            row.get::<_, Option<String>>(1)?,
                            row.get::<_, Option<String>>(2)?,
                        ))
                    },
                )?;
                let settings = connection.query_row(
                    "SELECT enabled, interval_minutes, fetch_on_app_start,
                            fetch_on_network_restore
                     FROM intelligence_ladder_settings WHERE singleton = 1",
                    [],
                    |row| {
                        Ok(IntelligenceAutoFetchSettings {
                            enabled: row.get(0)?,
                            interval_minutes: row.get(1)?,
                            fetch_on_app_start: row.get(2)?,
                            fetch_on_network_restore: row.get(3)?,
                        })
                    },
                )?;
                let mut statement = connection.prepare(
                    "SELECT rank, model, effort, iq
                     FROM intelligence_ladder_entries ORDER BY rank",
                )?;
                let entries = statement
                    .query_map([], |row| {
                        Ok(IntelligenceRankingEntry {
                            rank: row.get(0)?,
                            model: row.get(1)?,
                            effort: row.get(2)?,
                            iq: row.get(3)?,
                        })
                    })?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                let fetched_at = fetched_at_ms
                    .map(|value| {
                        TimestampMs::new(value).map_err(|_| DatabaseError::InvalidInput {
                            field: "fetched_at_ms",
                        })
                    })
                    .transpose()?;
                if (fetched_at.is_none() && !entries.is_empty())
                    || (fetched_at.is_some() && validate_snapshot_entries(&entries).is_err())
                {
                    return Err(DatabaseError::InvalidInput { field: "entries" });
                }
                let last_error = match (last_error_code, last_error_message) {
                    (None, None) => None,
                    (Some(code), Some(message)) => Some(IntelligenceSafeError { code, message }),
                    _ => {
                        return Err(DatabaseError::InvalidInput {
                            field: "last_error",
                        })
                    }
                };
                Ok(PersistedIntelligenceLadderState {
                    revision: sqlite_u64(revision, "revision")?,
                    entries,
                    fetched_at,
                    last_error,
                    settings,
                })
            })
            .await
    }

    pub async fn replace_intelligence_ladder_snapshot(
        &self,
        entries: Vec<IntelligenceRankingEntry>,
        fetched_at: TimestampMs,
    ) -> Result<u64> {
        validate_snapshot_entries(&entries)?;
        self.replace_intelligence_ladder_snapshot_inner(entries, fetched_at, None)
            .await
    }

    async fn replace_intelligence_ladder_snapshot_inner(
        &self,
        entries: Vec<IntelligenceRankingEntry>,
        fetched_at: TimestampMs,
        fail_after_entries: Option<usize>,
    ) -> Result<u64> {
        self.database()
            .call(move |connection| {
                let transaction = connection.transaction()?;
                transaction.execute("DELETE FROM intelligence_ladder_entries", [])?;
                for (index, entry) in entries.into_iter().enumerate() {
                    transaction.execute(
                        "INSERT INTO intelligence_ladder_entries (rank, model, effort, iq)
                         VALUES (?1, ?2, ?3, ?4)",
                        params![entry.rank, entry.model, entry.effort, entry.iq],
                    )?;
                    if fail_after_entries == Some(index + 1) {
                        return Err(DatabaseError::InvalidInput {
                            field: "failure_injection",
                        });
                    }
                }
                transaction.execute(
                    "UPDATE intelligence_ladder_snapshot
                     SET fetched_at_ms = ?1, last_error_code = NULL,
                         last_error_message = NULL, updated_at_ms = ?1
                     WHERE singleton = 1",
                    [fetched_at.as_i64()],
                )?;
                let revision = bump_revision(&transaction)?;
                transaction.commit()?;
                Ok(revision)
            })
            .await
    }

    pub async fn record_intelligence_ladder_error(
        &self,
        error: IntelligenceSafeError,
        now: TimestampMs,
    ) -> Result<u64> {
        self.database()
            .call(move |connection| {
                let transaction = connection.transaction()?;
                transaction.execute(
                    "UPDATE intelligence_ladder_snapshot
                     SET last_error_code = ?1, last_error_message = ?2, updated_at_ms = ?3
                     WHERE singleton = 1",
                    params![error.code, error.message, now.as_i64()],
                )?;
                let revision = bump_revision(&transaction)?;
                transaction.commit()?;
                Ok(revision)
            })
            .await
    }

    pub async fn update_intelligence_ladder_settings(
        &self,
        settings: IntelligenceAutoFetchSettings,
        now: TimestampMs,
    ) -> Result<u64> {
        self.database()
            .call(move |connection| {
                let transaction = connection.transaction()?;
                transaction.execute(
                    "UPDATE intelligence_ladder_settings
                     SET enabled = ?1, interval_minutes = ?2, fetch_on_app_start = ?3,
                         fetch_on_network_restore = ?4, updated_at_ms = ?5
                     WHERE singleton = 1",
                    params![
                        settings.enabled,
                        settings.interval_minutes,
                        settings.fetch_on_app_start,
                        settings.fetch_on_network_restore,
                        now.as_i64()
                    ],
                )?;
                let revision = bump_revision(&transaction)?;
                transaction.commit()?;
                Ok(revision)
            })
            .await
    }

    pub async fn touch_intelligence_ladder_revision(&self) -> Result<u64> {
        self.database()
            .call(|connection| {
                let transaction = connection.transaction()?;
                let revision = bump_revision(&transaction)?;
                transaction.commit()?;
                Ok(revision)
            })
            .await
    }
}

fn validate_snapshot_entries(entries: &[IntelligenceRankingEntry]) -> Result<()> {
    if entries.len() < 3 || entries.len() > 100 {
        return Err(DatabaseError::InvalidInput { field: "entries" });
    }
    let mut identities = HashSet::with_capacity(entries.len());
    let mut previous_iq = f64::INFINITY;
    for (index, entry) in entries.iter().enumerate() {
        let expected_rank =
            u32::try_from(index + 1).map_err(|_| DatabaseError::InvalidInput { field: "rank" })?;
        let identity = format!(
            "{}\u{0}{}",
            entry.model.to_lowercase(),
            entry.effort.to_lowercase()
        );
        if entry.rank != expected_rank
            || entry.model.is_empty()
            || entry.effort.is_empty()
            || !entry.iq.is_finite()
            || !(0.0..=200.0).contains(&entry.iq)
            || entry.iq > previous_iq
            || !identities.insert(identity)
        {
            return Err(DatabaseError::InvalidInput { field: "entries" });
        }
        previous_iq = entry.iq;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::{
        intelligence_ladder::IntelligenceAutoFetchSettings,
        persistence::{Database, Repository},
    };

    use super::*;

    fn entries(prefix: &str) -> Vec<IntelligenceRankingEntry> {
        (1..=3)
            .map(|rank| IntelligenceRankingEntry {
                rank,
                model: format!("{prefix}-{rank}"),
                effort: "high".to_owned(),
                iq: 100.0 - f64::from(rank),
            })
            .collect()
    }

    #[tokio::test(flavor = "current_thread")]
    async fn snapshot_settings_and_error_survive_repository_recreation() {
        let directory = tempfile::tempdir().expect("tempdir is created");
        let database_path = directory.path().join("intelligence-ladder.sqlite3");
        let fetched_at = TimestampMs::new(1_700_000_000_000).expect("timestamp is valid");
        let settings = IntelligenceAutoFetchSettings {
            enabled: true,
            interval_minutes: 180,
            fetch_on_app_start: true,
            fetch_on_network_restore: true,
        };
        {
            let repository = Repository::new(
                Database::open(&database_path).expect("database opens for writing"),
            );
            repository
                .replace_intelligence_ladder_snapshot(entries("first"), fetched_at)
                .await
                .expect("snapshot is stored");
            repository
                .update_intelligence_ladder_settings(settings.clone(), fetched_at)
                .await
                .expect("settings are stored");
            repository
                .record_intelligence_ladder_error(
                    IntelligenceSafeError {
                        code: "network".to_owned(),
                        message: "最近一次获取失败，当前显示上次成功数据。".to_owned(),
                    },
                    fetched_at,
                )
                .await
                .expect("error is stored");
        }

        let restored = Repository::new(
            Database::open(&database_path).expect("database reopens after restart"),
        )
        .intelligence_ladder_state()
        .await
        .expect("state is restored");
        assert_eq!(restored.entries, entries("first"));
        assert_eq!(restored.fetched_at, Some(fetched_at));
        assert_eq!(restored.settings, settings);
        assert_eq!(restored.last_error.expect("error exists").code, "network");
        assert_eq!(restored.revision, 3);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn failure_preserves_last_known_good_and_success_replaces_it_atomically() {
        let repository = Repository::new(Database::open_in_memory().expect("database opens"));
        let first_at = TimestampMs::new(1_700_000_000_000).expect("timestamp is valid");
        let second_at = TimestampMs::new(1_700_000_100_000).expect("timestamp is valid");
        repository
            .replace_intelligence_ladder_snapshot(entries("first"), first_at)
            .await
            .expect("first snapshot is stored");
        repository
            .record_intelligence_ladder_error(
                IntelligenceSafeError {
                    code: "invalid_snapshot".to_owned(),
                    message: "排名页面结构不兼容，已保留上次成功数据。".to_owned(),
                },
                second_at,
            )
            .await
            .expect("failure is stored");
        let failed = repository
            .intelligence_ladder_state()
            .await
            .expect("failed state loads");
        assert_eq!(failed.entries, entries("first"));
        assert_eq!(failed.fetched_at, Some(first_at));

        repository
            .replace_intelligence_ladder_snapshot(entries("second"), second_at)
            .await
            .expect("second snapshot is stored");
        let recovered = repository
            .intelligence_ladder_state()
            .await
            .expect("recovered state loads");
        assert_eq!(recovered.entries, entries("second"));
        assert_eq!(recovered.fetched_at, Some(second_at));
        assert!(recovered.last_error.is_none());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn invalid_entry_rolls_back_the_entire_snapshot_replacement() {
        let repository = Repository::new(Database::open_in_memory().expect("database opens"));
        let first_at = TimestampMs::new(1_700_000_000_000).expect("timestamp is valid");
        repository
            .replace_intelligence_ladder_snapshot(entries("first"), first_at)
            .await
            .expect("first snapshot is stored");
        assert!(repository
            .replace_intelligence_ladder_snapshot_inner(entries("second"), first_at, Some(1))
            .await
            .is_err());

        let restored = repository
            .intelligence_ladder_state()
            .await
            .expect("old snapshot remains readable");
        assert_eq!(restored.entries, entries("first"));
        assert_eq!(restored.revision, 1);
    }
}
