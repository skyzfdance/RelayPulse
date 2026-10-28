//! 当前环境独立的诊断资源；写入、统计、清理和导出快照共用一把锁，与数据库无关。
mod event;
mod files;
mod package;
mod recorder;

use crate::shared::{
    environment::{AppEnvironment, EnvironmentContext},
    file_output::filesystem::{self, FileError, FileWriter},
};
pub(crate) use event::{Failure, Outcome};
pub(crate) use recorder::{DiagnosticFailure, Recorder};
use serde::Serialize;
use std::{
    fs,
    path::PathBuf,
    sync::{Mutex, MutexGuard},
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Debug, thiserror::Error)]
pub(crate) enum DiagnosticsError {
    #[error("diagnostic filesystem operation failed")]
    Io(#[from] std::io::Error),
    #[error("diagnostic path is not an isolated regular file or directory")]
    UnsafePath,
    #[error("diagnostic lock unavailable")]
    Lock,
    #[error("diagnostic event data is invalid")]
    InvalidData(#[from] serde_json::Error),
}

impl From<FileError> for DiagnosticsError {
    fn from(error: FileError) -> Self {
        match error {
            FileError::Io(error) => Self::Io(error),
            FileError::UnsafePath => Self::UnsafePath,
        }
    }
}

#[derive(Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LogStatistics {
    pub total_bytes: u64,
    pub file_count: u64,
}

#[derive(Default)]
struct State {
    startup: Option<Outcome>,
}

pub(crate) struct Diagnostics {
    root: PathBuf,
    environment: &'static str,
    state: Mutex<State>,
}

impl Diagnostics {
    pub(crate) fn new(environment: &EnvironmentContext) -> Self {
        Self {
            root: environment.paths().data_root().to_owned(),
            environment: match environment.environment() {
                AppEnvironment::Development => "development",
                AppEnvironment::Production => "production",
            },
            state: Mutex::new(State::default()),
        }
    }

    fn lock(&self) -> Result<MutexGuard<'_, State>, DiagnosticsError> {
        self.state.lock().map_err(|_| DiagnosticsError::Lock)
    }

    fn directory(&self, create: bool) -> Result<Option<PathBuf>, DiagnosticsError> {
        if create {
            filesystem::ensure_directory_tree(&self.root)?;
        } else if !filesystem::directory(&self.root, false)? {
            return Ok(None);
        }
        let directory = self.root.join("logs");
        Ok(filesystem::directory(&directory, create)?.then_some(directory))
    }

    pub(crate) fn log_directory(&self) -> Result<PathBuf, DiagnosticsError> {
        let _guard = self.lock()?;
        self.directory(true)?.ok_or(DiagnosticsError::UnsafePath)
    }

    pub(crate) fn record(
        &self,
        operation: &'static str,
        outcome: Outcome,
    ) -> Result<(), DiagnosticsError> {
        let mut guard = self.lock()?;
        if operation == "startup" {
            guard.startup = Some(outcome);
        }
        let path = self
            .directory(true)?
            .ok_or(DiagnosticsError::UnsafePath)?
            .join(files::EVENTS);
        let event = event::Event {
            timestamp_ms: now_ms(),
            operation: operation.into(),
            outcome,
        };
        let mut bytes = serde_json::to_vec(&event)?;
        bytes.push(b'\n');
        let mut file = FileWriter::open_append(&path)?;
        file.write_all(&bytes)?;
        file.flush()?;
        Ok(())
    }

    /// 诊断失败不能改变业务操作结果；stderr 也只输出固定分类，不泄露原始错误。
    pub(crate) fn record_best_effort(&self, operation: &'static str, outcome: Outcome) {
        if self.record(operation, outcome).is_err() {
            eprintln!("[diagnostics] event write failed");
        }
    }

    fn files(&self) -> Result<Vec<(PathBuf, u64)>, DiagnosticsError> {
        match self.directory(false)? {
            Some(directory) => files::logs(&directory),
            None => Ok(Vec::new()),
        }
    }

    fn statistics_unlocked(&self) -> Result<LogStatistics, DiagnosticsError> {
        let files = self.files()?;
        Ok(LogStatistics {
            total_bytes: files.iter().map(|(_, size)| size).sum(),
            file_count: files.len() as u64,
        })
    }

    pub(crate) fn statistics(&self) -> Result<LogStatistics, DiagnosticsError> {
        let _guard = self.lock()?;
        self.statistics_unlocked()
    }

    pub(crate) fn clear(&self) -> Result<LogStatistics, DiagnosticsError> {
        let _guard = self.lock()?;
        // 先验证全部候选，再删除；不持有长期开启的文件句柄，下一次写入可重新创建文件。
        for (path, _) in self.files()? {
            fs::remove_file(path)?;
        }
        self.statistics_unlocked()
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
#[path = "../../../tests/unit/diagnostics/logging.rs"]
mod tests;
