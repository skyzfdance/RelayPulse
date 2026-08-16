use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
};

use thiserror::Error;

use crate::{
    credentials::CredentialError,
    diagnostics::{Diagnostics, ProbeBatchSummary},
    domain::{
        AuthPreset, Clock, CredentialIntent, DiscoveryStatus, EntityId, GeneralSettings,
        HttpMethod, IdGenerator, LatestModelStatus, LifecycleState, Model, ModelSource,
        ParameterType, ProbeCapture, ProbeMode, ProbeParameter, ProbePolicy, ProbeResult,
        ProbeRunKind, ProbeStatus, ProbeTimingPolicy, RequestHeader, SecretValue, Site,
        SiteProtocol, ThemeMode, TimestampMs, TrayAction,
    },
    notifications::{
        ModelFailureReason, ModelRecoveryState, NotificationService, PreparedNotificationEvent,
    },
    persistence::{CleanupResult, DatabaseError, Repository},
};

use super::{
    scheduler::{is_failure, stable_next_probe_at},
    CredentialSaga, CredentialSagaError, DiscoveredModel, ModelCatalogClient, ModelCatalogError,
    ProbeExecutor, ScheduleSnapshot, ScheduledModel,
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
    #[error("a probe is already running for this model")]
    Conflict,
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

fn application_error_code(error: &ApplicationError) -> &'static str {
    match error {
        ApplicationError::InvalidInput { .. } => "invalid_input",
        ApplicationError::NotFound { .. } => "not_found",
        ApplicationError::CredentialUnavailable => "credential_unavailable",
        ApplicationError::DatabaseUnavailable => "database_unavailable",
        ApplicationError::Network => "network_error",
        ApplicationError::Timeout => "timeout",
        ApplicationError::Authentication => "authentication_failed",
        ApplicationError::RateLimited => "rate_limited",
        ApplicationError::Upstream => "upstream_error",
        ApplicationError::Protocol => "protocol_error",
        ApplicationError::Conflict => "probe_conflict",
        ApplicationError::Internal => "internal",
    }
}

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
    pub latest_model_statuses: Vec<LatestModelStatus>,
    pub latest_model_results: Vec<ProbeResult>,
    pub probe_records: Vec<ProbeRecordAggregate>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeRecordAggregate {
    pub result: ProbeResult,
    pub capture: ProbeCapture,
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
    pub slow_first_token_ms: u32,
    pub slow_total_ms: u32,
    pub failure_threshold: u8,
    pub recovery_threshold: u8,
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
    probe_executor: ProbeExecutor,
    active_probes: Arc<Mutex<HashSet<EntityId>>>,
    ids: Arc<dyn IdGenerator>,
    clock: Arc<dyn Clock>,
    diagnostics: Option<Diagnostics>,
    notification_service: Option<NotificationService>,
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
            probe_executor: ProbeExecutor::default(),
            active_probes: Arc::new(Mutex::new(HashSet::new())),
            ids,
            clock,
            diagnostics: None,
            notification_service: None,
        }
    }

    pub fn with_diagnostics(mut self, diagnostics: Option<Diagnostics>) -> Self {
        self.diagnostics = diagnostics;
        self
    }

    pub fn with_notification_service(mut self, notification_service: NotificationService) -> Self {
        self.notification_service = Some(notification_service);
        self
    }

    pub(crate) fn diagnostics(&self) -> Option<&Diagnostics> {
        self.diagnostics.as_ref()
    }

    pub async fn snapshot(&self) -> Result<ApplicationSnapshot> {
        let snapshot = self.repository.snapshot().await?;
        let mut sites = Vec::with_capacity(snapshot.sites.len());
        for site in snapshot.sites {
            let policies = snapshot
                .probe_policies
                .iter()
                .filter(|(policy, _)| policy.site_id == site.id)
                .cloned()
                .collect::<Vec<_>>();
            let (default_policy, default_parameters) = policies
                .iter()
                .find(|(policy, _)| policy.model_id.is_none())
                .cloned()
                .ok_or(ApplicationError::Internal)?;
            let models = snapshot
                .models
                .iter()
                .filter(|model| model.site_id == site.id)
                .cloned()
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
            let headers = snapshot
                .site_headers
                .iter()
                .filter(|header| header.site_id == site.id)
                .cloned()
                .collect();
            sites.push(SiteAggregate {
                site,
                default_policy,
                default_parameters,
                headers,
                models,
            });
        }
        Ok(ApplicationSnapshot {
            revision: snapshot.revision,
            sites,
            general_settings: snapshot.general_settings,
            latest_model_statuses: snapshot.latest_model_statuses,
            latest_model_results: snapshot.latest_model_results,
            probe_records: snapshot
                .probe_records
                .into_iter()
                .map(|(result, capture)| ProbeRecordAggregate { result, capture })
                .collect(),
        })
    }

    pub(crate) async fn schedule_snapshot(&self) -> Result<ScheduleSnapshot> {
        let latest = self
            .repository
            .list_latest_model_statuses()
            .await?
            .into_iter()
            .map(|status| (status.model_id.clone(), status))
            .collect::<HashMap<_, _>>();
        let mut models = Vec::new();
        for site in self
            .repository
            .list_sites()
            .await?
            .into_iter()
            .filter(|site| site.enabled)
        {
            let Some(interval_minutes) = site.timing.interval_minutes else {
                continue;
            };
            for model in self
                .repository
                .list_models(site.id.clone())
                .await?
                .into_iter()
                .filter(|model| model.enabled)
            {
                models.push(ScheduledModel {
                    site_id: site.id.clone(),
                    persisted_next_probe_at: latest
                        .get(&model.id)
                        .and_then(|status| status.next_probe_at),
                    model_id: model.id,
                    interval_minutes,
                });
            }
        }
        Ok(ScheduleSnapshot {
            now: self.clock.now(),
            models,
        })
    }

    pub async fn probe_all_enabled(&self) -> Result<ApplicationSnapshot> {
        self.probe_all_enabled_with_operation(None).await
    }

    pub(crate) async fn probe_all_enabled_with_operation(
        &self,
        operation_id: Option<&str>,
    ) -> Result<ApplicationSnapshot> {
        let sites = self.snapshot().await?.sites;
        let site_count = sites
            .iter()
            .filter(|site| site.site.enabled && site.models.iter().any(|model| model.model.enabled))
            .count();
        let model_count = sites
            .iter()
            .filter(|site| site.site.enabled)
            .map(|site| {
                site.models
                    .iter()
                    .filter(|model| model.model.enabled)
                    .count()
            })
            .sum::<usize>();
        let mut completed_count = 0usize;
        let mut failed_count = 0usize;

        for site in sites.iter().filter(|site| site.site.enabled) {
            for model in site.models.iter().filter(|model| model.model.enabled) {
                match self
                    .execute_probe(site, model, ProbeRunKind::Manual, operation_id)
                    .await
                {
                    Ok(ProbeStatus::Healthy | ProbeStatus::Slow) => {
                        completed_count = completed_count.saturating_add(1);
                    }
                    Ok(_) => failed_count = failed_count.saturating_add(1),
                    Err(error) => {
                        failed_count = failed_count.saturating_add(1);
                        self.record_manual_probe_batch(
                            operation_id,
                            site_count,
                            model_count,
                            completed_count,
                            failed_count,
                        );
                        return Err(error);
                    }
                }
            }
        }
        self.record_manual_probe_batch(
            operation_id,
            site_count,
            model_count,
            completed_count,
            failed_count,
        );
        self.snapshot().await
    }

    fn record_manual_probe_batch(
        &self,
        operation_id: Option<&str>,
        site_count: usize,
        model_count: usize,
        completed_count: usize,
        failed_count: usize,
    ) {
        let Some(operation_id) = operation_id else {
            return;
        };
        let skipped_count =
            model_count.saturating_sub(completed_count.saturating_add(failed_count));
        if let Some(diagnostics) = &self.diagnostics {
            diagnostics.record_probe_batch_finished(
                Some(operation_id),
                "user_action",
                ProbeBatchSummary {
                    site_count: u64::try_from(site_count).unwrap_or(u64::MAX),
                    model_count: u64::try_from(model_count).unwrap_or(u64::MAX),
                    completed_count: u64::try_from(completed_count).unwrap_or(u64::MAX),
                    failed_count: u64::try_from(failed_count).unwrap_or(u64::MAX),
                    skipped_count: u64::try_from(skipped_count).unwrap_or(u64::MAX),
                },
            );
        }
    }

    pub async fn probe_site(&self, site_id: EntityId) -> Result<ApplicationSnapshot> {
        self.probe_site_with_operation(site_id, None).await
    }

    pub(crate) async fn probe_site_with_operation(
        &self,
        site_id: EntityId,
        operation_id: Option<&str>,
    ) -> Result<ApplicationSnapshot> {
        let site = self
            .site(site_id)
            .await?
            .ok_or(ApplicationError::NotFound { entity: "site" })?;
        for model in site.models.iter().filter(|model| model.model.enabled) {
            self.execute_probe(&site, model, ProbeRunKind::Manual, operation_id)
                .await?;
        }
        self.snapshot().await
    }

    pub async fn probe_model(
        &self,
        site_id: EntityId,
        model_id: EntityId,
    ) -> Result<ApplicationSnapshot> {
        self.probe_model_with_operation(site_id, model_id, None)
            .await
    }

    pub(crate) async fn probe_model_with_operation(
        &self,
        site_id: EntityId,
        model_id: EntityId,
        operation_id: Option<&str>,
    ) -> Result<ApplicationSnapshot> {
        let site = self
            .site(site_id)
            .await?
            .ok_or(ApplicationError::NotFound { entity: "site" })?;
        let model = site
            .models
            .iter()
            .find(|model| model.model.id == model_id)
            .ok_or(ApplicationError::NotFound { entity: "model" })?;
        self.execute_probe(&site, model, ProbeRunKind::Manual, operation_id)
            .await?;
        self.snapshot().await
    }

    pub(crate) async fn probe_scheduled_model(
        &self,
        site_id: EntityId,
        model_id: EntityId,
    ) -> Result<Option<(ApplicationSnapshot, ProbeStatus)>> {
        let Some(site) = self.site(site_id).await? else {
            return Ok(None);
        };
        if !site.site.enabled || site.site.timing.interval_minutes.is_none() {
            return Ok(None);
        }
        let Some(model) = site
            .models
            .iter()
            .find(|model| model.model.id == model_id && model.model.enabled)
        else {
            return Ok(None);
        };
        let status = self
            .execute_probe(&site, model, ProbeRunKind::Scheduled, None)
            .await?;
        self.snapshot()
            .await
            .map(|snapshot| Some((snapshot, status)))
    }

    async fn execute_probe(
        &self,
        site: &SiteAggregate,
        model: &ModelAggregate,
        run_kind: ProbeRunKind,
        operation_id: Option<&str>,
    ) -> Result<ProbeStatus> {
        let result_id = self.ids.next_id();
        if let Some(diagnostics) = &self.diagnostics {
            diagnostics.record_probe_started(
                result_id.as_str(),
                operation_id,
                run_kind,
                site.site.protocol,
            );
        }
        let mut observation = None;
        let outcome: Result<()> = async {
            let _active_probe = self.begin_probe(&model.model.id)?;
            let secret = match site.site.credential_ref.as_ref() {
                Some(reference) => Some(self.credentials.read_secret(reference.clone()).await?),
                None => None,
            };
            let effective_policy =
                effective_probe_policy(&site.default_policy, model.policy.as_ref());
            let parameters = if model.policy.is_some() {
                &model.parameters
            } else {
                &site.default_parameters
            };
            let started_at = self.clock.now();
            let execution = self
                .probe_executor
                .execute(
                    &site.site,
                    &model.model,
                    &effective_policy,
                    parameters,
                    &site.headers,
                    secret.as_ref(),
                )
                .await;
            let finished_at = self.clock.now();
            observation = Some((
                execution.status,
                execution.http_status,
                execution.total_time_ms,
                execution.error_code.clone(),
            ));
            let result = ProbeResult {
                id: result_id.clone(),
                site_id: site.site.id.clone(),
                model_id: model.model.id.clone(),
                run_kind,
                request_method: effective_policy
                    .custom_method
                    .unwrap_or(site.site.probe_method),
                endpoint: execution.endpoint.clone(),
                probe_mode: effective_policy.mode,
                started_at,
                finished_at: Some(finished_at),
                status: execution.status,
                response_headers_ms: execution.response_headers_ms,
                first_token_ms: execution.first_token_ms,
                total_time_ms: Some(execution.total_time_ms),
                http_status: execution.http_status,
                provider_model: execution.provider_model,
                finish_reason: execution.finish_reason,
                error_code: execution.error_code,
                error_message: execution.error_message,
                token_usage: execution.token_usage,
                ended_normally: execution.ended_normally,
                retry_count: 0,
                request_id: execution.request_id,
                revision: 0,
            };
            let next_probe_at = site.site.timing.interval_minutes.map(|interval_minutes| {
                stable_next_probe_at(
                    finished_at,
                    &site.site.id,
                    &model.model.id,
                    interval_minutes,
                )
            });
            let general_settings = self.repository.load_general_settings().await?;
            let mut capture = execution.capture;
            capture.probe_result_id = result_id.clone();
            if !general_settings.diagnostic_capture_enabled {
                capture.request_headers_json = "[]".to_owned();
                capture.request_body.clear();
                capture.request_captured_bytes = 0;
                capture.request_truncated = false;
                capture.response_headers_json = "[]".to_owned();
                capture.response_body.clear();
                capture.response_chunks_json = "[]".to_owned();
                capture.response_captured_bytes = 0;
                capture.response_truncated = false;
            }
            let record = self
                .repository
                .record_probe(result, capture, site.site.timing.clone(), next_probe_at)
                .await?;
            let notification_event =
                if record.became_latest && site.site.enabled && model.model.enabled {
                    match prepare_probe_notification(
                        record.previous_status.as_ref(),
                        &record.latest_status,
                        execution.status,
                        &result_id,
                        finished_at,
                        &site.site.name,
                        model
                            .model
                            .display_name
                            .as_deref()
                            .unwrap_or(&model.model.model_key),
                    ) {
                        Ok(event) => event,
                        Err(_) => {
                            if let Some(diagnostics) = &self.diagnostics {
                                diagnostics.record_background_error(
                                    "notification-publisher",
                                    "notification_prepare_failed",
                                );
                            }
                            None
                        }
                    }
                } else {
                    None
                };
            if let (Some(service), Some(event)) =
                (self.notification_service.as_ref(), notification_event)
            {
                if service.publish(event).await.is_err() {
                    if let Some(diagnostics) = &self.diagnostics {
                        diagnostics.record_background_error(
                            "notification-publisher",
                            "notification_publish_failed",
                        );
                    }
                }
            }
            if self
                .clear_expired_probe_records(general_settings.history_retention_days, finished_at)
                .await
                .is_err()
            {
                if let Some(diagnostics) = &self.diagnostics {
                    diagnostics.record_background_error(
                        "probe-history-cleanup",
                        "probe_history_cleanup_failed",
                    );
                }
            }
            Ok(())
        }
        .await;

        match outcome {
            Ok(()) => {
                if let (Some(diagnostics), Some((status, http_status, duration_ms, error_code))) =
                    (&self.diagnostics, observation.as_ref())
                {
                    diagnostics.record_probe_finished(
                        result_id.as_str(),
                        operation_id,
                        *status,
                        *http_status,
                        *duration_ms,
                        error_code.as_deref(),
                    );
                }
                Ok(observation
                    .as_ref()
                    .map(|value| value.0)
                    .unwrap_or(ProbeStatus::Unknown))
            }
            Err(error) => {
                if let Some(diagnostics) = &self.diagnostics {
                    let (status, http_status, duration_ms) = observation
                        .as_ref()
                        .map_or((ProbeStatus::Unknown, None, 0), |value| {
                            (value.0, value.1, value.2)
                        });
                    diagnostics.record_probe_finished(
                        result_id.as_str(),
                        operation_id,
                        status,
                        http_status,
                        duration_ms,
                        Some(application_error_code(&error)),
                    );
                }
                Err(error)
            }
        }
    }

    fn begin_probe(&self, model_id: &EntityId) -> Result<ActiveProbeGuard> {
        let mut active = self
            .active_probes
            .lock()
            .map_err(|_| ApplicationError::Internal)?;
        if !active.insert(model_id.clone()) {
            return Err(ApplicationError::Conflict);
        }
        Ok(ActiveProbeGuard {
            active: Arc::clone(&self.active_probes),
            model_id: model_id.clone(),
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
        let (probe_endpoint, auth_preset) = protocol_probe_defaults(input.protocol);
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
            probe_endpoint: probe_endpoint.to_owned(),
            content_type: "application/json".to_owned(),
            auth_preset,
            stream_enabled: false,
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
        apply_protocol_change(&mut site, input.protocol);
        site.name = input.name.trim().to_owned();
        site.base_url = input.base_url.trim().to_owned();
        site.updated_at = self.clock.now();

        match input.credential {
            CredentialIntent::Keep => {
                self.repository.update_site(site).await?;
            }
            CredentialIntent::Replace(secret) => {
                self.credentials.replace_site_secret(site, secret).await?;
            }
            CredentialIntent::Clear => {
                self.credentials.clear_site_secret(site).await?;
            }
        }
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
        let timing = ProbeTimingPolicy::new(
            input.interval_minutes,
            input.slow_first_token_ms,
            input.slow_total_ms,
            input.failure_threshold,
            input.recovery_threshold,
        )
        .map_err(|error| ApplicationError::InvalidInput {
            field: error.field().unwrap_or("probe_timing"),
        })?;
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
        let mut site = self.require_site(&input.site_id).await?;
        site.models_endpoint = input.models_endpoint;
        site.probe_endpoint = input.probe_endpoint;
        site.timing = timing;
        site.updated_at = self.clock.now();
        self.repository
            .update_site_with_headers(site, headers)
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
            custom_stream_enabled: input.enabled.then_some(false),
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
        tray_action: TrayAction,
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
                tray_action,
                theme,
                history_retention_days,
                diagnostic_capture_enabled,
                updated_at: self.clock.now(),
            })
            .await?;
        self.clear_expired_probe_records(history_retention_days, self.clock.now())
            .await?;
        self.snapshot().await
    }

    async fn clear_expired_probe_records(
        &self,
        retention_days: u16,
        now: crate::domain::TimestampMs,
    ) -> Result<CleanupResult> {
        const MILLISECONDS_PER_DAY: i64 = 86_400_000;
        let retention_ms = i64::from(retention_days).saturating_mul(MILLISECONDS_PER_DAY);
        let cutoff =
            crate::domain::TimestampMs::new(now.as_i64().saturating_sub(retention_ms).max(0))
                .map_err(|_| ApplicationError::Internal)?;
        self.repository
            .clear_probe_results_before(cutoff)
            .await
            .map_err(Into::into)
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

fn prepare_probe_notification(
    previous: Option<&LatestModelStatus>,
    latest: &LatestModelStatus,
    observed: ProbeStatus,
    probe_result_id: &EntityId,
    occurred_at: TimestampMs,
    site_name: &str,
    model_name: &str,
) -> std::result::Result<Option<PreparedNotificationEvent>, crate::notifications::NotificationError>
{
    let was_failure = previous.is_some_and(|status| is_failure(status.status));
    let has_failed = is_failure(latest.status);
    if !was_failure && has_failed {
        PreparedNotificationEvent::model_failed(
            probe_result_id.as_str(),
            occurred_at,
            probe_result_id.as_str(),
            latest.model_id.as_str(),
            site_name,
            model_name,
            failure_reason(observed),
        )
        .map(Some)
    } else if was_failure && matches!(latest.status, ProbeStatus::Healthy | ProbeStatus::Slow) {
        let state = if latest.status == ProbeStatus::Slow {
            ModelRecoveryState::Slow
        } else {
            ModelRecoveryState::Healthy
        };
        PreparedNotificationEvent::model_recovered(
            probe_result_id.as_str(),
            occurred_at,
            probe_result_id.as_str(),
            latest.model_id.as_str(),
            site_name,
            model_name,
            state,
        )
        .map(Some)
    } else {
        Ok(None)
    }
}

fn failure_reason(status: ProbeStatus) -> ModelFailureReason {
    match status {
        ProbeStatus::AuthError => ModelFailureReason::Authentication,
        ProbeStatus::RateLimited => ModelFailureReason::RateLimited,
        ProbeStatus::Timeout => ModelFailureReason::Timeout,
        ProbeStatus::NetworkError => ModelFailureReason::Network,
        ProbeStatus::ModelUnavailable | ProbeStatus::UpstreamError => ModelFailureReason::Upstream,
        ProbeStatus::ProtocolError | ProbeStatus::ValidationFailed => ModelFailureReason::Protocol,
        ProbeStatus::Healthy
        | ProbeStatus::Slow
        | ProbeStatus::Probing
        | ProbeStatus::Cancelled
        | ProbeStatus::Unknown => ModelFailureReason::Unknown,
    }
}

struct ActiveProbeGuard {
    active: Arc<Mutex<HashSet<EntityId>>>,
    model_id: EntityId,
}

impl Drop for ActiveProbeGuard {
    fn drop(&mut self) {
        if let Ok(mut active) = self.active.lock() {
            active.remove(&self.model_id);
        }
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

fn protocol_probe_defaults(protocol: SiteProtocol) -> (&'static str, AuthPreset) {
    match protocol {
        SiteProtocol::OpenAiCompatible => ("/chat/completions", AuthPreset::Bearer),
        SiteProtocol::OpenAiResponses => ("/responses", AuthPreset::Bearer),
        SiteProtocol::Anthropic => ("/messages", AuthPreset::XApiKey),
        SiteProtocol::CustomHttp => ("/probe", AuthPreset::None),
    }
}

fn apply_protocol_change(site: &mut Site, protocol: SiteProtocol) {
    if site.protocol == protocol {
        return;
    }
    let previous_default_endpoint = protocol_probe_defaults(site.protocol).0;
    let (next_default_endpoint, auth_preset) = protocol_probe_defaults(protocol);
    if site.probe_endpoint == previous_default_endpoint {
        site.probe_endpoint = next_default_endpoint.to_owned();
    }
    site.protocol = protocol;
    site.auth_preset = auth_preset;
}

fn effective_probe_policy(
    site_default: &ProbePolicy,
    model_policy: Option<&ProbePolicy>,
) -> ProbePolicy {
    let mut effective = model_policy
        .cloned()
        .unwrap_or_else(|| site_default.clone());
    if effective.mode == ProbeMode::Inherit {
        effective.mode = match site_default.mode {
            ProbeMode::Inherit => ProbeMode::DisabledThinking,
            mode => mode,
        };
    }
    effective
}

#[cfg(test)]
mod tests {
    use std::{
        collections::VecDeque,
        fs,
        io::{Read, Write},
        net::TcpListener,
        sync::Mutex,
        thread,
    };

    use crate::{
        credentials::{CredentialService, InMemoryCredentialStore},
        domain::{FixedClock, ProbeStatus, TimestampMs},
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

    #[test]
    fn effective_probe_policy_resolves_model_inheritance() {
        let site_id = EntityId::parse("site-a").expect("site id is valid");
        let model_id = EntityId::parse("model-a").expect("model id is valid");
        let timestamp = TimestampMs::new(100).expect("timestamp is valid");
        let site_default = ProbePolicy {
            id: EntityId::parse("policy-default").expect("policy id is valid"),
            site_id: site_id.clone(),
            model_id: None,
            mode: ProbeMode::MinimumReasoning,
            max_output_tokens: 8,
            prompt: "只回复 OK".to_owned(),
            custom_endpoint: None,
            custom_method: None,
            custom_stream_enabled: None,
            created_at: timestamp,
            updated_at: timestamp,
        };
        let mut model_policy = site_default.clone();
        model_policy.id = EntityId::parse("policy-model").expect("policy id is valid");
        model_policy.model_id = Some(model_id);
        model_policy.mode = ProbeMode::Inherit;

        let effective = effective_probe_policy(&site_default, Some(&model_policy));

        assert_eq!(effective.mode, ProbeMode::MinimumReasoning);

        model_policy.mode = ProbeMode::DisabledThinking;
        assert_eq!(
            effective_probe_policy(&site_default, Some(&model_policy)).mode,
            ProbeMode::DisabledThinking
        );
        assert_eq!(
            effective_probe_policy(&site_default, None).mode,
            ProbeMode::MinimumReasoning
        );
    }

    #[test]
    fn protocol_changes_migrate_defaults_and_preserve_custom_endpoints() {
        let mut site = crate::persistence::repository::test_support::site("site-a", 0);

        apply_protocol_change(&mut site, SiteProtocol::Anthropic);
        assert_eq!(site.probe_endpoint, "/messages");
        assert_eq!(site.protocol, SiteProtocol::Anthropic);
        assert_eq!(site.auth_preset, AuthPreset::XApiKey);

        site.probe_endpoint = "/custom-probe".to_owned();
        apply_protocol_change(&mut site, SiteProtocol::OpenAiResponses);
        assert_eq!(site.probe_endpoint, "/custom-probe");
        assert_eq!(site.auth_preset, AuthPreset::Bearer);
    }

    #[test]
    fn notifications_are_created_only_for_stable_failure_edges() {
        let model_id = EntityId::parse("model-a").expect("model id is valid");
        let probe_id = EntityId::parse("probe-a").expect("probe id is valid");
        let occurred_at = TimestampMs::new(1_700_000_000_000).expect("timestamp is valid");
        let status = |value| LatestModelStatus {
            model_id: model_id.clone(),
            probe_result_id: Some(probe_id.clone()),
            status: value,
            consecutive_failures: u16::from(is_failure(value)),
            consecutive_successes: u16::from(matches!(
                value,
                ProbeStatus::Healthy | ProbeStatus::Slow
            )),
            next_probe_at: None,
            revision: 0,
            updated_at: occurred_at,
        };
        let healthy = status(ProbeStatus::Healthy);
        let failed = status(ProbeStatus::Timeout);

        let failure = prepare_probe_notification(
            Some(&healthy),
            &failed,
            ProbeStatus::Timeout,
            &probe_id,
            occurred_at,
            "Site A",
            "Model A",
        )
        .expect("notification preparation succeeds")
        .expect("healthy to failed creates an event");
        assert_eq!(failure.event_type, "probe.model_failed");
        assert!(prepare_probe_notification(
            Some(&failed),
            &failed,
            ProbeStatus::Timeout,
            &probe_id,
            occurred_at,
            "Site A",
            "Model A",
        )
        .expect("notification preparation succeeds")
        .is_none());

        let recovery = prepare_probe_notification(
            Some(&failed),
            &healthy,
            ProbeStatus::Healthy,
            &probe_id,
            occurred_at,
            "Site A",
            "Model A",
        )
        .expect("notification preparation succeeds")
        .expect("failed to healthy creates an event");
        assert_eq!(recovery.event_type, "probe.model_recovered");

        assert!(prepare_probe_notification(
            Some(&healthy),
            &failed,
            ProbeStatus::Timeout,
            &probe_id,
            occurred_at,
            &"x".repeat(121),
            "Model A",
        )
        .is_err());
    }

    fn serve_probe() -> (String, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("probe server binds");
        let address = listener
            .local_addr()
            .expect("probe server address is available");
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("probe request connects");
            let mut request = [0u8; 16 * 1024];
            let _ = stream
                .read(&mut request)
                .expect("probe request is readable");
            let response = concat!(
                "HTTP/1.1 200 OK\r\n",
                "Content-Type: text/event-stream\r\n",
                "Connection: close\r\n\r\n",
                "data: {\"choices\":[{\"delta\":{\"content\":\"OK\"},\"finish_reason\":\"stop\"}],",
                "\"usage\":{\"prompt_tokens\":4,\"completion_tokens\":1,\"total_tokens\":5}}\n\n",
                "data: [DONE]\n\n"
            );
            stream
                .write_all(response.as_bytes())
                .expect("probe response is writable");
        });
        (format!("http://{address}"), handle)
    }

    #[tokio::test(flavor = "current_thread")]
    async fn single_model_probe_ignores_enabled_state_and_returns_persisted_usage() {
        let (base_url, server) = serve_probe();
        let diagnostics_directory = tempfile::tempdir().expect("temporary directory created");
        let diagnostics =
            Diagnostics::initialize(diagnostics_directory.path()).expect("diagnostics initialized");
        let repository = Repository::new(Database::open_in_memory().expect("database opens"));
        let store = InMemoryCredentialStore::default();
        let ids: Arc<dyn IdGenerator> = Arc::new(SequenceIds::new(&[
            "site-a",
            "policy-a",
            "credential-a",
            "operation-create",
            "model-a",
            "result-a",
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
        let service = ApplicationService::new(repository, saga, ids, clock)
            .with_diagnostics(Some(diagnostics));

        service
            .create_site(CreateSite {
                name: "Site A".to_owned(),
                protocol: SiteProtocol::OpenAiCompatible,
                base_url,
                enabled: true,
                secret: SecretValue::new("fixture-secret").expect("secret is valid"),
            })
            .await
            .expect("site is created");
        service
            .create_model(CreateModel {
                site_id: EntityId::parse("site-a").expect("site id is valid"),
                model_key: "model-a".to_owned(),
                enabled: false,
            })
            .await
            .expect("disabled model is created");

        let snapshot = service
            .probe_model_with_operation(
                EntityId::parse("site-a").expect("site id is valid"),
                EntityId::parse("model-a").expect("model id is valid"),
                Some("op_fixture"),
            )
            .await
            .expect("manual probe succeeds for a disabled model");

        assert_eq!(snapshot.probe_records.len(), 1);
        assert_eq!(snapshot.probe_records[0].result.id.as_str(), "result-a");
        assert_eq!(
            snapshot.probe_records[0].result.status,
            ProbeStatus::Healthy
        );
        assert_eq!(
            snapshot.probe_records[0].result.token_usage.total_tokens,
            Some(5)
        );
        assert_eq!(snapshot.latest_model_statuses.len(), 1);
        assert_eq!(
            snapshot.latest_model_statuses[0].status,
            ProbeStatus::Healthy
        );
        assert!(snapshot.latest_model_statuses[0].next_probe_at.is_some());
        assert_eq!(snapshot.latest_model_results.len(), 1);
        assert!(!snapshot.sites[0].models[0].model.enabled);
        let log = fs::read_to_string(diagnostics_directory.path().join("logs/relaypulse.log"))
            .expect("diagnostic log reads");
        assert!(log.contains(r#""event":"probe-started""#));
        assert!(log.contains(r#""probeId":"result-a""#));
        assert!(log.contains(r#""operationId":"op_fixture""#));
        assert!(log.contains(r#""event":"probe-finished""#));
        server.join().expect("probe server stops");
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
                protocol: SiteProtocol::Anthropic,
                base_url: "https://renamed.example.test/v1".to_owned(),
                credential: CredentialIntent::Keep,
            })
            .await
            .expect("site updated with existing credential");
        assert_eq!(snapshot.sites[0].site.name, "Renamed Site");
        assert!(snapshot.sites[0].site.credential_ref.is_some());
        assert_eq!(snapshot.sites[0].site.auth_preset, AuthPreset::XApiKey);
        assert_eq!(snapshot.sites[0].site.probe_endpoint, "/messages");

        let snapshot = service
            .update_site(UpdateSite {
                site_id: EntityId::parse("site-a").expect("fixture id is valid"),
                name: "Cleared Site".to_owned(),
                protocol: SiteProtocol::OpenAiResponses,
                base_url: "https://responses.example.test/v1".to_owned(),
                credential: CredentialIntent::Clear,
            })
            .await
            .expect("credential cleared");
        assert_eq!(snapshot.sites[0].site.name, "Cleared Site");
        assert_eq!(
            snapshot.sites[0].site.protocol,
            SiteProtocol::OpenAiResponses
        );
        assert_eq!(snapshot.sites[0].site.auth_preset, AuthPreset::Bearer);
        assert_eq!(
            snapshot.sites[0].site.base_url,
            "https://responses.example.test/v1"
        );
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
        assert_eq!(snapshot.sites[1].site.probe_endpoint, "/responses");
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
