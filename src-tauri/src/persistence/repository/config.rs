use std::collections::HashSet;

use rusqlite::{params, Connection, OptionalExtension};

use crate::domain::{
    EntityId, GeneralSettings, Model, ProbeParameter, ProbePolicy, ProbeTimingPolicy,
    RequestHeader, Site,
};

use super::{bump_revision, entity_exists, require_changed, DatabaseError, Repository, Result};

impl Repository {
    pub async fn list_sites(&self) -> Result<Vec<Site>> {
        self.database
            .call(|connection| {
                let mut statement = connection.prepare(
                    "SELECT id, name, protocol, base_url, credential_ref, enabled, sort_order,
                            models_method, models_endpoint, probe_method, probe_endpoint,
                            content_type, auth_preset, stream_enabled, probe_interval_minutes,
                            slow_first_token_ms, slow_total_ms, failure_threshold,
                            recovery_threshold, lifecycle_state,
                            created_at_ms, updated_at_ms
                     FROM sites ORDER BY sort_order, id",
                )?;
                let sites = statement
                    .query_map([], site_from_row)?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                Ok(sites)
            })
            .await
    }

    pub async fn create_site(&self, site: Site, default_policy: ProbePolicy) -> Result<u64> {
        if default_policy.site_id != site.id || default_policy.model_id.is_some() {
            return Err(DatabaseError::InvalidInput {
                field: "default_policy",
            });
        }

        self.database
            .call(move |connection| {
                let transaction = connection.transaction()?;
                insert_site(&transaction, &site)?;
                insert_probe_policy(&transaction, &default_policy)?;
                let revision = bump_revision(&transaction)?;
                transaction.commit()?;
                Ok(revision)
            })
            .await
    }

    pub async fn update_site(&self, site: Site) -> Result<u64> {
        self.database
            .call(move |connection| {
                let transaction = connection.transaction()?;
                update_site_inner(&transaction, &site)?;
                let revision = bump_revision(&transaction)?;
                transaction.commit()?;
                Ok(revision)
            })
            .await
    }

    pub async fn delete_site(&self, site_id: crate::domain::EntityId) -> Result<u64> {
        self.database
            .call(move |connection| {
                let transaction = connection.transaction()?;
                let changed = transaction.execute("DELETE FROM sites WHERE id = ?1", [&site_id])?;
                require_changed(changed, "site")?;
                compact_site_sort_orders(&transaction)?;
                let revision = bump_revision(&transaction)?;
                transaction.commit()?;
                Ok(revision)
            })
            .await
    }

    pub async fn reorder_sites(&self, site_ids: Vec<crate::domain::EntityId>) -> Result<u64> {
        self.database
            .call(move |connection| {
                let transaction = connection.transaction()?;
                let count = transaction
                    .query_row("SELECT COUNT(*) FROM sites", [], |row| row.get::<_, i64>(0))?;
                if count
                    != i64::try_from(site_ids.len())
                        .map_err(|_| DatabaseError::InvalidInput { field: "site_ids" })?
                {
                    return Err(DatabaseError::InvalidInput { field: "site_ids" });
                }
                for site_id in &site_ids {
                    if !entity_exists(&transaction, "sites", site_id)? {
                        return Err(DatabaseError::InvalidInput { field: "site_ids" });
                    }
                }

                write_site_sort_orders(&transaction, &site_ids)?;

                let revision = bump_revision(&transaction)?;
                transaction.commit()?;
                Ok(revision)
            })
            .await
    }

    pub async fn list_models(&self, site_id: crate::domain::EntityId) -> Result<Vec<Model>> {
        self.database
            .call(move |connection| {
                let mut statement = connection.prepare(
                    "SELECT id, site_id, model_key, display_name, source, discovery_status,
                            enabled, sort_order, created_at_ms, updated_at_ms
                     FROM models WHERE site_id = ?1 ORDER BY sort_order, id",
                )?;
                let models = statement
                    .query_map([site_id], model_from_row)?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                Ok(models)
            })
            .await
    }

    pub async fn create_model(&self, model: Model) -> Result<u64> {
        self.database
            .call(move |connection| {
                let transaction = connection.transaction()?;
                insert_model(&transaction, &model)?;
                let revision = bump_revision(&transaction)?;
                transaction.commit()?;
                Ok(revision)
            })
            .await
    }

    pub async fn update_model(&self, model: Model) -> Result<u64> {
        self.database
            .call(move |connection| {
                let transaction = connection.transaction()?;
                let changed = transaction.execute(
                    "UPDATE models SET
                       model_key = ?3, display_name = ?4, source = ?5,
                       discovery_status = ?6, enabled = ?7, updated_at_ms = ?8
                     WHERE id = ?1 AND site_id = ?2",
                    params![
                        model.id,
                        model.site_id,
                        model.model_key,
                        model.display_name,
                        model.source,
                        model.discovery_status,
                        model.enabled,
                        model.updated_at,
                    ],
                )?;
                require_changed(changed, "model")?;
                let revision = bump_revision(&transaction)?;
                transaction.commit()?;
                Ok(revision)
            })
            .await
    }

    pub async fn sync_discovered_models(
        &self,
        site_id: crate::domain::EntityId,
        models: Vec<Model>,
    ) -> Result<u64> {
        if models.iter().any(|model| {
            model.site_id != site_id || model.source != crate::domain::ModelSource::Discovered
        }) {
            return Err(DatabaseError::InvalidInput {
                field: "discovered_models",
            });
        }

        self.database
            .call(move |connection| {
                let transaction = connection.transaction()?;
                if !entity_exists(&transaction, "sites", &site_id)? {
                    return Err(DatabaseError::NotFound { entity: "site" });
                }
                let synchronized_ids = models
                    .iter()
                    .map(|model| model.id.clone())
                    .collect::<HashSet<_>>();
                let existing_discovered_ids = {
                    let mut statement = transaction
                        .prepare("SELECT id FROM models WHERE site_id = ?1 AND source = ?2")?;
                    let ids = statement
                        .query_map(
                            params![site_id, crate::domain::ModelSource::Discovered],
                            |row| row.get::<_, crate::domain::EntityId>(0),
                        )?
                        .collect::<rusqlite::Result<Vec<_>>>()?;
                    ids
                };
                for model_id in existing_discovered_ids {
                    if !synchronized_ids.contains(&model_id) {
                        transaction.execute(
                            "DELETE FROM models WHERE id = ?1 AND site_id = ?2",
                            params![model_id, site_id],
                        )?;
                    }
                }
                for model in &models {
                    let identity = transaction
                        .query_row(
                            "SELECT site_id, source FROM models WHERE id = ?1",
                            [&model.id],
                            |row| {
                                Ok((
                                    row.get::<_, crate::domain::EntityId>(0)?,
                                    row.get::<_, crate::domain::ModelSource>(1)?,
                                ))
                            },
                        )
                        .optional()?;
                    if identity.as_ref().is_some_and(|(existing_site_id, source)| {
                        existing_site_id != &site_id
                            || *source != crate::domain::ModelSource::Discovered
                    }) {
                        return Err(DatabaseError::InvalidInput {
                            field: "discovered_model_identity",
                        });
                    }
                    transaction.execute(
                        "INSERT INTO models (
                           id, site_id, model_key, display_name, source, discovery_status,
                           enabled, sort_order, created_at_ms, updated_at_ms
                         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                         ON CONFLICT(id) DO UPDATE SET
                           model_key = excluded.model_key,
                           display_name = excluded.display_name,
                           discovery_status = excluded.discovery_status,
                           enabled = excluded.enabled,
                           sort_order = excluded.sort_order,
                           updated_at_ms = excluded.updated_at_ms",
                        params![
                            model.id,
                            model.site_id,
                            model.model_key,
                            model.display_name,
                            model.source,
                            model.discovery_status,
                            model.enabled,
                            model.sort_order,
                            model.created_at,
                            model.updated_at,
                        ],
                    )?;
                }
                compact_model_sort_orders(&transaction, &site_id)?;
                let revision = bump_revision(&transaction)?;
                transaction.commit()?;
                Ok(revision)
            })
            .await
    }

    pub async fn delete_model(
        &self,
        site_id: crate::domain::EntityId,
        model_id: crate::domain::EntityId,
    ) -> Result<u64> {
        self.database
            .call(move |connection| {
                let transaction = connection.transaction()?;
                let changed = transaction.execute(
                    "DELETE FROM models WHERE id = ?1 AND site_id = ?2",
                    params![model_id, site_id],
                )?;
                require_changed(changed, "model")?;
                compact_model_sort_orders(&transaction, &site_id)?;
                let revision = bump_revision(&transaction)?;
                transaction.commit()?;
                Ok(revision)
            })
            .await
    }

    pub async fn reorder_models(
        &self,
        site_id: crate::domain::EntityId,
        model_ids: Vec<crate::domain::EntityId>,
    ) -> Result<u64> {
        self.database
            .call(move |connection| {
                let transaction = connection.transaction()?;
                let count = transaction.query_row(
                    "SELECT COUNT(*) FROM models WHERE site_id = ?1",
                    [&site_id],
                    |row| row.get::<_, i64>(0),
                )?;
                if count
                    != i64::try_from(model_ids.len())
                        .map_err(|_| DatabaseError::InvalidInput { field: "model_ids" })?
                {
                    return Err(DatabaseError::InvalidInput { field: "model_ids" });
                }
                for model_id in &model_ids {
                    let belongs = transaction.query_row(
                        "SELECT EXISTS(
                           SELECT 1 FROM models WHERE id = ?1 AND site_id = ?2
                         )",
                        params![model_id, site_id],
                        |row| row.get::<_, bool>(0),
                    )?;
                    if !belongs {
                        return Err(DatabaseError::InvalidInput { field: "model_ids" });
                    }
                }

                write_model_sort_orders(&transaction, &site_id, &model_ids)?;

                let revision = bump_revision(&transaction)?;
                transaction.commit()?;
                Ok(revision)
            })
            .await
    }

    pub async fn replace_probe_policy(
        &self,
        policy: ProbePolicy,
        parameters: Vec<ProbeParameter>,
    ) -> Result<u64> {
        if parameters
            .iter()
            .any(|parameter| parameter.policy_id != policy.id)
        {
            return Err(DatabaseError::InvalidInput {
                field: "probe_parameters",
            });
        }

        self.database
            .call(move |connection| {
                let transaction = connection.transaction()?;
                upsert_probe_policy(&transaction, &policy)?;
                transaction.execute(
                    "DELETE FROM custom_probe_parameters WHERE policy_id = ?1",
                    [&policy.id],
                )?;
                for parameter in &parameters {
                    transaction.execute(
                        "INSERT INTO custom_probe_parameters (
                           id, policy_id, parameter_key, value_type, value_json, sort_order
                         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                        params![
                            parameter.id,
                            parameter.policy_id,
                            parameter.key,
                            parameter.value_type,
                            parameter.value_json,
                            parameter.sort_order,
                        ],
                    )?;
                }
                let revision = bump_revision(&transaction)?;
                transaction.commit()?;
                Ok(revision)
            })
            .await
    }

    pub async fn list_probe_policies(
        &self,
        site_id: crate::domain::EntityId,
    ) -> Result<Vec<(ProbePolicy, Vec<ProbeParameter>)>> {
        self.database
            .call(move |connection| {
                let policies = {
                    let mut statement = connection.prepare(
                        "SELECT id, site_id, model_id, mode, max_output_tokens, prompt,
                                custom_endpoint, custom_method, custom_stream_enabled,
                                created_at_ms, updated_at_ms
                         FROM probe_policies WHERE site_id = ?1
                         ORDER BY model_id IS NOT NULL, model_id, id",
                    )?;
                    let policies = statement
                        .query_map([site_id], policy_from_row)?
                        .collect::<rusqlite::Result<Vec<_>>>()?;
                    policies
                };

                let mut result = Vec::with_capacity(policies.len());
                let mut statement = connection.prepare(
                    "SELECT id, policy_id, parameter_key, value_type, value_json, sort_order
                     FROM custom_probe_parameters WHERE policy_id = ?1 ORDER BY sort_order, id",
                )?;
                for policy in policies {
                    let parameters = statement
                        .query_map([&policy.id], parameter_from_row)?
                        .collect::<rusqlite::Result<Vec<_>>>()?;
                    result.push((policy, parameters));
                }
                Ok(result)
            })
            .await
    }

    pub async fn replace_site_headers(
        &self,
        site_id: crate::domain::EntityId,
        headers: Vec<RequestHeader>,
    ) -> Result<u64> {
        if headers.iter().any(|header| header.site_id != site_id) {
            return Err(DatabaseError::InvalidInput { field: "headers" });
        }

        self.database
            .call(move |connection| {
                let transaction = connection.transaction()?;
                replace_site_headers_inner(&transaction, &site_id, &headers)?;
                let revision = bump_revision(&transaction)?;
                transaction.commit()?;
                Ok(revision)
            })
            .await
    }

    pub async fn update_site_with_headers(
        &self,
        site: Site,
        headers: Vec<RequestHeader>,
    ) -> Result<u64> {
        if headers.iter().any(|header| header.site_id != site.id) {
            return Err(DatabaseError::InvalidInput { field: "headers" });
        }

        self.database
            .call(move |connection| {
                let transaction = connection.transaction()?;
                update_site_inner(&transaction, &site)?;
                replace_site_headers_inner(&transaction, &site.id, &headers)?;
                let revision = bump_revision(&transaction)?;
                transaction.commit()?;
                Ok(revision)
            })
            .await
    }

    pub async fn list_site_headers(
        &self,
        site_id: crate::domain::EntityId,
    ) -> Result<Vec<RequestHeader>> {
        self.database
            .call(move |connection| {
                let mut statement = connection.prepare(
                    "SELECT id, site_id, header_name, value_template, sort_order
                     FROM site_headers WHERE site_id = ?1 ORDER BY sort_order, id",
                )?;
                let headers = statement
                    .query_map([site_id], header_from_row)?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                Ok(headers)
            })
            .await
    }

    pub async fn load_general_settings(&self) -> Result<GeneralSettings> {
        self.database
            .call(|connection| {
                Ok(connection.query_row(
                    "SELECT launch_at_startup, continue_in_background, tray_action, theme,
                            history_retention_days, diagnostic_capture_enabled, updated_at_ms
                     FROM general_settings WHERE singleton = 1",
                    [],
                    |row| {
                        Ok(GeneralSettings {
                            launch_at_startup: row.get(0)?,
                            continue_in_background: row.get(1)?,
                            tray_action: row.get(2)?,
                            theme: row.get(3)?,
                            history_retention_days: row.get(4)?,
                            diagnostic_capture_enabled: row.get(5)?,
                            updated_at: row.get(6)?,
                        })
                    },
                )?)
            })
            .await
    }

    pub async fn update_general_settings(&self, settings: GeneralSettings) -> Result<u64> {
        self.database
            .call(move |connection| {
                let transaction = connection.transaction()?;
                let changed = transaction.execute(
                    "UPDATE general_settings SET
                       launch_at_startup = ?1, continue_in_background = ?2, tray_action = ?3,
                       theme = ?4, history_retention_days = ?5,
                       diagnostic_capture_enabled = ?6, updated_at_ms = ?7
                     WHERE singleton = 1",
                    params![
                        settings.launch_at_startup,
                        settings.continue_in_background,
                        settings.tray_action,
                        settings.theme,
                        settings.history_retention_days,
                        settings.diagnostic_capture_enabled,
                        settings.updated_at,
                    ],
                )?;
                require_changed(changed, "general_settings")?;
                let revision = bump_revision(&transaction)?;
                transaction.commit()?;
                Ok(revision)
            })
            .await
    }
}

fn update_site_inner(connection: &Connection, site: &Site) -> Result<()> {
    let previous_interval = connection
        .query_row(
            "SELECT probe_interval_minutes FROM sites WHERE id = ?1",
            [&site.id],
            |row| row.get::<_, Option<u16>>(0),
        )
        .optional()?
        .ok_or(DatabaseError::NotFound { entity: "site" })?;
    let changed = connection.execute(
        "UPDATE sites SET
           name = ?2, protocol = ?3, base_url = ?4, credential_ref = ?5,
           enabled = ?6, models_method = ?7, models_endpoint = ?8,
           probe_method = ?9, probe_endpoint = ?10, content_type = ?11,
           auth_preset = ?12, stream_enabled = ?13,
           probe_interval_minutes = ?14, slow_first_token_ms = ?15,
           slow_total_ms = ?16, failure_threshold = ?17,
           recovery_threshold = ?18, lifecycle_state = ?19,
           updated_at_ms = ?20
         WHERE id = ?1",
        params![
            site.id,
            site.name,
            site.protocol,
            site.base_url,
            site.credential_ref,
            site.enabled,
            site.models_method,
            site.models_endpoint,
            site.probe_method,
            site.probe_endpoint,
            site.content_type,
            site.auth_preset,
            site.stream_enabled,
            site.timing.interval_minutes,
            site.timing.slow_first_token_ms,
            site.timing.slow_total_ms,
            site.timing.failure_threshold,
            site.timing.recovery_threshold,
            site.lifecycle_state,
            site.updated_at,
        ],
    )?;
    require_changed(changed, "site")?;
    if previous_interval != site.timing.interval_minutes {
        connection.execute(
            "UPDATE latest_model_status
             SET next_probe_at_ms = NULL
             WHERE model_id IN (SELECT id FROM models WHERE site_id = ?1)",
            [&site.id],
        )?;
    }
    Ok(())
}

fn replace_site_headers_inner(
    connection: &Connection,
    site_id: &EntityId,
    headers: &[RequestHeader],
) -> Result<()> {
    if !entity_exists(connection, "sites", site_id)? {
        return Err(DatabaseError::NotFound { entity: "site" });
    }
    connection.execute("DELETE FROM site_headers WHERE site_id = ?1", [site_id])?;
    for header in headers {
        connection.execute(
            "INSERT INTO site_headers (
               id, site_id, header_name, value_template, sort_order
             ) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                header.id,
                header.site_id,
                header.name,
                header.value_template,
                header.sort_order,
            ],
        )?;
    }
    Ok(())
}

pub(super) fn compact_site_sort_orders(connection: &Connection) -> Result<()> {
    let site_ids = connection
        .prepare("SELECT id FROM sites ORDER BY sort_order, id")?
        .query_map([], |row| row.get::<_, EntityId>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    write_site_sort_orders(connection, &site_ids)
}

fn write_site_sort_orders(connection: &Connection, site_ids: &[EntityId]) -> Result<()> {
    let max_sort_order = connection.query_row(
        "SELECT COALESCE(MAX(sort_order), -1) FROM sites",
        [],
        |row| row.get::<_, i64>(0),
    )?;
    let offset = sort_order_offset(max_sort_order, site_ids.len(), "site_ids")?;
    connection.execute("UPDATE sites SET sort_order = sort_order + ?1", [offset])?;
    for (sort_order, site_id) in site_ids.iter().enumerate() {
        let sort_order = i64::try_from(sort_order)
            .map_err(|_| DatabaseError::InvalidInput { field: "site_ids" })?;
        connection.execute(
            "UPDATE sites SET sort_order = ?1 WHERE id = ?2",
            params![sort_order, site_id],
        )?;
    }
    Ok(())
}

fn compact_model_sort_orders(connection: &Connection, site_id: &EntityId) -> Result<()> {
    let model_ids = connection
        .prepare("SELECT id FROM models WHERE site_id = ?1 ORDER BY sort_order, id")?
        .query_map([site_id], |row| row.get::<_, EntityId>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    write_model_sort_orders(connection, site_id, &model_ids)
}

fn write_model_sort_orders(
    connection: &Connection,
    site_id: &EntityId,
    model_ids: &[EntityId],
) -> Result<()> {
    let max_sort_order = connection.query_row(
        "SELECT COALESCE(MAX(sort_order), -1) FROM models WHERE site_id = ?1",
        [site_id],
        |row| row.get::<_, i64>(0),
    )?;
    let offset = sort_order_offset(max_sort_order, model_ids.len(), "model_ids")?;
    connection.execute(
        "UPDATE models SET sort_order = sort_order + ?1 WHERE site_id = ?2",
        params![offset, site_id],
    )?;
    for (sort_order, model_id) in model_ids.iter().enumerate() {
        let sort_order = i64::try_from(sort_order)
            .map_err(|_| DatabaseError::InvalidInput { field: "model_ids" })?;
        connection.execute(
            "UPDATE models SET sort_order = ?1 WHERE id = ?2 AND site_id = ?3",
            params![sort_order, model_id, site_id],
        )?;
    }
    Ok(())
}

fn sort_order_offset(max_sort_order: i64, len: usize, field: &'static str) -> Result<i64> {
    max_sort_order
        .checked_add(i64::try_from(len).map_err(|_| DatabaseError::InvalidInput { field })?)
        .and_then(|value| value.checked_add(1))
        .ok_or(DatabaseError::InvalidInput { field })
}

pub(super) fn insert_site(connection: &Connection, site: &Site) -> Result<()> {
    connection.execute(
        "INSERT INTO sites (
           id, name, protocol, base_url, credential_ref, enabled, sort_order,
           models_method, models_endpoint, probe_method, probe_endpoint, content_type,
           auth_preset, stream_enabled, probe_interval_minutes, slow_first_token_ms,
           slow_total_ms, failure_threshold,
           recovery_threshold, lifecycle_state, created_at_ms, updated_at_ms
         ) VALUES (
           ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
           ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22
         )",
        params![
            site.id,
            site.name,
            site.protocol,
            site.base_url,
            site.credential_ref,
            site.enabled,
            site.sort_order,
            site.models_method,
            site.models_endpoint,
            site.probe_method,
            site.probe_endpoint,
            site.content_type,
            site.auth_preset,
            site.stream_enabled,
            site.timing.interval_minutes,
            site.timing.slow_first_token_ms,
            site.timing.slow_total_ms,
            site.timing.failure_threshold,
            site.timing.recovery_threshold,
            site.lifecycle_state,
            site.created_at,
            site.updated_at,
        ],
    )?;
    Ok(())
}

fn insert_model(connection: &Connection, model: &Model) -> Result<()> {
    connection.execute(
        "INSERT INTO models (
           id, site_id, model_key, display_name, source, discovery_status,
           enabled, sort_order, created_at_ms, updated_at_ms
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            model.id,
            model.site_id,
            model.model_key,
            model.display_name,
            model.source,
            model.discovery_status,
            model.enabled,
            model.sort_order,
            model.created_at,
            model.updated_at,
        ],
    )?;
    Ok(())
}

fn upsert_probe_policy(connection: &Connection, policy: &ProbePolicy) -> Result<()> {
    let existing_identity = connection
        .query_row(
            "SELECT site_id, model_id FROM probe_policies WHERE id = ?1",
            [&policy.id],
            |row| {
                Ok((
                    row.get::<_, crate::domain::EntityId>(0)?,
                    row.get::<_, Option<crate::domain::EntityId>>(1)?,
                ))
            },
        )
        .optional()?;
    if existing_identity
        .as_ref()
        .is_some_and(|(site_id, model_id)| {
            site_id != &policy.site_id || model_id != &policy.model_id
        })
    {
        return Err(DatabaseError::InvalidInput {
            field: "probe_policy_identity",
        });
    }

    connection.execute(
        "INSERT INTO probe_policies (
           id, site_id, model_id, mode, max_output_tokens, prompt,
           custom_endpoint, custom_method, custom_stream_enabled,
           created_at_ms, updated_at_ms
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
         ON CONFLICT(id) DO UPDATE SET
           mode = excluded.mode,
           max_output_tokens = excluded.max_output_tokens,
           prompt = excluded.prompt,
           custom_endpoint = excluded.custom_endpoint,
           custom_method = excluded.custom_method,
           custom_stream_enabled = excluded.custom_stream_enabled,
           updated_at_ms = excluded.updated_at_ms",
        params![
            policy.id,
            policy.site_id,
            policy.model_id,
            policy.mode,
            policy.max_output_tokens,
            policy.prompt,
            policy.custom_endpoint,
            policy.custom_method,
            policy.custom_stream_enabled,
            policy.created_at,
            policy.updated_at,
        ],
    )?;
    Ok(())
}

pub(super) fn insert_probe_policy(connection: &Connection, policy: &ProbePolicy) -> Result<()> {
    connection.execute(
        "INSERT INTO probe_policies (
           id, site_id, model_id, mode, max_output_tokens, prompt,
           custom_endpoint, custom_method, custom_stream_enabled,
           created_at_ms, updated_at_ms
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            policy.id,
            policy.site_id,
            policy.model_id,
            policy.mode,
            policy.max_output_tokens,
            policy.prompt,
            policy.custom_endpoint,
            policy.custom_method,
            policy.custom_stream_enabled,
            policy.created_at,
            policy.updated_at,
        ],
    )?;
    Ok(())
}

pub(super) fn site_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Site> {
    Ok(Site {
        id: row.get(0)?,
        name: row.get(1)?,
        protocol: row.get(2)?,
        base_url: row.get(3)?,
        credential_ref: row.get(4)?,
        enabled: row.get(5)?,
        sort_order: row.get(6)?,
        models_method: row.get(7)?,
        models_endpoint: row.get(8)?,
        probe_method: row.get(9)?,
        probe_endpoint: row.get(10)?,
        content_type: row.get(11)?,
        auth_preset: row.get(12)?,
        stream_enabled: row.get(13)?,
        timing: ProbeTimingPolicy {
            interval_minutes: row.get(14)?,
            slow_first_token_ms: row.get(15)?,
            slow_total_ms: row.get(16)?,
            failure_threshold: row.get(17)?,
            recovery_threshold: row.get(18)?,
        },
        lifecycle_state: row.get(19)?,
        created_at: row.get(20)?,
        updated_at: row.get(21)?,
    })
}

pub(super) fn model_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Model> {
    Ok(Model {
        id: row.get(0)?,
        site_id: row.get(1)?,
        model_key: row.get(2)?,
        display_name: row.get(3)?,
        source: row.get(4)?,
        discovery_status: row.get(5)?,
        enabled: row.get(6)?,
        sort_order: row.get(7)?,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
    })
}

pub(super) fn policy_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ProbePolicy> {
    Ok(ProbePolicy {
        id: row.get(0)?,
        site_id: row.get(1)?,
        model_id: row.get(2)?,
        mode: row.get(3)?,
        max_output_tokens: row.get(4)?,
        prompt: row.get(5)?,
        custom_endpoint: row.get(6)?,
        custom_method: row.get(7)?,
        custom_stream_enabled: row.get(8)?,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
    })
}

pub(super) fn parameter_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ProbeParameter> {
    Ok(ProbeParameter {
        id: row.get(0)?,
        policy_id: row.get(1)?,
        key: row.get(2)?,
        value_type: row.get(3)?,
        value_json: row.get(4)?,
        sort_order: row.get(5)?,
    })
}

pub(super) fn header_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<RequestHeader> {
    Ok(RequestHeader {
        id: row.get(0)?,
        site_id: row.get(1)?,
        name: row.get(2)?,
        value_template: row.get(3)?,
        sort_order: row.get(4)?,
    })
}

#[cfg(test)]
mod tests {
    use crate::{
        domain::{
            GeneralSettings, HttpMethod, ParameterType, ProbeMode, ProbeParameter, ProbeStatus,
            RequestHeader, ThemeMode, TrayAction,
        },
        persistence::Database,
    };

    use super::*;
    use crate::persistence::repository::test_support::{
        capture, id, latest, model, probe_result, site, site_policy, timestamp,
    };

    #[tokio::test(flavor = "current_thread")]
    async fn configuration_round_trips_after_reopening() {
        let directory = tempfile::tempdir().expect("tempdir created");
        let path = directory.path().join("relaypulse.sqlite3");
        let mut site = site("site-a", 0);
        site.protocol = crate::domain::SiteProtocol::Anthropic;
        let mut model = model("model-a", &site.id, 0);
        let default_policy = site_policy(&site.id);
        let model_policy = ProbePolicy {
            id: id("policy-model-a"),
            site_id: site.id.clone(),
            model_id: Some(model.id.clone()),
            mode: ProbeMode::MinimumReasoning,
            max_output_tokens: 16,
            prompt: "READY".to_owned(),
            custom_endpoint: Some("/responses".to_owned()),
            custom_method: Some(HttpMethod::Post),
            custom_stream_enabled: Some(true),
            created_at: timestamp(3),
            updated_at: timestamp(3),
        };
        let parameter = ProbeParameter {
            id: id("parameter-a"),
            policy_id: model_policy.id.clone(),
            key: "reasoning_effort".to_owned(),
            value_type: ParameterType::String,
            value_json: "\"low\"".to_owned(),
            sort_order: 0,
        };
        let header = RequestHeader {
            id: id("header-a"),
            site_id: site.id.clone(),
            name: "X-Workspace".to_owned(),
            value_template: "relaypulse".to_owned(),
            sort_order: 0,
        };
        let settings = GeneralSettings {
            launch_at_startup: true,
            continue_in_background: false,
            tray_action: TrayAction::Disabled,
            theme: ThemeMode::Dark,
            history_retention_days: 90,
            diagnostic_capture_enabled: false,
            updated_at: timestamp(4),
        };

        {
            let repository = Repository::new(Database::open(&path).expect("database opens"));
            assert_eq!(
                repository
                    .create_site(site.clone(), default_policy.clone())
                    .await
                    .expect("site created"),
                1
            );
            assert_eq!(
                repository
                    .create_model(model.clone())
                    .await
                    .expect("model created"),
                2
            );
            site.name = "Updated Site".to_owned();
            site.protocol = crate::domain::SiteProtocol::OpenAiResponses;
            site.updated_at = timestamp(5);
            assert_eq!(
                repository
                    .update_site(site.clone())
                    .await
                    .expect("site updated"),
                3
            );
            model.enabled = false;
            model.updated_at = timestamp(6);
            assert_eq!(
                repository
                    .update_model(model.clone())
                    .await
                    .expect("model updated"),
                4
            );
            repository
                .replace_probe_policy(model_policy.clone(), vec![parameter.clone()])
                .await
                .expect("model policy stored");
            repository
                .replace_site_headers(site.id.clone(), vec![header.clone()])
                .await
                .expect("headers stored");
            assert_eq!(
                repository
                    .update_general_settings(settings.clone())
                    .await
                    .expect("settings stored"),
                7
            );
        }

        let repository = Repository::new(Database::open(&path).expect("database reopens"));
        assert_eq!(repository.revision().await.expect("revision read"), 7);
        assert_eq!(
            repository.list_sites().await.expect("sites read"),
            vec![site]
        );
        assert_eq!(
            repository
                .list_models(id("site-a"))
                .await
                .expect("models read"),
            vec![model]
        );
        assert_eq!(
            repository
                .list_site_headers(id("site-a"))
                .await
                .expect("headers read"),
            vec![header]
        );
        assert_eq!(
            repository
                .list_probe_policies(id("site-a"))
                .await
                .expect("policies read"),
            vec![(default_policy, vec![]), (model_policy, vec![parameter])]
        );
        assert_eq!(
            repository
                .load_general_settings()
                .await
                .expect("settings read"),
            settings
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn changing_probe_interval_clears_persisted_next_probe_times() {
        let repository = Repository::new(Database::open_in_memory().expect("database opens"));
        let mut site = site("site-a", 0);
        let model = model("model-a", &site.id, 0);
        repository
            .create_site(site.clone(), site_policy(&site.id))
            .await
            .expect("site created");
        repository
            .create_model(model.clone())
            .await
            .expect("model created");

        let result = probe_result("result-a", &site.id, &model.id, 100, ProbeStatus::Healthy);
        repository
            .record_probe(
                result.clone(),
                capture(&result.id),
                ProbeTimingPolicy::default(),
                latest(&result).next_probe_at,
            )
            .await
            .expect("probe recorded");
        assert!(repository
            .list_latest_model_statuses()
            .await
            .expect("statuses read")[0]
            .next_probe_at
            .is_some());

        site.name = "Renamed Site".to_owned();
        site.updated_at = timestamp(10);
        repository
            .update_site(site.clone())
            .await
            .expect("site updated");
        assert!(repository
            .list_latest_model_statuses()
            .await
            .expect("statuses read")[0]
            .next_probe_at
            .is_some());

        site.timing.interval_minutes = Some(1);
        site.updated_at = timestamp(11);
        repository
            .update_site(site)
            .await
            .expect("probe interval updated");
        assert!(repository
            .list_latest_model_statuses()
            .await
            .expect("statuses read")[0]
            .next_probe_at
            .is_none());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn site_and_header_update_rolls_back_as_one_transaction() {
        let repository = Repository::new(Database::open_in_memory().expect("database opens"));
        let site = site("site-a", 0);
        repository
            .create_site(site.clone(), site_policy(&site.id))
            .await
            .expect("site created");
        let existing_header = RequestHeader {
            id: id("header-existing"),
            site_id: site.id.clone(),
            name: "X-Existing".to_owned(),
            value_template: "existing".to_owned(),
            sort_order: 0,
        };
        repository
            .replace_site_headers(site.id.clone(), vec![existing_header.clone()])
            .await
            .expect("existing header stored");
        let revision = repository.revision().await.expect("revision read");

        let mut updated_site = site.clone();
        updated_site.name = "Partially Applied".to_owned();
        updated_site.updated_at = timestamp(10);
        let duplicate_headers = vec![
            RequestHeader {
                id: id("header-a"),
                site_id: site.id.clone(),
                name: "X-Duplicate".to_owned(),
                value_template: "first".to_owned(),
                sort_order: 0,
            },
            RequestHeader {
                id: id("header-b"),
                site_id: site.id.clone(),
                name: "X-Duplicate".to_owned(),
                value_template: "second".to_owned(),
                sort_order: 1,
            },
        ];

        assert!(repository
            .update_site_with_headers(updated_site, duplicate_headers)
            .await
            .is_err());
        assert_eq!(
            repository.revision().await.expect("revision read"),
            revision
        );
        assert_eq!(
            repository.list_sites().await.expect("sites read"),
            vec![site.clone()]
        );
        assert_eq!(
            repository
                .list_site_headers(site.id)
                .await
                .expect("headers read"),
            vec![existing_header]
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn reordering_is_collision_free_and_invalid_input_rolls_back() {
        let repository = Repository::new(Database::open_in_memory().expect("database opens"));
        let site_a = site("site-a", 0);
        let site_b = site("site-b", 1);
        repository
            .create_site(site_a.clone(), site_policy(&site_a.id))
            .await
            .expect("site a created");
        repository
            .create_site(site_b.clone(), site_policy(&site_b.id))
            .await
            .expect("site b created");

        let revision = repository
            .reorder_sites(vec![site_b.id.clone(), site_a.id.clone()])
            .await
            .expect("sites reordered");
        assert_eq!(revision, 3);
        let ordered = repository.list_sites().await.expect("sites read");
        assert_eq!(
            ordered
                .iter()
                .map(|site| site.id.clone())
                .collect::<Vec<_>>(),
            vec![site_b.id.clone(), site_a.id.clone()]
        );

        assert!(repository
            .reorder_sites(vec![site_a.id.clone()])
            .await
            .is_err());
        assert_eq!(repository.revision().await.expect("revision read"), 3);
        let still_ordered = repository.list_sites().await.expect("sites read");
        assert_eq!(
            still_ordered
                .iter()
                .map(|site| site.id.clone())
                .collect::<Vec<_>>(),
            vec![site_b.id, site_a.id]
        );

        let mut rebound_policy = site_policy(&id("site-a"));
        rebound_policy.site_id = id("site-b");
        assert!(repository
            .replace_probe_policy(rebound_policy, vec![])
            .await
            .is_err());
        assert_eq!(repository.revision().await.expect("revision read"), 3);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn deleting_resequences_site_and_model_sort_orders() {
        let repository = Repository::new(Database::open_in_memory().expect("database opens"));
        let site_a = site("site-a", 0);
        let site_b = site("site-b", 1);
        repository
            .create_site(site_a.clone(), site_policy(&site_a.id))
            .await
            .expect("site a created");
        repository
            .create_site(site_b.clone(), site_policy(&site_b.id))
            .await
            .expect("site b created");
        repository
            .reorder_sites(vec![site_b.id.clone(), site_a.id.clone()])
            .await
            .expect("sites reordered");
        repository
            .delete_site(site_b.id)
            .await
            .expect("leading site deleted");
        assert_eq!(
            repository
                .list_sites()
                .await
                .expect("sites read")
                .iter()
                .map(|site| site.sort_order)
                .collect::<Vec<_>>(),
            vec![0]
        );

        let model_a = model("model-a", &site_a.id, 0);
        let model_b = model("model-b", &site_a.id, 1);
        repository
            .create_model(model_a.clone())
            .await
            .expect("model a created");
        repository
            .create_model(model_b.clone())
            .await
            .expect("model b created");
        repository
            .reorder_models(
                site_a.id.clone(),
                vec![model_b.id.clone(), model_a.id.clone()],
            )
            .await
            .expect("models reordered");
        repository
            .delete_model(site_a.id.clone(), model_b.id)
            .await
            .expect("leading model deleted");
        assert_eq!(
            repository
                .list_models(site_a.id)
                .await
                .expect("models read")
                .iter()
                .map(|model| model.sort_order)
                .collect::<Vec<_>>(),
            vec![0]
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn discovered_model_sync_is_transactional_and_preserves_manual_models() {
        let repository = Repository::new(Database::open_in_memory().expect("database opens"));
        let site = site("site-a", 0);
        repository
            .create_site(site.clone(), site_policy(&site.id))
            .await
            .expect("site created");
        let manual = model("manual-model", &site.id, 0);
        repository
            .create_model(manual.clone())
            .await
            .expect("manual model created");
        let mut obsolete = model("obsolete-model", &site.id, 1);
        obsolete.source = crate::domain::ModelSource::Discovered;
        repository
            .create_model(obsolete)
            .await
            .expect("obsolete discovered model created");
        let mut discovered = model("discovered-model", &site.id, 2);
        discovered.source = crate::domain::ModelSource::Discovered;
        discovered.enabled = true;

        assert_eq!(
            repository
                .sync_discovered_models(site.id.clone(), vec![discovered.clone()])
                .await
                .expect("discovered models synchronized"),
            4
        );
        let stored = repository
            .list_models(site.id.clone())
            .await
            .expect("models read");
        let mut compacted_discovered = discovered.clone();
        compacted_discovered.sort_order = 1;
        assert_eq!(stored, vec![manual.clone(), compacted_discovered]);

        let mut invalid = discovered;
        invalid.site_id = id("another-site");
        assert!(matches!(
            repository
                .sync_discovered_models(site.id.clone(), vec![invalid])
                .await,
            Err(DatabaseError::InvalidInput {
                field: "discovered_models"
            })
        ));
        assert_eq!(
            repository
                .list_models(site.id)
                .await
                .expect("models still readable"),
            stored
        );
        assert_eq!(repository.revision().await.expect("revision read"), 4);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn deleting_a_model_is_scoped_to_its_site() {
        let repository = Repository::new(Database::open_in_memory().expect("database opens"));
        let site = site("site-a", 0);
        let model = model("model-a", &site.id, 0);
        repository
            .create_site(site.clone(), site_policy(&site.id))
            .await
            .expect("site created");
        repository
            .create_model(model.clone())
            .await
            .expect("model created");

        assert!(repository
            .delete_model(id("wrong-site"), model.id.clone())
            .await
            .is_err());
        assert_eq!(repository.revision().await.expect("revision read"), 2);
        assert_eq!(
            repository
                .delete_model(site.id.clone(), model.id.clone())
                .await
                .expect("model deleted"),
            3
        );
        assert!(repository
            .list_models(site.id)
            .await
            .expect("models read")
            .is_empty());
    }
}
