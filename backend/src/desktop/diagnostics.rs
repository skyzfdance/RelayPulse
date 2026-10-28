//! 系统目录打开和保存对话框留在宿主；实际诊断文件由 shared/diagnostics 生成。
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use tauri::{AppHandle, Runtime};
use tauri_plugin_dialog::DialogExt;

#[derive(Debug, thiserror::Error)]
pub(crate) enum DiagnosticsPlatformError {
    #[error("system directory open failed")]
    OpenDirectory,
    #[error("save destination unavailable")]
    SaveDialog,
}

pub(crate) trait DiagnosticsPlatform: Send + Sync {
    fn open_directory(&self, path: &Path) -> Result<(), DiagnosticsPlatformError>;
    fn save_destination(&self) -> Result<Option<PathBuf>, DiagnosticsPlatformError>;
}

pub(crate) struct DiagnosticsDesktop(pub Arc<dyn DiagnosticsPlatform>);
pub(super) struct SystemDiagnostics<R: Runtime>(pub AppHandle<R>);

impl<R: Runtime> DiagnosticsPlatform for SystemDiagnostics<R> {
    fn open_directory(&self, path: &Path) -> Result<(), DiagnosticsPlatformError> {
        super::directories::open(path).map_err(|_| DiagnosticsPlatformError::OpenDirectory)
    }

    fn save_destination(&self) -> Result<Option<PathBuf>, DiagnosticsPlatformError> {
        self.0
            .dialog()
            .file()
            .add_filter("JSON 诊断包", &["json"])
            .set_file_name("relaypulse-diagnostics.json")
            .blocking_save_file()
            .map(|path| {
                path.into_path()
                    .map_err(|_| DiagnosticsPlatformError::SaveDialog)
            })
            .transpose()
    }
}
