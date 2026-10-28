//! 系统文件管理器的叶子能力；路径定位和授权由调用方业务负责。
use std::{path::Path, sync::Arc};
#[derive(Debug, thiserror::Error)]
#[error("system directory open failed")]
pub(crate) struct OpenDirectoryError;
pub(crate) trait DirectoryOpener: Send + Sync {
    fn open(&self, path: &Path) -> Result<(), OpenDirectoryError>;
}
pub(crate) struct DirectoryDesktop(pub Arc<dyn DirectoryOpener>);
pub(super) struct SystemDirectories;
impl DirectoryOpener for SystemDirectories {
    fn open(&self, path: &Path) -> Result<(), OpenDirectoryError> {
        open(path)
    }
}
pub(super) fn open(path: &Path) -> Result<(), OpenDirectoryError> {
    #[cfg(target_os = "macos")]
    let mut command = std::process::Command::new("open");
    #[cfg(target_os = "windows")]
    let mut command = std::process::Command::new("explorer.exe");
    #[cfg(target_os = "linux")]
    let mut command = std::process::Command::new("xdg-open");
    #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
    {
        if command
            .arg(path)
            .status()
            .map_err(|_| OpenDirectoryError)?
            .success()
        {
            Ok(())
        } else {
            Err(OpenDirectoryError)
        }
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        let _ = path;
        Err(OpenDirectoryError)
    }
}
