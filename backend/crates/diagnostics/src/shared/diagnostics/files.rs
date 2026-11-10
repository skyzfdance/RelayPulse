//! 文件边界：只管理约定的诊断文件，不递归、不跟随链接。
use super::DiagnosticsError;
use crate::shared::file_output::filesystem;
use std::{
    fs,
    path::{Path, PathBuf},
};

pub(super) const EVENTS: &str = "diagnostic-events.jsonl";

fn owned(name: &str) -> bool {
    name == EVENTS
        || name == "relaypulse.log"
        || name
            .strip_prefix("relaypulse.log.")
            .is_some_and(|suffix| !suffix.is_empty() && suffix.bytes().all(|b| b.is_ascii_digit()))
}

pub(super) fn logs(directory: &Path) -> Result<Vec<(PathBuf, u64)>, DiagnosticsError> {
    let mut files = Vec::new();
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        if entry.file_name().to_str().is_some_and(owned) {
            if let Some(meta) = filesystem::regular_file(&entry.path())? {
                files.push((entry.path(), meta.len()));
            }
        }
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(files)
}
