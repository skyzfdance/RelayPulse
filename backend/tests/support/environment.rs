//! 所有夹具都由测试调用方传入临时目录；不解析真实用户目录。
use crate::shared::environment::{
    AppEnvironment, ApplicationInfo, EnvironmentContext, EnvironmentDefinition, EnvironmentError,
};

pub fn resolve(
    identifier: &str,
    base: &Path,
    data: &Path,
) -> Result<EnvironmentContext, EnvironmentError> {
    let definitions = [
        EnvironmentDefinition {
            identifier: "com.relaypulse.app.dev",
            environment: AppEnvironment::Development,
            data_root: base.join("com.relaypulse.app.dev"),
        },
        EnvironmentDefinition {
            identifier: "com.relaypulse.app",
            environment: AppEnvironment::Production,
            data_root: base.join("com.relaypulse.app"),
        },
    ];
    EnvironmentContext::resolve(
        identifier,
        data,
        &definitions,
        ApplicationInfo::new("FixtureHost", "99.7.3")?,
    )
}
use std::path::Path;

pub fn environment(base: &Path, identifier: &str) -> EnvironmentContext {
    resolve(identifier, base, &base.join(identifier)).unwrap()
}
