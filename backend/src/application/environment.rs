//! Product identity policy lives at the host composition boundary.
use crate::shared::environment::{
    AppEnvironment, ApplicationInfo, EnvironmentContext, EnvironmentDefinition, EnvironmentError,
};
use std::path::Path;

const IDENTITIES: &[(&str, AppEnvironment)] = &[
    ("com.relaypulse.app.dev", AppEnvironment::Development),
    ("com.relaypulse.app", AppEnvironment::Production),
    #[cfg(debug_assertions)]
    (
        "com.relaypulse.app.migration-test",
        AppEnvironment::Development,
    ),
];
pub(crate) fn identify(identifier: &str) -> Result<AppEnvironment, EnvironmentError> {
    IDENTITIES
        .iter()
        .find(|(id, _)| *id == identifier)
        .map(|(_, environment)| *environment)
        .ok_or_else(|| EnvironmentError::UnknownApplicationIdentity(identifier.to_owned()))
}
pub(crate) fn resolve(
    identifier: &str,
    system: &Path,
    data: &Path,
) -> Result<EnvironmentContext, EnvironmentError> {
    identify(identifier)?;
    let definitions: Vec<_> = IDENTITIES
        .iter()
        .map(|(identifier, environment)| EnvironmentDefinition {
            identifier,
            environment: *environment,
            data_root: system.join(identifier),
        })
        .collect();
    EnvironmentContext::resolve(
        identifier,
        data,
        &definitions,
        ApplicationInfo::new("RelayPulse", env!("CARGO_PKG_VERSION"))?,
    )
}

#[cfg(test)]
#[path = "../../tests/unit/host_environment.rs"]
mod tests;
