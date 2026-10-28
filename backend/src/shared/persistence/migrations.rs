use super::PersistenceError;
use rusqlite::{Connection, TransactionBehavior};

/// 有名称的初始化/变更步骤；SQL 由所属业务模块提供，顺序由 composition 登记。
/// 后续变更使用新的描述性名称，不修改已经执行的步骤。
#[derive(Clone, Copy)]
pub(crate) struct Migration {
    pub name: &'static str,
    pub sql: &'static str,
}

pub(super) fn apply(
    connection: &mut Connection,
    migrations: &[Migration],
) -> Result<(), PersistenceError> {
    // 立即取得写锁，两个启动实例不能同时判断为首次初始化。
    // 表、默认数据和执行记录共用事务；任一步失败全部回滚。
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let old_version: i64 = transaction.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if old_version != 0 {
        return Err(PersistenceError::UnsupportedSchema(
            "legacy user_version is not supported",
        ));
    }
    let has_history: bool = transaction.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type = 'table' AND name = 'schema_migrations')",
        [], |row| row.get(0),
    )?;
    if !has_history {
        let has_objects: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%')",
            [],
            |row| row.get(0),
        )?;
        if has_objects {
            // 本轮明确不兼容旧结构，但也不自动删除或覆盖已有文件。
            return Err(PersistenceError::UnsupportedSchema(
                "existing database has no named schema history",
            ));
        }
        transaction.execute_batch(
            "CREATE TABLE schema_migrations (
                position INTEGER PRIMARY KEY CHECK (position >= 0),
                name TEXT NOT NULL UNIQUE
             ) STRICT;",
        )?;
    }
    let applied = {
        let mut statement = transaction
            .prepare("SELECT position, name FROM schema_migrations ORDER BY position")?;
        let rows = statement.query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };
    // 已执行步骤必须是当前清单的前缀，拒绝未知步骤、顺序变化和较新结构。
    for (index, (position, name)) in applied.iter().enumerate() {
        if usize::try_from(*position).ok() != Some(index)
            || migrations.get(index).map(|step| step.name) != Some(name.as_str())
        {
            return Err(PersistenceError::UnsupportedSchema(
                "named schema history does not match this application",
            ));
        }
    }
    for (position, step) in migrations.iter().enumerate().skip(applied.len()) {
        let position = i64::try_from(position)
            .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
        transaction.execute_batch(step.sql)?;
        transaction.execute(
            "INSERT INTO schema_migrations (position, name) VALUES (?1, ?2)",
            rusqlite::params![position, step.name],
        )?;
    }
    transaction.commit()?;
    Ok(())
}
