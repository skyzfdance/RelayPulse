//! 已有输出包的同步访问。调用方须放在受生命周期跟踪的阻塞 worker 内。
use super::{filesystem, paths, OutputError, OutputStore, Result};
use std::path::{Path, PathBuf};

impl OutputStore {
    pub(crate) fn inspect<T>(&self, id: &str, read: impl FnOnce(&Path) -> Result<T>) -> Result<T> {
        paths::identifier(id)?;
        let state = self.0.state.lock().map_err(|_| OutputError)?;
        if state.active.contains(id) || !filesystem::directory(&self.0.root, false)? {
            return Err(OutputError);
        }
        let published = self.0.root.join(id);
        let partial = self.0.root.join(format!("{id}.partial"));
        let path = match (
            filesystem::directory(&published, false)?,
            filesystem::directory(&partial, false)?,
        ) {
            (true, false) => published,
            (false, true) => partial,
            _ => return Err(OutputError),
        };
        read(&path)
    }
    /// 不沿用后台回收的“跳过活动文件即成功”：显式删除必须确认每个文件已删除。
    pub(crate) fn remove_ids(&self, ids: &[String]) -> Result<()> {
        let state = self.0.state.lock().map_err(|_| OutputError)?;
        if ids.iter().any(|id| state.active.contains(id)) {
            return Err(OutputError);
        }
        if !filesystem::directory(&self.0.root, false)? {
            return Ok(());
        }
        let mut packages = Vec::new();
        for id in ids {
            paths::identifier(id)?;
            for name in [id.clone(), format!("{id}.partial")] {
                let path = self.0.root.join(name);
                if filesystem::directory(&path, false)? {
                    packages.push(package(path)?);
                }
            }
        }
        remove(packages)
    }
    pub(crate) fn remove_all(&self) -> Result<()> {
        let state = self.0.state.lock().map_err(|_| OutputError)?;
        if !state.active.is_empty() {
            return Err(OutputError);
        }
        if !filesystem::directory(&self.0.root, false)? {
            return Ok(());
        }
        let mut packages = Vec::new();
        for entry in std::fs::read_dir(&self.0.root)? {
            let entry = entry?;
            let name = entry.file_name().into_string().map_err(|_| OutputError)?;
            // 与枚举规则一致：只管理合法 ID 的输出包，保留 .DS_Store 等外部条目。
            if paths::identifier(name.strip_suffix(".partial").unwrap_or(&name)).is_err() {
                continue;
            }
            packages.push(package(entry.path())?);
        }
        remove(packages)
    }
}
fn package(path: PathBuf) -> Result<(PathBuf, Vec<PathBuf>)> {
    if !filesystem::directory(&path, false)? {
        return Err(OutputError);
    }
    let files = std::fs::read_dir(&path)?
        .map(|e| e.map(|e| e.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    for file in &files {
        filesystem::regular_file(file)?.ok_or(OutputError)?;
    }
    Ok((path, files))
}
fn remove(packages: Vec<(PathBuf, Vec<PathBuf>)>) -> Result<()> {
    for (path, files) in packages {
        for file in files {
            std::fs::remove_file(file)?;
        }
        std::fs::remove_dir(path)?;
    }
    Ok(())
}
