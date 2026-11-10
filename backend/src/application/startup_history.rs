//! 启动操作记录的有限补记：模块独占暂存格式和文件，恢复时沿用原事件 ID 与时间。
use crate::application::action_definitions::actions as OperationAction;
use crate::modules::operation_logs::{
    OperationEvent, OperationLogContent, OperationLogs, OperationResult, OperationSource,
};
use crate::shared::{
    environment::AppPaths,
    file_output::filesystem::{self, FileWriter},
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Pending {
    #[serde(default = "marker_version")]
    format_version: u8,
    id: String,
    occurred_at_ms: i64,
    stage: StartupFailureStage,
}
#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StartupFailureStage {
    Persistence,
    Configuration,
    Notifications,
    Sites,
    IqTest,
}
fn marker_version() -> u8 {
    1
}
fn marker(root: &Path) -> PathBuf {
    root.join("startup-operation-pending.json")
}
fn event(pending: &Pending) -> OperationEvent {
    let reason = match pending.stage {
        StartupFailureStage::Persistence => "业务数据库初始化失败。",
        StartupFailureStage::Configuration => "通用设置校验未通过。",
        StartupFailureStage::Notifications => "通知设置校验未通过。",
        StartupFailureStage::Sites => "站点配置校验未通过。",
        StartupFailureStage::IqTest => "IQ 数据或删除恢复校验未通过。",
    };
    let mut event = OperationEvent::new(
        OperationAction::ApplicationBlocked,
        OperationSource::System,
        OperationResult::Blocked,
        "RelayPulse",
        "应用启动受阻",
        OperationLogContent::Failure {
            activity: "应用启动".into(),
            target: "RelayPulse".into(),
            reason: reason.into(),
        },
    );
    event.occurred_at_ms = Some(pending.occurred_at_ms);
    event.id = Some(pending.id.clone());
    event
}
/// 数据库尚不可用时只保留最近一次受阻事件；补记失败不改变启动结果。
pub fn defer_startup_failure(root: &Path, stage: StartupFailureStage) {
    if save(root, stage).is_err() {
        eprintln!("[operation_logs] startup fallback unavailable");
    }
}
fn save(root: &Path, stage: StartupFailureStage) -> Result<(), ()> {
    filesystem::ensure_directory_tree(root).map_err(|_| ())?;
    // 只清理本模块遗留且文件名含合法 UUID 的普通临时文件。
    for entry in std::fs::read_dir(root).map_err(|_| ())? {
        let entry = entry.map_err(|_| ())?;
        let name = entry.file_name();
        let Some(id) = name
            .to_str()
            .and_then(|name| name.strip_prefix("startup-operation-"))
            .and_then(|name| name.strip_suffix(".tmp"))
        else {
            continue;
        };
        if uuid::Uuid::parse_str(id).is_ok()
            && filesystem::regular_file(&entry.path())
                .map_err(|_| ())?
                .is_some()
        {
            std::fs::remove_file(entry.path()).map_err(|_| ())?;
        }
    }
    let pending = Pending {
        format_version: 1,
        id: uuid::Uuid::new_v4().to_string(),
        occurred_at_ms: time::OffsetDateTime::now_utc()
            .unix_timestamp_nanos()
            .div_euclid(1_000_000) as i64,
        stage,
    };
    let path = marker(root);
    filesystem::regular_file(&path).map_err(|_| ())?;
    if path.exists() {
        let previous: Pending =
            serde_json::from_slice(&std::fs::read(&path).map_err(|_| ())?).map_err(|_| ())?;
        if previous.format_version != 1 {
            return Err(());
        }
    }
    let bytes = serde_json::to_vec(&pending).map_err(|_| ())?;
    // 暂存协议属于操作记录模块，不依赖业务数据库或诊断日志。
    let temporary = root.join(format!("startup-operation-{}.tmp", pending.id));
    let result = (|| {
        let mut writer = FileWriter::create_new(&temporary).map_err(|_| ())?;
        writer
            .write_all(&bytes)
            .and_then(|_| writer.sync_all())
            .map_err(|_| ())?;
        drop(writer);
        std::fs::rename(&temporary, &path).map_err(|_| ())?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(temporary);
    }
    result
}
fn replay(paths: &AppPaths, logs: &OperationLogs) {
    let path = marker(paths.data_root());
    let Ok(Some(metadata)) = filesystem::regular_file(&path) else {
        return;
    };
    if metadata.len() > 1024 {
        eprintln!("[operation_logs] invalid startup fallback");
        return;
    }
    let pending = std::fs::read(&path)
        .ok()
        .and_then(|b| serde_json::from_slice::<Pending>(&b).ok());
    let Some(pending) =
        pending.filter(|p| p.format_version == 1 && uuid::Uuid::parse_str(&p.id).is_ok())
    else {
        eprintln!("[operation_logs] invalid startup fallback");
        return;
    };
    if logs.record(event(&pending)).is_ok() {
        let _ = std::fs::remove_file(path);
    }
}

/// 调用方确认应用已就绪后，先补记上次受阻事件，再记录本次就绪。
pub(crate) fn record_startup_ready(logs: &OperationLogs, paths: &AppPaths) {
    replay(paths, logs);
    logs.record_best_effort(OperationEvent::new(
        OperationAction::ApplicationReady,
        OperationSource::System,
        OperationResult::Success,
        "RelayPulse",
        "应用已就绪",
        OperationLogContent::Snapshot { fields: vec![] },
    ));
}

#[cfg(test)]
#[path = "../../tests/integration/startup/operation_fallback.rs"]
mod tests;
