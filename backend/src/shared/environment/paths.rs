//! 跨平台的只读路径检查，不依赖 Tauri，也不创建目录。

use std::{
    ffi::OsString,
    fs, io,
    path::{Path, PathBuf},
};

use super::EnvironmentError;

pub(super) fn validate(current: &Path, other: &Path) -> Result<PathBuf, EnvironmentError> {
    // 当前应用根目录不能通过符号链接跳转到其他位置，包括悬空链接。
    match fs::symlink_metadata(current) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err(conflict(
                current,
                "application root must not be a symbolic link",
            ));
        }
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(inspection_error(current, error)),
    }

    let current = resolve_directory(current)?;
    let other = resolve_directory(other)?;
    // starts_with 比较路径组件，避免把 .app.dev 当作 .app 的子目录。
    if current.starts_with(&other) || other.starts_with(&current) {
        return Err(conflict(
            &current,
            "development and production roots overlap",
        ));
    }
    Ok(current)
}

/// 目标不存在时，只规范化最近存在的父目录，再拼回缺失部分。
/// 遇到文件、损坏链接或权限错误即停止，不将它们当成“首次运行”。
fn resolve_directory(path: &Path) -> Result<PathBuf, EnvironmentError> {
    let mut existing = path;
    let mut missing: Vec<OsString> = Vec::new();
    loop {
        match fs::symlink_metadata(existing) {
            Ok(_) => {
                let resolved = fs::canonicalize(existing)
                    .map_err(|error| inspection_error(existing, error))?;
                let metadata =
                    fs::metadata(&resolved).map_err(|error| inspection_error(existing, error))?;
                if !metadata.is_dir() {
                    return Err(conflict(existing, "a directory is required here"));
                }
                let mut result = resolved;
                for part in missing.into_iter().rev() {
                    result.push(part);
                }
                return Ok(result);
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                let Some(name) = existing.file_name() else {
                    return Err(inspection_error(existing, error));
                };
                let Some(parent) = existing.parent() else {
                    return Err(inspection_error(existing, error));
                };
                missing.push(name.to_os_string());
                existing = parent;
            }
            Err(error) => return Err(inspection_error(existing, error)),
        }
    }
}

fn conflict(path: &Path, reason: &'static str) -> EnvironmentError {
    EnvironmentError::DataDirectoryConflict {
        path: path.to_owned(),
        reason,
    }
}

fn inspection_error(path: &Path, source: io::Error) -> EnvironmentError {
    EnvironmentError::DataDirectoryInspectionFailed {
        path: path.to_owned(),
        source,
    }
}
