use super::{
    privacy, query, sqlite::Store, OperationAction, OperationEvent, OperationLogContent,
    OperationLogDetail, OperationLogError, OperationLogListItem, OperationLogPage,
    OperationLogQuery, OperationResult, OperationSource,
};
use crate::shared::persistence::Database;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

pub(crate) struct OperationLogs {
    store: Store,
}
impl OperationLogs {
    pub(crate) fn new(database: Arc<Database>) -> Self {
        Self {
            store: Store::new(database),
        }
    }
    /// No queue or detached writer: completion includes the SQLite attempt; the owning Arc
    /// releases the shared connection normally on shutdown.
    pub(crate) fn record(&self, mut event: OperationEvent) -> Result<(), OperationLogError> {
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
            action: event.action,
            action_group: event.action.group(),
            action_label: event.action.label().into(),
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
    pub(crate) fn record_best_effort(&self, event: OperationEvent) {
        if self.record(event).is_err() {
            eprintln!("[operation-history] event write failed");
        }
    }
    pub(crate) async fn query(
        self: &Arc<Self>,
        query: OperationLogQuery,
    ) -> Result<OperationLogPage, OperationLogError> {
        query.validate()?;
        let this = self.clone();
        tauri::async_runtime::spawn_blocking(move || this.store.query(query))
            .await
            .map_err(|_| OperationLogError::Unavailable)?
    }
    pub(crate) async fn detail(
        self: &Arc<Self>,
        id: String,
    ) -> Result<OperationLogDetail, OperationLogError> {
        query::log_id(&id)?;
        let this = self.clone();
        tauri::async_runtime::spawn_blocking(move || this.store.detail(&id))
            .await
            .map_err(|_| OperationLogError::Unavailable)?
    }
    pub(crate) fn report_read_failure(&self, target: ReadFailureTarget, kind: ReadFailureKind) {
        // 前端在确认本次可见读取失败后上报一次；独立重试不能按页面和时间窗口合并。
        let action = match kind {
            ReadFailureKind::Page => OperationAction::PageLoadFailed,
            ReadFailureKind::Detail => OperationAction::DetailReadFailed,
        };
        self.record_best_effort(OperationEvent::new(
            action,
            OperationSource::User,
            OperationResult::Failure,
            target.label(),
            "页面内容暂时无法读取。",
            OperationLogContent::Failure {
                activity: action.label().into(),
                target: target.label().into(),
                reason: "前端加载或通信失败，请重试。".into(),
            },
        ));
    }
}
#[derive(Clone, Default)]
pub(crate) struct OperationRecorder(Option<Arc<OperationLogs>>);
impl OperationRecorder {
    pub(crate) fn new(logs: Arc<OperationLogs>) -> Self {
        Self(Some(logs))
    }
    pub(crate) fn record_best_effort(&self, event: OperationEvent) {
        if let Some(logs) = &self.0 {
            logs.record_best_effort(event);
        }
    }
}
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ReadFailureTarget {
    Route,
    SiteList,
    SiteDetail,
    ProbeRecords,
    ProbeRecordDetail,
    NotificationSettings,
    NotificationTemplate,
    GeneralSettings,
    Panel,
}
impl ReadFailureTarget {
    fn label(self) -> &'static str {
        match self {
            Self::Route => "应用页面",
            Self::SiteList => "站点列表",
            Self::SiteDetail => "站点详情",
            Self::ProbeRecords => "探测记录",
            Self::ProbeRecordDetail => "探测记录详情",
            Self::NotificationSettings => "通知设置",
            Self::NotificationTemplate => "通知模板",
            Self::GeneralSettings => "通用设置",
            Self::Panel => "监测面板",
        }
    }
}
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ReadFailureKind {
    Page,
    Detail,
}
