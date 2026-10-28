use super::{validation, *};
use crate::shared::persistence::{Database, Migration};
use rusqlite::{params, types::Type, Connection};
use std::sync::Arc;

pub(crate) const SCHEMA: Migration = Migration {
    name: "notification_configuration",
    sql: include_str!("notifications.sql"),
};

pub(crate) struct SqliteNotificationStore {
    database: Arc<Database>,
}
impl SqliteNotificationStore {
    pub(crate) fn new(database: Arc<Database>) -> Self {
        Self { database }
    }
}

fn corrupt(error: impl std::error::Error + Send + Sync + 'static) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(0, Type::Text, Box::new(error))
}

fn boolean(value: i64) -> rusqlite::Result<bool> {
    match value {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(corrupt(NotificationError::CorruptSettings)),
    }
}

fn load(connection: &Connection) -> rusqlite::Result<StoredSettings> {
    let enabled = connection.query_row(
        "SELECT enabled FROM notification_settings WHERE singleton = 1",
        [],
        |row| boolean(row.get(0)?),
    )?;
    let mut statement = connection
        .prepare("SELECT id, status, custom_template FROM notification_rules ORDER BY id")?;
    let rules = statement
        .query_map([], |row| {
            let custom: Option<String> = row.get(2)?;
            Ok(StoredRule {
                id: row.get(0)?,
                status: boolean(row.get(1)?)?,
                custom: custom
                    .map(|value| serde_json::from_str(&value).map_err(corrupt))
                    .transpose()?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let stored = StoredSettings { enabled, rules };
    validation::stored(&stored).map_err(corrupt)?;
    Ok(stored)
}

impl NotificationStore for SqliteNotificationStore {
    fn load(&self) -> Result<StoredSettings, NotificationError> {
        Ok(self
            .database
            .with_connection(|connection| load(connection))?)
    }
    fn save_settings(
        &self,
        settings: &NotificationSettings,
    ) -> Result<StoredSettings, NotificationError> {
        Ok(self.database.with_connection(|connection| {
            let transaction = connection.transaction()?;
            load(&transaction)?; // 不以保存请求掩盖已损坏或缺失的配置。
            transaction.execute(
                "UPDATE notification_settings SET enabled = ?1 WHERE singleton = 1",
                [settings.notifications_enabled],
            )?;
            for rule in &settings.rules {
                let count = transaction.execute(
                    "UPDATE notification_rules SET status = ?1 WHERE id = ?2",
                    params![rule.status, rule.id],
                )?;
                if count != 1 {
                    return Err(rusqlite::Error::QueryReturnedNoRows);
                }
            }
            let saved = load(&transaction)?;
            transaction.commit()?;
            Ok(saved)
        })?)
    }
    fn save_template(
        &self,
        id: &str,
        custom: Option<&TemplateUpdate>,
    ) -> Result<StoredSettings, NotificationError> {
        let json = custom
            .map(serde_json::to_string)
            .transpose()
            .map_err(|_| NotificationError::CorruptSettings)?;
        Ok(self.database.with_connection(|connection| {
            let transaction = connection.transaction()?;
            load(&transaction)?;
            let count = transaction.execute(
                "UPDATE notification_rules SET custom_template = ?1 WHERE id = ?2",
                params![json, id],
            )?;
            if count != 1 {
                return Err(rusqlite::Error::QueryReturnedNoRows);
            }
            let saved = load(&transaction)?;
            transaction.commit()?;
            Ok(saved)
        })?)
    }
}
