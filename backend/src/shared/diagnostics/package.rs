//! 诊断包只投影受控事件；旧日志可能包含正文，绝不原样复制进导出文件。
use super::{event::Event, files, now_ms, Diagnostics, DiagnosticsError, LogStatistics, Outcome};
use crate::shared::file_output::filesystem::{self, FileWriter};
use serde::Serialize;
use std::{
    fs,
    io::{BufRead, BufReader},
    path::Path,
};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Package {
    format_version: u8,
    app_version: &'static str,
    environment: &'static str,
    generated_at_ms: u64,
    startup: Option<Outcome>,
    log_statistics: LogStatistics,
    probe_summary: Unavailable,
    events: Vec<Event>,
}

#[derive(Serialize)]
struct Unavailable {
    available: bool,
    reason: &'static str,
}

impl Diagnostics {
    pub(crate) fn package(&self) -> Result<Vec<u8>, DiagnosticsError> {
        let guard = self.lock()?;
        let statistics = self.statistics_unlocked()?;
        let mut events = Vec::new();
        if let Some(directory) = self.directory(false)? {
            let path = directory.join(files::EVENTS);
            if filesystem::regular_file(&path)?.is_some() {
                for line in BufReader::new(fs::File::open(path)?).lines() {
                    // 严格反序列化白名单，拒绝未知字段；不把损坏行或原始错误带入诊断包。
                    events.push(serde_json::from_str(&line?)?);
                }
            }
        }
        Ok(serde_json::to_vec_pretty(&Package {
            format_version: 1,
            app_version: env!("CARGO_PKG_VERSION"),
            environment: self.environment,
            generated_at_ms: now_ms(),
            startup: guard.startup,
            log_statistics: statistics,
            probe_summary: Unavailable {
                available: false,
                reason: "probe_summary_not_implemented",
            },
            events,
        })?)
    }

    pub(crate) fn export(&self, destination: &Path) -> Result<(), DiagnosticsError> {
        // 禁止把诊断包保存到任一环境的业务目录，避免覆盖数据库或自己的日志。
        let parent = destination
            .parent()
            .ok_or(DiagnosticsError::UnsafePath)?
            .canonicalize()?;
        let data_parent = self
            .root
            .parent()
            .ok_or(DiagnosticsError::UnsafePath)?
            .canonicalize()?;
        for identity in ["com.relaypulse.app.dev", "com.relaypulse.app"] {
            if parent.starts_with(data_parent.join(identity)) {
                return Err(DiagnosticsError::UnsafePath);
            }
        }
        filesystem::regular_file(destination)?;
        let bytes = self.package()?;
        let temporary = parent.join(format!(
            ".relaypulse-diagnostics-{}.tmp",
            uuid::Uuid::new_v4()
        ));
        // 完整写入并同步后才替换目标；失败清理临时文件并向命令层报错。
        let result = (|| {
            let mut file = FileWriter::create_new(&temporary)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            drop(file);
            fs::rename(&temporary, destination)?;
            Ok::<_, DiagnosticsError>(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(temporary);
        }
        result
    }
}
