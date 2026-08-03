use rusqlite::{params, OptionalExtension};

use crate::domain::{
    CredentialAction, CredentialOperation, CredentialOperationStatus, EntityId, LifecycleState,
    ProbePolicy, Site, TimestampMs,
};

use super::{
    bump_revision,
    config::{compact_site_sort_orders, insert_probe_policy, insert_site},
    require_changed, DatabaseError, Repository, Result,
};

impl Repository {
    pub async fn begin_credential_site_create(
        &self,
        mut site: Site,
        default_policy: ProbePolicy,
        operation: CredentialOperation,
    ) -> Result<u64> {
        if operation.action != CredentialAction::Create
            || operation.status != CredentialOperationStatus::IntentRecorded
            || operation.site_id != site.id
            || operation.new_credential_ref.is_none()
            || operation.old_credential_ref.is_some()
            || default_policy.site_id != site.id
            || default_policy.model_id.is_some()
        {
            return Err(DatabaseError::InvalidInput {
                field: "credential_create",
            });
        }
        site.credential_ref = None;
        site.lifecycle_state = LifecycleState::CredentialPending;

        self.database
            .call(move |connection| {
                let transaction = connection.transaction()?;
                insert_site(&transaction, &site)?;
                insert_probe_policy(&transaction, &default_policy)?;
                insert_operation(&transaction, &operation)?;
                let revision = bump_revision(&transaction)?;
                transaction.commit()?;
                Ok(revision)
            })
            .await
    }

    pub async fn begin_existing_credential_operation(
        &self,
        operation_id: EntityId,
        site_id: EntityId,
        action: CredentialAction,
        new_credential_ref: Option<EntityId>,
        now: TimestampMs,
    ) -> Result<CredentialOperation> {
        if action == CredentialAction::Create
            || (action == CredentialAction::Replace) != new_credential_ref.is_some()
        {
            return Err(DatabaseError::InvalidInput {
                field: "credential_operation",
            });
        }

        self.database
            .call(move |connection| {
                let transaction = connection.transaction()?;
                let (old_credential_ref, lifecycle_state) = transaction
                    .query_row(
                        "SELECT credential_ref, lifecycle_state FROM sites WHERE id = ?1",
                        [&site_id],
                        |row| {
                            Ok((
                                row.get::<_, Option<EntityId>>(0)?,
                                row.get::<_, LifecycleState>(1)?,
                            ))
                        },
                    )
                    .optional()?
                    .ok_or(DatabaseError::NotFound { entity: "site" })?;
                if lifecycle_state != LifecycleState::Ready {
                    return Err(DatabaseError::InvalidInput {
                        field: "site_lifecycle",
                    });
                }

                let operation = CredentialOperation {
                    id: operation_id,
                    site_id: site_id.clone(),
                    action,
                    status: CredentialOperationStatus::IntentRecorded,
                    new_credential_ref,
                    old_credential_ref,
                    non_secret_payload_json: "{}".to_owned(),
                    created_at: now,
                    updated_at: now,
                    completed_at: None,
                };
                insert_operation(&transaction, &operation)?;
                let pending_state = if action == CredentialAction::Delete {
                    LifecycleState::DeletePending
                } else {
                    LifecycleState::CredentialPending
                };
                transaction.execute(
                    "UPDATE sites SET lifecycle_state = ?1, updated_at_ms = ?2 WHERE id = ?3",
                    params![pending_state, now, site_id],
                )?;
                bump_revision(&transaction)?;
                transaction.commit()?;
                Ok(operation)
            })
            .await
    }

    pub async fn mark_credential_written(
        &self,
        operation_id: EntityId,
        updated_at: TimestampMs,
    ) -> Result<u64> {
        self.database
            .call(move |connection| {
                let transaction = connection.transaction()?;
                let changed = transaction.execute(
                    "UPDATE credential_operations
                     SET status = 'credential_written', updated_at_ms = ?2
                     WHERE id = ?1 AND status = 'intent_recorded'",
                    params![operation_id, updated_at],
                )?;
                require_changed(changed, "credential_operation")?;
                let revision = bump_revision(&transaction)?;
                transaction.commit()?;
                Ok(revision)
            })
            .await
    }

    pub async fn commit_credential_configuration(
        &self,
        operation_id: EntityId,
        updated_at: TimestampMs,
        cleanup_pending: bool,
    ) -> Result<u64> {
        self.database
            .call(move |connection| {
                let transaction = connection.transaction()?;
                let operation = load_operation(&transaction, &operation_id)?.ok_or(
                    DatabaseError::NotFound {
                        entity: "credential_operation",
                    },
                )?;
                if !matches!(
                    operation.status,
                    CredentialOperationStatus::IntentRecorded
                        | CredentialOperationStatus::CredentialWritten
                ) {
                    return Err(DatabaseError::InvalidInput {
                        field: "credential_operation_status",
                    });
                }
                match operation.action {
                    CredentialAction::Create | CredentialAction::Replace => {
                        let credential_ref = operation.new_credential_ref.as_ref().ok_or(
                            DatabaseError::InvalidInput {
                                field: "new_credential_ref",
                            },
                        )?;
                        let changed = transaction.execute(
                            "UPDATE sites SET credential_ref = ?1, lifecycle_state = 'ready',
                                              updated_at_ms = ?2
                             WHERE id = ?3",
                            params![credential_ref, updated_at, operation.site_id],
                        )?;
                        require_changed(changed, "site")?;
                    }
                    CredentialAction::Clear => {
                        let changed = transaction.execute(
                            "UPDATE sites SET credential_ref = NULL, lifecycle_state = 'ready',
                                              updated_at_ms = ?1
                             WHERE id = ?2",
                            params![updated_at, operation.site_id],
                        )?;
                        require_changed(changed, "site")?;
                    }
                    CredentialAction::Delete => {
                        let changed = transaction
                            .execute("DELETE FROM sites WHERE id = ?1", [&operation.site_id])?;
                        require_changed(changed, "site")?;
                        compact_site_sort_orders(&transaction)?;
                    }
                }

                let (status, completed_at) = if cleanup_pending {
                    (CredentialOperationStatus::CleanupPending, None)
                } else {
                    (CredentialOperationStatus::ConfigCommitted, Some(updated_at))
                };
                transaction.execute(
                    "UPDATE credential_operations
                     SET status = ?2, updated_at_ms = ?3, completed_at_ms = ?4
                     WHERE id = ?1",
                    params![operation_id, status, updated_at, completed_at],
                )?;
                let revision = bump_revision(&transaction)?;
                transaction.commit()?;
                Ok(revision)
            })
            .await
    }

    pub async fn complete_credential_cleanup(
        &self,
        operation_id: EntityId,
        completed_at: TimestampMs,
    ) -> Result<u64> {
        self.database
            .call(move |connection| {
                let transaction = connection.transaction()?;
                let changed = transaction.execute(
                    "UPDATE credential_operations
                     SET status = 'config_committed', updated_at_ms = ?2, completed_at_ms = ?2
                     WHERE id = ?1 AND status = 'cleanup_pending'",
                    params![operation_id, completed_at],
                )?;
                require_changed(changed, "credential_operation")?;
                let revision = bump_revision(&transaction)?;
                transaction.commit()?;
                Ok(revision)
            })
            .await
    }

    pub async fn abort_credential_operation(
        &self,
        operation_id: EntityId,
        site_id: EntityId,
        remove_site: bool,
        updated_at: TimestampMs,
    ) -> Result<u64> {
        self.database
            .call(move |connection| {
                let transaction = connection.transaction()?;
                if remove_site {
                    transaction.execute("DELETE FROM sites WHERE id = ?1", [&site_id])?;
                    compact_site_sort_orders(&transaction)?;
                } else {
                    transaction.execute(
                        "UPDATE sites SET lifecycle_state = 'ready', updated_at_ms = ?1
                         WHERE id = ?2",
                        params![updated_at, site_id],
                    )?;
                }
                let changed = transaction.execute(
                    "DELETE FROM credential_operations WHERE id = ?1 AND site_id = ?2",
                    params![operation_id, site_id],
                )?;
                require_changed(changed, "credential_operation")?;
                let revision = bump_revision(&transaction)?;
                transaction.commit()?;
                Ok(revision)
            })
            .await
    }

    pub async fn list_pending_credential_operations(&self) -> Result<Vec<CredentialOperation>> {
        self.database
            .call(|connection| {
                let mut statement = connection.prepare(
                    "SELECT id, site_id, action, status, new_credential_ref,
                            old_credential_ref, non_secret_payload_json, created_at_ms,
                            updated_at_ms, completed_at_ms
                     FROM credential_operations
                     WHERE status != 'config_committed'
                     ORDER BY created_at_ms, id",
                )?;
                let operations = statement
                    .query_map([], operation_from_row)?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                Ok(operations)
            })
            .await
    }

    pub async fn list_credential_operations(&self) -> Result<Vec<CredentialOperation>> {
        self.database
            .call(|connection| {
                let mut statement = connection.prepare(
                    "SELECT id, site_id, action, status, new_credential_ref,
                            old_credential_ref, non_secret_payload_json, created_at_ms,
                            updated_at_ms, completed_at_ms
                     FROM credential_operations ORDER BY created_at_ms, id",
                )?;
                let operations = statement
                    .query_map([], operation_from_row)?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                Ok(operations)
            })
            .await
    }
}

fn insert_operation(
    connection: &rusqlite::Connection,
    operation: &CredentialOperation,
) -> Result<()> {
    connection.execute(
        "INSERT INTO credential_operations (
           id, site_id, action, status, new_credential_ref, old_credential_ref,
           non_secret_payload_json, created_at_ms, updated_at_ms, completed_at_ms
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            operation.id,
            operation.site_id,
            operation.action,
            operation.status,
            operation.new_credential_ref,
            operation.old_credential_ref,
            operation.non_secret_payload_json,
            operation.created_at,
            operation.updated_at,
            operation.completed_at,
        ],
    )?;
    Ok(())
}

fn load_operation(
    connection: &rusqlite::Connection,
    operation_id: &EntityId,
) -> Result<Option<CredentialOperation>> {
    Ok(connection
        .query_row(
            "SELECT id, site_id, action, status, new_credential_ref,
                    old_credential_ref, non_secret_payload_json, created_at_ms,
                    updated_at_ms, completed_at_ms
             FROM credential_operations WHERE id = ?1",
            [operation_id],
            operation_from_row,
        )
        .optional()?)
}

fn operation_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<CredentialOperation> {
    Ok(CredentialOperation {
        id: row.get(0)?,
        site_id: row.get(1)?,
        action: row.get(2)?,
        status: row.get(3)?,
        new_credential_ref: row.get(4)?,
        old_credential_ref: row.get(5)?,
        non_secret_payload_json: row.get(6)?,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
        completed_at: row.get(9)?,
    })
}
