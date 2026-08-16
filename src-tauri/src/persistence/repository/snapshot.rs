use std::collections::HashMap;

use crate::domain::{GeneralSettings, ProbeParameter};

use super::{
    config::{header_from_row, model_from_row, parameter_from_row, policy_from_row, site_from_row},
    current_revision,
    probes::{latest_status_from_row, probe_capture_from_row_at, probe_result_from_row},
    Repository, RepositorySnapshot, Result,
};

impl Repository {
    pub async fn snapshot(&self) -> Result<RepositorySnapshot> {
        self.database
            .call(|connection| {
                let transaction = connection.transaction()?;
                let sites = transaction
                    .prepare(
                        "SELECT id, name, protocol, base_url, credential_ref, enabled, sort_order,
                                models_method, models_endpoint, probe_method, probe_endpoint,
                                content_type, auth_preset, stream_enabled, probe_interval_minutes,
                                slow_first_token_ms, slow_total_ms, failure_threshold,
                                recovery_threshold, lifecycle_state, created_at_ms, updated_at_ms
                         FROM sites ORDER BY sort_order, id",
                    )?
                    .query_map([], site_from_row)?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                let models = transaction
                    .prepare(
                        "SELECT id, site_id, model_key, display_name, source, discovery_status,
                                enabled, sort_order, created_at_ms, updated_at_ms
                         FROM models ORDER BY site_id, sort_order, id",
                    )?
                    .query_map([], model_from_row)?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                let policies = transaction
                    .prepare(
                        "SELECT id, site_id, model_id, mode, max_output_tokens, prompt,
                                custom_endpoint, custom_method, custom_stream_enabled,
                                created_at_ms, updated_at_ms
                         FROM probe_policies
                         ORDER BY site_id, model_id IS NOT NULL, model_id, id",
                    )?
                    .query_map([], policy_from_row)?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                let parameters = transaction
                    .prepare(
                        "SELECT id, policy_id, parameter_key, value_type, value_json, sort_order
                         FROM custom_probe_parameters ORDER BY policy_id, sort_order, id",
                    )?
                    .query_map([], parameter_from_row)?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                let mut parameters_by_policy =
                    HashMap::<crate::domain::EntityId, Vec<ProbeParameter>>::new();
                for parameter in parameters {
                    parameters_by_policy
                        .entry(parameter.policy_id.clone())
                        .or_default()
                        .push(parameter);
                }
                let probe_policies = policies
                    .into_iter()
                    .map(|policy| {
                        let parameters =
                            parameters_by_policy.remove(&policy.id).unwrap_or_default();
                        (policy, parameters)
                    })
                    .collect();
                let site_headers = transaction
                    .prepare(
                        "SELECT id, site_id, header_name, value_template, sort_order
                         FROM site_headers ORDER BY site_id, sort_order, id",
                    )?
                    .query_map([], header_from_row)?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                let general_settings = transaction.query_row(
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
                )?;
                let latest_model_statuses = transaction
                    .prepare(
                        "SELECT model_id, probe_result_id, status, consecutive_failures,
                                consecutive_successes, next_probe_at_ms, revision, updated_at_ms
                         FROM latest_model_status ORDER BY model_id",
                    )?
                    .query_map([], latest_status_from_row)?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                let latest_model_results = transaction
                    .prepare(
                        "SELECT result.id, result.site_id, result.model_id, result.run_kind,
                                result.started_at_ms, result.finished_at_ms, result.status,
                                result.response_headers_ms, result.first_token_ms,
                                result.total_time_ms, result.http_status, result.provider_model,
                                result.finish_reason, result.error_code, result.error_message,
                                result.input_tokens, result.output_tokens, result.total_tokens,
                                result.usage_reported, result.ended_normally, result.retry_count,
                                result.request_id, result.revision, result.request_method,
                                result.endpoint, result.probe_mode
                         FROM latest_model_status AS latest
                         JOIN probe_results AS result ON result.id = latest.probe_result_id
                         ORDER BY result.model_id",
                    )?
                    .query_map([], probe_result_from_row)?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                let probe_records = transaction
                    .prepare(
                        "SELECT result.id, result.site_id, result.model_id, result.run_kind,
                                result.started_at_ms, result.finished_at_ms, result.status,
                                result.response_headers_ms, result.first_token_ms,
                                result.total_time_ms, result.http_status, result.provider_model,
                                result.finish_reason, result.error_code, result.error_message,
                                result.input_tokens, result.output_tokens, result.total_tokens,
                                result.usage_reported, result.ended_normally, result.retry_count,
                                result.request_id, result.revision, result.request_method,
                                result.endpoint, result.probe_mode,
                                capture.probe_result_id, capture.request_headers_json,
                                capture.request_body, capture.request_captured_bytes,
                                capture.request_limit_bytes, capture.request_truncated,
                                capture.response_mode, capture.response_content_type,
                                capture.response_headers_json, capture.response_body,
                                capture.response_chunks_json, capture.response_captured_bytes,
                                capture.response_limit_bytes, capture.response_truncated
                         FROM probe_results AS result
                         JOIN probe_captures AS capture ON capture.probe_result_id = result.id
                         ORDER BY result.started_at_ms DESC, result.id DESC
                         LIMIT 100",
                    )?
                    .query_map([], |row| {
                        Ok((
                            probe_result_from_row(row)?,
                            probe_capture_from_row_at(row, 26)?,
                        ))
                    })?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                let revision = current_revision(&transaction)?;
                transaction.commit()?;
                Ok(RepositorySnapshot {
                    revision,
                    sites,
                    models,
                    probe_policies,
                    site_headers,
                    general_settings,
                    latest_model_statuses,
                    latest_model_results,
                    probe_records,
                })
            })
            .await
    }
}
