//! 创建资源并连接依赖；不读取设置作业务决策，也不启动后台任务。
use crate::{
    modules::{
        configuration::{ConfigurationService, SqliteSettingsStore, SCHEMA},
        notifications::{
            NotificationService, SqliteNotificationStore, SCHEMA as NOTIFICATIONS_SCHEMA,
        },
    },
    shared::{
        diagnostics::Diagnostics,
        environment::EnvironmentContext,
        persistence::{Database, PersistenceError},
    },
};

pub(crate) struct Application {
    pub environment: EnvironmentContext,
    pub configuration: std::sync::Arc<ConfigurationService>,
    pub sites: std::sync::Arc<crate::application::sites::Sites>,
    pub monitoring: std::sync::Arc<crate::modules::monitoring::Monitoring>,
    pub notifications: std::sync::Arc<NotificationService>,
    pub panel: std::sync::Arc<crate::application::snapshots::panel::Panel>,
    pub tray: std::sync::Arc<crate::application::snapshots::tray::Tray>,
}

pub(crate) fn diagnostics(environment: &EnvironmentContext) -> std::sync::Arc<Diagnostics> {
    std::sync::Arc::new(Diagnostics::new(environment))
}

pub(crate) fn compose(
    environment: EnvironmentContext,
    diagnostics: std::sync::Arc<Diagnostics>,
) -> Result<Application, PersistenceError> {
    // SQL 归业务模块，按历史顺序追加具名 schema；各业务存储共享同一连接和锁。
    let database = std::sync::Arc::new(Database::open(
        environment.paths(),
        &[
            SCHEMA,
            NOTIFICATIONS_SCHEMA,
            crate::modules::sites::SCHEMA,
            crate::modules::monitoring::SCHEMA,
        ],
    )?);
    let changes = std::sync::Arc::new(crate::shared::state_changes::StateChanges::default());
    let configuration = std::sync::Arc::new(
        ConfigurationService::new(SqliteSettingsStore::new(database.clone()))
            .with_diagnostics(diagnostics.clone())
            .with_changes(changes.clone()),
    );
    let notifications = std::sync::Arc::new(
        NotificationService::new(SqliteNotificationStore::new(database.clone()))
            .with_diagnostics(diagnostics.clone()),
    );
    let client = std::sync::Arc::new(crate::shared::model_client::ModelClient::default());
    let access = std::sync::Arc::new(crate::modules::sites::SiteAccess::default());
    let site_service = std::sync::Arc::new(
        crate::modules::sites::SiteService::new(database.clone(), client.clone(), access)
            .with_changes(changes),
    );
    let monitoring = std::sync::Arc::new(crate::modules::monitoring::Monitoring::new(
        site_service.clone(),
        client,
        database,
        configuration.clone(),
        diagnostics.clone(),
        Some(environment.paths().data_root().join("probe-records")),
    ));
    let panel = std::sync::Arc::new(crate::application::snapshots::panel::Panel::new(
        configuration.clone(),
        site_service.clone(),
        monitoring.clone(),
        diagnostics.clone(),
    ));
    let tray = std::sync::Arc::new(crate::application::snapshots::tray::Tray::new(
        configuration.clone(),
        site_service.clone(),
    ));
    let sites = std::sync::Arc::new(crate::application::sites::Sites::new(
        site_service,
        monitoring.clone(),
        diagnostics,
    ));
    Ok(Application {
        environment,
        configuration,
        notifications,
        sites,
        monitoring,
        panel,
        tray,
    })
}
