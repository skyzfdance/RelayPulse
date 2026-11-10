//! Capture configuration while it still exists; successful mutations are recorded before projection.
use super::{Operation, Sites};
use crate::application::action_definitions::actions as OperationAction;
use crate::modules::{
    operation_logs::*,
    sites::{CommitObserver, CredentialAction, Site, SiteCommit, SiteError, TaskError},
};
use serde_json::{json, Value};

/// IPC 只报告边界事实；摘要、结果和历史内容仍由站点用例决定。
pub(crate) enum SiteBoundaryFailure {
    InvalidRequest,
}
#[derive(Clone)]
pub(super) struct Audit {
    action: relaypulse_operations::ActionDefinition,
    object: String,
    before: Value,
    requested: Value,
    recorded: bool,
    read: bool,
    suppress_success: bool,
}
impl Sites {
    pub(crate) fn record_boundary_failure(
        &self,
        action: relaypulse_operations::ActionDefinition,
        failure: SiteBoundaryFailure,
    ) {
        let reason = match failure {
            SiteBoundaryFailure::InvalidRequest => "请求字段缺失、类型错误或含未知字段。",
        };
        self.operation_logs.record_best_effort(OperationEvent::new(
            action,
            OperationSource::User,
            OperationResult::Failure,
            "站点配置",
            "站点请求参数不合法",
            OperationLogContent::Failure {
                activity: action.label().into(),
                target: "站点配置".into(),
                reason: reason.into(),
            },
        ));
    }
    pub(super) fn prepare_audit(&self, operation: &Operation) -> Audit {
        // No asynchronous preflight reads before acquiring the existing business lease.
        let object = operation
            .site_id()
            .filter(|id| *id != "__create_site__")
            .map(|id| format!("站点 {id}"))
            .unwrap_or_else(|| "站点配置".to_owned());
        let before = Value::Null;
        let (action, requested) = match operation {
            Operation::List => (OperationAction::PageLoadFailed, Value::Null),
            Operation::Get(_) => (OperationAction::DetailReadFailed, Value::Null),
            Operation::Create(v) => (
                OperationAction::SiteCreated,
                json!({"名称": v.name, "协议": v.protocol, "baseUrl":v.base_url, "启用":v.enabled, "凭据":if v.secret.is_empty(){"未配置"}else{"已配置"}}),
            ),
            Operation::Save(_, v) => (
                OperationAction::SiteUpdated,
                json!({"名称":v.name,"协议":v.protocol,"baseUrl":v.base_url,"凭据":if v.credential_action == CredentialAction::Replace {"已更换"}else{"保持不变"}}),
            ),
            Operation::Connection(_, v) => (OperationAction::SiteUpdated, json!(v)),
            Operation::Delete(_) => (OperationAction::SiteDeleted, Value::Null),
            Operation::Reorder(ids) => (OperationAction::SiteReordered, json!({"ids":ids})),
            Operation::Enabled(_, v) => (OperationAction::SiteStatusChanged, json!({"enabled":v})),
            Operation::AddModel(_, v) => (OperationAction::ModelCreated, json!({"name":v})),
            Operation::DeleteModel(_, v) => (OperationAction::ModelDeleted, json!({"modelId":v})),
            Operation::ModelEnabled(_, id, v) => (
                OperationAction::ModelStatusChanged,
                json!({"modelId":id,"enabled":v}),
            ),
            Operation::Sync(_, v) => (OperationAction::ModelsSynchronized, json!({"models":v})),
            Operation::Apply(_, v) => (
                OperationAction::ModelsSynchronized,
                json!({"models":v.models,"added":v.added,"removed":v.removed}),
            ),
            Operation::Test(_, id, v) => (
                OperationAction::ManualProbe,
                json!({"modelId":id,"draft":v}),
            ),
            Operation::Policy(_, id, v, c) => (
                if c.is_some() {
                    OperationAction::TestDraftSaved
                } else {
                    OperationAction::ProbeConfigSaved
                },
                json!({"modelId":id,"policy":v,"customRequest":c}),
            ),
            Operation::Run(..) => (OperationAction::ManualProbe, Value::Null),
        };
        let object = match operation {
            Operation::Create(v) => v.name.clone(),
            Operation::Save(_, v) => format!("{}（{}）", v.name, object),
            Operation::AddModel(_, name) => format!("{object} / {name}"),
            Operation::DeleteModel(_, id)
            | Operation::ModelEnabled(_, id, _)
            | Operation::Test(_, id, _) => format!("{object} / 模型 {id}"),
            _ => object,
        };
        Audit {
            action,
            object,
            before,
            requested,
            recorded: matches!(operation, Operation::Run(..)),
            read: matches!(operation, Operation::List | Operation::Get(_)),
            suppress_success: matches!(operation, Operation::Sync(..)),
        }
    }
}
impl Audit {
    pub(super) fn observer(&self, logs: OperationRecorder) -> CommitObserver {
        let mut audit = self.clone();
        Box::new(move |committed| {
            let after = match committed {
                SiteCommit::Created(site) => {
                    audit.capture_site(&site);
                    serde_json::json!(site)
                }
                SiteCommit::Changed(snapshots) => {
                    let (before, after) = *snapshots;
                    audit.capture_site(&before);
                    serde_json::json!(after)
                }
                SiteCommit::Deleted(before) => {
                    audit.capture_site(&before);
                    Value::Null
                }
                SiteCommit::Reordered(before, after) => {
                    fn order(sites: Vec<Site>) -> Value {
                        json!({"顺序":sites.iter().map(|site|json!({"id":site.id,"name":site.name})).collect::<Vec<_>>()})
                    }
                    audit.before = order(before);
                    audit.requested = order(after);
                    Value::Null
                }
            };
            audit.succeeded(&logs, after);
        })
    }
    pub(super) fn mark_committed(&mut self) {
        self.recorded = true;
    }
    pub(super) fn capture_site(&mut self, site: &Site) {
        let model_id = self.requested.get("modelId").and_then(Value::as_str);
        let model = model_id.and_then(|id| site.models.iter().find(|model| model.id == id));
        self.object = if let Some(model) = model {
            format!("{} / {}", site.name, model.name)
        } else if self.action == OperationAction::ModelCreated {
            format!(
                "{} / {}",
                site.name,
                self.requested["name"].as_str().unwrap_or("未知模型")
            )
        } else {
            site.name.clone()
        };
        self.before = if self.action == OperationAction::ModelDeleted {
            serde_json::json!(model)
        } else {
            serde_json::json!(site)
        };
    }
    pub(super) fn succeeded(&mut self, logs: &OperationRecorder, after: Value) {
        if self.recorded || self.read || self.suppress_success {
            return;
        }
        let content = match self.action {
            OperationAction::SiteDeleted | OperationAction::ModelDeleted => {
                OperationLogContent::Deletion {
                    summary: format!("已删除 {}", self.object),
                    fields: snapshot_fields(&self.before),
                }
            }
            OperationAction::SiteCreated
            | OperationAction::ModelCreated
            | OperationAction::ManualProbe => OperationLogContent::Snapshot {
                fields: snapshot_fields(&json!({"提交内容":self.requested,"保存结果":after})),
            },
            OperationAction::SiteReordered => OperationLogContent::Changes {
                changes: changes(&self.before, &self.requested),
            },
            _ => {
                // Compare the same committed configuration shape on both sides. The requested
                // input is not a second namespace that would make every field appear deleted.
                let mut fields = changes(&self.before, &after);
                if self.requested.get("凭据").and_then(Value::as_str) == Some("已更换") {
                    fields.retain(|field| field.field != "credentialConfigured");
                    fields.push(ChangeField {
                        field: "凭据".into(),
                        before: if self.before["credentialConfigured"] == true {
                            "已配置"
                        } else {
                            "未配置"
                        }
                        .into(),
                        after: "已更换".into(),
                        sensitive: Some(true),
                    });
                }
                OperationLogContent::Changes { changes: fields }
            }
        };
        logs.record_best_effort(OperationEvent::new(
            self.action,
            OperationSource::User,
            OperationResult::Success,
            self.object.clone(),
            format!("已完成：{}", self.object),
            content,
        ));
        self.recorded = true;
    }
    pub(super) fn failed(&mut self, logs: &OperationRecorder, error: &SiteError) {
        if self.recorded {
            return;
        }
        let (result, reason) = match error {
            SiteError::Task(TaskError::Closing) => {
                (OperationResult::Cancelled, "应用正在退出，操作已取消。")
            }
            SiteError::Task(TaskError::Busy) => (
                OperationResult::Blocked,
                "对象正在执行其他任务，请稍后重试。",
            ),
            SiteError::Storage => (OperationResult::Failure, "无法读写本地站点数据。"),
            SiteError::Capture => (OperationResult::Failure, "无法保存探测记录。"),
            SiteError::Http(_) => (
                OperationResult::Failure,
                "远端请求未能完成，请检查连接和凭据。",
            ),
            SiteError::Config(_) => (
                OperationResult::Failure,
                "站点配置不合法、对象不存在或配置已变化。",
            ),
            SiteError::Worker => (OperationResult::Failure, "站点任务执行失败。"),
        };
        let mut event = OperationEvent::new(
            self.action,
            OperationSource::User,
            result,
            self.object.clone(),
            reason,
            OperationLogContent::Failure {
                activity: "站点操作".into(),
                target: self.object.clone(),
                reason: reason.into(),
            },
        );
        if !self.requested.is_null() {
            event.explanation = format!(
                "本次尝试的提交内容：{}",
                snapshot_fields(&self.requested)
                    .into_iter()
                    .map(|field| format!("{}：{}", field.label, field.value))
                    .collect::<Vec<_>>()
                    .join("；")
            );
        }
        logs.record_best_effort(event);
        self.recorded = true;
    }
    pub(super) fn test_failed(&mut self, logs: &OperationRecorder) {
        logs.record_best_effort(OperationEvent::new(
            self.action,
            OperationSource::User,
            OperationResult::Failure,
            self.object.clone(),
            "草稿测试未通过",
            crate::modules::monitoring::operation_history::failure(
                "草稿请求未通过；草稿测试不创建正式探测记录。",
            ),
        ));
        self.recorded = true;
    }
}
