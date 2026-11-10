use super::MigrationError;
use crate::application::{composition::BASELINE, data_transfer};
use crate::shared::persistence::Database;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};
use uuid::Uuid;

type Result<T> = std::result::Result<T, MigrationError>;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SwitchJournal {
    format_version: u8,
    id: Uuid,
}
struct Paths {
    journal: PathBuf,
    converted: PathBuf,
    backup: PathBuf,
}
fn paths(root: &Path, id: Uuid) -> Result<Paths> {
    let parent = root
        .parent()
        .ok_or_else(|| MigrationError::failed("数据目录没有可用的父目录。"))?;
    let name = root
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| MigrationError::failed("数据目录名称无法识别。"))?;
    Ok(Paths {
        journal: parent.join(format!(".{name}.upgrade.json")),
        converted: parent.join(format!("{name}.upgrade-{id}")),
        backup: parent.join(format!("{name}.backup-before-upgrade-{id}")),
    })
}
fn exists(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err(MigrationError::failed("迁移目录或状态文件不能是符号链接。"))
        }
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(MigrationError::failed(format!("无法检查迁移目录：{error}"))),
    }
}
fn sync_parent(root: &Path) -> Result<()> {
    #[cfg(not(unix))]
    let _ = root;
    #[cfg(unix)]
    fs::File::open(
        root.parent()
            .ok_or_else(|| MigrationError::failed("数据目录无父目录。"))?,
    )
    .and_then(|file| file.sync_all())
    .map_err(|error| MigrationError::failed(format!("无法保存数据目录切换状态：{error}")))?;
    Ok(())
}
fn write_journal(root: &Path, paths: &Paths, journal: &SwitchJournal) -> Result<()> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let temporary = paths
        .journal
        .with_extension(format!("json.{}.tmp", journal.id));
    let mut file = options.open(&temporary).map_err(|error| {
        MigrationError::failed(format!("无法创建迁移切换记录，原目录未被替换：{error}"))
    })?;
    file.write_all(
        &serde_json::to_vec(journal)
            .map_err(|_| MigrationError::failed("无法生成迁移切换记录。"))?,
    )
    .and_then(|()| file.sync_all())
    .map_err(|error| {
        MigrationError::failed(format!("无法保存迁移切换记录，原目录未被替换：{error}"))
    })?;
    drop(file);
    fs::rename(temporary, &paths.journal).map_err(|error| {
        MigrationError::failed(format!("无法登记迁移切换记录，原目录未被替换：{error}"))
    })?;
    sync_parent(root)
}
fn finish(root: &Path, paths: &Paths) -> Result<()> {
    sync_parent(root)?;
    fs::remove_file(&paths.journal).map_err(|error| {
        MigrationError::failed(format!("无法清理迁移切换记录，请重启应用完成恢复：{error}"))
    })?;
    sync_parent(root)
}

pub(super) fn convert_and_activate(root: &Path) -> Result<PathBuf> {
    restore_interrupted(root)?;
    let journal = SwitchJournal {
        format_version: 1,
        id: Uuid::new_v4(),
    };
    let paths = paths(root, journal.id)?;
    data_transfer::convert_offline(root, &paths.converted).map_err(|error| {
        MigrationError::failed(format!("数据转换或校验失败，原数据保持不变：{error}"))
    })?;
    write_journal(root, &paths, &journal)?;
    activate(root, &paths, |source, destination| {
        fs::rename(source, destination)
    })?;
    Ok(paths.backup)
}

fn activate(
    root: &Path,
    paths: &Paths,
    rename: impl Fn(&Path, &Path) -> std::io::Result<()>,
) -> Result<()> {
    if exists(&paths.backup)? {
        return Err(MigrationError::failed(
            "备份目录已存在，停止切换以保留已有备份。",
        ));
    }
    rename(root, &paths.backup).map_err(|error| {
        MigrationError::failed(format!("无法备份旧数据目录，原数据未被替换：{error}"))
    })?;
    if let Err(error) = rename(&paths.converted, root) {
        if rename(&paths.backup, root).is_err() {
            return Err(MigrationError::failed(format!(
                "数据目录切换未完成，请重启应用恢复。旧数据保存在 {}：{error}",
                paths.backup.display()
            )));
        }
        finish(root, paths)?;
        return Err(MigrationError::failed(format!(
            "数据目录切换失败，已恢复旧数据：{error}"
        )));
    }
    finish(root, paths)
}

/// Runs before database initialization, so an interrupted rename can never be
/// mistaken for a first launch and silently initialize a fresh, empty database.
pub(crate) fn restore_interrupted(root: &Path) -> Result<()> {
    let journal_path = paths(root, Uuid::nil())?.journal;
    if !exists(&journal_path)? {
        return Ok(());
    }
    if fs::metadata(&journal_path)
        .map_err(|error| MigrationError::failed(format!("无法读取迁移状态：{error}")))?
        .len()
        > 4096
    {
        return Err(MigrationError::failed(
            "迁移切换记录无法识别，已停止启动并保留现有目录。",
        ));
    }
    let journal: SwitchJournal = serde_json::from_slice(
        &fs::read(&journal_path)
            .map_err(|error| MigrationError::failed(format!("无法读取迁移状态：{error}")))?,
    )
    .map_err(|_| MigrationError::failed("迁移切换记录损坏，已停止启动并保留现有目录。"))?;
    if journal.format_version != 1 {
        return Err(MigrationError::failed("迁移切换记录版本不受支持。"));
    }
    let paths = paths(root, journal.id)?;
    match (
        exists(root)?,
        exists(&paths.backup)?,
        exists(&paths.converted)?,
    ) {
        // Conversion passed but no directory has moved yet.
        (true, false, true) => finish(root, &paths),
        // Restore the original if the process stopped between the two renames.
        (false, true, true) => {
            fs::rename(&paths.backup, root).map_err(|error| {
                MigrationError::failed(format!(
                    "无法恢复旧数据目录，备份仍在 {}：{error}",
                    paths.backup.display()
                ))
            })?;
            finish(root, &paths)
        }
        // The replacement completed; verify the current database before proceeding.
        (true, true, false) => {
            let database = fs::symlink_metadata(root.join("relaypulse.sqlite3")).map_err(|_| {
                MigrationError::failed("迁移后的数据库文件缺失，已保留旧数据备份并停止启动。")
            })?;
            if !database.is_file() || database.file_type().is_symlink() || database.len() == 0 {
                return Err(MigrationError::failed(
                    "迁移后的数据库文件无效，已保留旧数据备份并停止启动。",
                ));
            }
            if root.join("conversion-incomplete").exists() {
                return Err(MigrationError::failed(
                    "数据转换未完成，已保留旧数据备份并停止启动。",
                ));
            }
            Database::inspect_directory(root, BASELINE).map_err(|_| {
                MigrationError::failed("迁移后的数据库未通过检查，已保留旧数据备份并停止启动。")
            })?;
            finish(root, &paths)
        }
        _ => Err(MigrationError::failed(
            "迁移目录状态不完整，已停止启动；现有数据和备份均予以保留。",
        )),
    }
}

#[cfg(test)]
#[path = "../../../tests/integration/startup/migration_switch.rs"]
mod tests;
