mod config_service;
mod credential_saga;
mod model_catalog;
mod probe_executor;
mod scheduler;

fn outbound_user_agent(purpose: &str) -> String {
    format!(
        "RelayPulse/{} ({}; {}; {purpose})",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH,
    )
}

pub use config_service::{
    ApplicationError, ApplicationService, ApplicationSnapshot, CreateModel, CreateSite,
    CustomProbeUpdate, ModelAggregate, ModelSyncOutcome, ModelSyncRemoval, ParameterValue,
    ProbeRecordAggregate, ProbeSettingsUpdate, SiteAdvancedUpdate, SiteAggregate, UpdateSite,
};
pub use credential_saga::{
    CredentialMutationOutcome, CredentialSaga, CredentialSagaError, RecoverySummary,
};
pub use model_catalog::{DiscoveredModel, ModelCatalogClient, ModelCatalogError};
pub use probe_executor::{
    resolve_probe_endpoint, ProbeEndpointError, ProbeExecution, ProbeExecutor,
};
pub use scheduler::{start_scheduler, ScheduleSnapshot, ScheduledModel, SchedulerHandle};
