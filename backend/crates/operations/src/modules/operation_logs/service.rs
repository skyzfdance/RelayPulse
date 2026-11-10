use super::{
    privacy, query, sqlite::Store, OperationEvent, OperationLogDetail, OperationLogError,
    OperationLogListItem, OperationLogPage, OperationLogQuery, OperationSource,
};
use crate::shared::persistence::Database;
use std::sync::Arc;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

pub struct OperationLogs {
    store: Store,
    accepting: std::sync::Mutex<bool>,
    activity: Arc<relaypulse_runtime::tasks::BlockingActivity>,
}
impl OperationLogs {
    pub fn new(database: Arc<Database>) -> Self {
        Self {
            store: Store::new(database),
            accepting: std::sync::Mutex::new(true),
            activity: Arc::default(),
        }
    }
    pub fn with_history_decoder(mut self, decoder: Arc<dyn super::HistoryDecoder>) -> Self {
        self.store.decoder = decoder;
        self
    }
    /// No queue or detached writer: completion includes the SQLite attempt; the owning Arc
    /// releases the shared connection normally on shutdown.
    pub fn record(&self, mut event: OperationEvent) -> Result<(), OperationLogError> {
        let accepting = self
            .accepting
            .lock()
            .map_err(|_| OperationLogError::Unavailable)?;
        if !*accepting {
            return Err(OperationLogError::Unavailable);
        }
        for value in [event.action.id, event.action.group] {
            if value.is_empty()
                || value.len() > 200
                || !value
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
            {
                return Err(OperationLogError::Validation {
                    field: "action",
                    message: "动作标识或分组无效。",
                });
            }
        }
        if event.action.label.trim().is_empty() {
            return Err(OperationLogError::Validation {
                field: "actionLabel",
                message: "动作名称不能为空。",
            });
        }
        privacy::sanitize(&mut event);
        let at = event.occurred_at_ms.unwrap_or_else(|| {
            OffsetDateTime::now_utc()
                .unix_timestamp_nanos()
                .div_euclid(1_000_000) as i64
        });
        let occurred_at = OffsetDateTime::from_unix_timestamp_nanos(i128::from(at) * 1_000_000)
            .map_err(|_| OperationLogError::Unavailable)?
            .format(&Rfc3339)
            .map_err(|_| OperationLogError::Unavailable)?;
        let item = OperationLogListItem {
            id: event.id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
            occurred_at,
            source: event.source,
            actor_snapshot: match event.source {
                OperationSource::System => "系统任务",
                OperationSource::User => "本机用户",
            }
            .into(),
            action: event.action.id.into(),
            action_group: event.action.group.into(),
            action_label: event.action.label.into(),
            object_snapshot: event.object,
            result: event.result,
            summary: event.summary,
            detail_behavior: event.detail_behavior,
        };
        self.store.insert(
            at,
            &OperationLogDetail {
                item,
                explanation: event.explanation,
                content: event.content,
            },
        )
    }
    pub fn record_best_effort(&self, event: OperationEvent) {
        if self.record(event).is_err() {
            eprintln!("[operation-history] event write failed");
        }
    }
    /// Seal new work before draining existing blocking database readers.
    pub fn close(&self) {
        *self.accepting.lock().unwrap_or_else(|e| e.into_inner()) = false;
    }
    pub async fn drained(&self) {
        self.activity.drained().await;
    }
    fn begin_read(&self) -> Result<relaypulse_runtime::tasks::BlockingGuard, OperationLogError> {
        let accepting = self
            .accepting
            .lock()
            .map_err(|_| OperationLogError::Unavailable)?;
        if !*accepting {
            return Err(OperationLogError::Unavailable);
        }
        Ok(self.activity.enter())
    }
    pub async fn query(
        self: &Arc<Self>,
        query: OperationLogQuery,
    ) -> Result<OperationLogPage, OperationLogError> {
        query.validate()?;
        let held = self.begin_read()?;
        let this = self.clone();
        tokio::task::spawn_blocking(move || {
            let _held = held;
            this.store.query(query)
        })
        .await
        .map_err(|_| OperationLogError::Unavailable)?
    }
    pub async fn detail(
        self: &Arc<Self>,
        id: String,
    ) -> Result<OperationLogDetail, OperationLogError> {
        query::log_id(&id)?;
        let held = self.begin_read()?;
        let this = self.clone();
        tokio::task::spawn_blocking(move || {
            let _held = held;
            this.store.detail(&id)
        })
        .await
        .map_err(|_| OperationLogError::Unavailable)?
    }
}

#[derive(Clone, Default)]
pub struct OperationRecorder(Option<Arc<OperationLogs>>);
impl OperationRecorder {
    pub fn new(logs: Arc<OperationLogs>) -> Self {
        Self(Some(logs))
    }
    pub fn record_best_effort(&self, event: OperationEvent) {
        if let Some(logs) = &self.0 {
            logs.record_best_effort(event);
        }
    }
}
