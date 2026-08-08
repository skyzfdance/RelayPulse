use std::collections::HashSet;

use rusqlite::{params, Connection, OptionalExtension};

use crate::{
    domain::TimestampMs,
    notifications::{
        NotificationDelivery, NotificationEventCatalogItem, NotificationRule,
        NotificationSettingsState, NotificationSettingsUpdate, NotificationSettingsUpdateOutcome,
        NotificationTemplateMode, NotificationTemplateOverride, QueuedNotificationEvent,
    },
};

use super::{bump_revision, sqlite_i64, sqlite_u64, DatabaseError, Repository, Result};

const DISPATCH_LEASE_MS: i64 = 30_000;

impl Repository {
    pub async fn notification_settings(
        &self,
        catalog: Vec<NotificationEventCatalogItem>,
    ) -> Result<NotificationSettingsState> {
        self.database()
            .call(move |connection| load_notification_settings(connection, &catalog))
            .await
    }

    pub async fn update_notification_settings(
        &self,
        update: NotificationSettingsUpdate,
        catalog: Vec<NotificationEventCatalogItem>,
        now: TimestampMs,
    ) -> Result<NotificationSettingsUpdateOutcome> {
        self.database()
            .call(move |connection| {
                let transaction = connection.transaction()?;
                let expected_revision = sqlite_i64(update.revision, "revision")?;
                let changed = transaction.execute(
                    "UPDATE notification_settings
                     SET notifications_enabled = ?1, revision = revision + 1, updated_at_ms = ?2
                     WHERE singleton = 1 AND revision = ?3",
                    params![
                        update.notifications_enabled,
                        now.as_i64(),
                        expected_revision
                    ],
                )?;
                if changed == 0 {
                    return Err(DatabaseError::Conflict {
                        entity: "notification-settings",
                    });
                }

                let catalog_types = catalog
                    .iter()
                    .map(|item| item.event_type.as_str())
                    .collect::<HashSet<_>>();
                if update.rules.len() != catalog.len()
                    || update
                        .rules
                        .iter()
                        .any(|rule| !catalog_types.contains(rule.event_type.as_str()))
                {
                    return Err(DatabaseError::CatalogChanged);
                }

                for rule in &update.rules {
                    transaction.execute(
                        "INSERT INTO notification_rules (event_type, enabled, updated_at_ms)
                         VALUES (?1, ?2, ?3)
                         ON CONFLICT(event_type) DO UPDATE SET
                           enabled = excluded.enabled,
                           updated_at_ms = excluded.updated_at_ms",
                        params![rule.event_type, rule.enabled, now.as_i64()],
                    )?;
                    transaction.execute(
                        "DELETE FROM notification_rule_channels WHERE event_type = ?1",
                        [&rule.event_type],
                    )?;
                    for channel_id in &rule.channel_ids {
                        transaction.execute(
                            "INSERT INTO notification_rule_channels (event_type, channel_id)
                             VALUES (?1, ?2)",
                            params![rule.event_type, channel_id],
                        )?;
                    }
                    transaction.execute(
                        "DELETE FROM notification_template_overrides WHERE event_type = ?1",
                        [&rule.event_type],
                    )?;
                    let descriptor = catalog
                        .iter()
                        .find(|item| item.event_type == rule.event_type)
                        .map(|item| &item.template_descriptor)
                        .ok_or(DatabaseError::CatalogChanged)?;
                    for template_override in &rule.template_overrides {
                        let redundant_default = template_override.mode
                            == NotificationTemplateMode::Default
                            && template_override.marker_id == descriptor.default_marker_id
                            && template_override.title_template
                                == descriptor.default_title_template
                            && template_override.body_template == descriptor.default_body_template;
                        if redundant_default {
                            continue;
                        }
                        transaction.execute(
                            "INSERT INTO notification_template_overrides (
                               event_type, channel_id, mode, marker_id, title_template,
                               body_template, template_schema_version, updated_at_ms
                             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                            params![
                                template_override.event_type,
                                template_override.channel_id,
                                template_mode_name(template_override.mode),
                                template_override.marker_id,
                                template_override.title_template,
                                template_override.body_template,
                                template_override.template_schema_version,
                                now.as_i64(),
                            ],
                        )?;
                    }
                }

                let app_revision = bump_revision(&transaction)?;
                let settings = load_notification_settings(&transaction, &catalog)?;
                transaction.commit()?;
                Ok(NotificationSettingsUpdateOutcome {
                    settings,
                    app_revision,
                })
            })
            .await
    }

    pub(crate) async fn enqueue_notification(
        &self,
        event: QueuedNotificationEvent,
        catalog: Vec<NotificationEventCatalogItem>,
        now: TimestampMs,
    ) -> Result<bool> {
        self.database()
            .call(move |connection| {
                let transaction = connection.transaction()?;
                let Some(catalog_item) = catalog
                    .iter()
                    .find(|item| item.event_type == event.event_type)
                else {
                    return Err(DatabaseError::CatalogChanged);
                };
                if catalog_item.payload_version != event.payload_version {
                    return Err(DatabaseError::CatalogChanged);
                }

                let notifications_enabled = transaction.query_row(
                    "SELECT notifications_enabled FROM notification_settings WHERE singleton = 1",
                    [],
                    |row| row.get::<_, bool>(0),
                )?;
                if !notifications_enabled {
                    return Ok(false);
                }

                let persisted_enabled = transaction
                    .query_row(
                        "SELECT enabled FROM notification_rules WHERE event_type = ?1",
                        [&event.event_type],
                        |row| row.get::<_, bool>(0),
                    )
                    .optional()?;
                if !persisted_enabled.unwrap_or(catalog_item.default_enabled) {
                    return Ok(false);
                }
                let channel_ids = persisted_rule_channels(&transaction, &event.event_type)?
                    .unwrap_or_else(|| catalog_item.supported_channels.clone());
                if channel_ids.is_empty() {
                    return Ok(false);
                }

                let inserted = transaction.execute(
                    "INSERT INTO notification_events (
                       id, event_type, payload_version, occurred_at_ms, title, body,
                       dedupe_key, created_at_ms
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                     ON CONFLICT(dedupe_key) DO NOTHING",
                    params![
                        event.id,
                        event.event_type,
                        event.payload_version,
                        event.occurred_at_ms,
                        event.title,
                        event.body,
                        event.dedupe_key,
                        now.as_i64(),
                    ],
                )?;
                if inserted == 0 {
                    return Ok(false);
                }
                for channel_id in channel_ids {
                    if !catalog_item.supported_channels.contains(&channel_id) {
                        return Err(DatabaseError::CatalogChanged);
                    }
                    transaction.execute(
                        "INSERT INTO notification_outbox (
                           event_id, channel_id, status, attempt_count, next_attempt_at_ms,
                           created_at_ms, updated_at_ms
                         ) VALUES (?1, ?2, 'pending', 0, ?3, ?3, ?3)",
                        params![event.id, channel_id, now.as_i64()],
                    )?;
                }
                transaction.commit()?;
                Ok(true)
            })
            .await
    }

    pub(crate) async fn notification_template_override(
        &self,
        event_type: &str,
        channel_id: &str,
    ) -> Result<Option<NotificationTemplateOverride>> {
        let event_type = event_type.to_owned();
        let channel_id = channel_id.to_owned();
        self.database()
            .call(move |connection| {
                connection
                    .query_row(
                        "SELECT event_type, channel_id, mode, marker_id, title_template,
                                body_template, template_schema_version
                         FROM notification_template_overrides
                         WHERE event_type = ?1 AND channel_id = ?2",
                        params![event_type, channel_id],
                        template_override_from_row,
                    )
                    .optional()
                    .map_err(DatabaseError::from)
            })
            .await
    }

    pub(crate) async fn claim_notification(
        &self,
        now: TimestampMs,
        lease_token: String,
    ) -> Result<Option<NotificationDelivery>> {
        self.database()
            .call(move |connection| {
                let transaction = connection.transaction()?;
                transaction.execute(
                    "UPDATE notification_outbox
                     SET status = 'pending', lease_token = NULL, lease_until_ms = NULL,
                         next_attempt_at_ms = ?1, updated_at_ms = ?1
                     WHERE status = 'dispatching' AND lease_until_ms <= ?1",
                    [now.as_i64()],
                )?;

                let candidate = transaction
                    .query_row(
                        "SELECT o.event_id, e.event_type, o.channel_id, e.title, e.body,
                                o.attempt_count
                         FROM notification_outbox o
                         JOIN notification_events e ON e.id = o.event_id
                         WHERE o.status = 'pending' AND o.next_attempt_at_ms <= ?1
                         ORDER BY o.next_attempt_at_ms, o.created_at_ms, o.event_id
                         LIMIT 1",
                        [now.as_i64()],
                        |row| {
                            Ok((
                                row.get::<_, String>(0)?,
                                row.get::<_, String>(1)?,
                                row.get::<_, String>(2)?,
                                row.get::<_, String>(3)?,
                                row.get::<_, String>(4)?,
                                row.get::<_, i64>(5)?,
                            ))
                        },
                    )
                    .optional()?;
                let Some((event_id, event_type, channel_id, title, body, attempt_count)) =
                    candidate
                else {
                    transaction.commit()?;
                    return Ok(None);
                };
                let lease_until = now.as_i64().saturating_add(DISPATCH_LEASE_MS);
                let changed = transaction.execute(
                    "UPDATE notification_outbox
                     SET status = 'dispatching', attempt_count = attempt_count + 1,
                         lease_token = ?1, lease_until_ms = ?2, updated_at_ms = ?3
                     WHERE event_id = ?4 AND channel_id = ?5 AND status = 'pending'",
                    params![lease_token, lease_until, now.as_i64(), event_id, channel_id,],
                )?;
                if changed != 1 {
                    return Err(DatabaseError::Conflict {
                        entity: "notification-outbox",
                    });
                }
                transaction.commit()?;
                let attempt_count =
                    u32::try_from(attempt_count.saturating_add(1)).map_err(|_| {
                        DatabaseError::InvalidInput {
                            field: "attempt_count",
                        }
                    })?;
                Ok(Some(NotificationDelivery {
                    event_id,
                    event_type,
                    channel_id,
                    title,
                    body,
                    attempt_count,
                    lease_token,
                }))
            })
            .await
    }

    pub(crate) async fn notification_policy_allows(
        &self,
        delivery: &NotificationDelivery,
        catalog: Vec<NotificationEventCatalogItem>,
    ) -> Result<bool> {
        let event_type = delivery.event_type.clone();
        let channel_id = delivery.channel_id.clone();
        self.database()
            .call(move |connection| {
                let enabled = connection.query_row(
                    "SELECT notifications_enabled FROM notification_settings WHERE singleton = 1",
                    [],
                    |row| row.get::<_, bool>(0),
                )?;
                if !enabled {
                    return Ok(false);
                }
                let Some(catalog_item) = catalog.iter().find(|item| item.event_type == event_type)
                else {
                    return Ok(false);
                };
                let rule_enabled = connection
                    .query_row(
                        "SELECT enabled FROM notification_rules WHERE event_type = ?1",
                        [&event_type],
                        |row| row.get::<_, bool>(0),
                    )
                    .optional()?
                    .unwrap_or(catalog_item.default_enabled);
                if !rule_enabled {
                    return Ok(false);
                }
                let channels = persisted_rule_channels(connection, &event_type)?
                    .unwrap_or_else(|| catalog_item.supported_channels.clone());
                Ok(channels.contains(&channel_id))
            })
            .await
    }

    pub(crate) async fn finish_notification(
        &self,
        delivery: &NotificationDelivery,
        status: &'static str,
        error_code: Option<&'static str>,
        retry_at: Option<TimestampMs>,
        now: TimestampMs,
    ) -> Result<()> {
        if !matches!(status, "pending" | "dispatched" | "failed" | "suppressed") {
            return Err(DatabaseError::InvalidInput {
                field: "outbox_status",
            });
        }
        let event_id = delivery.event_id.clone();
        let channel_id = delivery.channel_id.clone();
        let lease_token = delivery.lease_token.clone();
        self.database()
            .call(move |connection| {
                let next_attempt_at = retry_at.unwrap_or(now).as_i64();
                let changed = connection.execute(
                    "UPDATE notification_outbox
                     SET status = ?1, error_code = ?2, next_attempt_at_ms = ?3,
                         lease_token = NULL, lease_until_ms = NULL, updated_at_ms = ?4
                     WHERE event_id = ?5 AND channel_id = ?6
                       AND status = 'dispatching' AND lease_token = ?7",
                    params![
                        status,
                        error_code,
                        next_attempt_at,
                        now.as_i64(),
                        event_id,
                        channel_id,
                        lease_token,
                    ],
                )?;
                if changed != 1 {
                    return Err(DatabaseError::Conflict {
                        entity: "notification-outbox",
                    });
                }
                Ok(())
            })
            .await
    }

    pub(crate) async fn cleanup_terminal_notifications(
        &self,
        terminal_before: TimestampMs,
    ) -> Result<usize> {
        self.database()
            .call(move |connection| {
                connection
                    .execute(
                        "DELETE FROM notification_events
                         WHERE EXISTS (
                           SELECT 1 FROM notification_outbox terminal
                           WHERE terminal.event_id = notification_events.id
                         )
                           AND NOT EXISTS (
                             SELECT 1 FROM notification_outbox retained
                             WHERE retained.event_id = notification_events.id
                               AND (
                                 retained.status IN ('pending', 'dispatching')
                                 OR retained.updated_at_ms > ?1
                               )
                           )",
                        [terminal_before.as_i64()],
                    )
                    .map_err(DatabaseError::from)
            })
            .await
    }
}

fn load_notification_settings(
    connection: &Connection,
    catalog: &[NotificationEventCatalogItem],
) -> Result<NotificationSettingsState> {
    let (revision, notifications_enabled) = connection.query_row(
        "SELECT revision, notifications_enabled FROM notification_settings WHERE singleton = 1",
        [],
        |row| Ok((row.get::<_, i64>(0)?, row.get::<_, bool>(1)?)),
    )?;
    let mut rules = Vec::with_capacity(catalog.len());
    for event in catalog {
        let enabled = connection
            .query_row(
                "SELECT enabled FROM notification_rules WHERE event_type = ?1",
                [&event.event_type],
                |row| row.get::<_, bool>(0),
            )
            .optional()?
            .unwrap_or(event.default_enabled);
        let channel_ids = persisted_rule_channels(connection, &event.event_type)?
            .unwrap_or_else(|| event.supported_channels.clone());
        let template_overrides = persisted_template_overrides(connection, &event.event_type)?;
        rules.push(NotificationRule {
            event_type: event.event_type.clone(),
            enabled,
            channel_ids,
            template_overrides,
        });
    }
    Ok(NotificationSettingsState {
        revision: sqlite_u64(revision, "revision")?,
        notifications_enabled,
        rules,
    })
}

fn persisted_template_overrides(
    connection: &Connection,
    event_type: &str,
) -> Result<Vec<NotificationTemplateOverride>> {
    let mut statement = connection.prepare(
        "SELECT event_type, channel_id, mode, marker_id, title_template,
                body_template, template_schema_version
         FROM notification_template_overrides
         WHERE event_type = ?1 ORDER BY channel_id",
    )?;
    let result = statement
        .query_map([event_type], template_override_from_row)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(DatabaseError::from);
    result
}

fn template_override_from_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<NotificationTemplateOverride> {
    let mode = match row.get::<_, String>(2)?.as_str() {
        "default" => NotificationTemplateMode::Default,
        "custom" => NotificationTemplateMode::Custom,
        _ => return Err(rusqlite::Error::InvalidQuery),
    };
    Ok(NotificationTemplateOverride {
        event_type: row.get(0)?,
        channel_id: row.get(1)?,
        mode,
        marker_id: row.get(3)?,
        title_template: row.get(4)?,
        body_template: row.get(5)?,
        template_schema_version: row.get(6)?,
    })
}

fn template_mode_name(mode: NotificationTemplateMode) -> &'static str {
    match mode {
        NotificationTemplateMode::Default => "default",
        NotificationTemplateMode::Custom => "custom",
    }
}

fn persisted_rule_channels(
    connection: &Connection,
    event_type: &str,
) -> Result<Option<Vec<String>>> {
    let exists = connection
        .query_row(
            "SELECT 1 FROM notification_rules WHERE event_type = ?1",
            [event_type],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    if !exists {
        return Ok(None);
    }
    let mut statement = connection.prepare(
        "SELECT channel_id FROM notification_rule_channels
         WHERE event_type = ?1 ORDER BY channel_id",
    )?;
    let channels = statement
        .query_map([event_type], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(Some(channels))
}
