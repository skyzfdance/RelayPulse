use super::{monitor_error, Operation, Reply, SiteError, Sites};
use crate::modules::sites::SiteMutation;
impl Sites {
    pub(super) async fn dispatch(&self, operation: Operation) -> Result<Reply, SiteError> {
        match operation {
            Operation::List => self.list().await,
            Operation::Get(id) => self.project(self.sites.get(id).await?, false).await,
            Operation::Create(input) => Ok(Reply::Id(self.sites.create(input).await?)),
            Operation::Sync(id, names) => {
                Ok(Reply::Changes(self.sites.check_sync(id, names).await?))
            }
            Operation::Apply(id, input) => {
                self.sites.apply_sync(id, input).await?;
                self.monitoring.cleanup_records().await;
                Ok(Reply::Empty)
            }
            Operation::Test(id, model_id, request) => Ok(Reply::Test(
                self.sites.test_draft(id, model_id, request).await?.into(),
            )),
            Operation::Run(id, model_id) => {
                let site = self
                    .monitoring
                    .run(id, model_id)
                    .await
                    .map_err(monitor_error)?;
                self.project(site, false).await
            }
            Operation::Reorder(ids) => {
                self.sites.reorder(ids).await?;
                self.list().await
            }
            Operation::Delete(id) => {
                self.sites.delete(id).await?;
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
                let site = self.sites.mutate(id, mutation).await?;
                if cleanup {
                    self.monitoring.cleanup_records().await;
                }
                self.project(site, item).await
            }
        }
    }
}
