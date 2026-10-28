//! 正式历史的公开用例；命令层不接触存储或文件适配器。
use super::{
    capture::{CaptureStore, CapturedRecord},
    dto::*,
    error::{RecordError, Result},
    projection,
    queries::StoredRecord,
    query::{self, ProbeRecordsQuery, TokenStatsQuery},
    RecordStore,
};
use crate::modules::monitoring::{health, Monitoring};
use std::{collections::BTreeMap, path::Path};

impl Monitoring {
    pub(crate) async fn get_probe_record_options(&self) -> Result<ProbeRecordOptions> {
        if *self.cancellation().subscribe().borrow() {
            return Err(RecordError::Unavailable);
        }
        let sites = self
            .sites
            .list()
            .await
            .map_err(|_| RecordError::Unavailable)?;
        let mut models = BTreeMap::<String, Vec<String>>::new();
        for site in &sites {
            for model in &site.models {
                models
                    .entry(model.name.clone())
                    .or_default()
                    .push(site.id.clone());
            }
        }
        Ok(ProbeRecordOptions {
            sites: sites
                .into_iter()
                .map(|s| RecordSiteOption {
                    id: s.id,
                    name: s.name,
                })
                .collect(),
            models: models
                .into_iter()
                .map(|(name, site_ids)| RecordModelOption { name, site_ids })
                .collect(),
        })
    }
    pub(crate) async fn get_probe_records(
        &self,
        query: ProbeRecordsQuery,
    ) -> Result<RecordsPage<ProbeRecordListItem>> {
        query.validate()?;
        self.read_records("probe_records_list", move |store, _, retention, now| {
            let rows = store
                .list(query.at(now, retention)?)
                .map_err(|_| RecordError::Unavailable)?;
            Ok(RecordsPage {
                items: rows
                    .items
                    .into_iter()
                    .map(StoredRecord::list_item)
                    .collect::<Result<_>>()?,
                total: rows.total,
                page: rows.page,
                retained_count: rows.retained_count,
            })
        })
        .await
    }
    pub(crate) async fn get_probe_token_stats(
        &self,
        query: TokenStatsQuery,
    ) -> Result<TokenStatsPage> {
        let query = query.records();
        query.validate()?;
        self.read_records("probe_token_stats", move |store, _, retention, now| {
            store
                .token_stats(query.at(now, retention)?)
                .map_err(|_| RecordError::Unavailable)
        })
        .await
    }
    pub(crate) async fn get_probe_record_detail(&self, id: String) -> Result<ProbeRecordDetail> {
        query::record_id(&id)?;
        self.read_records(
            "probe_record_detail",
            move |store, captures, retention, now| {
                let row = record(&store, &id, retention, now)?;
                let log = read_log(captures, &row, false, RecordError::Unavailable)?;
                row.detail(log.as_ref())
            },
        )
        .await
    }
    pub(crate) async fn get_probe_record_request(
        &self,
        id: String,
    ) -> Result<ProbeRecordRequestContent> {
        query::record_id(&id)?;
        self.read_records(
            "probe_record_request",
            move |store, captures, retention, now| {
                let row = record(&store, &id, retention, now)?;
                let log = read_log(captures, &row, false, RecordError::RequestUnavailable)?;
                Ok(projection::request(row, log))
            },
        )
        .await
    }
    pub(crate) async fn get_probe_record_response(
        &self,
        id: String,
    ) -> Result<ProbeRecordResponseContent> {
        query::record_id(&id)?;
        self.read_records(
            "probe_record_response",
            move |store, captures, retention, now| {
                let row = record(&store, &id, retention, now)?;
                let log = read_log(
                    captures,
                    &row,
                    row.failed(),
                    RecordError::ResponseUnavailable,
                )?;
                projection::response(row, log)
            },
        )
        .await
    }
    pub(crate) async fn open_probe_record_log_directory(
        &self,
        id: String,
        open: impl FnOnce(&Path) -> Result<()> + Send + 'static,
    ) -> Result<OpenProbeRecordLogResult> {
        query::record_id(&id)?;
        self.read_records(
            "probe_record_log_open",
            move |store, captures, retention, now| {
                let row = record(&store, &id, retention, now)?;
                let log = read_log(captures, &row, false, RecordError::LogUnavailable)?
                    .ok_or(RecordError::LogUnavailable)?;
                open(log.path.parent().ok_or(RecordError::LogUnavailable)?)?;
                Ok(OpenProbeRecordLogResult {
                    message: "已打开探测日志目录。",
                })
            },
        )
        .await
    }
    pub(crate) async fn delete_probe_record(&self, id: String) -> Result<()> {
        query::record_id(&id)?;
        self.write_records(
            "probe_record_delete",
            move |store, captures, retention, now| {
                let row = record(&store, &id, retention, now)?;
                if captures.is_none() && row.capture_enabled == Some(true) {
                    return Err(RecordError::Unavailable);
                }
                if let Some(captures) = captures {
                    captures
                        .delete(id.clone())
                        .map_err(|_| RecordError::Unavailable)?;
                }
                store.delete(&id).map_err(|_| RecordError::Unavailable)
            },
        )
        .await
    }
    pub(crate) async fn clear_probe_records(&self) -> Result<()> {
        self.write_records("probe_records_clear", move |store, captures, _, _| {
            if captures.is_none() && store.has_captures().map_err(|_| RecordError::Unavailable)? {
                return Err(RecordError::Unavailable);
            }
            if let Some(captures) = captures {
                captures.clear().map_err(|_| RecordError::Unavailable)?;
            }
            store.clear().map_err(|_| RecordError::Unavailable)
        })
        .await
    }
    async fn read_records<T: Send + 'static>(
        &self,
        operation: &'static str,
        work: impl FnOnce(RecordStore, Option<CaptureStore>, u16, u64) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        self.diagnostics
            .observe_async(operation, async {
                let cancellation = self.cancellation();
                let guard = tokio::select! {
                    biased;
                    _=cancellation.cancelled()=>return Err(RecordError::Unavailable),
                    guard=self.record_access.clone().read_owned()=>guard,
                };
                self.record_work(move |store, captures, retention, now| {
                    let _guard = guard;
                    work(store, captures, retention, now)
                })
                .await
            })
            .await
    }
    async fn write_records<T: Send + 'static>(
        &self,
        operation: &'static str,
        work: impl FnOnce(RecordStore, Option<CaptureStore>, u16, u64) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        self.diagnostics
            .observe_async(operation, async {
                if *self.cancellation().subscribe().borrow() {
                    return Err(RecordError::Unavailable);
                }
                let guard = self
                    .record_access
                    .clone()
                    .try_write_owned()
                    .map_err(|_| RecordError::Busy)?;
                self.record_work(move |store, captures, retention, now| {
                    let _guard = guard;
                    work(store, captures, retention, now)
                })
                .await
            })
            .await
    }
    async fn record_work<T: Send + 'static>(
        &self,
        work: impl FnOnce(RecordStore, Option<CaptureStore>, u16, u64) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        let store = self.storage.records();
        let captures = self.captures.clone();
        let general = self.general.clone();
        self.storage
            .blocking(move || {
                let retention = general
                    .load()
                    .map_err(|_| RecordError::Unavailable)?
                    .history_retention_days;
                work(store, captures, retention, health::now_ms())
            })
            .await
            .map_err(|_| RecordError::Unavailable)?
    }
}
fn record(store: &RecordStore, id: &str, retention: u16, now: u64) -> Result<StoredRecord> {
    store
        .get(
            id,
            now.saturating_sub(u64::from(retention) * 86_400_000),
            now,
        )
        .map_err(|_| RecordError::Unavailable)?
        .ok_or(RecordError::NotFound)
}
fn read_log(
    captures: Option<CaptureStore>,
    row: &StoredRecord,
    body: bool,
    error: RecordError,
) -> Result<Option<CapturedRecord>> {
    if row.capture_enabled == Some(false) {
        return Ok(None);
    }
    match captures {
        Some(store) => store.read(&row.id, body).map(Some).map_err(|_| error),
        None if row.capture_enabled == Some(true) => Err(error),
        None => Ok(None),
    }
}
