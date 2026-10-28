//! 托盘只表达配置是否有效，不表达健康颜色或当前请求是否在执行。
use crate::{
    modules::{
        configuration::{ConfigurationError, ConfigurationService},
        sites::{SiteError, SiteService, TaskError},
    },
    shared::state_changes::StateChanges,
};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TrayState {
    Hidden,
    Default,
    Active,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum TrayError {
    #[error(transparent)]
    Configuration(#[from] ConfigurationError),
    #[error(transparent)]
    Site(#[from] SiteError),
    #[error("托盘配置正在变化，请重试。")]
    Changing,
}

pub(crate) struct Tray {
    configuration: Arc<ConfigurationService>,
    sites: Arc<SiteService>,
    changes: Arc<StateChanges>,
}

impl Tray {
    pub(crate) fn new(configuration: Arc<ConfigurationService>, sites: Arc<SiteService>) -> Self {
        Self {
            changes: sites.changes(),
            configuration,
            sites,
        }
    }

    pub(crate) fn changes(&self) -> &Arc<StateChanges> {
        &self.changes
    }

    pub(crate) async fn snapshot(&self) -> Result<TrayState, TrayError> {
        let cancellation = self.sites.cancellation();
        tokio::select! {
            biased;
            _ = cancellation.cancelled() => Err(SiteError::Task(TaskError::Closing).into()),
            result = self.read() => result,
        }
    }

    async fn read(&self) -> Result<TrayState, TrayError> {
        let mut changes = self.changes.subscribe();
        let mut retries = 0;
        loop {
            let before = *changes.borrow_and_update();
            if before.writers != 0 {
                changes.changed().await.map_err(|_| TrayError::Changing)?;
                continue;
            }
            let settings = self.configuration.load()?;
            let state = if !settings.show_tray_icon {
                TrayState::Hidden
            } else if self
                .sites
                .list()
                .await?
                .iter()
                .any(|site| site.enabled && site.models.iter().any(|model| model.enabled))
            {
                TrayState::Active
            } else {
                TrayState::Default
            };
            if before == self.changes.version() {
                return Ok(state);
            }
            retries += 1;
            if retries >= 16 {
                return Err(TrayError::Changing);
            }
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/integration/panel/tray_configuration.rs"]
mod tests;
