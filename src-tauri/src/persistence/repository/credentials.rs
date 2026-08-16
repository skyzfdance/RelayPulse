use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::domain::{
    AuthPreset, CredentialAction, CredentialOperation, CredentialOperationStatus, EntityId,
    LifecycleState, ProbePolicy, Site, SiteProtocol, TimestampMs,
};

use super::{
    bump_revision,
    config::{compact_site_sort_orders, insert_probe_policy, insert_site},
    require_changed, DatabaseError, Repository, Result,
};

#[derive(Debug, Serialize, Deserialize)]
struct CredentialSitePayload {
    name: String,
    protocol: String,
    base_url: String,
    auth_preset: String,
}

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
        site_update: Option<Site>,
        now: TimestampMs,
    ) -> Result<CredentialOperation> {
        if action == CredentialAction::Create
            || (action == CredentialAction::Replace) != new_credential_ref.is_some()
            || site_update
                .as_ref()
                .is_some_and(|site| site.id != site_id || action == CredentialAction::Delete)
        {
            return Err(DatabaseError::InvalidInput {
                field: "credential_operation",
            });
        }
        let non_secret_payload_json = site_update
            .as_ref()
            .map(encode_site_payload)
            .transpose()?
            .unwrap_or_else(|| "{}".to_owned());

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
                    non_secret_payload_json,
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
                let site_update = decode_site_payload(&operation.non_secret_payload_json)?;
                match operation.action {
                    CredentialAction::Create | CredentialAction::Replace => {
                        let credential_ref = operation.new_credential_ref.as_ref().ok_or(
                            DatabaseError::InvalidInput {
                                field: "new_credential_ref",
                            },
                        )?;
                        commit_site_configuration(
                            &transaction,
                            &operation.site_id,
                            Some(credential_ref),
                            site_update.as_ref(),
                            updated_at,
                        )?;
                    }
                    CredentialAction::Clear => {
                        commit_site_configuration(
                            &transaction,
                            &operation.site_id,
                            None,
                            site_update.as_ref(),
                            updated_at,
                        )?;
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

    pub async fn quarantine_credential_operation(
        &self,
        operation_id: EntityId,
        site_id: EntityId,
        action: CredentialAction,
        updated_at: TimestampMs,
    ) -> Result<u64> {
        self.database
            .call(move |connection| {
                let transaction = connection.transaction()?;
                if action == CredentialAction::Create {
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
                    "UPDATE credential_operations
                     SET status = 'recovery_failed', updated_at_ms = ?4, completed_at_ms = ?4
                     WHERE id = ?1 AND site_id = ?2 AND action = ?3
                       AND status IN ('intent_recorded', 'credential_written')",
                    params![operation_id, site_id, action, updated_at],
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
                     WHERE status IN ('intent_recorded', 'credential_written', 'cleanup_pending')
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

fn encode_site_payload(site: &Site) -> Result<String> {
    serde_json::to_string(&CredentialSitePayload {
        name: site.name.clone(),
        protocol: protocol_name(site.protocol).to_owned(),
        base_url: site.base_url.clone(),
        auth_preset: auth_preset_name(site.auth_preset).to_owned(),
    })
    .map_err(|_| DatabaseError::InvalidInput {
        field: "credential_payload",
    })
}

fn decode_site_payload(value: &str) -> Result<Option<CredentialSitePayload>> {
    if value.trim() == "{}" {
        return Ok(None);
    }
    let payload = serde_json::from_str::<CredentialSitePayload>(value).map_err(|_| {
        DatabaseError::InvalidInput {
            field: "credential_payload",
        }
    })?;
    if payload.name.trim().is_empty() || payload.base_url.trim().is_empty() {
        return Err(DatabaseError::InvalidInput {
            field: "credential_payload",
        });
    }
    Ok(Some(payload))
}

fn commit_site_configuration(
    connection: &rusqlite::Connection,
    site_id: &EntityId,
    credential_ref: Option<&EntityId>,
    site_update: Option<&CredentialSitePayload>,
    updated_at: TimestampMs,
) -> Result<()> {
    let changed = if let Some(site_update) = site_update {
        let protocol = parse_protocol(&site_update.protocol)?;
        let auth_preset = parse_auth_preset(&site_update.auth_preset)?;
        connection.execute(
            "UPDATE sites SET name = ?1, protocol = ?2, base_url = ?3, auth_preset = ?4,
                              credential_ref = ?5, lifecycle_state = 'ready', updated_at_ms = ?6
             WHERE id = ?7",
            params![
                site_update.name,
                protocol,
                site_update.base_url,
                auth_preset,
                credential_ref,
                updated_at,
                site_id,
            ],
        )?
    } else {
        connection.execute(
            "UPDATE sites SET credential_ref = ?1, lifecycle_state = 'ready', updated_at_ms = ?2
             WHERE id = ?3",
            params![credential_ref, updated_at, site_id],
        )?
    };
    require_changed(changed, "site")
}

fn protocol_name(value: SiteProtocol) -> &'static str {
    match value {
        SiteProtocol::OpenAiCompatible => "openai-compatible",
        SiteProtocol::Anthropic => "anthropic",
        SiteProtocol::OpenAiResponses => "openai-responses",
        SiteProtocol::CustomHttp => "custom-http",
    }
}

fn parse_protocol(value: &str) -> Result<SiteProtocol> {
    match value {
        "openai-compatible" => Ok(SiteProtocol::OpenAiCompatible),
        "anthropic" => Ok(SiteProtocol::Anthropic),
        "openai-responses" => Ok(SiteProtocol::OpenAiResponses),
        "custom-http" => Ok(SiteProtocol::CustomHttp),
        _ => Err(DatabaseError::InvalidInput {
            field: "credential_payload",
        }),
    }
}

fn auth_preset_name(value: AuthPreset) -> &'static str {
    match value {
        AuthPreset::Bearer => "bearer",
        AuthPreset::XApiKey => "x-api-key",
        AuthPreset::ApiKey => "api-key",
        AuthPreset::None => "none",
    }
}

fn parse_auth_preset(value: &str) -> Result<AuthPreset> {
    match value {
        "bearer" => Ok(AuthPreset::Bearer),
        "x-api-key" => Ok(AuthPreset::XApiKey),
        "api-key" => Ok(AuthPreset::ApiKey),
        "none" => Ok(AuthPreset::None),
        _ => Err(DatabaseError::InvalidInput {
            field: "credential_payload",
        }),
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
