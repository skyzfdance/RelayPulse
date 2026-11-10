//! SQLite 技术资源：连接、串行访问和具名 schema 管理，不包含业务读写 SQL。
mod migrations;
pub use migrations::{schema_objects, validate as validate_format, Migration, FORMAT_VERSION};

use crate::shared::environment::AppPaths;
use rusqlite::Connection;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
    time::Duration,
};

#[derive(Debug, thiserror::Error)]
pub enum PersistenceError {
    #[error("cannot prepare database path {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("database path conflict: {0}")]
    PathConflict(PathBuf),
    #[error("database connection lock is poisoned")]
    LockPoisoned,
    #[error("unsupported database schema: {0}")]
    UnsupportedSchema(&'static str),
    #[error("database operation failed: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

/// 多个业务存储通过 Arc 共享唯一连接及互斥锁；最后一个所有者释放时关闭连接。
/// 异步调用方安排阻塞工作；此层不启动业务后台任务。
pub struct Database {
    connection: Mutex<Connection>,
}

impl Database {
    pub fn inspect_directory(root: &Path, baseline: &[Migration]) -> Result<(), PersistenceError> {
        check_path(root, true)?;
        if root.join("conversion-incomplete").exists() {
            return Err(PersistenceError::UnsupportedSchema(
                "offline conversion is incomplete",
            ));
        }
        for name in [
            "relaypulse.sqlite3",
            "relaypulse.sqlite3-wal",
            "relaypulse.sqlite3-shm",
            "relaypulse.sqlite3-journal",
        ] {
            check_path(&root.join(name), false)?;
        }
        validate_existing(root, baseline)?;
        Ok(())
    }

    pub fn open(paths: &AppPaths, migrations: &[Migration]) -> Result<Self, PersistenceError> {
        Self::open_directory(paths.data_root(), migrations)
    }

    pub fn open_directory(root: &Path, migrations: &[Migration]) -> Result<Self, PersistenceError> {
        if root.join("conversion-incomplete").exists() {
            return Err(PersistenceError::UnsupportedSchema(
                "offline conversion is incomplete",
            ));
        }
        // 根目录校验不等于子文件安全：拒绝数据库及 sidecar 指向另一环境。
        check_path(root, true)?;
        fs::create_dir_all(root).map_err(|source| PersistenceError::Io {
            path: root.into(),
            source,
        })?;
        let actual = fs::canonicalize(root).map_err(|source| PersistenceError::Io {
            path: root.into(),
            source,
        })?;
        if actual != root {
            return Err(PersistenceError::PathConflict(root.into()));
        }
        for name in [
            "relaypulse.sqlite3",
            "relaypulse.sqlite3-wal",
            "relaypulse.sqlite3-shm",
            "relaypulse.sqlite3-journal",
        ] {
            check_path(&root.join(name), false)?;
        }
        validate_existing(root, migrations)?;
        // 数据库现在也持有站点凭据；在打开 SQLite 和生成 sidecar 前限制本地权限。
        #[cfg(unix)]
        {
            use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
            let database_path = root.join("relaypulse.sqlite3");
            let protect = || -> std::io::Result<()> {
                fs::set_permissions(root, fs::Permissions::from_mode(0o700))?;
                fs::OpenOptions::new()
                    .write(true)
                    .create(true)
                    .truncate(false)
                    .mode(0o600)
                    .open(&database_path)?;
                fs::set_permissions(&database_path, fs::Permissions::from_mode(0o600))
            };
            protect().map_err(|source| PersistenceError::Io {
                path: database_path,
                source,
            })?;
        }
        let mut connection = Connection::open(root.join("relaypulse.sqlite3"))?;
        connection.busy_timeout(Duration::from_secs(5))?;
        // 只执行 composition 登记的具名步骤；拒绝不支持的已有结构，不自动重建。
        connection.pragma_update(None, "foreign_keys", "ON")?;
        migrations::apply(&mut connection, migrations)?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.pragma_update(None, "synchronous", "NORMAL")?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    /// 适配器在闭包内使用 rusqlite 事务；失败时 Transaction 的 Drop 自动回滚。
    /// 不返回连接引用，避免调用方把事务带出锁的生命周期。
    pub fn with_connection<T>(
        &self,
        operation: impl FnOnce(&mut Connection) -> rusqlite::Result<T>,
    ) -> Result<T, PersistenceError> {
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| PersistenceError::LockPoisoned)?;
        Ok(operation(&mut connection)?)
    }
}

/// SQLite read-only handles may write a WAL shared-memory index. Inspect a disposable
/// copy so refusing an unknown format never changes the existing directory.
fn validate_existing(root: &Path, baseline: &[Migration]) -> Result<(), PersistenceError> {
    let path = root.join("relaypulse.sqlite3");
    match fs::metadata(&path) {
        Ok(metadata) if metadata.len() > 0 => (),
        Ok(_) => return Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(source) => return Err(PersistenceError::Io { path, source }),
    };
    let copy = tempfile::tempdir().map_err(|source| PersistenceError::Io {
        path: root.into(),
        source,
    })?;
    for name in [
        "relaypulse.sqlite3",
        "relaypulse.sqlite3-wal",
        "relaypulse.sqlite3-shm",
        "relaypulse.sqlite3-journal",
    ] {
        let source_path = root.join(name);
        if source_path.exists() {
            fs::copy(&source_path, copy.path().join(name)).map_err(|source| {
                PersistenceError::Io {
                    path: source_path,
                    source,
                }
            })?;
        }
    }
    let read = Connection::open_with_flags(
        copy.path().join("relaypulse.sqlite3"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?;
    // Empty files left by a rolled-back first initialization remain retryable.
    if !migrations::schema_objects(&read)?.is_empty() {
        migrations::validate(&read, baseline)?;
    }
    Ok(())
}

fn check_path(path: &Path, directory: bool) -> Result<(), PersistenceError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink()
                || (directory && !metadata.is_dir())
                || (!directory && !metadata.is_file())
            {
                return Err(PersistenceError::PathConflict(path.into()));
            }
            // Unix 硬链接也可能把两个环境连接到同一个数据库文件。
            #[cfg(unix)]
            if !directory {
                use std::os::unix::fs::MetadataExt;
                if metadata.nlink() > 1 {
                    return Err(PersistenceError::PathConflict(path.into()));
                }
            }
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(PersistenceError::Io {
            path: path.into(),
            source,
        }),
    }
}

#[cfg(any(test, feature = "test-support"))]
#[path = "../../../../../tests/support/environment.rs"]
pub mod test_support;
#[cfg(test)]
#[path = "../../../../../tests/unit/persistence.rs"]
mod tests;
