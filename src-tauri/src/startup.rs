use std::{
    fs,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};

use crate::{
    application::{ApplicationService, CredentialSaga, SchedulerHandle},
    credentials::{CredentialService, LocalCredentialStore},
    diagnostics::Diagnostics,
    domain::{SystemClock, UuidGenerator},
    intelligence_ladder::{
        IntelligenceLadderSchedulerHandle, IntelligenceLadderService, ProcessIntelligenceScraper,
    },
    notifications::NotificationService,
    persistence::{Database, DatabaseError, Repository},
};

const DATABASE_FILE_NAME: &str = "relaypulse.sqlite3";
const CREDENTIAL_FILE_NAME: &str = "credentials.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartupErrorCode {
    DataDirectoryUnavailable,
    DatabaseSchemaTooNew,
    DatabaseMigrationFailed,
    DatabaseUnavailable,
    CredentialWorkerUnavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartupFailure {
    code: StartupErrorCode,
    message: &'static str,
}

impl StartupFailure {
    pub fn code(&self) -> StartupErrorCode {
        self.code
    }

    pub fn message(&self) -> &'static str {
        self.message
    }

    pub fn data_directory() -> Self {
        Self {
            code: StartupErrorCode::DataDirectoryUnavailable,
            message: "RelayPulse cannot access its application data directory.",
        }
    }

    fn credential_worker() -> Self {
        Self {
            code: StartupErrorCode::CredentialWorkerUnavailable,
            message: "RelayPulse cannot start its credential worker.",
        }
    }

    fn from_database_error(error: DatabaseError) -> Self {
        match error {
            DatabaseError::SchemaTooNew { .. } => Self {
                code: StartupErrorCode::DatabaseSchemaTooNew,
                message: "The database was created by a newer RelayPulse version.",
            },
            DatabaseError::Migration(_) => Self {
                code: StartupErrorCode::DatabaseMigrationFailed,
                message: "RelayPulse could not upgrade the database safely.",
            },
            DatabaseError::Sqlite(_)
            | DatabaseError::WorkerUnavailable
            | DatabaseError::WorkerStopped
            | DatabaseError::WorkerPanicked
            | DatabaseError::InvalidInput { .. }
            | DatabaseError::NotFound { .. }
            | DatabaseError::Conflict { .. }
            | DatabaseError::CatalogChanged => Self {
                code: StartupErrorCode::DatabaseUnavailable,
                message: "RelayPulse cannot open its database.",
            },
        }
    }
}

pub struct BackendState {
    database: Option<Database>,
    application_service: Option<ApplicationService>,
    notification_service: Option<NotificationService>,
    intelligence_ladder_service: Option<IntelligenceLadderService>,
    credential_saga: Option<CredentialSaga>,
    startup_failure: Option<StartupFailure>,
    diagnostics: Option<Diagnostics>,
    continue_in_background: AtomicBool,
    scheduler: Mutex<Option<SchedulerHandle>>,
    intelligence_ladder_scheduler: Mutex<Option<IntelligenceLadderSchedulerHandle>>,
}

impl BackendState {
    pub fn initialize(data_directory: PathBuf) -> Self {
        Self::initialize_with_intelligence_callback(data_directory, |_| {})
    }

    pub fn initialize_with_intelligence_callback<F>(data_directory: PathBuf, on_change: F) -> Self
    where
        F: Fn(u64) + Send + Sync + 'static,
    {
        if fs::create_dir_all(&data_directory).is_err() {
            return Self::blocked(StartupFailure::data_directory());
        }

        let diagnostics = Diagnostics::initialize(&data_directory).ok();
        match Database::open(data_directory.join(DATABASE_FILE_NAME)) {
            Ok(database) => {
                let credentials = match CredentialService::start(LocalCredentialStore::new(
                    data_directory.join(CREDENTIAL_FILE_NAME),
                )) {
                    Ok(credentials) => credentials,
                    Err(_) => {
                        return Self::blocked_with_diagnostics(
                            StartupFailure::credential_worker(),
                            diagnostics,
                        )
                    }
                };
                let repository = Repository::new(database.clone());
                let notification_service = NotificationService::new(repository.clone())
                    .with_diagnostics(diagnostics.clone());
                let ids = Arc::new(UuidGenerator);
                let clock = Arc::new(SystemClock);
                let credential_saga = CredentialSaga::new(
                    repository.clone(),
                    credentials,
                    ids.clone(),
                    clock.clone(),
                );
                let intelligence_ladder_service = IntelligenceLadderService::with_dependencies(
                    repository.clone(),
                    Arc::new(ProcessIntelligenceScraper),
                    clock.clone(),
                    on_change,
                )
                .with_diagnostics(diagnostics.clone());
                let application_service =
                    ApplicationService::new(repository, credential_saga.clone(), ids, clock)
                        .with_diagnostics(diagnostics.clone())
                        .with_notification_service(notification_service.clone());
                Self {
                    database: Some(database),
                    application_service: Some(application_service),
                    notification_service: Some(notification_service),
                    intelligence_ladder_service: Some(intelligence_ladder_service),
                    credential_saga: Some(credential_saga),
                    startup_failure: None,
                    diagnostics,
                    continue_in_background: AtomicBool::new(true),
                    scheduler: Mutex::new(None),
                    intelligence_ladder_scheduler: Mutex::new(None),
                }
            }
            Err(error) => Self::blocked_with_diagnostics(
                StartupFailure::from_database_error(error),
                diagnostics,
            ),
        }
    }

    pub fn blocked(failure: StartupFailure) -> Self {
        Self::blocked_with_diagnostics(failure, None)
    }

    fn blocked_with_diagnostics(failure: StartupFailure, diagnostics: Option<Diagnostics>) -> Self {
        if let Some(diagnostics) = &diagnostics {
            diagnostics.record_startup_failed(&failure);
        }
        Self {
            database: None,
            application_service: None,
            notification_service: None,
            intelligence_ladder_service: None,
            credential_saga: None,
            startup_failure: Some(failure),
            diagnostics,
            continue_in_background: AtomicBool::new(false),
            scheduler: Mutex::new(None),
            intelligence_ladder_scheduler: Mutex::new(None),
        }
    }

    pub fn database(&self) -> Option<&Database> {
        self.database.as_ref()
    }

    pub fn startup_failure(&self) -> Option<&StartupFailure> {
        self.startup_failure.as_ref()
    }

    pub fn credential_saga(&self) -> Option<&CredentialSaga> {
        self.credential_saga.as_ref()
    }

    pub fn application_service(&self) -> Option<&ApplicationService> {
        self.application_service.as_ref()
    }

    pub fn notification_service(&self) -> Option<&NotificationService> {
        self.notification_service.as_ref()
    }

    pub fn intelligence_ladder_service(&self) -> Option<&IntelligenceLadderService> {
        self.intelligence_ladder_service.as_ref()
    }

    pub fn diagnostics(&self) -> Option<&Diagnostics> {
        self.diagnostics.as_ref()
    }

    pub fn continue_in_background(&self) -> bool {
        self.continue_in_background.load(Ordering::Relaxed)
    }

    pub fn set_continue_in_background(&self, value: bool) {
        self.continue_in_background.store(value, Ordering::Relaxed);
    }

    pub fn set_scheduler(&self, scheduler: SchedulerHandle) {
        if let Ok(mut current) = self.scheduler.lock() {
            *current = Some(scheduler);
        }
    }

    pub fn set_intelligence_ladder_scheduler(&self, scheduler: IntelligenceLadderSchedulerHandle) {
        if let Ok(mut current) = self.intelligence_ladder_scheduler.lock() {
            *current = Some(scheduler);
        }
    }

    pub fn is_ready(&self) -> bool {
        self.database.is_some()
    }
}

#[cfg(test)]
mod tests {
    use std::{env, fs};

    use rusqlite::Connection;

    use super::*;
    use crate::persistence::LATEST_SCHEMA_VERSION;

    #[test]
    fn initializes_a_ready_database() {
        let directory = tempfile::tempdir().expect("tempdir created");
        let state = BackendState::initialize(directory.path().to_path_buf());

        assert!(state.is_ready());
        assert!(state.database().is_some());
        assert!(state.application_service().is_some());
        assert!(state.notification_service().is_some());
        assert!(state.intelligence_ladder_service().is_some());
        assert!(state.credential_saga().is_some());
        assert!(state.startup_failure().is_none());
    }

    #[test]
    fn blocks_startup_without_replacing_a_corrupt_database() {
        let directory = tempfile::tempdir().expect("tempdir created");
        let path = directory.path().join(DATABASE_FILE_NAME);
        let original = b"not-a-sqlite-database";
        fs::write(&path, original).expect("corrupt fixture written");

        let state = BackendState::initialize(directory.path().to_path_buf());

        assert!(!state.is_ready());
        assert_eq!(
            state.startup_failure().map(StartupFailure::code),
            Some(StartupErrorCode::DatabaseUnavailable)
        );
        assert_eq!(fs::read(path).expect("fixture can be read"), original);
    }

    #[test]
    fn blocks_startup_for_a_newer_schema() {
        let directory = tempfile::tempdir().expect("tempdir created");
        let path = directory.path().join(DATABASE_FILE_NAME);
        let connection = Connection::open(&path).expect("database created");
        connection
            .pragma_update(None, "user_version", LATEST_SCHEMA_VERSION + 1)
            .expect("user version set");
        drop(connection);

        let state = BackendState::initialize(directory.path().to_path_buf());

        assert!(!state.is_ready());
        assert_eq!(
            state.startup_failure().map(StartupFailure::code),
            Some(StartupErrorCode::DatabaseSchemaTooNew)
        );
    }

    #[tokio::test(flavor = "current_thread")]
    #[ignore = "uses an explicitly selected real app database and its local credential file"]
    async fn real_model_catalog_smoke_test() {
        let data_directory = env::var_os("RELAYPULSE_REAL_DATA_DIR")
            .map(PathBuf::from)
            .expect("RELAYPULSE_REAL_DATA_DIR must select the real app data directory");
        let expected_site = env::var("RELAYPULSE_REAL_SITE")
            .expect("RELAYPULSE_REAL_SITE must select one configured site");
        let state = BackendState::initialize(data_directory);
        let service = state.application_service().unwrap_or_else(|| {
            panic!(
                "real backend state initializes: {:?}",
                state.startup_failure()
            )
        });
        let snapshot = service.snapshot().await.expect("real snapshot loads");
        let site = snapshot
            .sites
            .iter()
            .find(|aggregate| aggregate.site.name == expected_site)
            .expect("selected real site exists");

        let outcome = service
            .refresh_models(site.site.id.clone(), Vec::new())
            .await
            .expect("selected real site model catalog synchronizes");
        let refreshed = match outcome {
            crate::application::ModelSyncOutcome::Applied(snapshot) => snapshot,
            crate::application::ModelSyncOutcome::ConfirmationRequired { removed_models } => {
                let confirmed_removed_model_ids =
                    removed_models.into_iter().map(|model| model.id).collect();
                match service
                    .refresh_models(site.site.id.clone(), confirmed_removed_model_ids)
                    .await
                    .expect("selected real site model removals are confirmed")
                {
                    crate::application::ModelSyncOutcome::Applied(snapshot) => snapshot,
                    crate::application::ModelSyncOutcome::ConfirmationRequired { .. } => {
                        panic!("model catalog changed during the real sync smoke test")
                    }
                }
            }
        };
        let refreshed_site = refreshed
            .sites
            .iter()
            .find(|aggregate| aggregate.site.id == site.site.id)
            .expect("selected real site remains available");

        assert!(
            !refreshed_site.models.is_empty(),
            "selected real site returned no models"
        );
        eprintln!(
            "real model sync completed for {}: {} models, revision {}",
            refreshed_site.site.name,
            refreshed_site.models.len(),
            refreshed.revision
        );
    }

    #[tokio::test(flavor = "current_thread")]
    #[ignore = "uses an explicitly selected real app database and its local credential file"]
    async fn real_snapshot_smoke_test() {
        let data_directory = env::var_os("RELAYPULSE_REAL_DATA_DIR")
            .map(PathBuf::from)
            .expect("RELAYPULSE_REAL_DATA_DIR must select the real app data directory");
        let state = BackendState::initialize(data_directory);
        let service = state.application_service().unwrap_or_else(|| {
            panic!(
                "real backend state initializes: {:?}",
                state.startup_failure()
            )
        });
        let snapshot = service.snapshot().await.expect("real snapshot loads");

        assert!(
            !snapshot.sites.is_empty(),
            "real snapshot has configured sites"
        );
        assert!(
            snapshot.sites.iter().any(|site| !site.models.is_empty()),
            "real snapshot has configured models"
        );
        eprintln!(
            "real snapshot loaded: {} sites, {} models, revision {}",
            snapshot.sites.len(),
            snapshot
                .sites
                .iter()
                .map(|site| site.models.len())
                .sum::<usize>(),
            snapshot.revision
        );
    }
}
