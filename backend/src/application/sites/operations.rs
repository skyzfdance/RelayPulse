use super::{monitor_error, Operation, Reply, SiteError, Sites};
use crate::application::action_definitions::actions as OperationAction;
use crate::modules::sites::SiteMutation;
impl Sites {
    pub(super) async fn dispatch(
        &self,
        operation: Operation,
        audit: &mut super::audit::Audit,
    ) -> Result<Reply, SiteError> {
        match operation {
            Operation::List => self.list().await,
            Operation::Get(id) => self.project(self.sites.get(id).await?, false).await,
            Operation::Create(input) => {
                let id = self
                    .sites
                    .create(input, audit.observer(self.operation_logs.clone()))
                    .await?;
                audit.mark_committed();
                Ok(Reply::Id(id))
            }
            Operation::Sync(id, names) => {
                Ok(Reply::Changes(self.sites.check_sync(id, names).await?))
            }
            Operation::Apply(id, input) => {
                self.sites
                    .apply_sync(id, input, audit.observer(self.operation_logs.clone()))
                    .await?;
                audit.mark_committed();
                self.monitoring.cleanup_records().await;
                Ok(Reply::Empty)
            }
            Operation::Test(id, model_id, request) => {
                let (result, snapshot) = self.sites.test_draft(id, model_id, request).await?;
                audit.capture_site(&snapshot);
                if result.success {
                    audit.succeeded(
                        &self.operation_logs,
                        serde_json::json!({"草稿测试":"已完成，不生成正式记录"}),
                    );
                } else {
                    audit.test_failed(&self.operation_logs);
                }
                Ok(Reply::Test(result.into()))
            }
            Operation::Run(id, model_id) => {
                let site = self
                    .monitoring
                    .run(id, model_id)
                    .await
                    .map_err(monitor_error)?;
                self.project(site, false).await.inspect_err(|_| {
                    use crate::modules::operation_logs::*;
                    self.operation_logs.record_best_effort(OperationEvent::new(
                        OperationAction::DetailReadFailed,
                        OperationSource::User,
                        OperationResult::Failure,
                        "站点详情",
                        "探测完成后无法读取站点详情",
                        OperationLogContent::Failure {
                            activity: "读取站点详情".into(),
                            target: "站点详情".into(),
                            reason: "本地健康状态暂时不可用。".into(),
                        },
                    ));
                })
            }
            Operation::Reorder(ids) => {
                self.sites
                    .reorder(ids, audit.observer(self.operation_logs.clone()))
                    .await?;
                audit.mark_committed();
                self.list().await
            }
            Operation::Delete(id) => {
                self.sites
                    .delete(id, audit.observer(self.operation_logs.clone()))
                    .await?;
                audit.mark_committed();
                self.monitoring.cleanup_records().await;
                Ok(Reply::Empty)
            }
            operation => {
                let id = operation.site_id().expect("site mutation").to_string();
                let cleanup = matches!(operation, Operation::DeleteModel(..));
                let item = matches!(operation, Operation::Enabled(..));
                let mutation = match operation {
                    Operation::Save(_, value) => SiteMutation::Basic(value),
                    Operation::Connection(_, value) => SiteMutation::Connection(value),
                    Operation::Enabled(_, value) => SiteMutation::Enabled(value),
                    Operation::AddModel(_, name) => SiteMutation::AddModel(name),
                    Operation::DeleteModel(_, model) => SiteMutation::DeleteModel(model),
                    Operation::ModelEnabled(_, model, value) => {
                        SiteMutation::ModelEnabled(model, value)
                    }
                    Operation::Policy(_, model, policy, custom) => {
                        SiteMutation::Policy(model, policy, custom)
                    }
                    _ => unreachable!("handled above"),
                };
                let site = self
                    .sites
                    .mutate_observed(id, mutation, audit.observer(self.operation_logs.clone()))
                    .await?;
                audit.mark_committed();
                if cleanup {
                    self.monitoring.cleanup_records().await;
                }
                self.project(site, item).await
            }
        }
    }
}
