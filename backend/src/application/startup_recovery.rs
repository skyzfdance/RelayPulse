//! User-authorized startup upgrade: reuse the offline converter, preserve the original
//! directory, and journal the directory switch before either rename can take place.
mod activation;

use crate::startup::{StartupError, StartupState};
use serde::Serialize;
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
};

#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub(crate) struct MigrationError {
    pub code: &'static str,
    pub message: String,
}
impl MigrationError {
    pub(super) fn failed(message: impl Into<String>) -> Self {
        Self {
            code: "startup_migration_failed",
            message: message.into(),
        }
    }
}

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum StartupPhase {
    Ready,
    MigrationRequired,
    Migrating,
    MigrationCompleted,
    Blocked,
}
#[derive(Clone, Serialize)]
pub(crate) struct StartupStatus {
    pub status: StartupPhase,
    pub message: String,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MigrationOutcome {
    pub backup_directory: String,
}

pub(crate) struct StartupRecovery {
    root: Option<PathBuf>,
    initial: StartupStatus,
    eligible: bool,
    busy: AtomicBool,
    completed: Mutex<Option<MigrationOutcome>>,
}

impl StartupRecovery {
    pub fn new(root: Option<PathBuf>, state: &StartupState) -> Self {
        let (status, message) = match state {
            StartupState::Ready(_) => (StartupPhase::Ready, String::new()),
            StartupState::Blocked(error) => {
                (StartupPhase::Blocked, format!("应用启动受阻：{error}"))
            }
        };
        Self {
            root,
            initial: StartupStatus { status, message },
            eligible: matches!(state, StartupState::Blocked(StartupError::Persistence(_))),
            busy: AtomicBool::new(false),
            completed: Mutex::new(None),
        }
    }

    pub fn is_busy(&self) -> bool {
        self.busy.load(Ordering::SeqCst)
    }

    pub fn status(&self) -> StartupStatus {
        if self.is_busy() {
            return StartupStatus {
                status: StartupPhase::Migrating,
                message: "正在备份并迁移数据，请保持应用运行。".into(),
            };
        }
        if self.can_restart() {
            return StartupStatus {
                status: StartupPhase::MigrationCompleted,
                message: "数据迁移已完成，重启后即可继续使用。".into(),
            };
        }
        if self.eligible {
            if let Some(root) = &self.root {
                match super::data_transfer::inspect_source(root) {
                    Ok(()) => return StartupStatus { status: StartupPhase::MigrationRequired, message: "检测到旧版本数据，需要转换后才能继续使用。原数据会完整保留为备份，成功后自动重启。".into() },
                    Err(error) => return StartupStatus { status: StartupPhase::Blocked, message: format!("现有数据暂时无法迁移，原数据未被修改：{error}") },
                }
            }
        }
        self.initial.clone()
    }

    pub fn can_restart(&self) -> bool {
        !self.is_busy() && self.completed.try_lock().is_ok_and(|value| value.is_some())
    }

    pub fn migrate(&self) -> Result<MigrationOutcome, MigrationError> {
        if self
            .busy
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err(MigrationError {
                code: "startup_migration_busy",
                message: "数据迁移正在进行，请勿重复操作。".into(),
            });
        }
        struct BusyGuard<'a>(&'a AtomicBool);
        impl Drop for BusyGuard<'_> {
            fn drop(&mut self) {
                self.0.store(false, Ordering::SeqCst);
            }
        }
        let _busy = BusyGuard(&self.busy);
        let mut completed = self
            .completed
            .lock()
            .map_err(|_| MigrationError::failed("无法取得迁移状态，请重启应用后重试。"))?;
        if let Some(result) = &*completed {
            return Ok(result.clone());
        }
        if !self.eligible {
            return Err(MigrationError::failed("当前启动状态不允许迁移数据。"));
        }
        let root = self
            .root
            .as_ref()
            .ok_or_else(|| MigrationError::failed("无法取得当前应用的数据目录。"))?;
        super::data_transfer::inspect_source(root).map_err(|error| {
            MigrationError::failed(format!("旧数据未通过检查，原数据未被修改：{error}"))
        })?;
        let backup = activation::convert_and_activate(root)?;
        let outcome = MigrationOutcome {
            backup_directory: backup.to_string_lossy().into_owned(),
        };
        *completed = Some(outcome.clone());
        Ok(outcome)
    }
}

pub(crate) use activation::restore_interrupted;

#[cfg(test)]
#[path = "../../tests/integration/startup/migration.rs"]
mod tests;
