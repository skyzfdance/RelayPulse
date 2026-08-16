use std::sync::Arc;

use thiserror::Error;
use tokio::sync::Mutex as AsyncMutex;

use crate::{
    credentials::{CredentialError, CredentialService},
    domain::{
        Clock, CredentialAction, CredentialOperation, CredentialOperationStatus, EntityId,
        IdGenerator, ProbePolicy, SecretValue, Site,
    },
    persistence::{DatabaseError, Repository},
};

#[derive(Debug, Error)]
pub enum CredentialSagaError {
    #[error(transparent)]
    Credential(#[from] CredentialError),
    #[error(transparent)]
    Database(#[from] DatabaseError),
}

pub type Result<T> = std::result::Result<T, CredentialSagaError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CredentialMutationOutcome {
    pub revision: u64,
    pub cleanup_pending: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecoverySummary {
    pub recovered: u64,
    pub failed: u64,
    pub remaining: u64,
}

#[derive(Clone)]
pub struct CredentialSaga {
    repository: Repository,
    credentials: CredentialService,
    ids: Arc<dyn IdGenerator>,
    clock: Arc<dyn Clock>,
    operation_gate: Arc<AsyncMutex<()>>,
}

impl CredentialSaga {
    pub fn new(
        repository: Repository,
        credentials: CredentialService,
        ids: Arc<dyn IdGenerator>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            repository,
            credentials,
            ids,
            clock,
            operation_gate: Arc::new(AsyncMutex::new(())),
        }
    }

    pub async fn create_site(
        &self,
        site: Site,
        default_policy: ProbePolicy,
        secret: SecretValue,
    ) -> Result<CredentialMutationOutcome> {
        let _operation_guard = self.operation_gate.lock().await;
        let credential_ref = self.ids.next_id();
        let operation_id = self.ids.next_id();
        let now = self.clock.now();
        let operation = CredentialOperation {
            id: operation_id.clone(),
            site_id: site.id.clone(),
            action: CredentialAction::Create,
            status: CredentialOperationStatus::IntentRecorded,
            new_credential_ref: Some(credential_ref.clone()),
            old_credential_ref: None,
            non_secret_payload_json: "{}".to_owned(),
            created_at: now,
            updated_at: now,
            completed_at: None,
        };
        self.repository
            .begin_credential_site_create(site.clone(), default_policy, operation)
            .await?;

        if let Err(error) = self.credentials.put(credential_ref, secret).await {
            return Err(error.into());
        }

        self.repository
            .mark_credential_written(operation_id.clone(), self.clock.now())
            .await?;
        let revision = self
            .repository
            .commit_credential_configuration(operation_id, self.clock.now(), false)
            .await?;
        Ok(CredentialMutationOutcome {
            revision,
            cleanup_pending: false,
        })
    }

    pub async fn replace_site_secret(
        &self,
        site: Site,
        secret: SecretValue,
    ) -> Result<CredentialMutationOutcome> {
        let _operation_guard = self.operation_gate.lock().await;
        let site_id = site.id.clone();
        let new_credential_ref = self.ids.next_id();
        let operation_id = self.ids.next_id();
        let operation = self
            .repository
            .begin_existing_credential_operation(
                operation_id.clone(),
                site_id.clone(),
                CredentialAction::Replace,
                Some(new_credential_ref.clone()),
                Some(site),
                self.clock.now(),
            )
            .await?;

        if let Err(error) = self.credentials.put(new_credential_ref, secret).await {
            return Err(error.into());
        }

        self.repository
            .mark_credential_written(operation_id.clone(), self.clock.now())
            .await?;
        let needs_cleanup = operation.old_credential_ref.is_some();
        let revision = self
            .repository
            .commit_credential_configuration(operation_id.clone(), self.clock.now(), needs_cleanup)
            .await?;

        if let Some(old_credential_ref) = operation.old_credential_ref {
            if self.credentials.delete(old_credential_ref).await.is_err() {
                return Ok(CredentialMutationOutcome {
                    revision,
                    cleanup_pending: true,
                });
            }
            let revision = self
                .repository
                .complete_credential_cleanup(operation_id, self.clock.now())
                .await?;
            return Ok(CredentialMutationOutcome {
                revision,
                cleanup_pending: false,
            });
        }

        Ok(CredentialMutationOutcome {
            revision,
            cleanup_pending: false,
        })
    }

    pub async fn clear_site_secret(&self, site: Site) -> Result<CredentialMutationOutcome> {
        let _operation_guard = self.operation_gate.lock().await;
        self.remove_site_credential(site.id.clone(), CredentialAction::Clear, Some(site))
            .await
    }

    pub async fn delete_site(&self, site_id: EntityId) -> Result<CredentialMutationOutcome> {
        let _operation_guard = self.operation_gate.lock().await;
        self.remove_site_credential(site_id, CredentialAction::Delete, None)
            .await
    }

    pub async fn read_secret(&self, reference: EntityId) -> Result<SecretValue> {
        self.credentials.get(reference).await.map_err(Into::into)
    }

    pub async fn reconcile(&self) -> Result<RecoverySummary> {
        let _operation_guard = self.operation_gate.lock().await;
        let operations = self.repository.list_pending_credential_operations().await?;
        let mut recovered = 0_u64;
        let mut failed = 0_u64;
        for operation in operations {
            match self.recover_operation(&operation).await {
                Ok(true) => recovered += 1,
                Ok(false) => {}
                Err(error) => {
                    failed += 1;
                    if operation.status != CredentialOperationStatus::CleanupPending
                        && Self::is_irrecoverable_recovery_error(&error)
                    {
                        let _ = self.quarantine_operation(&operation).await;
                    }
                }
            }
        }
        let remaining = u64::try_from(
            self.repository
                .list_pending_credential_operations()
                .await?
                .len(),
        )
        .unwrap_or(u64::MAX);
        Ok(RecoverySummary {
            recovered,
            failed,
            remaining,
        })
    }

    async fn remove_site_credential(
        &self,
        site_id: EntityId,
        action: CredentialAction,
        site_update: Option<Site>,
    ) -> Result<CredentialMutationOutcome> {
        let operation_id = self.ids.next_id();
        let operation = self
            .repository
            .begin_existing_credential_operation(
                operation_id.clone(),
                site_id.clone(),
                action,
                None,
                site_update,
                self.clock.now(),
            )
            .await?;

        if let Some(old_credential_ref) = operation.old_credential_ref {
            if let Err(error) = self.credentials.delete(old_credential_ref).await {
                return Err(error.into());
            }
        }

        self.repository
            .mark_credential_written(operation_id.clone(), self.clock.now())
            .await?;
        let revision = self
            .repository
            .commit_credential_configuration(operation_id, self.clock.now(), false)
            .await?;
        Ok(CredentialMutationOutcome {
            revision,
            cleanup_pending: false,
        })
    }

    async fn recover_operation(&self, operation: &CredentialOperation) -> Result<bool> {
        if operation.status == CredentialOperationStatus::CleanupPending {
            if let Some(old_credential_ref) = operation.old_credential_ref.clone() {
                if self.credentials.delete(old_credential_ref).await.is_err() {
                    return Ok(false);
                }
            }
            self.repository
                .complete_credential_cleanup(operation.id.clone(), self.clock.now())
                .await?;
            return Ok(true);
        }

        match operation.action {
            CredentialAction::Create | CredentialAction::Replace => {
                let new_credential_ref =
                    operation
                        .new_credential_ref
                        .clone()
                        .ok_or(DatabaseError::InvalidInput {
                            field: "new_credential_ref",
                        })?;
                let exists = match self.credentials.exists(new_credential_ref).await {
                    Ok(exists) => exists,
                    Err(_) => return Ok(false),
                };
                if !exists {
                    self.repository
                        .abort_credential_operation(
                            operation.id.clone(),
                            operation.site_id.clone(),
                            operation.action == CredentialAction::Create,
                            self.clock.now(),
                        )
                        .await?;
                    return Ok(true);
                }

                self.mark_written_if_needed(operation).await?;
                let cleanup_pending = operation.action == CredentialAction::Replace
                    && operation.old_credential_ref.is_some();
                self.repository
                    .commit_credential_configuration(
                        operation.id.clone(),
                        self.clock.now(),
                        cleanup_pending,
                    )
                    .await?;
                if let Some(old_credential_ref) = operation.old_credential_ref.clone() {
                    if self.credentials.delete(old_credential_ref).await.is_err() {
                        return Ok(false);
                    }
                    self.repository
                        .complete_credential_cleanup(operation.id.clone(), self.clock.now())
                        .await?;
                }
                Ok(true)
            }
            CredentialAction::Clear | CredentialAction::Delete => {
                if let Some(old_credential_ref) = operation.old_credential_ref.as_ref() {
                    let exists = match self.credentials.exists(old_credential_ref.clone()).await {
                        Ok(exists) => exists,
                        Err(_) => return Ok(false),
                    };
                    if exists
                        && self
                            .credentials
                            .delete(old_credential_ref.clone())
                            .await
                            .is_err()
                    {
                        return Ok(false);
                    }
                }
                self.mark_written_if_needed(operation).await?;
                self.repository
                    .commit_credential_configuration(operation.id.clone(), self.clock.now(), false)
                    .await?;
                Ok(true)
            }
        }
    }

    async fn mark_written_if_needed(&self, operation: &CredentialOperation) -> Result<()> {
        if operation.status == CredentialOperationStatus::IntentRecorded {
            self.repository
                .mark_credential_written(operation.id.clone(), self.clock.now())
                .await?;
        }
        Ok(())
    }

    fn is_irrecoverable_recovery_error(error: &CredentialSagaError) -> bool {
        matches!(
            error,
            CredentialSagaError::Database(
                DatabaseError::InvalidInput { .. } | DatabaseError::NotFound { .. }
            )
        )
    }

    async fn quarantine_operation(&self, operation: &CredentialOperation) -> Result<()> {
        if matches!(
            operation.action,
            CredentialAction::Create | CredentialAction::Replace
        ) {
            if let Some(new_credential_ref) = operation.new_credential_ref.clone() {
                match self.credentials.exists(new_credential_ref.clone()).await {
                    Ok(true) => self.credentials.delete(new_credential_ref).await?,
                    Ok(false) => {}
                    Err(error) => return Err(error.into()),
                }
            }
        }
        self.repository
            .quarantine_credential_operation(
                operation.id.clone(),
                operation.site_id.clone(),
                operation.action,
                self.clock.now(),
            )
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::VecDeque,
        sync::{mpsc, Mutex},
        time::Duration,
    };

    use crate::{
        credentials::{CredentialError, CredentialStore, InMemoryCredentialStore},
        domain::{FixedClock, LifecycleState, TimestampMs},
        persistence::Database,
    };

    use super::*;
    use crate::persistence::repository::test_support::{id, site, site_policy};

    struct SequenceIds {
        values: Mutex<VecDeque<EntityId>>,
    }

    impl SequenceIds {
        fn new(values: &[&str]) -> Self {
            Self {
                values: Mutex::new(values.iter().map(|value| id(value)).collect()),
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

    fn saga(
        repository: Repository,
        memory: &InMemoryCredentialStore,
        ids: &[&str],
    ) -> CredentialSaga {
        CredentialSaga::new(
            repository,
            CredentialService::start(memory.clone()).expect("credential service starts"),
            Arc::new(SequenceIds::new(ids)),
            Arc::new(FixedClock::new(
                TimestampMs::new(100).expect("timestamp is valid"),
            )),
        )
    }

    struct BlockingExistsStore {
        inner: InMemoryCredentialStore,
        entered: Mutex<Option<mpsc::SyncSender<()>>>,
        release: Mutex<mpsc::Receiver<()>>,
    }

    impl CredentialStore for BlockingExistsStore {
        fn put(&self, reference: &EntityId, secret: SecretValue) -> crate::credentials::Result<()> {
            self.inner.put(reference, secret)
        }

        fn get(&self, reference: &EntityId) -> crate::credentials::Result<SecretValue> {
            self.inner.get(reference)
        }

        fn exists(&self, reference: &EntityId) -> crate::credentials::Result<bool> {
            if let Some(entered) = self.entered.lock().expect("entered mutex poisoned").take() {
                entered.send(()).map_err(|_| CredentialError::Unavailable)?;
                self.release
                    .lock()
                    .expect("release mutex poisoned")
                    .recv_timeout(Duration::from_secs(2))
                    .map_err(|_| CredentialError::Unavailable)?;
            }
            self.inner.exists(reference)
        }

        fn delete(&self, reference: &EntityId) -> crate::credentials::Result<()> {
            self.inner.delete(reference)
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn create_site_commits_only_after_the_secret_is_written() {
        let repository = Repository::new(Database::open_in_memory().expect("database opens"));
        let memory = InMemoryCredentialStore::default();
        let saga = saga(
            repository.clone(),
            &memory,
            &["credential-a", "operation-a"],
        );
        let site = site("site-a", 0);

        let outcome = saga
            .create_site(
                site.clone(),
                site_policy(&site.id),
                SecretValue::new("secret-a").expect("secret is valid"),
            )
            .await
            .expect("site created");

        assert_eq!(outcome.revision, 3);
        assert!(!outcome.cleanup_pending);
        assert!(memory.matches(&id("credential-a"), "secret-a"));
        let sites = repository.list_sites().await.expect("sites read");
        assert_eq!(sites.len(), 1);
        assert_eq!(sites[0].credential_ref, Some(id("credential-a")));
        assert_eq!(sites[0].lifecycle_state, LifecycleState::Ready);
        let operations = repository
            .list_credential_operations()
            .await
            .expect("operations read");
        assert_eq!(operations.len(), 1);
        assert_eq!(
            operations[0].status,
            CredentialOperationStatus::ConfigCommitted
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn create_failure_keeps_the_intent_until_reconciliation_confirms_no_secret() {
        let repository = Repository::new(Database::open_in_memory().expect("database opens"));
        let memory = InMemoryCredentialStore::default();
        memory.fail_next(CredentialError::Unavailable);
        let saga = saga(
            repository.clone(),
            &memory,
            &["credential-a", "operation-a"],
        );
        let site = site("site-a", 0);

        assert!(matches!(
            saga.create_site(
                site.clone(),
                site_policy(&site.id),
                SecretValue::new("secret-a").expect("secret is valid"),
            )
            .await,
            Err(CredentialSagaError::Credential(
                CredentialError::Unavailable
            ))
        ));
        let sites = repository.list_sites().await.expect("sites read");
        assert_eq!(sites.len(), 1);
        assert_eq!(sites[0].lifecycle_state, LifecycleState::CredentialPending);
        let operations = repository
            .list_pending_credential_operations()
            .await
            .expect("pending operations read");
        assert_eq!(operations.len(), 1);
        assert_eq!(
            operations[0].status,
            CredentialOperationStatus::IntentRecorded
        );
        assert!(!memory.matches(&id("credential-a"), "secret-a"));

        assert_eq!(
            saga.reconcile().await.expect("reconciliation succeeds"),
            RecoverySummary {
                recovered: 1,
                failed: 0,
                remaining: 0,
            }
        );
        assert!(repository
            .list_sites()
            .await
            .expect("sites read")
            .is_empty());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn reconciliation_commits_a_create_when_put_failed_after_writing() {
        let repository = Repository::new(Database::open_in_memory().expect("database opens"));
        let memory = InMemoryCredentialStore::default();
        memory.fail_next_put_after_write(CredentialError::Unavailable);
        let saga = saga(
            repository.clone(),
            &memory,
            &["credential-a", "operation-a"],
        );
        let site = site("site-a", 0);

        assert!(matches!(
            saga.create_site(
                site.clone(),
                site_policy(&site.id),
                SecretValue::new("secret-a").expect("secret is valid"),
            )
            .await,
            Err(CredentialSagaError::Credential(
                CredentialError::Unavailable
            ))
        ));
        assert!(memory.matches(&id("credential-a"), "secret-a"));
        assert_eq!(
            repository.list_sites().await.expect("sites read")[0].lifecycle_state,
            LifecycleState::CredentialPending
        );

        assert_eq!(
            saga.reconcile().await.expect("reconciliation succeeds"),
            RecoverySummary {
                recovered: 1,
                failed: 0,
                remaining: 0,
            }
        );
        let committed_site = &repository.list_sites().await.expect("sites read")[0];
        assert_eq!(committed_site.credential_ref, Some(id("credential-a")));
        assert_eq!(committed_site.lifecycle_state, LifecycleState::Ready);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn reconciliation_commits_a_replacement_when_put_failed_after_writing() {
        let repository = Repository::new(Database::open_in_memory().expect("database opens"));
        let memory = InMemoryCredentialStore::default();
        let saga = saga(
            repository.clone(),
            &memory,
            &[
                "credential-old",
                "operation-create",
                "credential-new",
                "operation-replace",
            ],
        );
        let site = site("site-a", 0);
        saga.create_site(
            site.clone(),
            site_policy(&site.id),
            SecretValue::new("old-secret").expect("secret is valid"),
        )
        .await
        .expect("site created");
        memory.fail_next_put_after_write(CredentialError::Unavailable);

        assert!(matches!(
            saga.replace_site_secret(
                site.clone(),
                SecretValue::new("new-secret").expect("secret is valid"),
            )
            .await,
            Err(CredentialSagaError::Credential(
                CredentialError::Unavailable
            ))
        ));
        assert!(memory.matches(&id("credential-old"), "old-secret"));
        assert!(memory.matches(&id("credential-new"), "new-secret"));

        assert_eq!(
            saga.reconcile().await.expect("reconciliation succeeds"),
            RecoverySummary {
                recovered: 1,
                failed: 0,
                remaining: 0,
            }
        );
        assert!(!memory.matches(&id("credential-old"), "old-secret"));
        assert!(memory.matches(&id("credential-new"), "new-secret"));
        assert_eq!(
            repository.list_sites().await.expect("sites read")[0].credential_ref,
            Some(id("credential-new"))
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn replace_cleanup_is_recovered_and_delete_removes_the_active_secret() {
        let repository = Repository::new(Database::open_in_memory().expect("database opens"));
        let memory = InMemoryCredentialStore::default();
        let saga = saga(
            repository.clone(),
            &memory,
            &[
                "credential-old",
                "operation-create",
                "credential-new",
                "operation-replace",
                "operation-delete",
            ],
        );
        let site = site("site-a", 0);
        saga.create_site(
            site.clone(),
            site_policy(&site.id),
            SecretValue::new("old-secret").expect("secret is valid"),
        )
        .await
        .expect("site created");

        let mut updated_site = site.clone();
        updated_site.name = "Updated Site".to_owned();
        updated_site.protocol = crate::domain::SiteProtocol::Anthropic;
        updated_site.base_url = "https://anthropic.example.test".to_owned();
        updated_site.auth_preset = crate::domain::AuthPreset::XApiKey;
        memory.fail_next_delete(CredentialError::Unavailable);
        let outcome = saga
            .replace_site_secret(
                updated_site.clone(),
                SecretValue::new("new-secret").expect("secret is valid"),
            )
            .await
            .expect("new secret committed");
        assert!(outcome.cleanup_pending);
        assert!(memory.matches(&id("credential-old"), "old-secret"));
        assert!(memory.matches(&id("credential-new"), "new-secret"));
        let committed_site = repository
            .list_sites()
            .await
            .expect("sites read")
            .into_iter()
            .next()
            .expect("site remains");
        assert_eq!(committed_site.name, updated_site.name);
        assert_eq!(committed_site.protocol, updated_site.protocol);
        assert_eq!(committed_site.base_url, updated_site.base_url);
        assert_eq!(committed_site.auth_preset, updated_site.auth_preset);
        assert_eq!(
            repository
                .list_pending_credential_operations()
                .await
                .expect("pending operations read")
                .len(),
            1
        );

        assert_eq!(
            saga.reconcile().await.expect("reconciliation succeeds"),
            RecoverySummary {
                recovered: 1,
                failed: 0,
                remaining: 0,
            }
        );
        assert!(!memory.matches(&id("credential-old"), "old-secret"));
        assert!(memory.matches(&id("credential-new"), "new-secret"));

        saga.delete_site(site.id.clone())
            .await
            .expect("site deleted");
        assert!(!memory.matches(&id("credential-new"), "new-secret"));
        assert!(repository
            .list_sites()
            .await
            .expect("sites read")
            .is_empty());
        assert!(repository
            .list_pending_credential_operations()
            .await
            .expect("pending operations read")
            .is_empty());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn clear_secret_preserves_the_site_and_removes_its_credential_reference() {
        let repository = Repository::new(Database::open_in_memory().expect("database opens"));
        let memory = InMemoryCredentialStore::default();
        let saga = saga(
            repository.clone(),
            &memory,
            &["credential-old", "operation-create", "operation-clear"],
        );
        let site = site("site-a", 0);
        saga.create_site(
            site.clone(),
            site_policy(&site.id),
            SecretValue::new("old-secret").expect("secret is valid"),
        )
        .await
        .expect("site created");

        let outcome = saga
            .clear_site_secret(site.clone())
            .await
            .expect("secret cleared");

        assert!(!outcome.cleanup_pending);
        assert!(!memory.matches(&id("credential-old"), "old-secret"));
        let sites = repository.list_sites().await.expect("sites read");
        assert_eq!(sites.len(), 1);
        assert_eq!(sites[0].credential_ref, None);
        assert_eq!(sites[0].lifecycle_state, LifecycleState::Ready);
        assert!(repository
            .list_pending_credential_operations()
            .await
            .expect("pending operations read")
            .is_empty());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn reconciliation_finishes_a_clear_when_delete_failed_after_removing_the_secret() {
        let repository = Repository::new(Database::open_in_memory().expect("database opens"));
        let memory = InMemoryCredentialStore::default();
        let saga = saga(
            repository.clone(),
            &memory,
            &["credential-old", "operation-create", "operation-clear"],
        );
        let site = site("site-a", 0);
        saga.create_site(
            site.clone(),
            site_policy(&site.id),
            SecretValue::new("old-secret").expect("secret is valid"),
        )
        .await
        .expect("site created");
        memory.fail_next_delete_after_delete(CredentialError::Unavailable);

        assert!(matches!(
            saga.clear_site_secret(site.clone()).await,
            Err(CredentialSagaError::Credential(
                CredentialError::Unavailable
            ))
        ));
        assert!(!memory.matches(&id("credential-old"), "old-secret"));
        let pending_site = &repository.list_sites().await.expect("sites read")[0];
        assert_eq!(pending_site.credential_ref, Some(id("credential-old")));
        assert_eq!(
            pending_site.lifecycle_state,
            LifecycleState::CredentialPending
        );

        assert_eq!(
            saga.reconcile().await.expect("reconciliation succeeds"),
            RecoverySummary {
                recovered: 1,
                failed: 0,
                remaining: 0,
            }
        );
        let committed_site = &repository.list_sites().await.expect("sites read")[0];
        assert_eq!(committed_site.credential_ref, None);
        assert_eq!(committed_site.lifecycle_state, LifecycleState::Ready);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn reconciliation_keeps_an_operation_pending_while_the_store_is_unavailable() {
        let repository = Repository::new(Database::open_in_memory().expect("database opens"));
        let memory = InMemoryCredentialStore::default();
        let site = site("site-a", 0);
        let operation = CredentialOperation {
            id: id("operation-a"),
            site_id: site.id.clone(),
            action: CredentialAction::Create,
            status: CredentialOperationStatus::IntentRecorded,
            new_credential_ref: Some(id("credential-a")),
            old_credential_ref: None,
            non_secret_payload_json: "{}".to_owned(),
            created_at: TimestampMs::new(100).expect("timestamp is valid"),
            updated_at: TimestampMs::new(100).expect("timestamp is valid"),
            completed_at: None,
        };
        repository
            .begin_credential_site_create(site.clone(), site_policy(&site.id), operation)
            .await
            .expect("intent recorded");
        memory.fail_next(CredentialError::Unavailable);

        let saga = saga(repository.clone(), &memory, &[]);
        assert_eq!(
            saga.reconcile()
                .await
                .expect("unavailable store is deferred"),
            RecoverySummary {
                recovered: 0,
                failed: 0,
                remaining: 1,
            }
        );
        let sites = repository.list_sites().await.expect("sites read");
        assert_eq!(sites.len(), 1);
        assert_eq!(sites[0].lifecycle_state, LifecycleState::CredentialPending);

        assert_eq!(
            saga.reconcile().await.expect("reconciliation resumes"),
            RecoverySummary {
                recovered: 1,
                failed: 0,
                remaining: 0,
            }
        );
        assert!(repository
            .list_sites()
            .await
            .expect("sites read")
            .is_empty());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn startup_reconciliation_commits_a_secret_written_before_a_crash() {
        let repository = Repository::new(Database::open_in_memory().expect("database opens"));
        let memory = InMemoryCredentialStore::default();
        let site = site("site-a", 0);
        let operation = CredentialOperation {
            id: id("operation-a"),
            site_id: site.id.clone(),
            action: CredentialAction::Create,
            status: CredentialOperationStatus::IntentRecorded,
            new_credential_ref: Some(id("credential-a")),
            old_credential_ref: None,
            non_secret_payload_json: "{}".to_owned(),
            created_at: TimestampMs::new(100).expect("timestamp is valid"),
            updated_at: TimestampMs::new(100).expect("timestamp is valid"),
            completed_at: None,
        };
        repository
            .begin_credential_site_create(site.clone(), site_policy(&site.id), operation)
            .await
            .expect("intent recorded");
        memory
            .put(
                &id("credential-a"),
                SecretValue::new("secret-a").expect("secret is valid"),
            )
            .expect("secret written before simulated crash");

        let saga = saga(repository.clone(), &memory, &[]);
        assert_eq!(
            saga.reconcile().await.expect("reconciliation succeeds"),
            RecoverySummary {
                recovered: 1,
                failed: 0,
                remaining: 0,
            }
        );
        let sites = repository.list_sites().await.expect("sites read");
        assert_eq!(sites.len(), 1);
        assert_eq!(sites[0].credential_ref, Some(id("credential-a")));
        assert_eq!(sites[0].lifecycle_state, LifecycleState::Ready);
        assert!(memory.matches(&id("credential-a"), "secret-a"));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn reconciliation_quarantines_invalid_operations_and_continues() {
        let database = Database::open_in_memory().expect("database opens");
        let repository = Repository::new(database.clone());
        let memory = InMemoryCredentialStore::default();
        let first_site = site("site-a", 0);
        let second_site = site("site-b", 1);
        for (site, operation_id, credential_id) in [
            (&first_site, "operation-a", "credential-a"),
            (&second_site, "operation-b", "credential-b"),
        ] {
            repository
                .begin_credential_site_create(
                    site.clone(),
                    site_policy(&site.id),
                    CredentialOperation {
                        id: id(operation_id),
                        site_id: site.id.clone(),
                        action: CredentialAction::Create,
                        status: CredentialOperationStatus::IntentRecorded,
                        new_credential_ref: Some(id(credential_id)),
                        old_credential_ref: None,
                        non_secret_payload_json: "{}".to_owned(),
                        created_at: TimestampMs::new(100).expect("timestamp is valid"),
                        updated_at: TimestampMs::new(100).expect("timestamp is valid"),
                        completed_at: None,
                    },
                )
                .await
                .expect("intent recorded");
        }
        database
            .call(|connection| {
                connection.execute(
                    "UPDATE credential_operations SET new_credential_ref = NULL
                     WHERE id = 'operation-a'",
                    [],
                )?;
                Ok(())
            })
            .await
            .expect("operation corrupted for recovery fixture");
        memory
            .put(
                &id("credential-b"),
                SecretValue::new("secret-b").expect("secret is valid"),
            )
            .expect("second credential written");
        let saga = saga(repository.clone(), &memory, &[]);

        assert_eq!(
            saga.reconcile().await.expect("reconciliation continues"),
            RecoverySummary {
                recovered: 1,
                failed: 1,
                remaining: 0,
            }
        );
        let operations = repository
            .list_credential_operations()
            .await
            .expect("operations read");
        assert_eq!(operations.len(), 2);
        assert_eq!(
            operations[0].status,
            CredentialOperationStatus::RecoveryFailed
        );
        assert_eq!(
            operations[1].status,
            CredentialOperationStatus::ConfigCommitted
        );
        let sites = repository.list_sites().await.expect("sites read");
        assert_eq!(sites.len(), 1);
        assert_eq!(sites[0].id, second_site.id);
        assert_eq!(sites[0].credential_ref, Some(id("credential-b")));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn reconciliation_and_live_mutations_share_the_same_gate() {
        let repository = Repository::new(Database::open_in_memory().expect("database opens"));
        let memory = InMemoryCredentialStore::default();
        let recovering_site = site("site-a", 0);
        repository
            .begin_credential_site_create(
                recovering_site.clone(),
                site_policy(&recovering_site.id),
                CredentialOperation {
                    id: id("operation-a"),
                    site_id: recovering_site.id.clone(),
                    action: CredentialAction::Create,
                    status: CredentialOperationStatus::IntentRecorded,
                    new_credential_ref: Some(id("credential-a")),
                    old_credential_ref: None,
                    non_secret_payload_json: "{}".to_owned(),
                    created_at: TimestampMs::new(100).expect("timestamp is valid"),
                    updated_at: TimestampMs::new(100).expect("timestamp is valid"),
                    completed_at: None,
                },
            )
            .await
            .expect("recovery intent recorded");
        let (entered_sender, entered_receiver) = mpsc::sync_channel(1);
        let (release_sender, release_receiver) = mpsc::channel();
        let credentials = CredentialService::start(BlockingExistsStore {
            inner: memory.clone(),
            entered: Mutex::new(Some(entered_sender)),
            release: Mutex::new(release_receiver),
        })
        .expect("credential service starts");
        let saga = CredentialSaga::new(
            repository.clone(),
            credentials,
            Arc::new(SequenceIds::new(&["credential-b", "operation-b"])),
            Arc::new(FixedClock::new(
                TimestampMs::new(100).expect("timestamp is valid"),
            )),
        );

        let reconciliation = tokio::spawn({
            let saga = saga.clone();
            async move { saga.reconcile().await }
        });
        tokio::task::spawn_blocking(move || entered_receiver.recv_timeout(Duration::from_secs(2)))
            .await
            .expect("credential lookup wait task joins")
            .expect("reconciliation reached credential lookup");
        let live_site = site("site-b", 1);
        let mutation = tokio::spawn({
            let saga = saga.clone();
            let live_site = live_site.clone();
            async move {
                saga.create_site(
                    live_site.clone(),
                    site_policy(&live_site.id),
                    SecretValue::new("secret-b").expect("secret is valid"),
                )
                .await
            }
        });
        tokio::task::yield_now().await;

        let sites_during_recovery = repository.list_sites().await.expect("sites read");
        assert_eq!(sites_during_recovery.len(), 1);
        assert_eq!(sites_during_recovery[0].id, recovering_site.id);

        release_sender.send(()).expect("credential lookup released");
        assert_eq!(
            reconciliation
                .await
                .expect("reconciliation task joins")
                .expect("reconciliation succeeds"),
            RecoverySummary {
                recovered: 1,
                failed: 0,
                remaining: 0,
            }
        );
        mutation
            .await
            .expect("mutation task joins")
            .expect("mutation succeeds");
        let sites = repository.list_sites().await.expect("sites read");
        assert_eq!(sites.len(), 1);
        assert_eq!(sites[0].id, live_site.id);
        assert_eq!(sites[0].credential_ref, Some(id("credential-b")));
    }
}
