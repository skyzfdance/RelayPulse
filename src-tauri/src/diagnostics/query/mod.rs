mod projection;
mod reader;
mod types;

use std::{
    collections::{HashMap, VecDeque},
    fs,
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, AtomicUsize, Ordering},
        Mutex,
    },
    time::{Duration, Instant, SystemTime},
};

use serde_json::Value;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
use uuid::Uuid;

use self::{
    projection::Projector,
    reader::{read_lines_reverse, ReadMetrics},
};
use super::{sanitize_log_event, Diagnostics};

use self::types::{OperationCategory, ProjectedRecord};
pub use self::types::{
    OperationHistoryPageDto, OperationHistoryQueryDto, OperationHistoryQueryError,
    OperationRecordDetailsDto,
};

const MAX_PAGE_SIZE: usize = 50;
const MAX_CONCURRENT_QUERIES: usize = 2;
const MAX_CURSOR_ENTRIES: usize = 128;
const MAX_RECORD_REFERENCES: usize = 512;
const TOKEN_TTL: Duration = Duration::from_secs(15 * 60);
const MAX_TOKEN_BYTES: usize = 96;

pub(super) struct QueryState {
    generation: AtomicU64,
    active_queries: AtomicUsize,
    cache: Mutex<QueryCache>,
    last_metrics: Mutex<ReadMetrics>,
}

impl QueryState {
    pub(super) fn new() -> Self {
        Self {
            generation: AtomicU64::new(1),
            active_queries: AtomicUsize::new(0),
            cache: Mutex::new(QueryCache::default()),
            last_metrics: Mutex::new(ReadMetrics::default()),
        }
    }

    pub(super) fn invalidate(&self) {
        self.generation.fetch_add(1, Ordering::AcqRel);
        if let Ok(mut cache) = self.cache.lock() {
            cache.clear();
        }
    }

    fn generation(&self) -> u64 {
        self.generation.load(Ordering::Acquire)
    }

    fn enter(&self) -> Result<QueryPermit<'_>, OperationHistoryQueryError> {
        let mut current = self.active_queries.load(Ordering::Acquire);
        loop {
            if current >= MAX_CONCURRENT_QUERIES {
                return Err(OperationHistoryQueryError::QueryBusy);
            }
            match self.active_queries.compare_exchange_weak(
                current,
                current + 1,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return Ok(QueryPermit { state: self }),
                Err(actual) => current = actual,
            }
        }
    }
}

struct QueryPermit<'a> {
    state: &'a QueryState,
}

impl Drop for QueryPermit<'_> {
    fn drop(&mut self) {
        self.state.active_queries.fetch_sub(1, Ordering::AcqRel);
    }
}

#[derive(Default)]
struct QueryCache {
    cursors: HashMap<String, CursorEntry>,
    cursor_order: VecDeque<String>,
    records: HashMap<String, RecordEntry>,
    record_order: VecDeque<String>,
}

impl QueryCache {
    fn clear(&mut self) {
        self.cursors.clear();
        self.cursor_order.clear();
        self.records.clear();
        self.record_order.clear();
    }

    fn purge_expired(&mut self, now: Instant) {
        self.cursors
            .retain(|_, entry| now.duration_since(entry.created_at) <= TOKEN_TTL);
        self.records
            .retain(|_, entry| now.duration_since(entry.created_at) <= TOKEN_TTL);
        self.cursor_order
            .retain(|token| self.cursors.contains_key(token));
        self.record_order
            .retain(|token| self.records.contains_key(token));
    }

    fn insert_cursor(&mut self, token: String, entry: CursorEntry) {
        while self.cursors.len() >= MAX_CURSOR_ENTRIES {
            if let Some(oldest) = self.cursor_order.pop_front() {
                self.cursors.remove(&oldest);
            } else {
                break;
            }
        }
        self.cursor_order.push_back(token.clone());
        self.cursors.insert(token, entry);
    }

    fn insert_record(&mut self, token: String, entry: RecordEntry) {
        while self.records.len() >= MAX_RECORD_REFERENCES {
            if let Some(oldest) = self.record_order.pop_front() {
                self.records.remove(&oldest);
            } else {
                break;
            }
        }
        self.record_order.push_back(token.clone());
        self.records.insert(token, entry);
    }
}

#[derive(Clone)]
struct CursorEntry {
    created_at: Instant,
    snapshot: LogSnapshot,
    filter: QueryFilter,
    limit: usize,
    anchor: SortKey,
    dataset_version: String,
}

struct RecordEntry {
    created_at: Instant,
    generation: u64,
    details: OperationRecordDetailsDto,
}

#[derive(Debug, Clone)]
struct LogSnapshot {
    generation: u64,
    files: Vec<LogFileSnapshot>,
}

#[derive(Debug, Clone)]
struct LogFileSnapshot {
    path: PathBuf,
    upper_bound: u64,
    modified: Option<SystemTime>,
    active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct QueryFilter {
    started_after: Option<i128>,
    started_before: Option<i128>,
    category: Option<OperationCategory>,
    operation_query: Option<String>,
}

#[derive(Debug, Clone)]
struct ValidatedQuery {
    limit: usize,
    filter: QueryFilter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SortKey {
    occurred_at_unix_nanos: i128,
    reverse_ordinal: u64,
}

#[derive(Debug)]
struct Candidate {
    key: SortKey,
    record: ProjectedRecord,
}

impl Diagnostics {
    pub fn list_operation_records(
        &self,
        query: OperationHistoryQueryDto,
    ) -> Result<OperationHistoryPageDto, OperationHistoryQueryError> {
        let validated = validate_query(&query)?;
        let _permit = self.inner.query_state.enter()?;
        let (snapshot, anchor, dataset_version) = match query.cursor.as_deref() {
            Some(token) => self.resume_cursor(token, &validated)?,
            None => (self.capture_log_snapshot()?, None, opaque_token("dataset")),
        };

        let mut candidates = self.scan_snapshot(&snapshot, &validated.filter, anchor)?;
        if !self.snapshot_is_valid(&snapshot) {
            return Err(OperationHistoryQueryError::CursorExpired);
        }
        candidates.sort_by(|left, right| newest_first(left.key, right.key));
        let has_more = candidates.len() > validated.limit;
        let next_anchor = has_more.then(|| candidates[validated.limit - 1].key);
        candidates.truncate(validated.limit);

        let now = Instant::now();
        let mut cache = self
            .inner
            .query_state
            .cache
            .lock()
            .map_err(|_| OperationHistoryQueryError::BackendUnavailable)?;
        cache.purge_expired(now);
        let mut items = Vec::with_capacity(candidates.len());
        for mut candidate in candidates {
            let record_ref = opaque_token("record");
            candidate.record.details.record_ref.clone_from(&record_ref);
            items.push(candidate.record.summary());
            cache.insert_record(
                record_ref,
                RecordEntry {
                    created_at: now,
                    generation: snapshot.generation,
                    details: candidate.record.details,
                },
            );
        }

        let next_cursor = if has_more {
            let anchor = next_anchor.ok_or(OperationHistoryQueryError::BackendUnavailable)?;
            let token = opaque_token("cursor");
            cache.insert_cursor(
                token.clone(),
                CursorEntry {
                    created_at: now,
                    snapshot,
                    filter: validated.filter,
                    limit: validated.limit,
                    anchor,
                    dataset_version: dataset_version.clone(),
                },
            );
            Some(token)
        } else {
            None
        };

        Ok(OperationHistoryPageDto {
            items,
            next_cursor,
            dataset_version,
        })
    }

    pub fn get_operation_record_details(
        &self,
        record_ref: &str,
    ) -> Result<OperationRecordDetailsDto, OperationHistoryQueryError> {
        let _permit = self.inner.query_state.enter()?;
        if !valid_token(record_ref, "record") {
            return Err(OperationHistoryQueryError::NotFound);
        }
        let now = Instant::now();
        let generation = self.inner.query_state.generation();
        let mut cache = self
            .inner
            .query_state
            .cache
            .lock()
            .map_err(|_| OperationHistoryQueryError::BackendUnavailable)?;
        cache.purge_expired(now);
        let entry = cache
            .records
            .get(record_ref)
            .filter(|entry| entry.generation == generation)
            .ok_or(OperationHistoryQueryError::NotFound)?;
        Ok(entry.details.clone())
    }

    fn resume_cursor(
        &self,
        token: &str,
        query: &ValidatedQuery,
    ) -> Result<(LogSnapshot, Option<SortKey>, String), OperationHistoryQueryError> {
        if !valid_token(token, "cursor") {
            return Err(OperationHistoryQueryError::CursorExpired);
        }
        let now = Instant::now();
        let mut cache = self
            .inner
            .query_state
            .cache
            .lock()
            .map_err(|_| OperationHistoryQueryError::BackendUnavailable)?;
        cache.purge_expired(now);
        let cursor = cache
            .cursors
            .get(token)
            .filter(|cursor| cursor.filter == query.filter && cursor.limit == query.limit)
            .cloned()
            .ok_or(OperationHistoryQueryError::CursorExpired)?;
        if !self.snapshot_is_valid(&cursor.snapshot) {
            return Err(OperationHistoryQueryError::CursorExpired);
        }
        Ok((cursor.snapshot, Some(cursor.anchor), cursor.dataset_version))
    }

    fn capture_log_snapshot(&self) -> Result<LogSnapshot, OperationHistoryQueryError> {
        let _guard = self
            .inner
            .write_lock
            .lock()
            .map_err(|_| OperationHistoryQueryError::BackendUnavailable)?;
        let generation = self.inner.query_state.generation();
        let mut files = Vec::new();
        for path in self.log_paths_oldest_first().into_iter().rev() {
            match fs::metadata(&path) {
                Ok(metadata) if metadata.is_file() => files.push(LogFileSnapshot {
                    active: path == self.inner.log_file,
                    path,
                    upper_bound: metadata.len(),
                    modified: metadata.modified().ok(),
                }),
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => return Err(OperationHistoryQueryError::BackendUnavailable),
            }
        }
        Ok(LogSnapshot { generation, files })
    }

    fn snapshot_is_valid(&self, snapshot: &LogSnapshot) -> bool {
        if snapshot.generation != self.inner.query_state.generation() {
            return false;
        }
        snapshot.files.iter().all(|file| {
            fs::metadata(&file.path).is_ok_and(|metadata| {
                metadata.is_file()
                    && if file.active {
                        metadata.len() >= file.upper_bound
                    } else {
                        metadata.len() == file.upper_bound
                            && metadata.modified().ok() == file.modified
                    }
            })
        })
    }

    fn scan_snapshot(
        &self,
        snapshot: &LogSnapshot,
        filter: &QueryFilter,
        anchor: Option<SortKey>,
    ) -> Result<Vec<Candidate>, OperationHistoryQueryError> {
        let (candidates, metrics) = scan_candidates(snapshot, filter, anchor, MAX_PAGE_SIZE + 1)?;
        if let Ok(mut last_metrics) = self.inner.query_state.last_metrics.lock() {
            *last_metrics = metrics;
        }
        Ok(candidates)
    }
}

fn scan_candidates(
    snapshot: &LogSnapshot,
    filter: &QueryFilter,
    anchor: Option<SortKey>,
    capacity: usize,
) -> Result<(Vec<Candidate>, ReadMetrics), OperationHistoryQueryError> {
    let mut metrics = ReadMetrics::default();
    let mut projector = Projector::new();
    let mut reverse_ordinal = 0u64;
    let mut candidates = Vec::with_capacity(capacity);
    let mut damaged_lines = 0u64;

    for file in &snapshot.files {
        let completed = read_lines_reverse(&file.path, file.upper_bound, &mut metrics, |line| {
            let Ok(value) = serde_json::from_slice::<Value>(line) else {
                damaged_lines = damaged_lines.saturating_add(1);
                return true;
            };
            if value.get("schemaVersion").and_then(Value::as_u64) != Some(1) {
                return true;
            }
            let Some(event) = sanitize_log_event(value) else {
                return true;
            };
            let Some(record) = projector.project(event) else {
                return true;
            };
            let key = SortKey {
                occurred_at_unix_nanos: record.occurred_at_unix_nanos,
                reverse_ordinal,
            };
            reverse_ordinal = reverse_ordinal.saturating_add(1);
            if !matches_filter(&record, filter)
                || anchor.is_some_and(|anchor| !is_older_than(key, anchor))
            {
                return true;
            }
            candidates.push(Candidate { key, record });
            candidates.sort_by(|left, right| newest_first(left.key, right.key));
            candidates.truncate(capacity);
            true
        })
        .map_err(|_| {
            if anchor.is_some() {
                OperationHistoryQueryError::CursorExpired
            } else {
                OperationHistoryQueryError::BackendUnavailable
            }
        })?;
        if !completed {
            return Err(OperationHistoryQueryError::BackendUnavailable);
        }
    }
    metrics.damaged_lines = damaged_lines;
    Ok((candidates, metrics))
}

fn validate_query(
    query: &OperationHistoryQueryDto,
) -> Result<ValidatedQuery, OperationHistoryQueryError> {
    let limit = usize::try_from(query.limit)
        .ok()
        .filter(|limit| (1..=MAX_PAGE_SIZE).contains(limit))
        .ok_or(OperationHistoryQueryError::InvalidQuery)?;
    let started_after = query
        .started_after
        .as_deref()
        .map(parse_timestamp)
        .transpose()?
        .map(|value| value.unix_timestamp_nanos());
    let started_before = query
        .started_before
        .as_deref()
        .map(parse_timestamp)
        .transpose()?
        .map(|value| value.unix_timestamp_nanos());
    if started_after
        .zip(started_before)
        .is_some_and(|(after, before)| after > before)
    {
        return Err(OperationHistoryQueryError::InvalidQuery);
    }
    let category = query
        .category
        .as_deref()
        .map(OperationCategory::parse)
        .transpose_option()?;
    let operation_query = query
        .operation_query
        .as_deref()
        .map(validate_operation_query)
        .transpose()?;
    Ok(ValidatedQuery {
        limit,
        filter: QueryFilter {
            started_after,
            started_before,
            category,
            operation_query,
        },
    })
}

trait TransposeOption<T> {
    fn transpose_option(self) -> Result<Option<T>, OperationHistoryQueryError>;
}

impl<T> TransposeOption<T> for Option<Option<T>> {
    fn transpose_option(self) -> Result<Option<T>, OperationHistoryQueryError> {
        match self {
            Some(Some(value)) => Ok(Some(value)),
            Some(None) => Err(OperationHistoryQueryError::InvalidQuery),
            None => Ok(None),
        }
    }
}

fn parse_timestamp(value: &str) -> Result<OffsetDateTime, OperationHistoryQueryError> {
    OffsetDateTime::parse(value, &Rfc3339).map_err(|_| OperationHistoryQueryError::InvalidQuery)
}

fn validate_operation_query(value: &str) -> Result<String, OperationHistoryQueryError> {
    let value = value.trim();
    if value.is_empty()
        || value.chars().count() > 80
        || value
            .chars()
            .any(|character| character.is_control() || matches!(character, '/' | '\\' | ':'))
    {
        return Err(OperationHistoryQueryError::InvalidQuery);
    }
    Ok(value.to_lowercase())
}

fn matches_filter(record: &ProjectedRecord, filter: &QueryFilter) -> bool {
    filter
        .started_after
        .is_none_or(|after| record.occurred_at_unix_nanos >= after)
        && filter
            .started_before
            .is_none_or(|before| record.occurred_at_unix_nanos <= before)
        && filter
            .category
            .is_none_or(|category| record.details.category == category)
        && filter.operation_query.as_ref().is_none_or(|query| {
            record
                .details
                .action_label
                .to_lowercase()
                .contains(query.as_str())
        })
}

fn newest_first(left: SortKey, right: SortKey) -> std::cmp::Ordering {
    right
        .occurred_at_unix_nanos
        .cmp(&left.occurred_at_unix_nanos)
        .then_with(|| left.reverse_ordinal.cmp(&right.reverse_ordinal))
}

fn is_older_than(candidate: SortKey, anchor: SortKey) -> bool {
    candidate.occurred_at_unix_nanos < anchor.occurred_at_unix_nanos
        || (candidate.occurred_at_unix_nanos == anchor.occurred_at_unix_nanos
            && candidate.reverse_ordinal > anchor.reverse_ordinal)
}

fn opaque_token(kind: &str) -> String {
    format!("{kind}_{}", Uuid::new_v4().simple())
}

fn valid_token(value: &str, kind: &str) -> bool {
    value.len() <= MAX_TOKEN_BYTES
        && value
            .strip_prefix(&format!("{kind}_"))
            .is_some_and(|suffix| {
                suffix.len() == 32
                    && suffix
                        .chars()
                        .all(|character| character.is_ascii_hexdigit())
            })
}

#[cfg(test)]
mod tests {
    use std::{
        fs::{self, OpenOptions},
        io::Write,
    };

    use serde_json::json;

    use super::*;

    fn query(limit: u64) -> OperationHistoryQueryDto {
        OperationHistoryQueryDto {
            cursor: None,
            limit,
            started_after: None,
            started_before: None,
            category: None,
            operation_query: None,
        }
    }

    #[test]
    fn paginates_a_stable_snapshot_and_ignores_later_appends() {
        let directory = tempfile::tempdir().expect("temporary directory created");
        let diagnostics = Diagnostics::initialize(directory.path()).expect("diagnostics start");
        diagnostics.append(json!({
            "event": "diagnostic-logs-cleared",
            "deletedFiles": 2,
            "reclaimedBytes": 2048,
        }));
        diagnostics.append(json!({
            "event": "diagnostics-exported",
            "fileCount": 2,
            "bundleSizeBytes": 4096,
            "locationKind": "user_selected",
            "requestBodiesExcluded": true,
        }));

        let first = diagnostics
            .list_operation_records(query(1))
            .expect("first page loads");
        assert_eq!(first.items.len(), 1);
        assert_eq!(first.items[0].action_label, "导出诊断包");
        let cursor = first.next_cursor.expect("next cursor exists");

        diagnostics.append(json!({
            "event": "diagnostic-logs-cleared",
            "deletedFiles": 1,
            "reclaimedBytes": 512,
        }));
        let second = diagnostics
            .list_operation_records(OperationHistoryQueryDto {
                cursor: Some(cursor),
                ..query(1)
            })
            .expect("second page loads from original high water mark");

        assert_eq!(second.dataset_version, first.dataset_version);
        assert_eq!(second.items.len(), 1);
        assert_eq!(second.items[0].action_label, "清理诊断日志");
        let third = diagnostics
            .list_operation_records(OperationHistoryQueryDto {
                cursor: second.next_cursor,
                ..query(1)
            })
            .expect("third page loads from original high water mark");
        assert_eq!(third.dataset_version, first.dataset_version);
        assert_eq!(third.items.len(), 1);
        assert_eq!(third.items[0].action_label, "启动 RelayPulse");
        assert!(third.next_cursor.is_none());
    }

    #[test]
    fn clearing_logs_invalidates_cursor_and_record_reference() {
        let directory = tempfile::tempdir().expect("temporary directory created");
        let diagnostics = Diagnostics::initialize(directory.path()).expect("diagnostics start");
        diagnostics.append(json!({
            "event": "diagnostics-exported",
            "fileCount": 1,
            "bundleSizeBytes": 512,
            "locationKind": "user_selected",
            "requestBodiesExcluded": true,
        }));
        diagnostics.append(json!({
            "event": "diagnostic-logs-cleared",
            "deletedFiles": 1,
            "reclaimedBytes": 512,
        }));
        let page = diagnostics
            .list_operation_records(query(1))
            .expect("page loads");
        let record_ref = page.items[0].record_ref.clone();
        let cursor = page.next_cursor.expect("cursor exists");

        diagnostics.clear_logs().expect("logs clear");

        assert_eq!(
            diagnostics
                .list_operation_records(OperationHistoryQueryDto {
                    cursor: Some(cursor),
                    ..query(1)
                })
                .expect_err("cursor expires"),
            OperationHistoryQueryError::CursorExpired
        );
        assert_eq!(
            diagnostics
                .get_operation_record_details(&record_ref)
                .expect_err("record expires"),
            OperationHistoryQueryError::NotFound
        );
    }

    #[test]
    fn skips_corrupt_oversized_and_incomplete_lines() {
        let directory = tempfile::tempdir().expect("temporary directory created");
        let diagnostics = Diagnostics::initialize(directory.path()).expect("diagnostics start");
        let path = directory.path().join("logs/relaypulse.log");
        let mut file = OpenOptions::new()
            .append(true)
            .open(path)
            .expect("log opens");
        writeln!(file, "not-json").expect("corrupt line writes");
        writeln!(file, "{}", "x".repeat(reader::MAX_LINE_BYTES + 1))
            .expect("oversized line writes");
        write!(file, "{{\"schemaVersion\":1").expect("partial tail writes");

        let page = diagnostics
            .list_operation_records(query(10))
            .expect("query remains available");

        assert!(page
            .items
            .iter()
            .any(|item| item.action_label == "启动 RelayPulse"));
    }

    #[test]
    fn rejects_invalid_filters_and_tampered_tokens() {
        let directory = tempfile::tempdir().expect("temporary directory created");
        let diagnostics = Diagnostics::initialize(directory.path()).expect("diagnostics start");

        assert_eq!(
            diagnostics
                .list_operation_records(OperationHistoryQueryDto {
                    operation_query: Some("../secret".to_owned()),
                    ..query(10)
                })
                .expect_err("path-like search is rejected"),
            OperationHistoryQueryError::InvalidQuery
        );
        assert_eq!(
            diagnostics
                .list_operation_records(OperationHistoryQueryDto {
                    cursor: Some("cursor_tampered".to_owned()),
                    ..query(10)
                })
                .expect_err("tampered cursor expires"),
            OperationHistoryQueryError::CursorExpired
        );
        assert_eq!(
            diagnostics
                .get_operation_record_details("record_tampered")
                .expect_err("tampered record is hidden"),
            OperationHistoryQueryError::NotFound
        );
    }

    #[test]
    fn applies_category_time_and_safe_action_text_filters_together() {
        let directory = tempfile::tempdir().expect("temporary directory created");
        let diagnostics = Diagnostics::initialize(directory.path()).expect("diagnostics start");
        diagnostics.record_log_limit_configured("op_fixture", 20 * 1024 * 1024);

        let page = diagnostics
            .list_operation_records(OperationHistoryQueryDto {
                limit: 10,
                cursor: None,
                started_after: Some("2020-01-01T00:00:00Z".to_owned()),
                started_before: Some("2030-01-01T00:00:00Z".to_owned()),
                category: Some("configuration_change".to_owned()),
                operation_query: Some("  日志容量  ".to_owned()),
            })
            .expect("combined filter loads");

        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].action_label, "调整诊断日志容量");
        let details = diagnostics
            .get_operation_record_details(&page.items[0].record_ref)
            .expect("details load");
        let serialized = serde_json::to_value(details).expect("details serialize");
        assert_eq!(serialized["detail"]["kind"], "configuration_change");
        assert!(serialized.get("operationId").is_none());
        assert!(serialized.get("sessionId").is_none());
    }

    #[test]
    fn rotation_invalidates_an_existing_cursor() {
        let directory = tempfile::tempdir().expect("temporary directory created");
        let diagnostics = Diagnostics::initialize(directory.path()).expect("diagnostics start");
        diagnostics.record_log_limit_configured("op_fixture", 20 * 1024 * 1024);
        let page = diagnostics
            .list_operation_records(query(1))
            .expect("page loads");
        let cursor = page.next_cursor.expect("cursor exists");
        fs::write(
            directory.path().join("logs/relaypulse.log"),
            vec![b'x'; super::super::MAX_LOG_FILE_BYTES as usize],
        )
        .expect("active log fills");

        diagnostics.record_state_change(None, 1, &["records"]);

        assert_eq!(
            diagnostics
                .list_operation_records(OperationHistoryQueryDto {
                    cursor: Some(cursor),
                    ..query(1)
                })
                .expect_err("rotated cursor expires"),
            OperationHistoryQueryError::CursorExpired
        );
    }

    #[test]
    fn concurrency_gate_rejects_the_third_query_without_waiting() {
        let state = QueryState::new();
        let first = state.enter().expect("first query enters");
        let second = state.enter().expect("second query enters");

        assert!(matches!(
            state.enter(),
            Err(OperationHistoryQueryError::QueryBusy)
        ));
        drop((first, second));
        assert!(state.enter().is_ok());
    }

    #[test]
    fn cursor_and_record_caches_stay_within_their_hard_limits() {
        let directory = tempfile::tempdir().expect("temporary directory created");
        let diagnostics = Diagnostics::initialize(directory.path()).expect("diagnostics start");
        diagnostics.record_log_limit_configured("op_fixture", 20 * 1024 * 1024);

        for _ in 0..(MAX_RECORD_REFERENCES + MAX_CURSOR_ENTRIES + 16) {
            let page = diagnostics
                .list_operation_records(query(1))
                .expect("page loads");
            assert!(page.next_cursor.is_some());
        }

        let cache = diagnostics
            .inner
            .query_state
            .cache
            .lock()
            .expect("query cache locks");
        assert!(cache.records.len() <= MAX_RECORD_REFERENCES);
        assert!(cache.cursors.len() <= MAX_CURSOR_ENTRIES);
    }

    #[test]
    fn ten_and_one_hundred_megabyte_fixtures_respect_scan_and_buffer_budgets() {
        for megabytes in [10u16, 100u16] {
            assert_bounded_fixture(megabytes);
        }
    }

    fn assert_bounded_fixture(megabytes: u16) {
        let directory = tempfile::tempdir().expect("temporary directory created");
        let first = Diagnostics::initialize(directory.path()).expect("diagnostics start");
        first
            .configure_max_total_megabytes(megabytes)
            .expect("capacity configures");
        let diagnostics = Diagnostics::initialize(directory.path()).expect("diagnostics restart");
        diagnostics.clear_logs().expect("startup logs clear");

        let file_count = usize::from(megabytes) / 2;
        let oversized_line = vec![b'x'; super::super::MAX_LOG_FILE_BYTES as usize - 1];
        for index in 0..file_count {
            let path = if index == 0 {
                directory.path().join("logs/relaypulse.log")
            } else {
                directory
                    .path()
                    .join(format!("logs/relaypulse.log.{index}"))
            };
            let mut file = fs::File::create(path).expect("fixture file creates");
            file.write_all(&oversized_line)
                .expect("fixture line writes");
            file.write_all(b"\n").expect("fixture newline writes");
        }

        let page = diagnostics
            .list_operation_records(query(10))
            .expect("bounded fixture scans");
        let metrics = *diagnostics
            .inner
            .query_state
            .last_metrics
            .lock()
            .expect("metrics lock");

        assert!(page.items.is_empty());
        assert_eq!(metrics.scanned_bytes, u64::from(megabytes) * 1024 * 1024);
        assert_eq!(metrics.oversized_lines, file_count as u64);
        assert!(metrics.scanned_bytes <= reader::MAX_SCAN_BYTES);
        const {
            assert!(reader::READ_BLOCK_BYTES + reader::MAX_LINE_BYTES <= 128 * 1024);
        }
    }
}
