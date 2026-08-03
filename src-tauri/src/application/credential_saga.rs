use std::sync::Arc;

use thiserror::Error;

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
    pub remaining: u64,
}

#[derive(Clone)]
pub struct CredentialSaga {
    repository: Repository,
    credentials: CredentialService,
    ids: Arc<dyn IdGenerator>,
    clock: Arc<dyn Clock>,
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
        }
    }

    pub async fn create_site(
        &self,
        site: Site,
        default_policy: ProbePolicy,
        secret: SecretValue,
    ) -> Result<CredentialMutationOutcome> {
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
            self.repository
                .abort_credential_operation(operation_id, site.id, true, self.clock.now())
                .await?;
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
        site_id: EntityId,
        secret: SecretValue,
    ) -> Result<CredentialMutationOutcome> {
        let new_credential_ref = self.ids.next_id();
        let operation_id = self.ids.next_id();
        let operation = self
            .repository
            .begin_existing_credential_operation(
                operation_id.clone(),
                site_id.clone(),
                CredentialAction::Replace,
                Some(new_credential_ref.clone()),
                self.clock.now(),
            )
            .await?;

        if let Err(error) = self.credentials.put(new_credential_ref, secret).await {
            self.repository
                .abort_credential_operation(operation_id, site_id, false, self.clock.now())
                .await?;
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

    pub async fn clear_site_secret(&self, site_id: EntityId) -> Result<CredentialMutationOutcome> {
        self.remove_site_credential(site_id, CredentialAction::Clear)
            .await
    }

    pub async fn delete_site(&self, site_id: EntityId) -> Result<CredentialMutationOutcome> {
        self.remove_site_credential(site_id, CredentialAction::Delete)
            .await
    }

    pub async fn read_secret(&self, reference: EntityId) -> Result<SecretValue> {
        self.credentials.get(reference).await.map_err(Into::into)
    }

    pub async fn reconcile(&self) -> Result<RecoverySummary> {
        let operations = self.repository.list_pending_credential_operations().await?;
        let mut recovered = 0_u64;
        for operation in operations {
            if self.recover_operation(operation).await? {
                recovered += 1;
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
            remaining,
        })
    }

    async fn remove_site_credential(
        &self,
        site_id: EntityId,
        action: CredentialAction,
    ) -> Result<CredentialMutationOutcome> {
        let operation_id = self.ids.next_id();
        let operation = self
            .repository
            .begin_existing_credential_operation(
                operation_id.clone(),
                site_id.clone(),
                action,
                None,
                self.clock.now(),
            )
            .await?;

        if let Some(old_credential_ref) = operation.old_credential_ref {
            if let Err(error) = self.credentials.delete(old_credential_ref).await {
                self.repository
                    .abort_credential_operation(operation_id, site_id, false, self.clock.now())
                    .await?;
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

    async fn recover_operation(&self, operation: CredentialOperation) -> Result<bool> {
        if operation.status == CredentialOperationStatus::CleanupPending {
            if let Some(old_credential_ref) = operation.old_credential_ref {
                if self.credentials.delete(old_credential_ref).await.is_err() {
                    return Ok(false);
                }
            }
            self.repository
                .complete_credential_cleanup(operation.id, self.clock.now())
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
                            operation.id,
                            operation.site_id,
                            operation.action == CredentialAction::Create,
                            self.clock.now(),
                        )
                        .await?;
                    return Ok(true);
                }

                self.mark_written_if_needed(&operation).await?;
                let cleanup_pending = operation.action == CredentialAction::Replace
                    && operation.old_credential_ref.is_some();
                self.repository
                    .commit_credential_configuration(
                        operation.id.clone(),
                        self.clock.now(),
                        cleanup_pending,
                    )
                    .await?;
                if let Some(old_credential_ref) = operation.old_credential_ref {
                    if self.credentials.delete(old_credential_ref).await.is_err() {
                        return Ok(false);
                    }
                    self.repository
                        .complete_credential_cleanup(operation.id, self.clock.now())
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
                self.mark_written_if_needed(&operation).await?;
                self.repository
                    .commit_credential_configuration(operation.id, self.clock.now(), false)
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
}

#[cfg(test)]
mod tests {
    use std::{collections::VecDeque, sync::Mutex};

    use crate::{
        credentials::{CredentialError, InMemoryCredentialStore},
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
    async fn create_failure_removes_the_pending_site_without_storing_the_secret() {
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
        assert!(repository
            .list_sites()
            .await
            .expect("sites read")
            .is_empty());
        assert!(repository
            .list_credential_operations()
            .await
            .expect("operations read")
            .is_empty());
        assert!(!memory.matches(&id("credential-a"), "secret-a"));
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

        memory.fail_next_delete(CredentialError::Unavailable);
        let outcome = saga
            .replace_site_secret(
                site.id.clone(),
                SecretValue::new("new-secret").expect("secret is valid"),
            )
            .await
            .expect("new secret committed");
        assert!(outcome.cleanup_pending);
        assert!(memory.matches(&id("credential-old"), "old-secret"));
        assert!(memory.matches(&id("credential-new"), "new-secret"));
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
                remaining: 0
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
            .clear_site_secret(site.id.clone())
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
                remaining: 1
            }
        );
        let sites = repository.list_sites().await.expect("sites read");
        assert_eq!(sites.len(), 1);
        assert_eq!(sites[0].lifecycle_state, LifecycleState::CredentialPending);

        assert_eq!(
            saga.reconcile().await.expect("reconciliation resumes"),
            RecoverySummary {
                recovered: 1,
                remaining: 0
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
        use crate::credentials::CredentialStore;

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
                remaining: 0
            }
        );
        let sites = repository.list_sites().await.expect("sites read");
        assert_eq!(sites.len(), 1);
        assert_eq!(sites[0].credential_ref, Some(id("credential-a")));
        assert_eq!(sites[0].lifecycle_state, LifecycleState::Ready);
        assert!(memory.matches(&id("credential-a"), "secret-a"));
    }
}
