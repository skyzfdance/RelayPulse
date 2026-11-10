//! 运行环境与数据根目录契约：只解析和检查，不创建或打开业务资源。

mod paths;

use std::{
    io,
    path::{Component, Path, PathBuf},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppEnvironment {
    Development,
    Production,
}

/// Host-owned identity and isolation policy. The runtime has no application ID catalog.
#[derive(Debug, Clone)]
pub struct EnvironmentDefinition {
    pub identifier: &'static str,
    pub environment: AppEnvironment,
    pub data_root: PathBuf,
}

#[derive(Debug, Clone)]
pub struct ApplicationInfo {
    name: String,
    version: String,
}
impl ApplicationInfo {
    pub fn new(
        name: impl Into<String>,
        version: impl Into<String>,
    ) -> Result<Self, EnvironmentError> {
        let value = Self {
            name: name.into(),
            version: version.into(),
        };
        if [&value.name, &value.version].iter().any(|part| {
            part.is_empty()
                || part.len() > 100
                || !part
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._-+".contains(&b))
        }) {
            return Err(EnvironmentError::InvalidDefinition);
        }
        Ok(value)
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn version(&self) -> &str {
        &self.version
    }
}

#[derive(Debug, thiserror::Error)]
pub enum EnvironmentError {
    #[error("invalid application metadata or isolation policy")]
    InvalidDefinition,
    #[error("unknown application identity: {0}")]
    UnknownApplicationIdentity(String),
    #[error("the host could not resolve the {0} directory")]
    SystemDataDirectoryUnavailable(&'static str),
    #[error("the application data directory does not match its environment")]
    DataDirectoryMismatch,
    #[error("data directory conflict at {path}: {reason}")]
    DataDirectoryConflict { path: PathBuf, reason: &'static str },
    #[error("cannot inspect data directory {path}: {source}")]
    DataDirectoryInspectionFailed {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

#[derive(Debug)]
pub struct AppPaths {
    data_root: PathBuf,
}

impl AppPaths {
    pub fn data_root(&self) -> &Path {
        &self.data_root
    }
}

#[derive(Debug)]
pub struct EnvironmentContext {
    environment: AppEnvironment,
    paths: AppPaths,
    application: ApplicationInfo,
    protected_roots: Vec<PathBuf>,
}

impl EnvironmentContext {
    /// 宿主提供路径来源；只有身份、目录归属及文件系统检查全部通过才返回上下文。
    pub fn resolve(
        identifier: &str,
        app_data_root: &Path,
        definitions: &[EnvironmentDefinition],
        application: ApplicationInfo,
    ) -> Result<Self, EnvironmentError> {
        // 第 1 步：只根据应用身份识别环境，不使用 debug/release 或运行时覆盖变量。
        if definitions.is_empty()
            || definitions.iter().any(|d| {
                d.identifier.is_empty()
                    || !d
                        .identifier
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
            })
        {
            return Err(EnvironmentError::InvalidDefinition);
        }
        let definition = definitions
            .iter()
            .find(|d| d.identifier == identifier)
            .ok_or_else(|| EnvironmentError::UnknownApplicationIdentity(identifier.to_owned()))?;
        let environment = definition.environment;

        // 第 2 步：先验证路径归属，错误配对不能进入文件系统检查。
        if !app_data_root.is_absolute()
            || app_data_root
                .components()
                .any(|part| matches!(part, Component::ParentDir))
            || definitions.iter().any(|d| {
                !d.data_root.is_absolute()
                    || d.data_root
                        .components()
                        .any(|part| matches!(part, Component::ParentDir))
            })
            || app_data_root != definition.data_root
        {
            return Err(EnvironmentError::DataDirectoryMismatch);
        }

        // 第 3 步：只检查目录元数据与真实路径，绝不读取另一环境的业务文件。
        paths::validate_current(app_data_root)?;
        let mut data_root = paths::resolve_directory(app_data_root)?;
        let mut protected_roots = Vec::new();
        for other in definitions {
            let root = &other.data_root;
            if other.identifier != identifier {
                data_root = paths::validate(app_data_root, root)?;
            }
            protected_roots.push(paths::resolve_directory(root)?);
        }
        Ok(Self {
            environment,
            paths: AppPaths { data_root },
            application,
            protected_roots,
        })
    }

    pub fn application(&self) -> &ApplicationInfo {
        &self.application
    }

    pub fn protected_roots(&self) -> &[PathBuf] {
        &self.protected_roots
    }

    pub fn environment(&self) -> AppEnvironment {
        self.environment
    }

    pub fn paths(&self) -> &AppPaths {
        &self.paths
    }
}

#[cfg(test)]
#[path = "../../../../../tests/unit/environment.rs"]
mod tests;
