use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use thiserror::Error;

use crate::{
    credentials::CredentialError,
    domain::{
        AuthPreset, Clock, CredentialIntent, DiscoveryStatus, EntityId, GeneralSettings,
        HttpMethod, IdGenerator, LifecycleState, Model, ModelSource, ParameterType, ProbeMode,
        ProbeParameter, ProbePolicy, ProbeTimingPolicy, RequestHeader, SecretValue, Site,
        SiteProtocol, ThemeMode,
    },
    persistence::{CleanupResult, DatabaseError, Repository},
};

use super::{
    CredentialSaga, CredentialSagaError, DiscoveredModel, ModelCatalogClient, ModelCatalogError,
};

#[derive(Debug, Error)]
pub enum ApplicationError {
    #[error("invalid value for {field}")]
    InvalidInput { field: &'static str },
    #[error("{entity} was not found")]
    NotFound { entity: &'static str },
    #[error("the credential store is unavailable")]
    CredentialUnavailable,
    #[error("the database is unavailable")]
    DatabaseUnavailable,
    #[error("the model catalog request could not reach the server")]
    Network,
    #[error("the model catalog request timed out")]
    Timeout,
    #[error("the model catalog credential was rejected")]
    Authentication,
    #[error("the model catalog request was rate limited")]
    RateLimited,
    #[error("the model catalog server returned an error")]
    Upstream,
    #[error("the model catalog response is not supported")]
    Protocol,
    #[error("an internal application operation failed")]
    Internal,
}

impl From<DatabaseError> for ApplicationError {
    fn from(error: DatabaseError) -> Self {
        match error {
            DatabaseError::InvalidInput { field } => Self::InvalidInput { field },
            DatabaseError::NotFound { entity } => Self::NotFound { entity },
            _ => Self::DatabaseUnavailable,
        }
    }
}

impl From<CredentialSagaError> for ApplicationError {
    fn from(error: CredentialSagaError) -> Self {
        match error {
            CredentialSagaError::Credential(CredentialError::Unavailable) => {
                Self::CredentialUnavailable
            }
            CredentialSagaError::Credential(
                CredentialError::WorkerUnavailable
                | CredentialError::WorkerStopped
                | CredentialError::WorkerPanicked,
            ) => Self::CredentialUnavailable,
            CredentialSagaError::Credential(CredentialError::NotFound) => {
                Self::CredentialUnavailable
            }
            CredentialSagaError::Database(error) => error.into(),
        }
    }
}

impl From<ModelCatalogError> for ApplicationError {
    fn from(error: ModelCatalogError) -> Self {
        match error {
            ModelCatalogError::InvalidUrl => Self::InvalidInput {
                field: "models_endpoint",
            },
            ModelCatalogError::InvalidHeader => Self::InvalidInput { field: "headers" },
            ModelCatalogError::MissingCredential => Self::CredentialUnavailable,
            ModelCatalogError::Network => Self::Network,
            ModelCatalogError::Timeout => Self::Timeout,
            ModelCatalogError::Authentication => Self::Authentication,
            ModelCatalogError::RateLimited => Self::RateLimited,
            ModelCatalogError::Upstream => Self::Upstream,
            ModelCatalogError::Protocol => Self::Protocol,
        }
    }
}

pub type Result<T> = std::result::Result<T, ApplicationError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelAggregate {
    pub model: Model,
    pub policy: Option<ProbePolicy>,
    pub parameters: Vec<ProbeParameter>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SiteAggregate {
    pub site: Site,
    pub default_policy: ProbePolicy,
    pub default_parameters: Vec<ProbeParameter>,
    pub headers: Vec<RequestHeader>,
    pub models: Vec<ModelAggregate>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplicationSnapshot {
    pub revision: u64,
    pub sites: Vec<SiteAggregate>,
    pub general_settings: GeneralSettings,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelSyncRemoval {
    pub id: EntityId,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelSyncOutcome {
    Applied(ApplicationSnapshot),
    ConfirmationRequired {
        removed_models: Vec<ModelSyncRemoval>,
    },
}

struct ModelSyncPlan {
    synchronized_models: Vec<Model>,
    removed_models: Vec<ModelSyncRemoval>,
}

pub struct CreateSite {
    pub name: String,
    pub protocol: SiteProtocol,
    pub base_url: String,
    pub enabled: bool,
    pub secret: SecretValue,
}

pub struct UpdateSite {
    pub site_id: EntityId,
    pub name: String,
    pub protocol: SiteProtocol,
    pub base_url: String,
    pub credential: CredentialIntent,
}

pub struct CreateModel {
    pub site_id: EntityId,
    pub model_key: String,
    pub enabled: bool,
}

pub struct SiteAdvancedUpdate {
    pub site_id: EntityId,
    pub models_endpoint: String,
    pub probe_endpoint: String,
    pub interval_minutes: Option<u16>,
    pub first_token_timeout_ms: u32,
    pub total_timeout_ms: u32,
    pub headers: Vec<(String, String)>,
}

pub struct ParameterValue {
    pub key: String,
    pub value_type: ParameterType,
    pub value_json: String,
}

pub struct ProbeSettingsUpdate {
    pub site_id: EntityId,
    pub model_id: Option<EntityId>,
    pub mode: ProbeMode,
    pub max_output_tokens: u16,
    pub prompt: String,
    pub parameters: Vec<ParameterValue>,
}

pub struct CustomProbeUpdate {
    pub site_id: EntityId,
    pub model_id: EntityId,
    pub enabled: bool,
    pub endpoint: String,
    pub parameters: Vec<ParameterValue>,
}

#[derive(Clone)]
pub struct ApplicationService {
    repository: Repository,
    credentials: CredentialSaga,
    model_catalog: ModelCatalogClient,
    ids: Arc<dyn IdGenerator>,
    clock: Arc<dyn Clock>,
}

impl ApplicationService {
    pub fn new(
        repository: Repository,
        credentials: CredentialSaga,
        ids: Arc<dyn IdGenerator>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            repository,
            credentials,
            model_catalog: ModelCatalogClient::default(),
            ids,
            clock,
        }
    }

    pub async fn snapshot(&self) -> Result<ApplicationSnapshot> {
        let revision = self.repository.revision().await?;
        let general_settings = self.repository.load_general_settings().await?;
        let mut aggregates = Vec::new();
        for site in self.repository.list_sites().await? {
            aggregates.push(self.load_site_aggregate(site).await?);
        }
        Ok(ApplicationSnapshot {
            revision,
            sites: aggregates,
            general_settings,
        })
    }

    pub async fn site(&self, site_id: EntityId) -> Result<Option<SiteAggregate>> {
        let site = self
            .repository
            .list_sites()
            .await?
            .into_iter()
            .find(|site| site.id == site_id);
        match site {
            Some(site) => Ok(Some(self.load_site_aggregate(site).await?)),
            None => Ok(None),
        }
    }

    pub async fn create_site(&self, input: CreateSite) -> Result<ApplicationSnapshot> {
        require_text(&input.name, "name")?;
        require_text(&input.base_url, "base_url")?;
        let now = self.clock.now();
        let sites = self.repository.list_sites().await?;
        let sort_order = next_sort_order(sites.iter().map(|site| site.sort_order))?;
        let site_id = self.ids.next_id();
        let site = Site {
            id: site_id.clone(),
            name: input.name.trim().to_owned(),
            protocol: input.protocol,
            base_url: input.base_url.trim().to_owned(),
            credential_ref: None,
            enabled: input.enabled,
            sort_order,
            models_method: HttpMethod::Get,
            models_endpoint: "/models".to_owned(),
            probe_method: HttpMethod::Post,
            probe_endpoint: "/chat/completions".to_owned(),
            content_type: "application/json".to_owned(),
            auth_preset: AuthPreset::Bearer,
            stream_enabled: true,
            timing: ProbeTimingPolicy::default(),
            lifecycle_state: LifecycleState::Ready,
            created_at: now,
            updated_at: now,
        };
        let policy = ProbePolicy {
            id: self.ids.next_id(),
            site_id,
            model_id: None,
            mode: ProbeMode::DisabledThinking,
            max_output_tokens: 8,
            prompt: "只回复 OK".to_owned(),
            custom_endpoint: None,
            custom_method: None,
            custom_stream_enabled: None,
            created_at: now,
            updated_at: now,
        };
        self.credentials
            .create_site(site, policy, input.secret)
            .await?;
        self.snapshot().await
    }

    pub async fn update_site(&self, input: UpdateSite) -> Result<ApplicationSnapshot> {
        require_text(&input.name, "name")?;
        require_text(&input.base_url, "base_url")?;
        let mut site = self.require_site(&input.site_id).await?;

        match input.credential {
            CredentialIntent::Keep => {}
            CredentialIntent::Replace(secret) => {
                self.credentials
                    .replace_site_secret(input.site_id.clone(), secret)
                    .await?;
                site = self.require_site(&input.site_id).await?;
            }
            CredentialIntent::Clear => {
                self.credentials
                    .clear_site_secret(input.site_id.clone())
                    .await?;
                site = self.require_site(&input.site_id).await?;
            }
        }
        site.name = input.name.trim().to_owned();
        site.protocol = input.protocol;
        site.base_url = input.base_url.trim().to_owned();
        site.updated_at = self.clock.now();
        self.repository.update_site(site).await?;
        self.snapshot().await
    }

    pub async fn delete_site(&self, site_id: EntityId) -> Result<ApplicationSnapshot> {
        self.credentials.delete_site(site_id).await?;
        self.snapshot().await
    }

    pub async fn reorder_sites(&self, site_ids: Vec<EntityId>) -> Result<ApplicationSnapshot> {
        self.repository.reorder_sites(site_ids).await?;
        self.snapshot().await
    }

    pub async fn set_site_enabled(
        &self,
        site_id: EntityId,
        enabled: bool,
    ) -> Result<ApplicationSnapshot> {
        let mut site = self.require_site(&site_id).await?;
        site.enabled = enabled;
        site.updated_at = self.clock.now();
        self.repository.update_site(site).await?;
        self.snapshot().await
    }

    pub async fn create_model(&self, input: CreateModel) -> Result<ApplicationSnapshot> {
        require_text(&input.model_key, "model_key")?;
        self.require_site(&input.site_id).await?;
        let models = self.repository.list_models(input.site_id.clone()).await?;
        if models.iter().any(|model| {
            model_identity_key(&model.model_key) == model_identity_key(&input.model_key)
        }) {
            return Err(ApplicationError::InvalidInput { field: "model_key" });
        }
        let sort_order = next_sort_order(models.iter().map(|model| model.sort_order))?;
        let now = self.clock.now();
        self.repository
            .create_model(Model {
                id: self.ids.next_id(),
                site_id: input.site_id,
                model_key: input.model_key.trim().to_owned(),
                display_name: None,
                source: ModelSource::Manual,
                discovery_status: DiscoveryStatus::Available,
                enabled: input.enabled,
                sort_order,
                created_at: now,
                updated_at: now,
            })
            .await?;
        self.snapshot().await
    }

    pub async fn delete_model(
        &self,
        site_id: EntityId,
        model_id: EntityId,
    ) -> Result<ApplicationSnapshot> {
        self.require_model(&site_id, &model_id).await?;
        self.repository.delete_model(site_id, model_id).await?;
        self.snapshot().await
    }

    pub async fn refresh_models(
        &self,
        site_id: EntityId,
        confirmed_removed_model_ids: Vec<EntityId>,
    ) -> Result<ModelSyncOutcome> {
        let site = self.require_site(&site_id).await?;
        let headers = self.repository.list_site_headers(site_id.clone()).await?;
        let secret = match site.auth_preset {
            AuthPreset::None => None,
            _ => {
                let reference = site
                    .credential_ref
                    .clone()
                    .ok_or(ApplicationError::CredentialUnavailable)?;
                Some(self.credentials.read_secret(reference).await?)
            }
        };
        let discovered = self
            .model_catalog
            .discover(&site, &headers, secret.as_ref())
            .await?;
        let existing = self.repository.list_models(site_id.clone()).await?;
        let plan = self.build_model_sync_plan(site_id.clone(), existing, discovered)?;
        let confirmed_removed_model_ids = confirmed_removed_model_ids.into_iter().collect();
        let expected_removed_model_ids = plan
            .removed_models
            .iter()
            .map(|model| model.id.clone())
            .collect::<HashSet<_>>();
        if !expected_removed_model_ids.is_empty()
            && expected_removed_model_ids != confirmed_removed_model_ids
        {
            return Ok(ModelSyncOutcome::ConfirmationRequired {
                removed_models: plan.removed_models,
            });
        }
        self.repository
            .sync_discovered_models(site_id, plan.synchronized_models)
            .await?;
        Ok(ModelSyncOutcome::Applied(self.snapshot().await?))
    }

    pub async fn set_model_enabled(
        &self,
        site_id: EntityId,
        model_id: EntityId,
        enabled: bool,
    ) -> Result<ApplicationSnapshot> {
        let mut model = self.require_model(&site_id, &model_id).await?;
        model.enabled = enabled;
        model.updated_at = self.clock.now();
        self.repository.update_model(model).await?;
        self.snapshot().await
    }

    pub async fn update_site_advanced(
        &self,
        input: SiteAdvancedUpdate,
    ) -> Result<ApplicationSnapshot> {
        require_text(&input.models_endpoint, "models_endpoint")?;
        require_text(&input.probe_endpoint, "probe_endpoint")?;
        let mut site = self.require_site(&input.site_id).await?;
        site.models_endpoint = input.models_endpoint;
        site.probe_endpoint = input.probe_endpoint;
        site.timing = ProbeTimingPolicy::new(
            input.interval_minutes,
            input.first_token_timeout_ms,
            input.total_timeout_ms,
            site.timing.slow_first_token_ms,
            site.timing.slow_total_ms,
            site.timing.failure_threshold,
            site.timing.recovery_threshold,
        )
        .map_err(|error| ApplicationError::InvalidInput {
            field: error.field().unwrap_or("probe_timing"),
        })?;
        site.updated_at = self.clock.now();
        self.repository.update_site(site).await?;

        let headers = input
            .headers
            .into_iter()
            .enumerate()
            .map(|(index, (name, value_template))| {
                require_text(&name, "headers")?;
                Ok(RequestHeader {
                    id: self.ids.next_id(),
                    site_id: input.site_id.clone(),
                    name,
                    value_template,
                    sort_order: u32::try_from(index)
                        .map_err(|_| ApplicationError::InvalidInput { field: "headers" })?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        self.repository
            .replace_site_headers(input.site_id, headers)
            .await?;
        self.snapshot().await
    }

    pub async fn update_probe_settings(
        &self,
        input: ProbeSettingsUpdate,
    ) -> Result<ApplicationSnapshot> {
        require_text(&input.prompt, "prompt")?;
        self.require_site(&input.site_id).await?;
        if let Some(model_id) = input.model_id.as_ref() {
            self.require_model(&input.site_id, model_id).await?;
        }
        let existing = self
            .repository
            .list_probe_policies(input.site_id.clone())
            .await?
            .into_iter()
            .find(|(policy, _)| policy.model_id == input.model_id);
        let now = self.clock.now();
        let (policy_id, created_at) = existing
            .as_ref()
            .map(|(policy, _)| (policy.id.clone(), policy.created_at))
            .unwrap_or_else(|| (self.ids.next_id(), now));
        let policy = ProbePolicy {
            id: policy_id.clone(),
            site_id: input.site_id,
            model_id: input.model_id,
            mode: input.mode,
            max_output_tokens: input.max_output_tokens,
            prompt: input.prompt,
            custom_endpoint: existing
                .as_ref()
                .and_then(|(policy, _)| policy.custom_endpoint.clone()),
            custom_method: existing
                .as_ref()
                .and_then(|(policy, _)| policy.custom_method),
            custom_stream_enabled: existing
                .as_ref()
                .and_then(|(policy, _)| policy.custom_stream_enabled),
            created_at,
            updated_at: now,
        };
        let parameters = input
            .parameters
            .into_iter()
            .enumerate()
            .map(|(index, parameter)| {
                require_text(&parameter.key, "parameters")?;
                Ok(ProbeParameter {
                    id: self.ids.next_id(),
                    policy_id: policy_id.clone(),
                    key: parameter.key,
                    value_type: parameter.value_type,
                    value_json: parameter.value_json,
                    sort_order: u32::try_from(index).map_err(|_| {
                        ApplicationError::InvalidInput {
                            field: "parameters",
                        }
                    })?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        self.repository
            .replace_probe_policy(policy, parameters)
            .await?;
        self.snapshot().await
    }

    pub async fn update_custom_probe(
        &self,
        input: CustomProbeUpdate,
    ) -> Result<ApplicationSnapshot> {
        if input.enabled {
            require_text(&input.endpoint, "custom_endpoint")?;
        }
        self.require_model(&input.site_id, &input.model_id).await?;
        let policies = self
            .repository
            .list_probe_policies(input.site_id.clone())
            .await?;
        let existing = policies
            .iter()
            .find(|(policy, _)| policy.model_id.as_ref() == Some(&input.model_id));
        let default_policy = policies
            .iter()
            .find(|(policy, _)| policy.model_id.is_none())
            .ok_or(ApplicationError::Internal)?;
        let now = self.clock.now();
        let policy = ProbePolicy {
            id: existing
                .map(|(policy, _)| policy.id.clone())
                .unwrap_or_else(|| self.ids.next_id()),
            site_id: input.site_id,
            model_id: Some(input.model_id),
            mode: existing
                .map(|(policy, _)| policy.mode)
                .unwrap_or(ProbeMode::Inherit),
            max_output_tokens: existing
                .map(|(policy, _)| policy.max_output_tokens)
                .unwrap_or(default_policy.0.max_output_tokens),
            prompt: existing
                .map(|(policy, _)| policy.prompt.clone())
                .unwrap_or_else(|| default_policy.0.prompt.clone()),
            custom_endpoint: input.enabled.then_some(input.endpoint),
            custom_method: input.enabled.then_some(HttpMethod::Post),
            custom_stream_enabled: input.enabled.then_some(true),
            created_at: existing.map(|(policy, _)| policy.created_at).unwrap_or(now),
            updated_at: now,
        };
        let policy_id = policy.id.clone();
        let parameters = input
            .parameters
            .into_iter()
            .enumerate()
            .map(|(index, parameter)| {
                require_text(&parameter.key, "parameters")?;
                Ok(ProbeParameter {
                    id: self.ids.next_id(),
                    policy_id: policy_id.clone(),
                    key: parameter.key,
                    value_type: parameter.value_type,
                    value_json: parameter.value_json,
                    sort_order: u32::try_from(index).map_err(|_| {
                        ApplicationError::InvalidInput {
                            field: "parameters",
                        }
                    })?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        self.repository
            .replace_probe_policy(policy, parameters)
            .await?;
        self.snapshot().await
    }

    pub async fn update_general_settings(
        &self,
        launch_at_startup: bool,
        continue_in_background: bool,
        theme: ThemeMode,
        history_retention_days: u16,
        diagnostic_capture_enabled: bool,
    ) -> Result<ApplicationSnapshot> {
        if !(1..=3_650).contains(&history_retention_days) {
            return Err(ApplicationError::InvalidInput {
                field: "history_retention_days",
            });
        }
        self.repository
            .update_general_settings(GeneralSettings {
                launch_at_startup,
                continue_in_background,
                theme,
                history_retention_days,
                diagnostic_capture_enabled,
                updated_at: self.clock.now(),
            })
            .await?;
        self.snapshot().await
    }

    pub async fn clear_probe_records(&self) -> Result<(CleanupResult, ApplicationSnapshot)> {
        let result = self.repository.clear_all_probe_results().await?;
        let snapshot = self.snapshot().await?;
        Ok((result, snapshot))
    }

    async fn load_site_aggregate(&self, site: Site) -> Result<SiteAggregate> {
        let policies = self.repository.list_probe_policies(site.id.clone()).await?;
        let (default_policy, default_parameters) = policies
            .iter()
            .find(|(policy, _)| policy.model_id.is_none())
            .cloned()
            .ok_or(ApplicationError::Internal)?;
        let models = self.repository.list_models(site.id.clone()).await?;
        let models = models
            .into_iter()
            .map(|model| {
                let policy = policies
                    .iter()
                    .find(|(policy, _)| policy.model_id.as_ref() == Some(&model.id));
                ModelAggregate {
                    model,
                    policy: policy.map(|(policy, _)| policy.clone()),
                    parameters: policy
                        .map(|(_, parameters)| parameters.clone())
                        .unwrap_or_default(),
                }
            })
            .collect();
        let headers = self.repository.list_site_headers(site.id.clone()).await?;
        Ok(SiteAggregate {
            site,
            default_policy,
            default_parameters,
            headers,
            models,
        })
    }

    async fn require_site(&self, site_id: &EntityId) -> Result<Site> {
        self.repository
            .list_sites()
            .await?
            .into_iter()
            .find(|site| &site.id == site_id)
            .ok_or(ApplicationError::NotFound { entity: "site" })
    }

    async fn require_model(&self, site_id: &EntityId, model_id: &EntityId) -> Result<Model> {
        self.repository
            .list_models(site_id.clone())
            .await?
            .into_iter()
            .find(|model| &model.id == model_id)
            .ok_or(ApplicationError::NotFound { entity: "model" })
    }

    fn build_model_sync_plan(
        &self,
        site_id: EntityId,
        existing: Vec<Model>,
        discovered: Vec<DiscoveredModel>,
    ) -> Result<ModelSyncPlan> {
        let now = self.clock.now();
        let mut discovered_by_key = HashMap::new();
        for model in discovered {
            discovered_by_key
                .entry(model_identity_key(&model.key))
                .or_insert(model);
        }
        let mut next_sort_order = match existing.iter().map(|model| model.sort_order).max() {
            Some(value) => value.checked_add(1).ok_or(ApplicationError::InvalidInput {
                field: "sort_order",
            })?,
            None => 0,
        };
        let mut synchronized = Vec::new();
        let mut removed = Vec::new();

        for mut model in existing {
            let remote = discovered_by_key.remove(&model_identity_key(&model.model_key));
            if model.source == ModelSource::Manual {
                continue;
            }
            match remote {
                Some(remote) => {
                    model.discovery_status = DiscoveryStatus::Available;
                    if remote.display_name.is_some() {
                        model.display_name = remote.display_name;
                    }
                    model.updated_at = now;
                    synchronized.push(model);
                }
                None => removed.push(ModelSyncRemoval {
                    id: model.id,
                    name: model.display_name.unwrap_or(model.model_key),
                }),
            }
        }

        let mut remaining = discovered_by_key.into_values().collect::<Vec<_>>();
        remaining.sort_by(|left, right| left.key.cmp(&right.key));
        for remote in remaining {
            let sort_order = next_sort_order;
            next_sort_order =
                next_sort_order
                    .checked_add(1)
                    .ok_or(ApplicationError::InvalidInput {
                        field: "sort_order",
                    })?;
            synchronized.push(Model {
                id: self.ids.next_id(),
                site_id: site_id.clone(),
                model_key: remote.key,
                display_name: remote.display_name,
                source: ModelSource::Discovered,
                discovery_status: DiscoveryStatus::Available,
                enabled: false,
                sort_order,
                created_at: now,
                updated_at: now,
            });
        }
        Ok(ModelSyncPlan {
            synchronized_models: synchronized,
            removed_models: removed,
        })
    }
}

fn model_identity_key(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn next_sort_order(orders: impl Iterator<Item = u32>) -> Result<u32> {
    orders
        .max()
        .map(|value| {
            value.checked_add(1).ok_or(ApplicationError::InvalidInput {
                field: "sort_order",
            })
        })
        .unwrap_or(Ok(0))
}

fn require_text(value: &str, field: &'static str) -> Result<()> {
    if value.trim().is_empty() {
        return Err(ApplicationError::InvalidInput { field });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{collections::VecDeque, sync::Mutex};

    use crate::{
        credentials::{CredentialService, InMemoryCredentialStore},
        domain::{FixedClock, TimestampMs},
        persistence::Database,
    };

    use super::*;

    struct SequenceIds {
        values: Mutex<VecDeque<EntityId>>,
    }

    impl SequenceIds {
        fn new(values: &[&str]) -> Self {
            Self {
                values: Mutex::new(
                    values
                        .iter()
                        .map(|value| EntityId::parse(*value).expect("fixture id is valid"))
                        .collect(),
                ),
            }
        }
    }

    impl IdGenerator for SequenceIds {
        fn next_id(&self) -> EntityId {
            self.values
                .lock()
                .expect("ids mutex poisoned")
                .pop_front()
                .expect("fixture id available")
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn configuration_workflow_uses_repository_and_credential_saga() {
        let repository = Repository::new(Database::open_in_memory().expect("database opens"));
        let store = InMemoryCredentialStore::default();
        let ids: Arc<dyn IdGenerator> = Arc::new(SequenceIds::new(&[
            "site-a",
            "policy-a",
            "credential-a",
            "operation-create",
            "model-a",
            "operation-clear",
            "operation-delete",
        ]));
        let clock: Arc<dyn Clock> = Arc::new(FixedClock::new(
            TimestampMs::new(100).expect("timestamp is valid"),
        ));
        let saga = CredentialSaga::new(
            repository.clone(),
            CredentialService::start(store.clone()).expect("credential worker starts"),
            ids.clone(),
            clock.clone(),
        );
        let service = ApplicationService::new(repository, saga, ids, clock);

        let snapshot = service
            .create_site(CreateSite {
                name: "Site A".to_owned(),
                protocol: SiteProtocol::OpenAiCompatible,
                base_url: "https://example.test/v1".to_owned(),
                enabled: true,
                secret: SecretValue::new("fixture-secret").expect("secret is valid"),
            })
            .await
            .expect("site created");
        assert_eq!(snapshot.sites.len(), 1);
        assert_eq!(snapshot.sites[0].site.id.to_string(), "site-a");
        assert_eq!(
            snapshot.sites[0]
                .site
                .credential_ref
                .as_ref()
                .map(ToString::to_string),
            Some("credential-a".to_owned())
        );
        assert!(store.matches(
            &EntityId::parse("credential-a").expect("fixture id is valid"),
            "fixture-secret"
        ));

        let snapshot = service
            .create_model(CreateModel {
                site_id: EntityId::parse("site-a").expect("fixture id is valid"),
                model_key: "model-a".to_owned(),
                enabled: true,
            })
            .await
            .expect("model created");
        assert_eq!(snapshot.sites[0].models.len(), 1);
        assert!(matches!(
            service
                .create_model(CreateModel {
                    site_id: EntityId::parse("site-a").expect("fixture id is valid"),
                    model_key: " MODEL-A ".to_owned(),
                    enabled: true,
                })
                .await,
            Err(ApplicationError::InvalidInput { field: "model_key" })
        ));

        let snapshot = service
            .update_site(UpdateSite {
                site_id: EntityId::parse("site-a").expect("fixture id is valid"),
                name: "Renamed Site".to_owned(),
                protocol: SiteProtocol::OpenAiCompatible,
                base_url: "https://renamed.example.test/v1".to_owned(),
                credential: CredentialIntent::Keep,
            })
            .await
            .expect("site updated with existing credential");
        assert_eq!(snapshot.sites[0].site.name, "Renamed Site");
        assert!(snapshot.sites[0].site.credential_ref.is_some());

        let snapshot = service
            .update_site(UpdateSite {
                site_id: EntityId::parse("site-a").expect("fixture id is valid"),
                name: "Renamed Site".to_owned(),
                protocol: SiteProtocol::OpenAiCompatible,
                base_url: "https://renamed.example.test/v1".to_owned(),
                credential: CredentialIntent::Clear,
            })
            .await
            .expect("credential cleared");
        assert!(snapshot.sites[0].site.credential_ref.is_none());
        assert!(!store.matches(
            &EntityId::parse("credential-a").expect("fixture id is valid"),
            "fixture-secret"
        ));

        let snapshot = service
            .delete_site(EntityId::parse("site-a").expect("fixture id is valid"))
            .await
            .expect("site deleted");
        assert!(snapshot.sites.is_empty());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn site_can_be_added_after_reorder_and_delete() {
        let repository = Repository::new(Database::open_in_memory().expect("database opens"));
        let store = InMemoryCredentialStore::default();
        let ids: Arc<dyn IdGenerator> = Arc::new(SequenceIds::new(&[
            "site-a",
            "policy-a",
            "credential-a",
            "operation-create-a",
            "site-b",
            "policy-b",
            "credential-b",
            "operation-create-b",
            "operation-delete-b",
            "site-c",
            "policy-c",
            "credential-c",
            "operation-create-c",
        ]));
        let clock: Arc<dyn Clock> = Arc::new(FixedClock::new(
            TimestampMs::new(100).expect("timestamp is valid"),
        ));
        let saga = CredentialSaga::new(
            repository.clone(),
            CredentialService::start(store).expect("credential worker starts"),
            ids.clone(),
            clock.clone(),
        );
        let service = ApplicationService::new(repository, saga, ids, clock);

        for name in ["Site A", "Site B"] {
            service
                .create_site(CreateSite {
                    name: name.to_owned(),
                    protocol: SiteProtocol::OpenAiCompatible,
                    base_url: "https://example.test/v1".to_owned(),
                    enabled: true,
                    secret: SecretValue::new(format!("secret-{name}")).expect("secret is valid"),
                })
                .await
                .expect("site created");
        }

        service
            .reorder_sites(vec![
                EntityId::parse("site-b").expect("fixture id is valid"),
                EntityId::parse("site-a").expect("fixture id is valid"),
            ])
            .await
            .expect("sites reordered");
        service
            .delete_site(EntityId::parse("site-b").expect("fixture id is valid"))
            .await
            .expect("leading site deleted");
        let snapshot = service
            .create_site(CreateSite {
                name: "Site C".to_owned(),
                protocol: SiteProtocol::OpenAiResponses,
                base_url: "https://responses.example.test".to_owned(),
                enabled: true,
                secret: SecretValue::new("secret-c").expect("secret is valid"),
            })
            .await
            .expect("replacement site created");

        assert_eq!(
            snapshot
                .sites
                .iter()
                .map(|aggregate| (aggregate.site.id.as_str(), aggregate.site.sort_order))
                .collect::<Vec<_>>(),
            vec![("site-a", 0), ("site-c", 1)]
        );
    }

    #[test]
    fn next_sort_order_appends_after_existing_gaps() {
        assert_eq!(next_sort_order([].into_iter()).expect("empty order"), 0);
        assert_eq!(next_sort_order([1].into_iter()).expect("gapped order"), 2);
        assert_eq!(
            next_sort_order([0, 3, 1].into_iter()).expect("unordered values"),
            4
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn discovery_sync_plan_preserves_user_state_and_reports_removed_models() {
        let repository = Repository::new(Database::open_in_memory().expect("database opens"));
        let ids: Arc<dyn IdGenerator> = Arc::new(SequenceIds::new(&["new-model"]));
        let clock: Arc<dyn Clock> = Arc::new(FixedClock::new(
            TimestampMs::new(200).expect("timestamp is valid"),
        ));
        let saga = CredentialSaga::new(
            repository.clone(),
            CredentialService::start(InMemoryCredentialStore::default())
                .expect("credential worker starts"),
            ids.clone(),
            clock.clone(),
        );
        let service = ApplicationService::new(repository, saga, ids, clock);
        let site_id = EntityId::parse("site-a").expect("fixture id is valid");
        let existing = vec![
            Model {
                id: EntityId::parse("available-model").expect("fixture id is valid"),
                site_id: site_id.clone(),
                model_key: "model-a".to_owned(),
                display_name: None,
                source: ModelSource::Discovered,
                discovery_status: DiscoveryStatus::Missing,
                enabled: true,
                sort_order: 0,
                created_at: TimestampMs::new(100).expect("timestamp is valid"),
                updated_at: TimestampMs::new(100).expect("timestamp is valid"),
            },
            Model {
                id: EntityId::parse("missing-model").expect("fixture id is valid"),
                site_id: site_id.clone(),
                model_key: "model-missing".to_owned(),
                display_name: None,
                source: ModelSource::Discovered,
                discovery_status: DiscoveryStatus::Available,
                enabled: true,
                sort_order: 1,
                created_at: TimestampMs::new(100).expect("timestamp is valid"),
                updated_at: TimestampMs::new(100).expect("timestamp is valid"),
            },
            Model {
                id: EntityId::parse("manual-model").expect("fixture id is valid"),
                site_id: site_id.clone(),
                model_key: "manual-model".to_owned(),
                display_name: None,
                source: ModelSource::Manual,
                discovery_status: DiscoveryStatus::Available,
                enabled: true,
                sort_order: 2,
                created_at: TimestampMs::new(100).expect("timestamp is valid"),
                updated_at: TimestampMs::new(100).expect("timestamp is valid"),
            },
        ];

        let plan = service
            .build_model_sync_plan(
                site_id,
                existing,
                vec![
                    DiscoveredModel {
                        key: "MODEL-A".to_owned(),
                        display_name: Some("Model A".to_owned()),
                    },
                    DiscoveredModel {
                        key: " MANUAL-MODEL ".to_owned(),
                        display_name: None,
                    },
                    DiscoveredModel {
                        key: "model-new".to_owned(),
                        display_name: None,
                    },
                    DiscoveredModel {
                        key: "MODEL-NEW".to_owned(),
                        display_name: Some("duplicate should be ignored".to_owned()),
                    },
                ],
            )
            .expect("model sync plan builds");

        assert_eq!(plan.synchronized_models.len(), 2);
        assert!(plan.synchronized_models.iter().any(|model| {
            model.id.as_str() == "available-model"
                && model.enabled
                && model.discovery_status == DiscoveryStatus::Available
                && model.display_name.as_deref() == Some("Model A")
        }));
        assert!(plan.synchronized_models.iter().any(|model| {
            model.id.as_str() == "new-model"
                && !model.enabled
                && model.sort_order == 3
                && model.discovery_status == DiscoveryStatus::Available
        }));
        assert!(!plan
            .synchronized_models
            .iter()
            .any(|model| model.source == ModelSource::Manual));
        assert_eq!(
            plan.removed_models,
            vec![ModelSyncRemoval {
                id: EntityId::parse("missing-model").expect("fixture id is valid"),
                name: "model-missing".to_owned(),
            }]
        );
    }
}
