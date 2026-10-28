use super::{
    access::Lease,
    sync::{self, PendingSync, SyncChecks},
    *,
};
use crate::shared::model_client::{
    Cancellation, CatalogRequest, CheckResult, ExecutionOptions, ModelClient, RequestTarget,
};
use crate::shared::state_changes::StateChanges;
use std::{future::Future, sync::Arc, time::Duration};
use zeroize::Zeroizing;
type ServiceResult<T> = std::result::Result<T, SiteError>;

pub(crate) enum SiteMutation {
    Basic(SiteDraft),
    Connection(Connection),
    Enabled(bool),
    AddModel(String),
    DeleteModel(String),
    ModelEnabled(String, bool),
    Policy(Option<String>, Option<ProbePolicy>, Option<CustomRequest>),
}
pub(crate) struct SiteService {
    store: SiteStore,
    client: Arc<ModelClient>,
    access: Arc<SiteAccess>,
    pending: SyncChecks,
    changes: Arc<StateChanges>,
}
impl SiteService {
    pub(crate) fn new(
        database: Arc<crate::shared::persistence::Database>,
        client: Arc<ModelClient>,
        access: Arc<SiteAccess>,
    ) -> Self {
        Self {
            store: SiteStore::new(database),
            client,
            access,
            pending: SyncChecks::default(),
            changes: Arc::default(),
        }
    }
    pub(crate) fn with_changes(mut self, changes: Arc<StateChanges>) -> Self {
        self.changes = changes;
        self
    }
    pub(crate) fn changes(&self) -> Arc<StateChanges> {
        self.changes.clone()
    }
    pub(crate) fn check_ready(&self) -> ServiceResult<()> {
        self.store.list()?;
        Ok(())
    }
    pub(crate) fn cancellation(&self) -> Cancellation {
        self.access.cancellation()
    }
    pub(crate) fn close(&self) {
        self.access.close();
    }
    pub(crate) async fn drained(&self) {
        self.access.drained().await;
    }
    async fn db<T: Send + 'static>(
        &self,
        work: impl FnOnce() -> Result<T> + Send + 'static,
    ) -> ServiceResult<T> {
        tokio::task::spawn_blocking(work)
            .await
            .map_err(|_| SiteError::Worker)?
            .map_err(Into::into)
    }
    async fn with_access<T, F, Fut>(&self, key: Option<String>, action: F) -> ServiceResult<T>
    where
        F: FnOnce(Arc<Lease>) -> Fut,
        Fut: Future<Output = ServiceResult<T>>,
    {
        let lease = self.access.acquire(key.as_deref())?;
        self.with_lease(lease, action).await
    }
    async fn with_lease<T, F, Fut>(&self, lease: Arc<Lease>, action: F) -> ServiceResult<T>
    where
        F: FnOnce(Arc<Lease>) -> Fut,
        Fut: Future<Output = ServiceResult<T>>,
    {
        let cancel = self.cancellation();
        tokio::select! {
            biased;
            _ = cancel.cancelled() => Err(TaskError::Closing.into()),
            result = action(lease.clone()) => result,
        }
    }
    pub(crate) async fn list(&self) -> ServiceResult<Vec<Site>> {
        let store = self.store.clone();
        self.db(move || store.list()).await
    }
    pub(crate) async fn get(&self, id: String) -> ServiceResult<Site> {
        let store = self.store.clone();
        self.db(move || store.get(&id)).await
    }
    pub(crate) async fn execution_snapshot(
        &self,
        site_id: String,
    ) -> ServiceResult<ExecutionSnapshot> {
        let store = self.store.clone();
        self.db(move || store.execution_snapshot(&site_id)).await
    }
    pub(crate) async fn begin_execution(
        &self,
        site_id: String,
    ) -> ServiceResult<SiteExecutionScope> {
        let lease = self.access.acquire(Some(&site_id))?;
        self.execution_scope(site_id, lease).await
    }
    pub(crate) async fn begin_scheduled_execution(
        &self,
        site_id: String,
    ) -> ServiceResult<SiteExecutionScope> {
        let lease = self.access.acquire_scheduled(&site_id)?;
        self.execution_scope(site_id, lease).await
    }
    async fn execution_scope(
        &self,
        site_id: String,
        lease: Arc<Lease>,
    ) -> ServiceResult<SiteExecutionScope> {
        let store = self.store.clone();
        let snapshot = lease
            .blocking(move || store.execution_snapshot(&site_id))
            .await
            .map_err(|_| SiteError::Worker)??;
        Ok(SiteExecutionScope::new(snapshot, lease))
    }
    pub(crate) async fn create(&self, input: CreateSite) -> ServiceResult<String> {
        self.with_access(Some("__create_site__".into()), |lease| async move {
            validation::name(&input.name)?;
            validation::base_url(&input.base_url)?;
            validation::credential(&input.secret)?;
            let connection = Connection::for_protocol(input.protocol);
            let request = CatalogRequest {
                endpoint: connection.models_path,
                target: RequestTarget {
                    protocol: input.protocol,
                    base_url: input.base_url.clone(),
                    headers: connection.headers,
                    secret: Zeroizing::new(input.secret.clone()),
                },
            };
            let names = self.discover(&request).await?;
            let store = self.store.clone();
            let changes = self.changes.clone();
            lease
                .blocking(move || {
                    let _write = changes.begin();
                    store.create(&input, &names)
                })
                .await
                .map_err(|_| SiteError::Worker)?
                .map_err(Into::into)
        })
        .await
    }
    pub(crate) async fn mutate(&self, id: String, change: SiteMutation) -> ServiceResult<Site> {
        let lease = if matches!(
            &change,
            SiteMutation::Enabled(_) | SiteMutation::ModelEnabled(..)
        ) {
            self.access.acquire_switch(&id)?
        } else {
            self.access.acquire(Some(&id))?
        };
        self.with_lease(lease, |lease| async move {
            let store = self.store.clone();
            let changes = self.changes.clone();
            lease
                .blocking(move || {
                    let _write = changes.begin();
                    match change {
                        SiteMutation::Basic(draft) => store.save_basic(&id, &draft),
                        SiteMutation::Connection(connection) => {
                            store.save_connection(&id, &connection)
                        }
                        SiteMutation::Enabled(enabled) => store.set_enabled(&id, enabled),
                        SiteMutation::AddModel(name) => store.add_model(&id, &name),
                        SiteMutation::DeleteModel(model) => store.delete_model(&id, &model),
                        SiteMutation::ModelEnabled(model, enabled) => {
                            store.set_model_enabled(&id, &model, enabled)
                        }
                        SiteMutation::Policy(model, policy, custom) => store.save_policy(
                            &id,
                            model.as_deref(),
                            policy.as_ref(),
                            custom.as_ref(),
                        ),
                    }
                })
                .await
                .map_err(|_| SiteError::Worker)?
                .map_err(Into::into)
        })
        .await
    }
    pub(crate) async fn delete(&self, id: String) -> ServiceResult<()> {
        self.with_access(Some(id.clone()), |lease| async move {
            let store = self.store.clone();
            let target = id.clone();
            let changes = self.changes.clone();
            lease
                .blocking(move || {
                    let _write = changes.begin();
                    store.delete(&target)
                })
                .await
                .map_err(|_| SiteError::Worker)??;
            self.pending.forget(&id)
        })
        .await
    }
    pub(crate) async fn reorder(&self, ids: Vec<String>) -> ServiceResult<()> {
        self.with_access(None, |lease| async move {
            let store = self.store.clone();
            let changes = self.changes.clone();
            lease
                .blocking(move || {
                    let _write = changes.begin();
                    store.reorder(&ids)
                })
                .await
                .map_err(|_| SiteError::Worker)?
                .map_err(Into::into)
        })
        .await
    }
    pub(crate) async fn check_sync(
        &self,
        id: String,
        names: Vec<String>,
    ) -> ServiceResult<SyncChanges> {
        self.with_access(Some(id.clone()), |_lease| async move {
            self.pending.forget(&id)?;
            let snapshot = self.execution_snapshot(id.clone()).await?;
            validation::names_match(&snapshot.site.models, &names)?;
            let remote = self.discover(&snapshot.catalog_request()).await?;
            let changes = sync::differences(&snapshot.site.models, &remote);
            self.pending.put(
                id.clone(),
                PendingSync {
                    revision: snapshot.site.revision,
                    changes: changes.clone(),
                },
            )?;
            Ok(changes)
        })
        .await
    }
    pub(crate) async fn apply_sync(&self, id: String, input: ApplySync) -> ServiceResult<()> {
        self.with_access(Some(id.clone()), |lease| async move {
            let pending = self.pending.get(&id)?;
            let expected: Vec<_> = pending
                .changes
                .removed
                .iter()
                .map(|m| m.id.clone())
                .collect();
            if !sync::same_set(&input.added, &pending.changes.added)
                || !sync::same_set(&input.removed, &expected)
            {
                return Err(SiteConfigError::Stale.into());
            }
            let store = self.store.clone();
            let target = id.clone();
            let changes = self.changes.clone();
            lease
                .blocking(move || {
                    let _write = changes.begin();
                    store.apply_sync(&target, pending.revision, &input)
                })
                .await
                .map_err(|_| SiteError::Worker)??;
            self.pending.forget(&id)
        })
        .await
    }
    pub(crate) async fn test_draft(
        &self,
        id: String,
        model_id: String,
        draft: CustomRequest,
    ) -> ServiceResult<CheckResult> {
        self.with_access(Some(id.clone()), |_lease| async move {
            let snapshot = self.execution_snapshot(id.clone()).await?;
            let request = snapshot.resolve_draft_input(&model_id, &draft)?;
            self.client
                .check_once(
                    &request,
                    ExecutionOptions::default(),
                    &self.cancellation(),
                    None,
                )
                .await
                .map_err(Into::into)
        })
        .await
    }
    async fn discover(&self, request: &CatalogRequest) -> ServiceResult<Vec<String>> {
        self.client
            .list_models(
                request,
                ExecutionOptions {
                    total_timeout: Duration::from_secs(60),
                    ..Default::default()
                },
                &self.cancellation(),
            )
            .await
            .map_err(Into::into)
    }
}
