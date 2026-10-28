use super::{GeneralSettings, Language, SettingsStore, StorageError, ThemeMode};
use crate::shared::persistence::{Database, Migration};
use rusqlite::{
    params,
    types::{FromSql, FromSqlError, FromSqlResult, ToSql, ToSqlOutput, ValueRef},
    Connection,
};

/// 组装层登记模块的建表步骤；shared 只接收 SQL，不反向依赖业务模块。
pub(crate) const SCHEMA: Migration = Migration {
    name: "configuration",
    sql: include_str!("configuration.sql"),
};

pub(crate) struct SqliteSettingsStore {
    database: std::sync::Arc<Database>,
}

impl SqliteSettingsStore {
    pub(crate) fn new(database: impl Into<std::sync::Arc<Database>>) -> Self {
        Self {
            database: database.into(),
        }
    }
}

impl SettingsStore for SqliteSettingsStore {
    fn load(&self) -> Result<GeneralSettings, StorageError> {
        Ok(self
            .database
            .with_connection(|connection| load(connection))?)
    }

    fn save(&self, settings: &GeneralSettings) -> Result<GeneralSettings, StorageError> {
        Ok(self.database.with_connection(|connection| {
            let transaction = connection.transaction()?;
            let changed = transaction.execute(
                "UPDATE general_settings SET
                   launch_at_startup = ?1, continue_in_background = ?2, show_tray_icon = ?3,
                   theme = ?4, language = ?5, history_retention_days = ?6
                 WHERE singleton = 1",
                params![
                    settings.launch_at_startup,
                    settings.continue_in_background,
                    settings.show_tray_icon,
                    settings.theme,
                    settings.language,
                    settings.history_retention_days
                ],
            )?;
            if changed != 1 {
                return Err(rusqlite::Error::QueryReturnedNoRows);
            }
            // 在同一事务内读回，再提交：返回的就是本次保存结果，不会混入其他请求。
            let saved = load(&transaction)?;
            transaction.commit()?;
            Ok(saved)
        })?)
    }
}

fn load(connection: &Connection) -> rusqlite::Result<GeneralSettings> {
    connection.query_row(
        "SELECT launch_at_startup, continue_in_background, show_tray_icon, theme,
                language, history_retention_days FROM general_settings WHERE singleton = 1",
        [],
        |row| {
            Ok(GeneralSettings {
                launch_at_startup: row.get(0)?,
                continue_in_background: row.get(1)?,
                show_tray_icon: row.get(2)?,
                theme: row.get(3)?,
                language: row.get(4)?,
                history_retention_days: row.get(5)?,
            })
        },
    )
}

// 数据库存储的枚举编码归适配器；业务类型只声明业务取值和对外序列化契约。
impl FromSql for ThemeMode {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        match value.as_str()? {
            "system" => Ok(Self::System),
            "light" => Ok(Self::Light),
            "dark" => Ok(Self::Dark),
            _ => Err(FromSqlError::InvalidType),
        }
    }
}
impl ToSql for ThemeMode {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(match self {
            Self::System => "system",
            Self::Light => "light",
            Self::Dark => "dark",
        }
        .into())
    }
}
impl FromSql for Language {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        match value.as_str()? {
            "zh-CN" => Ok(Self::ZhCn),
            _ => Err(FromSqlError::InvalidType),
        }
    }
}
impl ToSql for Language {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(match self {
            Self::ZhCn => "zh-CN",
        }
        .into())
    }
}
