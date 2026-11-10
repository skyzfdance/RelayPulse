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
    pub operation_logs: std::sync::Arc<crate::modules::operation_logs::OperationLogs>,
    pub panel: std::sync::Arc<crate::application::snapshots::panel::Panel>,
    pub tray: std::sync::Arc<crate::application::snapshots::tray::Tray>,
    pub iq_test: std::sync::Arc<crate::modules::iq_test::IqTest>,
}

pub(crate) const BASELINE: &[crate::shared::persistence::Migration] = &[
    SCHEMA,
    NOTIFICATIONS_SCHEMA,
    crate::modules::sites::SCHEMA,
    crate::modules::monitoring::SCHEMA,
    crate::modules::operation_logs::SCHEMA,
    crate::modules::iq_test::SCHEMA,
];

pub(crate) fn diagnostics(environment: &EnvironmentContext) -> std::sync::Arc<Diagnostics> {
    std::sync::Arc::new(Diagnostics::new(environment))
}

pub(crate) fn compose(
    environment: EnvironmentContext,
    diagnostics: std::sync::Arc<Diagnostics>,
) -> Result<Application, PersistenceError> {
    if environment
        .paths()
        .data_root()
        .join("conversion-incomplete")
        .exists()
    {
        return Err(PersistenceError::UnsupportedSchema(
            "offline conversion has not passed verification",
        ));
    }
    // SQL fragments form one complete baseline; startup never upgrades an existing format.
    let database = std::sync::Arc::new(Database::open(environment.paths(), BASELINE)?);
    let operation_logs = std::sync::Arc::new(
        crate::modules::operation_logs::OperationLogs::new(database.clone()).with_history_decoder(
            std::sync::Arc::new(crate::modules::monitoring::operation_history::ProbeHistoryDecoder),
        ),
    );
    let changes = std::sync::Arc::new(crate::shared::state_changes::StateChanges::default());
    let configuration = std::sync::Arc::new(
        ConfigurationService::new(SqliteSettingsStore::new(database.clone()))
            .with_diagnostics(diagnostics.clone())
            .with_operation_logs(operation_logs.clone())
            .with_changes(changes.clone()),
    );
    let notifications = std::sync::Arc::new(
        NotificationService::new(
            SqliteNotificationStore::new(database.clone()),
            crate::application::notifications::catalog(),
        )
        .with_diagnostics(diagnostics.clone()),
    );
    let client = std::sync::Arc::new(crate::shared::model_client::ModelClient::new(
        environment.application(),
    ));
    let access = std::sync::Arc::new(crate::modules::sites::SiteAccess::default());
    let site_service = std::sync::Arc::new(
        crate::modules::sites::SiteService::new(database.clone(), client.clone(), access)
            .with_changes(changes),
    );
    let monitoring = std::sync::Arc::new(
        crate::modules::monitoring::Monitoring::new(
            site_service.clone(),
            client.clone(),
            database.clone(),
            configuration.clone(),
            diagnostics.clone(),
            Some(environment.paths().data_root().join("probe-records")),
        )
        .with_operation_logs(operation_logs.clone()),
    );
    let iq_test = std::sync::Arc::new(
        crate::modules::iq_test::IqTest::new(
            site_service.authorize_iq_credentials(),
            client,
            database,
            environment.paths().data_root().join("iq-test"),
            std::sync::Arc::new(
                crate::application::snapshots::iq_test::IqCandidateHealth::new(monitoring.clone()),
            ),
        )
        .with_operation_logs(operation_logs.clone()),
    );
    let panel = std::sync::Arc::new(crate::application::snapshots::panel::Panel::new(
        configuration.clone(),
        site_service.clone(),
        monitoring.clone(),
        diagnostics.clone(),
        operation_logs.clone(),
    ));
    let tray = std::sync::Arc::new(crate::application::snapshots::tray::Tray::new(
        configuration.clone(),
        site_service.clone(),
    ));
    let sites = std::sync::Arc::new(
        crate::application::sites::Sites::new(site_service, monitoring.clone(), diagnostics)
            .with_operation_logs(operation_logs.clone()),
    );
    Ok(Application {
        environment,
        configuration,
        notifications,
        operation_logs,
        sites,
        monitoring,
        panel,
        tray,
        iq_test,
    })
}
