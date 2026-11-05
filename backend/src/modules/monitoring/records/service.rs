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
use crate::modules::{
    monitoring::{health, Monitoring},
    operation_logs::{
        OperationAction, OperationEvent, OperationLogContent, OperationResult, OperationSource,
        SnapshotField,
    },
};
use std::{collections::BTreeMap, path::Path};

/// 正式记录用例拥有诊断标识和失败动作；IPC 只选择实际调用的用例。
#[derive(Clone, Copy)]
pub(crate) enum ProbeRecordOperation {
    Options,
    List,
    Association,
    TokenStats,
    Detail,
    Request,
    Response,
    OpenLog,
    Delete,
    Clear,
}
impl ProbeRecordOperation {
    fn diagnostic_operation(self) -> &'static str {
        match self {
            Self::Options => "probe_records_options",
            Self::List => "probe_records_list",
            Self::Association => "probe_records_association",
            Self::TokenStats => "probe_token_stats",
            Self::Detail => "probe_record_detail",
            Self::Request => "probe_record_request",
            Self::Response => "probe_record_response",
            Self::OpenLog => "probe_record_log_open",
            Self::Delete => "probe_record_delete",
            Self::Clear => "probe_records_clear",
        }
    }
    fn failure_action(self) -> OperationAction {
        match self {
            Self::Options | Self::List | Self::Association | Self::TokenStats => {
                OperationAction::PageLoadFailed
            }
            Self::Detail | Self::Request | Self::Response | Self::OpenLog => {
                OperationAction::DetailReadFailed
            }
            Self::Delete => OperationAction::RecordDeleted,
            Self::Clear => OperationAction::RecordsCleared,
        }
    }
}

impl Monitoring {
    pub(crate) async fn get_probe_record_options(&self) -> Result<ProbeRecordOptions> {
        let result = async {
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
        .await;
        if let Err(error) = &result {
            self.record_history_failure(ProbeRecordOperation::Options, error);
        }
        result
    }
    pub(crate) async fn get_probe_records(
        &self,
        query: ProbeRecordsQuery,
    ) -> Result<RecordsPage<ProbeRecordListItem>> {
        self.validate_record_operation(ProbeRecordOperation::List, query.validate())?;
        self.read_records(
            ProbeRecordOperation::List,
            move |store, _, retention, now| {
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
            },
        )
        .await
    }
    pub(crate) async fn get_probe_records_by_ids(
        &self,
        ids: Vec<String>,
    ) -> Result<ProbeRecordAssociationLookup> {
        self.validate_record_operation(ProbeRecordOperation::Association, query::record_ids(&ids))?;
        self.read_records(
            ProbeRecordOperation::Association,
            move |store, _, retention, now| {
                store
                    .by_ids(
                        &ids,
                        now.saturating_sub(u64::from(retention) * 86_400_000),
                        now,
                    )
                    .map_err(|_| RecordError::Unavailable)
            },
        )
        .await
    }
    pub(crate) async fn get_probe_token_stats(
        &self,
        query: TokenStatsQuery,
    ) -> Result<TokenStatsPage> {
        let query = query.records();
        self.validate_record_operation(ProbeRecordOperation::TokenStats, query.validate())?;
        self.read_records(
            ProbeRecordOperation::TokenStats,
            move |store, _, retention, now| {
                store
                    .token_stats(query.at(now, retention)?)
                    .map_err(|_| RecordError::Unavailable)
            },
        )
        .await
    }
    pub(crate) async fn get_probe_record_detail(&self, id: String) -> Result<ProbeRecordDetail> {
        self.validate_record_operation(ProbeRecordOperation::Detail, query::record_id(&id))?;
        self.read_records(
            ProbeRecordOperation::Detail,
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
        self.validate_record_operation(ProbeRecordOperation::Request, query::record_id(&id))?;
        self.read_records(
            ProbeRecordOperation::Request,
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
        self.validate_record_operation(ProbeRecordOperation::Response, query::record_id(&id))?;
        self.read_records(
            ProbeRecordOperation::Response,
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
        self.validate_record_operation(ProbeRecordOperation::OpenLog, query::record_id(&id))?;
        self.read_records(
            ProbeRecordOperation::OpenLog,
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
        self.validate_record_operation(ProbeRecordOperation::Delete, query::record_id(&id))?;
        let logs = self.operation_logs.clone();
        self.write_records(
            ProbeRecordOperation::Delete,
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
                store.delete(&id).map_err(|_| RecordError::Unavailable)?;
                if let Some(logs) = logs {
                    let site = row.site_name.unwrap_or_else(|| "未知站点".into());
                    let model = row.model_name.unwrap_or_else(|| "未知模型".into());
                    logs.record_best_effort(OperationEvent::new(
                        OperationAction::RecordDeleted,
                        OperationSource::User,
                        OperationResult::Success,
                        format!("{site} / {model}"),
                        "已删除一条探测记录。",
                        OperationLogContent::Deletion {
                            summary: "探测记录及其日志文件已删除。".into(),
                            fields: vec![SnapshotField {
                                label: "记录 ID".into(),
                                value: id,
                                sensitive: None,
                            }],
                        },
                    ));
                }
                Ok(())
            },
        )
        .await
    }
    pub(crate) async fn clear_probe_records(&self) -> Result<()> {
        let logs = self.operation_logs.clone();
        self.write_records(ProbeRecordOperation::Clear, move |store, captures, _, _| {
            if captures.is_none() && store.has_captures().map_err(|_| RecordError::Unavailable)? {
                return Err(RecordError::Unavailable);
            }
            if let Some(captures) = captures {
                captures.clear().map_err(|_| RecordError::Unavailable)?;
            }
            let count = store.count().map_err(|_| RecordError::Unavailable)?;
            store.clear().map_err(|_| RecordError::Unavailable)?;
            if let Some(logs) = logs {
                logs.record_best_effort(OperationEvent::new(
                    OperationAction::RecordsCleared,
                    OperationSource::User,
                    OperationResult::Success,
                    "探测记录",
                    "已清理探测记录。",
                    OperationLogContent::Cleanup {
                        target: "探测记录及其日志文件".into(),
                        scope: "全部记录".into(),
                        count,
                        space: None,
                    },
                ));
            }
            Ok(())
        })
        .await
    }
    async fn read_records<T: Send + 'static>(
        &self,
        operation: ProbeRecordOperation,
        work: impl FnOnce(RecordStore, Option<CaptureStore>, u16, u64) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        let result = self
            .diagnostics
            .observe_async(operation.diagnostic_operation(), async {
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
            .await;
        if let Err(error) = &result {
            self.record_history_failure(operation, error);
        }
        result
    }
    async fn write_records<T: Send + 'static>(
        &self,
        operation: ProbeRecordOperation,
        work: impl FnOnce(RecordStore, Option<CaptureStore>, u16, u64) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        let result = self
            .diagnostics
            .observe_async(operation.diagnostic_operation(), async {
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
            .await;
        if let Err(error) = &result {
            self.record_history_failure(operation, error);
        }
        result
    }
    fn validate_record_operation<T>(
        &self,
        operation: ProbeRecordOperation,
        validation: Result<T>,
    ) -> Result<T> {
        validation.inspect_err(|error| self.record_history_failure(operation, error))
    }
    pub(crate) fn record_probe_record_boundary_failure(&self, operation: ProbeRecordOperation) {
        let Some(logs) = &self.operation_logs else {
            return;
        };
        let action = operation.failure_action();
        logs.record_best_effort(OperationEvent::new(
            action,
            OperationSource::User,
            OperationResult::Failure,
            "探测记录",
            "探测记录操作未完成。",
            OperationLogContent::Failure {
                activity: action.label().into(),
                target: "探测记录".into(),
                reason: "探测记录请求字段缺失、类型错误或含未知字段。".into(),
            },
        ));
    }
    fn record_history_failure(&self, operation: ProbeRecordOperation, error: &RecordError) {
        let Some(logs) = &self.operation_logs else {
            return;
        };
        let action = operation.failure_action();
        logs.record_best_effort(OperationEvent::new(
            action,
            OperationSource::User,
            if matches!(error, RecordError::Busy) {
                OperationResult::Blocked
            } else {
                OperationResult::Failure
            },
            "探测记录",
            "探测记录操作未完成。",
            OperationLogContent::Failure {
                activity: action.label().into(),
                target: "探测记录".into(),
                reason: error.to_string(),
            },
        ));
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
