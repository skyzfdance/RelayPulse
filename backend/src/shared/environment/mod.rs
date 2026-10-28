//! 运行环境与数据根目录契约：只解析和检查，不创建或打开业务资源。

mod paths;

use std::{
    io,
    path::{Component, Path, PathBuf},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AppEnvironment {
    Development,
    Production,
}

impl AppEnvironment {
    pub(crate) fn from_identifier(identifier: &str) -> Result<Self, EnvironmentError> {
        match identifier {
            "com.relaypulse.app.dev" => Ok(Self::Development),
            "com.relaypulse.app" => Ok(Self::Production),
            _ => Err(EnvironmentError::UnknownApplicationIdentity(
                identifier.to_owned(),
            )),
        }
    }

    fn identifier(self) -> &'static str {
        match self {
            Self::Development => "com.relaypulse.app.dev",
            Self::Production => "com.relaypulse.app",
        }
    }

    fn other(self) -> Self {
        match self {
            Self::Development => Self::Production,
            Self::Production => Self::Development,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum EnvironmentError {
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
pub(crate) struct AppPaths {
    data_root: PathBuf,
}

impl AppPaths {
    pub(crate) fn data_root(&self) -> &Path {
        &self.data_root
    }
}

#[derive(Debug)]
pub(crate) struct EnvironmentContext {
    environment: AppEnvironment,
    paths: AppPaths,
}

impl EnvironmentContext {
    /// 宿主提供路径来源；只有身份、目录归属及文件系统检查全部通过才返回上下文。
    pub(crate) fn resolve(
        identifier: &str,
        system_data_root: &Path,
        app_data_root: &Path,
    ) -> Result<Self, EnvironmentError> {
        // 第 1 步：只根据应用身份识别环境，不使用 debug/release 或运行时覆盖变量。
        let environment = AppEnvironment::from_identifier(identifier)?;

        // 第 2 步：先验证路径归属，错误配对不能进入文件系统检查。
        if !system_data_root.is_absolute()
            || !app_data_root.is_absolute()
            || system_data_root
                .components()
                .any(|part| matches!(part, Component::ParentDir))
            || app_data_root
                .components()
                .any(|part| matches!(part, Component::ParentDir))
            || app_data_root != system_data_root.join(environment.identifier())
        {
            return Err(EnvironmentError::DataDirectoryMismatch);
        }

        // 第 3 步：只检查目录元数据与真实路径，绝不读取另一环境的业务文件。
        let other_root = system_data_root.join(environment.other().identifier());
        let data_root = paths::validate(app_data_root, &other_root)?;
        Ok(Self {
            environment,
            paths: AppPaths { data_root },
        })
    }

    pub(crate) fn environment(&self) -> AppEnvironment {
        self.environment
    }

    pub(crate) fn paths(&self) -> &AppPaths {
        &self.paths
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/environment.rs"]
mod tests;
