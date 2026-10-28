//! 同步文件基础能力：路径检查、私有权限、独占创建或追加写入。
//! 不选择文件名、格式、保留期或完成状态，也不创建后台任务。
use std::{
    fs::{self, File, Metadata, OpenOptions},
    io::Write,
    path::Path,
};

#[derive(Debug, thiserror::Error)]
pub(crate) enum FileError {
    #[error("local filesystem operation failed")]
    Io(#[from] std::io::Error),
    #[error("local path is not an isolated regular file or directory")]
    UnsafePath,
}
type Result<T> = std::result::Result<T, FileError>;

/// 检查目录，必要时只创建这一层。父目录必须由调用方提供并已存在。
pub(crate) fn directory(path: &Path, create: bool) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.is_dir() && !meta.file_type().is_symlink() => Ok(true),
        Ok(_) => Err(FileError::UnsafePath),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            if !create {
                return Ok(false);
            }
            match create_directory(path) {
                Ok(()) => Ok(true),
                // 并发首次创建同一目录时，仍复核对方创建的条目类型。
                Err(FileError::Io(error)) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    directory(path, false)
                }
                Err(error) => Err(error),
            }
        }
        Err(error) => Err(error.into()),
    }
}

/// 首次运行时创建缺失的目录层级。path 应来自已校验、规范化的环境路径。
/// 已存在目录不改变权限；缺失部分逐层创建并检查，不直接跟随目标链接。
pub(crate) fn ensure_directory_tree(path: &Path) -> Result<()> {
    let mut missing = Vec::new();
    let mut ancestor = path;
    while !directory(ancestor, false)? {
        missing.push(ancestor);
        ancestor = ancestor.parent().ok_or(FileError::UnsafePath)?;
    }
    for path in missing.into_iter().rev() {
        directory(path, true)?;
    }
    Ok(())
}

/// 独占创建目录；已有目录也报错，适用于一次性输出会话。
pub(crate) fn create_directory(path: &Path) -> Result<()> {
    require_parent(path)?;
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path)?;
    Ok(())
}

pub(crate) fn regular_file(path: &Path) -> Result<Option<Metadata>> {
    match fs::symlink_metadata(path) {
        Ok(meta) => {
            check_regular(&meta)?;
            Ok(Some(meta))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}
fn check_regular(meta: &Metadata) -> Result<()> {
    if !meta.is_file() || meta.file_type().is_symlink() {
        return Err(FileError::UnsafePath);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if meta.nlink() != 1 {
            return Err(FileError::UnsafePath);
        }
    }
    Ok(())
}
fn require_parent(path: &Path) -> Result<()> {
    let parent = path.parent().ok_or(FileError::UnsafePath)?;
    if !directory(parent, false)? {
        return Err(FileError::UnsafePath);
    }
    Ok(())
}
fn write_options() -> OpenOptions {
    let mut options = OpenOptions::new();
    options.write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
}

/// 仅持有一个文件句柄；Drop 关闭，不发布、不删除、不隐式吞掉写入错误。
/// 此层不加业务锁，调用方协调写入与读取/清理；异步调用方负责调度阻塞 I/O。
pub(crate) struct FileWriter {
    file: File,
}
impl FileWriter {
    pub(crate) fn create_new(path: &Path) -> Result<Self> {
        require_parent(path)?;
        Self::checked(write_options().create_new(true).open(path)?)
    }
    pub(crate) fn open_append(path: &Path) -> Result<Self> {
        require_parent(path)?;
        regular_file(path)?;
        Self::checked(write_options().create(true).append(true).open(path)?)
    }
    fn checked(file: File) -> Result<Self> {
        check_regular(&file.metadata()?)?;
        Ok(Self { file })
    }
    pub(crate) fn write_all(&mut self, bytes: &[u8]) -> Result<()> {
        self.file.write_all(bytes)?;
        Ok(())
    }
    pub(crate) fn flush(&mut self) -> Result<()> {
        self.file.flush()?;
        Ok(())
    }
    pub(crate) fn sync_all(&self) -> Result<()> {
        self.file.sync_all()?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/file_output/filesystem.rs"]
mod tests;
