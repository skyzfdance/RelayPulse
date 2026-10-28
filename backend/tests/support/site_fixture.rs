use super::*;

pub(crate) struct Fixture {
    pub _directory: tempfile::TempDir,
    pub service: Arc<Sites>,
    pub database: Arc<Database>,
    pub diagnostics: Arc<Diagnostics>,
    pub store: SiteStore,
    pub sites: Arc<SiteService>,
    pub monitoring: Arc<Monitoring>,
    pub general: Arc<ConfigurationService>,
}
impl Fixture {
    pub(crate) fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let env = environment(directory.path(), "com.relaypulse.app.dev");
        let database = Arc::new(
            Database::open(
                env.paths(),
                &[
                    configuration::SCHEMA,
                    notifications::SCHEMA,
                    sites::SCHEMA,
                    monitoring::SCHEMA,
                ],
            )
            .unwrap(),
        );
        let diagnostics = Arc::new(Diagnostics::new(&env));
        let general = Arc::new(ConfigurationService::new(SqliteSettingsStore::new(
            database.clone(),
        )));
        let client = Arc::new(ModelClient::default());
        let store = SiteStore::new(database.clone());
        let sites = Arc::new(SiteService::new(
            database.clone(),
            client.clone(),
            Arc::new(SiteAccess::default()),
        ));
        let monitoring = Arc::new(Monitoring::new(
            sites.clone(),
            client,
            database.clone(),
            general.clone(),
            diagnostics.clone(),
            Some(env.paths().data_root().join("probe-records")),
        ));
        let service = Arc::new(Sites::new(
            sites.clone(),
            monitoring.clone(),
            diagnostics.clone(),
        ));
        Self {
            _directory: directory,
            service,
            database,
            diagnostics,
            store,
            sites,
            monitoring,
            general,
        }
    }
    pub(crate) fn site(&self, url: &str, names: &[&str]) -> String {
        self.store
            .create(
                &CreateSite {
                    name: "local".into(),
                    protocol: Protocol::CustomHttp,
                    base_url: url.into(),
                    secret: "secret-only-in-vault".into(),
                    enabled: false,
                },
                &names.iter().map(|n| n.to_string()).collect::<Vec<_>>(),
            )
            .unwrap()
    }
    pub(crate) async fn execute(&self, op: Operation) -> Value {
        serde_json::to_value(self.service.execute(op).await.unwrap()).unwrap()
    }
    pub(super) fn count(&self) -> i64 {
        self.database
            .with_connection(|c| {
                c.query_row("SELECT COUNT(*) FROM probe_records", [], |r| r.get(0))
            })
            .unwrap()
    }
}
