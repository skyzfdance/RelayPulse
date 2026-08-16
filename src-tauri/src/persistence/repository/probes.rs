use rusqlite::{params, OptionalExtension, TransactionBehavior};

use crate::domain::{
    LatestModelStatus, ProbeCapture, ProbeResult, ProbeStatus, ProbeTimingPolicy, TimestampMs,
    TokenUsage,
};

use super::{
    bump_revision, optional_sqlite_i64, sqlite_i64, unchanged_revision, CleanupResult,
    DatabaseError, ProbeCursor, ProbePage, ProbePageRequest, ProbeRecordOutcome, Repository,
    Result,
};

impl Repository {
    pub async fn record_probe(
        &self,
        result: ProbeResult,
        capture: ProbeCapture,
        timing: ProbeTimingPolicy,
        next_probe_at: Option<TimestampMs>,
    ) -> Result<ProbeRecordOutcome> {
        if capture.probe_result_id != result.id {
            return Err(DatabaseError::InvalidInput {
                field: "probe_record",
            });
        }
        let finished_at = result.finished_at.ok_or(DatabaseError::InvalidInput {
            field: "finished_at",
        })?;

        self.database
            .call(move |connection| {
                let transaction =
                    connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
                let previous_status = transaction
                    .query_row(
                        "SELECT model_id, probe_result_id, status, consecutive_failures,
                                consecutive_successes, next_probe_at_ms, revision, updated_at_ms
                         FROM latest_model_status WHERE model_id = ?1",
                        [&result.model_id],
                        latest_status_from_row,
                    )
                    .optional()?;
                let became_latest = previous_status.as_ref().is_none_or(|previous| {
                    finished_at > previous.updated_at
                        || (finished_at == previous.updated_at
                            && previous
                                .probe_result_id
                                .as_ref()
                                .is_none_or(|previous_id| result.id > *previous_id))
                });
                let revision = bump_revision(&transaction)?;
                let revision_sql = sqlite_i64(revision, "revision")?;
                let latest_status = if became_latest {
                    aggregate_latest_status(
                        previous_status.as_ref(),
                        result.status,
                        &timing,
                        result.model_id.clone(),
                        result.id.clone(),
                        finished_at,
                        next_probe_at,
                        revision,
                    )
                } else {
                    previous_status
                        .clone()
                        .expect("a stale result has an existing latest status")
                };
                let input_tokens =
                    optional_sqlite_i64(result.token_usage.input_tokens, "input_tokens")?;
                let output_tokens =
                    optional_sqlite_i64(result.token_usage.output_tokens, "output_tokens")?;
                let total_tokens =
                    optional_sqlite_i64(result.token_usage.total_tokens, "total_tokens")?;
                transaction.execute(
                    "INSERT INTO probe_results (
                       id, site_id, model_id, run_kind, started_at_ms, finished_at_ms,
                       status, response_headers_ms, first_token_ms, total_time_ms, http_status,
                       provider_model, finish_reason, error_code, error_message,
                       input_tokens, output_tokens, total_tokens, usage_reported, ended_normally,
                       retry_count, request_id, revision, request_method, endpoint, probe_mode
                     ) VALUES (
                       ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
                       ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26
                     )",
                    params![
                        result.id,
                        result.site_id,
                        result.model_id,
                        result.run_kind,
                        result.started_at,
                        result.finished_at,
                        result.status,
                        result.response_headers_ms,
                        result.first_token_ms,
                        result.total_time_ms,
                        result.http_status,
                        result.provider_model,
                        result.finish_reason,
                        result.error_code,
                        result.error_message,
                        input_tokens,
                        output_tokens,
                        total_tokens,
                        result.token_usage.reported,
                        result.ended_normally,
                        result.retry_count,
                        result.request_id,
                        revision_sql,
                        result.request_method,
                        result.endpoint,
                        result.probe_mode,
                    ],
                )?;
                transaction.execute(
                    "INSERT INTO probe_captures (
                       probe_result_id, request_headers_json, request_body,
                       request_captured_bytes, request_limit_bytes, request_truncated,
                       response_mode, response_content_type, response_headers_json,
                       response_body, response_chunks_json, response_captured_bytes,
                       response_limit_bytes, response_truncated
                     ) VALUES (
                       ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14
                     )",
                    params![
                        capture.probe_result_id,
                        capture.request_headers_json,
                        capture.request_body,
                        capture.request_captured_bytes,
                        capture.request_limit_bytes,
                        capture.request_truncated,
                        capture.response_mode,
                        capture.response_content_type,
                        capture.response_headers_json,
                        capture.response_body,
                        capture.response_chunks_json,
                        capture.response_captured_bytes,
                        capture.response_limit_bytes,
                        capture.response_truncated,
                    ],
                )?;
                if became_latest {
                    transaction.execute(
                        "INSERT INTO latest_model_status (
                           model_id, probe_result_id, status, consecutive_failures,
                           consecutive_successes, next_probe_at_ms, revision, updated_at_ms
                         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                         ON CONFLICT(model_id) DO UPDATE SET
                           probe_result_id = excluded.probe_result_id,
                           status = excluded.status,
                           consecutive_failures = excluded.consecutive_failures,
                           consecutive_successes = excluded.consecutive_successes,
                           next_probe_at_ms = excluded.next_probe_at_ms,
                           revision = excluded.revision,
                           updated_at_ms = excluded.updated_at_ms",
                        params![
                            latest_status.model_id,
                            latest_status.probe_result_id,
                            latest_status.status,
                            latest_status.consecutive_failures,
                            latest_status.consecutive_successes,
                            latest_status.next_probe_at,
                            revision_sql,
                            latest_status.updated_at,
                        ],
                    )?;
                }
                transaction.commit()?;
                Ok(ProbeRecordOutcome {
                    revision,
                    previous_status,
                    latest_status,
                    became_latest,
                })
            })
            .await
    }

    pub async fn page_probe_results(&self, request: ProbePageRequest) -> Result<ProbePage> {
        if !(1..=100).contains(&request.limit) {
            return Err(DatabaseError::InvalidInput { field: "limit" });
        }

        self.database
            .call(move |connection| {
                let cursor_started_at = request.before.as_ref().map(|cursor| cursor.started_at);
                let cursor_id = request.before.as_ref().map(|cursor| &cursor.id);
                let fetch_limit = u32::from(request.limit) + 1;
                let mut statement = connection.prepare(
                    "SELECT id, site_id, model_id, run_kind, started_at_ms, finished_at_ms,
                            status, response_headers_ms, first_token_ms, total_time_ms,
                            http_status, provider_model, finish_reason, error_code, error_message,
                            input_tokens, output_tokens, total_tokens, usage_reported,
                            ended_normally, retry_count, request_id, revision,
                            request_method, endpoint, probe_mode
                     FROM probe_results
                     WHERE (?1 IS NULL OR site_id = ?1)
                       AND (?2 IS NULL OR model_id = ?2)
                       AND (?3 IS NULL OR status = ?3)
                       AND (
                         ?4 IS NULL OR started_at_ms < ?4
                         OR (started_at_ms = ?4 AND id < ?5)
                       )
                     ORDER BY started_at_ms DESC, id DESC
                     LIMIT ?6",
                )?;
                let mut records = statement
                    .query_map(
                        params![
                            request.site_id.as_ref(),
                            request.model_id.as_ref(),
                            request.status.as_ref(),
                            cursor_started_at,
                            cursor_id,
                            fetch_limit,
                        ],
                        probe_result_from_row,
                    )?
                    .collect::<rusqlite::Result<Vec<_>>>()?;

                let has_more = records.len() > usize::from(request.limit);
                if has_more {
                    records.truncate(usize::from(request.limit));
                }
                let next_cursor = has_more.then(|| {
                    let last = records.last().expect("non-empty page with an extra row");
                    ProbeCursor {
                        started_at: last.started_at,
                        id: last.id.clone(),
                    }
                });
                Ok(ProbePage {
                    records,
                    next_cursor,
                })
            })
            .await
    }

    pub async fn load_probe_capture(
        &self,
        probe_result_id: crate::domain::EntityId,
    ) -> Result<Option<ProbeCapture>> {
        self.database
            .call(move |connection| {
                Ok(connection
                    .query_row(
                        "SELECT probe_result_id, request_headers_json, request_body,
                                request_captured_bytes, request_limit_bytes, request_truncated,
                                response_mode, response_content_type, response_headers_json,
                                response_body, response_chunks_json, response_captured_bytes,
                                response_limit_bytes, response_truncated
                         FROM probe_captures WHERE probe_result_id = ?1",
                        [probe_result_id],
                        probe_capture_from_row,
                    )
                    .optional()?)
            })
            .await
    }

    pub async fn list_latest_model_statuses(&self) -> Result<Vec<LatestModelStatus>> {
        self.database
            .call(|connection| {
                let mut statement = connection.prepare(
                    "SELECT model_id, probe_result_id, status, consecutive_failures,
                            consecutive_successes, next_probe_at_ms, revision, updated_at_ms
                     FROM latest_model_status ORDER BY model_id",
                )?;
                let statuses = statement
                    .query_map([], latest_status_from_row)?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                Ok(statuses)
            })
            .await
    }

    pub async fn list_latest_probe_results(&self) -> Result<Vec<ProbeResult>> {
        self.database
            .call(|connection| {
                let mut statement = connection.prepare(
                    "SELECT result.id, result.site_id, result.model_id, result.run_kind,
                            result.started_at_ms, result.finished_at_ms, result.status,
                            result.response_headers_ms, result.first_token_ms,
                            result.total_time_ms, result.http_status, result.provider_model,
                            result.finish_reason, result.error_code, result.error_message,
                            result.input_tokens, result.output_tokens, result.total_tokens,
                            result.usage_reported, result.ended_normally, result.retry_count,
                            result.request_id, result.revision, result.request_method,
                            result.endpoint, result.probe_mode
                     FROM latest_model_status AS latest
                     JOIN probe_results AS result ON result.id = latest.probe_result_id
                     ORDER BY result.model_id",
                )?;
                let results = statement
                    .query_map([], probe_result_from_row)?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                Ok(results)
            })
            .await
    }

    pub async fn clear_probe_results_before(&self, cutoff: TimestampMs) -> Result<CleanupResult> {
        self.database
            .call(move |connection| {
                let transaction = connection.transaction()?;
                let deleted = transaction.execute(
                    "DELETE FROM probe_results WHERE started_at_ms < ?1",
                    [cutoff],
                )?;
                let revision = if deleted > 0 {
                    bump_revision(&transaction)?
                } else {
                    unchanged_revision(&transaction)?
                };
                transaction.commit()?;
                Ok(CleanupResult {
                    deleted: u64::try_from(deleted)
                        .map_err(|_| DatabaseError::InvalidInput { field: "deleted" })?,
                    revision,
                })
            })
            .await
    }

    pub async fn clear_all_probe_results(&self) -> Result<CleanupResult> {
        self.database
            .call(move |connection| {
                let transaction = connection.transaction()?;
                let deleted = transaction.execute("DELETE FROM probe_results", [])?;
                let revision = if deleted > 0 {
                    bump_revision(&transaction)?
                } else {
                    unchanged_revision(&transaction)?
                };
                transaction.commit()?;
                Ok(CleanupResult {
                    deleted: u64::try_from(deleted)
                        .map_err(|_| DatabaseError::InvalidInput { field: "deleted" })?,
                    revision,
                })
            })
            .await
    }
}

#[allow(clippy::too_many_arguments)]
fn aggregate_latest_status(
    previous: Option<&LatestModelStatus>,
    observed: ProbeStatus,
    timing: &ProbeTimingPolicy,
    model_id: crate::domain::EntityId,
    probe_result_id: crate::domain::EntityId,
    finished_at: TimestampMs,
    next_probe_at: Option<TimestampMs>,
    revision: u64,
) -> LatestModelStatus {
    let success = matches!(observed, ProbeStatus::Healthy | ProbeStatus::Slow);
    let consecutive_failures = if success {
        0
    } else {
        previous.map_or(1, |status| status.consecutive_failures.saturating_add(1))
    };
    let consecutive_successes = if success {
        previous.map_or(1, |status| status.consecutive_successes.saturating_add(1))
    } else {
        0
    };
    let status = if success {
        if previous.is_some_and(|status| is_failure(status.status))
            && consecutive_successes < u16::from(timing.recovery_threshold)
        {
            previous.map_or(observed, |status| status.status)
        } else {
            observed
        }
    } else if consecutive_failures < u16::from(timing.failure_threshold) {
        previous.map_or(ProbeStatus::Unknown, |status| status.status)
    } else {
        observed
    };

    LatestModelStatus {
        model_id,
        probe_result_id: Some(probe_result_id),
        status,
        consecutive_failures,
        consecutive_successes,
        next_probe_at,
        revision,
        updated_at: finished_at,
    }
}

fn is_failure(status: ProbeStatus) -> bool {
    !matches!(
        status,
        ProbeStatus::Healthy | ProbeStatus::Slow | ProbeStatus::Unknown
    )
}

pub(super) fn probe_result_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ProbeResult> {
    Ok(ProbeResult {
        id: row.get(0)?,
        site_id: row.get(1)?,
        model_id: row.get(2)?,
        run_kind: row.get(3)?,
        request_method: row.get(23)?,
        endpoint: row.get(24)?,
        probe_mode: row.get(25)?,
        started_at: row.get(4)?,
        finished_at: row.get(5)?,
        status: row.get(6)?,
        response_headers_ms: row.get(7)?,
        first_token_ms: row.get(8)?,
        total_time_ms: row.get(9)?,
        http_status: row.get(10)?,
        provider_model: row.get(11)?,
        finish_reason: row.get(12)?,
        error_code: row.get(13)?,
        error_message: row.get(14)?,
        token_usage: TokenUsage {
            input_tokens: optional_row_u64(row, 15)?,
            output_tokens: optional_row_u64(row, 16)?,
            total_tokens: optional_row_u64(row, 17)?,
            reported: row.get(18)?,
        },
        ended_normally: row.get(19)?,
        retry_count: row.get(20)?,
        request_id: row.get(21)?,
        revision: row_u64(row, 22)?,
    })
}

pub(super) fn probe_capture_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ProbeCapture> {
    probe_capture_from_row_at(row, 0)
}

pub(super) fn probe_capture_from_row_at(
    row: &rusqlite::Row<'_>,
    offset: usize,
) -> rusqlite::Result<ProbeCapture> {
    Ok(ProbeCapture {
        probe_result_id: row.get(offset)?,
        request_headers_json: row.get(offset + 1)?,
        request_body: row.get(offset + 2)?,
        request_captured_bytes: row.get(offset + 3)?,
        request_limit_bytes: row.get(offset + 4)?,
        request_truncated: row.get(offset + 5)?,
        response_mode: row.get(offset + 6)?,
        response_content_type: row.get(offset + 7)?,
        response_headers_json: row.get(offset + 8)?,
        response_body: row.get(offset + 9)?,
        response_chunks_json: row.get(offset + 10)?,
        response_captured_bytes: row.get(offset + 11)?,
        response_limit_bytes: row.get(offset + 12)?,
        response_truncated: row.get(offset + 13)?,
    })
}

pub(super) fn latest_status_from_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<LatestModelStatus> {
    Ok(LatestModelStatus {
        model_id: row.get(0)?,
        probe_result_id: row.get(1)?,
        status: row.get(2)?,
        consecutive_failures: row.get(3)?,
        consecutive_successes: row.get(4)?,
        next_probe_at: row.get(5)?,
        revision: row_u64(row, 6)?,
        updated_at: row.get(7)?,
    })
}

fn row_u64(row: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<u64> {
    let value = row.get::<_, i64>(index)?;
    u64::try_from(value).map_err(|_| rusqlite::Error::IntegralValueOutOfRange(index, value))
}

fn optional_row_u64(row: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<Option<u64>> {
    let value = row.get::<_, Option<i64>>(index)?;
    value
        .map(|value| {
            u64::try_from(value).map_err(|_| rusqlite::Error::IntegralValueOutOfRange(index, value))
        })
        .transpose()
}

#[cfg(test)]
mod tests {
    use crate::{
        domain::ProbeStatus,
        persistence::{Database, Repository},
    };

    use super::*;
    use crate::persistence::repository::test_support::{
        capture, id, latest, model, probe_result, site, site_policy, timestamp,
    };

    async fn repository_with_model(
    ) -> (Repository, crate::domain::EntityId, crate::domain::EntityId) {
        let repository = Repository::new(Database::open_in_memory().expect("database opens"));
        let site = site("site-a", 0);
        let model = model("model-a", &site.id, 0);
        repository
            .create_site(site.clone(), site_policy(&site.id))
            .await
            .expect("site created");
        repository
            .create_model(model.clone())
            .await
            .expect("model created");
        (repository, site.id, model.id)
    }

    #[tokio::test(flavor = "current_thread")]
    async fn probe_write_is_atomic_when_capture_validation_fails() {
        let (repository, site_id, model_id) = repository_with_model().await;
        let result = probe_result("result-a", &site_id, &model_id, 100, ProbeStatus::Healthy);
        let mut invalid_capture = capture(&result.id);
        invalid_capture.request_captured_bytes = invalid_capture.request_limit_bytes + 1;

        assert!(repository
            .record_probe(
                result.clone(),
                invalid_capture,
                ProbeTimingPolicy::default(),
                latest(&result).next_probe_at,
            )
            .await
            .is_err());
        assert_eq!(repository.revision().await.expect("revision read"), 2);
        assert!(repository
            .page_probe_results(ProbePageRequest::default())
            .await
            .expect("page read")
            .records
            .is_empty());
        assert!(repository
            .list_latest_model_statuses()
            .await
            .expect("statuses read")
            .is_empty());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn status_aggregation_is_atomic_and_stale_results_cannot_overwrite_latest() {
        let (repository, site_id, model_id) = repository_with_model().await;
        let timing = ProbeTimingPolicy::default();

        let first_failure = probe_result(
            "result-failure-1",
            &site_id,
            &model_id,
            100,
            ProbeStatus::Timeout,
        );
        let first = repository
            .record_probe(
                first_failure.clone(),
                capture(&first_failure.id),
                timing.clone(),
                None,
            )
            .await
            .expect("first failure recorded");
        assert!(first.became_latest);
        assert_eq!(first.latest_status.status, ProbeStatus::Unknown);
        assert_eq!(first.latest_status.consecutive_failures, 1);

        let second_failure = probe_result(
            "result-failure-2",
            &site_id,
            &model_id,
            300,
            ProbeStatus::Timeout,
        );
        let second = repository
            .record_probe(
                second_failure.clone(),
                capture(&second_failure.id),
                timing.clone(),
                None,
            )
            .await
            .expect("second failure recorded");
        assert_eq!(second.previous_status, Some(first.latest_status));
        assert_eq!(second.latest_status.status, ProbeStatus::Timeout);
        assert_eq!(second.latest_status.consecutive_failures, 2);

        let stale_success = probe_result(
            "result-stale-success",
            &site_id,
            &model_id,
            200,
            ProbeStatus::Healthy,
        );
        let stale = repository
            .record_probe(
                stale_success.clone(),
                capture(&stale_success.id),
                timing.clone(),
                None,
            )
            .await
            .expect("stale result is retained as history");
        assert!(!stale.became_latest);
        assert_eq!(stale.latest_status, second.latest_status);

        let first_recovery = probe_result(
            "result-recovery-1",
            &site_id,
            &model_id,
            400,
            ProbeStatus::Healthy,
        );
        let recovery = repository
            .record_probe(
                first_recovery.clone(),
                capture(&first_recovery.id),
                timing,
                None,
            )
            .await
            .expect("recovery recorded");
        assert_eq!(recovery.latest_status.status, ProbeStatus::Timeout);
        assert_eq!(recovery.latest_status.consecutive_successes, 1);

        let page = repository
            .page_probe_results(ProbePageRequest {
                limit: 10,
                ..ProbePageRequest::default()
            })
            .await
            .expect("probe history read");
        assert_eq!(page.records.len(), 4);
        assert!(page
            .records
            .iter()
            .any(|result| result.id == stale_success.id));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn independent_repository_connections_serialize_status_updates() {
        let directory = tempfile::tempdir().expect("tempdir created");
        let path = directory.path().join("relaypulse.sqlite3");
        let first_repository = Repository::new(Database::open(&path).expect("database opens"));
        let site = site("site-a", 0);
        let model = model("model-a", &site.id, 0);
        first_repository
            .create_site(site.clone(), site_policy(&site.id))
            .await
            .expect("site created");
        first_repository
            .create_model(model.clone())
            .await
            .expect("model created");
        let second_repository = Repository::new(Database::open(&path).expect("database reopens"));

        let earlier = probe_result(
            "result-earlier",
            &site.id,
            &model.id,
            100,
            ProbeStatus::Timeout,
        );
        let later = probe_result(
            "result-later",
            &site.id,
            &model.id,
            200,
            ProbeStatus::Healthy,
        );
        let (earlier_outcome, later_outcome) = tokio::join!(
            first_repository.record_probe(
                earlier.clone(),
                capture(&earlier.id),
                ProbeTimingPolicy::default(),
                None,
            ),
            second_repository.record_probe(
                later.clone(),
                capture(&later.id),
                ProbeTimingPolicy::default(),
                None,
            ),
        );
        earlier_outcome.expect("earlier result recorded");
        later_outcome.expect("later result recorded");

        let latest = first_repository
            .list_latest_model_statuses()
            .await
            .expect("latest status read");
        assert_eq!(latest.len(), 1);
        assert_eq!(latest[0].probe_result_id, Some(later.id));
        assert_eq!(
            latest[0].updated_at,
            later.finished_at.expect("probe finished")
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn probe_pages_are_stable_and_cleanup_cascades_captures() {
        let (repository, site_id, model_id) = repository_with_model().await;
        for (value, started_at, status) in [
            ("result-a", 100, ProbeStatus::Healthy),
            ("result-b", 200, ProbeStatus::Slow),
            ("result-c", 200, ProbeStatus::Healthy),
        ] {
            let result = probe_result(value, &site_id, &model_id, started_at, status);
            repository
                .record_probe(
                    result.clone(),
                    capture(&result.id),
                    ProbeTimingPolicy::default(),
                    latest(&result).next_probe_at,
                )
                .await
                .expect("probe recorded");
        }

        let first = repository
            .page_probe_results(ProbePageRequest {
                limit: 2,
                ..ProbePageRequest::default()
            })
            .await
            .expect("first page read");
        assert_eq!(
            first
                .records
                .iter()
                .map(|record| record.id.clone())
                .collect::<Vec<_>>(),
            vec![id("result-c"), id("result-b")]
        );
        let second = repository
            .page_probe_results(ProbePageRequest {
                before: first.next_cursor,
                limit: 2,
                ..ProbePageRequest::default()
            })
            .await
            .expect("second page read");
        assert_eq!(
            second
                .records
                .iter()
                .map(|record| record.id.clone())
                .collect::<Vec<_>>(),
            vec![id("result-a")]
        );
        assert!(second.next_cursor.is_none());

        let slow = repository
            .page_probe_results(ProbePageRequest {
                status: Some(ProbeStatus::Slow),
                ..ProbePageRequest::default()
            })
            .await
            .expect("filtered page read");
        assert_eq!(slow.records.len(), 1);
        assert_eq!(slow.records[0].id, id("result-b"));

        let capture = repository
            .load_probe_capture(id("result-b"))
            .await
            .expect("capture query succeeds")
            .expect("capture exists");
        assert_eq!(capture.probe_result_id, id("result-b"));

        let cleanup = repository
            .clear_probe_results_before(timestamp(150))
            .await
            .expect("cleanup succeeds");
        assert_eq!(cleanup.deleted, 1);
        assert_eq!(cleanup.revision, 6);
        assert!(repository
            .load_probe_capture(id("result-a"))
            .await
            .expect("capture query succeeds")
            .is_none());

        let latest = repository
            .list_latest_model_statuses()
            .await
            .expect("statuses read");
        assert_eq!(latest.len(), 1);
        assert_eq!(latest[0].probe_result_id, Some(id("result-c")));

        let clear_all = repository
            .clear_all_probe_results()
            .await
            .expect("remaining probes cleared");
        assert_eq!(clear_all.deleted, 2);
        assert_eq!(clear_all.revision, 7);
        let latest = repository
            .list_latest_model_statuses()
            .await
            .expect("statuses read");
        assert_eq!(latest.len(), 1);
        assert_eq!(latest[0].probe_result_id, None);

        let no_op = repository
            .clear_all_probe_results()
            .await
            .expect("empty cleanup succeeds");
        assert_eq!(no_op.deleted, 0);
        assert_eq!(no_op.revision, 7);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn probe_data_round_trips_after_reopening() {
        let directory = tempfile::tempdir().expect("tempdir created");
        let path = directory.path().join("relaypulse.sqlite3");
        let site = site("site-a", 0);
        let model = model("model-a", &site.id, 0);
        let result = probe_result("result-a", &site.id, &model.id, 100, ProbeStatus::Healthy);
        let capture = capture(&result.id);

        {
            let repository = Repository::new(Database::open(&path).expect("database opens"));
            repository
                .create_site(site.clone(), site_policy(&site.id))
                .await
                .expect("site created");
            repository.create_model(model).await.expect("model created");
            repository
                .record_probe(
                    result.clone(),
                    capture.clone(),
                    ProbeTimingPolicy::default(),
                    latest(&result).next_probe_at,
                )
                .await
                .expect("probe recorded");
        }

        let repository = Repository::new(Database::open(&path).expect("database reopens"));
        let page = repository
            .page_probe_results(ProbePageRequest::default())
            .await
            .expect("page read");
        assert_eq!(page.records.len(), 1);
        assert_eq!(page.records[0].id, result.id);
        assert_eq!(page.records[0].revision, 3);
        assert_eq!(
            repository
                .load_probe_capture(result.id.clone())
                .await
                .expect("capture read"),
            Some(capture)
        );
        let latest = repository
            .list_latest_model_statuses()
            .await
            .expect("statuses read");
        assert_eq!(latest.len(), 1);
        assert_eq!(latest[0].probe_result_id, Some(result.id));
        assert_eq!(latest[0].revision, 3);
        let latest_results = repository
            .list_latest_probe_results()
            .await
            .expect("latest results read");
        assert_eq!(latest_results.len(), 1);
        assert_eq!(latest_results[0].id, page.records[0].id);
    }
}
