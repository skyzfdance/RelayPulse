//! 一次正式任务的审计租约；最后一个提交 worker 释放后才写入完整关联。
use super::actions as OperationAction;
use super::{HealthChange, HealthTransition, MonitoringError};
use crate::modules::{
    operation_logs::{
        OperationEvent, OperationLogContent, OperationLogs, OperationResult, OperationSource,
        SnapshotField,
    },
    sites::{SiteError, TaskError},
};
use std::sync::{Arc, Mutex};

pub(super) struct ProbeOperation {
    logs: Option<Arc<OperationLogs>>,
    source: OperationSource,
    scheduled: bool,
    state: Mutex<ProbeOperationState>,
}
struct ProbeOperationState {
    object: String,
    record_ids: Vec<String>,
    outcome: OperationResult,
    reason: String,
    suppressed: bool,
}
impl ProbeOperation {
    pub(super) fn new(logs: Option<Arc<OperationLogs>>, scheduled: bool) -> Arc<Self> {
        Arc::new(Self {
            logs,
            source: if scheduled {
                OperationSource::System
            } else {
                OperationSource::User
            },
            scheduled,
            state: Mutex::new(ProbeOperationState {
                object: "探测任务".into(),
                record_ids: vec![],
                outcome: OperationResult::Cancelled,
                reason: "任务在完成前被取消。".into(),
                suppressed: false,
            }),
        })
    }
    pub(super) fn snapshot(&self, site: &str, model: Option<&str>) {
        self.state.lock().unwrap_or_else(|e| e.into_inner()).object =
            model.map_or_else(|| site.to_owned(), |model| format!("{site} / {model}"));
    }
    pub(super) fn committed(&self, id: String) {
        self.state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .record_ids
            .push(id);
    }
    pub(super) fn finish(&self, result: &Result<crate::modules::sites::Site, MonitoringError>) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        match result {
            Ok(_) => {
                state.outcome = OperationResult::Success;
                state.reason = "正式探测任务已完成。".into();
            }
            Err(error) => {
                state.outcome = match error {
                    MonitoringError::Site(SiteError::Task(TaskError::Closing)) => {
                        OperationResult::Cancelled
                    }
                    MonitoringError::Site(SiteError::Task(TaskError::Busy)) => {
                        OperationResult::Blocked
                    }
                    _ => OperationResult::Failure,
                };
                // Display 始终来自受限的领域错误，不使用 Debug、HTTP 正文或原始驱动错误。
                state.reason = error.to_string();
                state.suppressed = self.scheduled
                    && matches!(
                        error,
                        MonitoringError::Site(SiteError::Task(TaskError::Busy))
                    );
            }
        }
    }
}
impl Drop for ProbeOperation {
    fn drop(&mut self) {
        let Some(logs) = &self.logs else {
            return;
        };
        let state = self.state.get_mut().unwrap_or_else(|e| e.into_inner());
        if state.suppressed
            || (self.scheduled
                && state.record_ids.is_empty()
                && state.outcome == OperationResult::Success)
        {
            return;
        }
        let mut outcome = state.outcome;
        let mut reason = state.reason.clone();
        if outcome == OperationResult::Success && state.record_ids.is_empty() {
            outcome = OperationResult::Blocked;
            reason = "没有符合条件的探测模型，未生成正式探测记录。".into();
        }
        let summary = match outcome {
            OperationResult::Success => "正式探测任务已完成。",
            OperationResult::Failure => "正式探测任务执行失败。",
            OperationResult::Cancelled => "正式探测任务已取消。",
            OperationResult::Blocked => "正式探测任务受阻。",
        };
        let mut event = OperationEvent::new(
            if self.scheduled {
                OperationAction::AutomaticProbe
            } else {
                OperationAction::ManualProbe
            },
            self.source,
            outcome,
            state.object.clone(),
            summary,
            super::operation_history::failure(reason.clone()),
        );
        event.explanation = reason;
        if !state.record_ids.is_empty() {
            event.detail_behavior = super::operation_history::records(
                std::mem::take(&mut state.record_ids),
                "available",
            );
        }
        // 同步 best effort；没有 detached task，不改变探测结果，也不递归记账。
        logs.record_best_effort(event);
    }
}

pub(super) fn health_event(logs: &OperationLogs, change: &HealthChange, site: &str) {
    let (action, summary, health) = match change.transition {
        HealthTransition::Failed => (OperationAction::ModelFailure, "模型进入故障状态。", "故障"),
        HealthTransition::Recovered => (OperationAction::ModelRecovered, "模型恢复正常。", "正常"),
    };
    logs.record_best_effort(OperationEvent::new(
        action,
        change.source,
        OperationResult::Success,
        format!("{site} / {}", change.model),
        summary,
        OperationLogContent::Snapshot {
            fields: vec![
                SnapshotField {
                    label: "健康状态".into(),
                    value: health.into(),
                    sensitive: None,
                },
                SnapshotField {
                    label: "服务主机".into(),
                    value: change.endpoint.clone(),
                    sensitive: None,
                },
            ],
        },
    ));
}
pub(super) fn cleanup_event(
    logs: &OperationLogs,
    action: relaypulse_operations::ActionDefinition,
    source: OperationSource,
    count: u64,
    target: &str,
) {
    if count == 0 {
        return;
    }
    logs.record_best_effort(OperationEvent::new(
        action,
        source,
        OperationResult::Success,
        target,
        "已清理超过保留期或失去关联的数据。",
        OperationLogContent::Cleanup {
            target: target.into(),
            scope: "保留期及关联完整性清理".into(),
            count,
            space: None,
        },
    ));
}
