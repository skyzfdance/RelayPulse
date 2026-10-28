use crate::{
    application::composition::{self, Application},
    modules::{
        configuration, monitoring, notifications,
        sites::{self, CreateSite, Protocol, SiteStore},
    },
    shared::{
        diagnostics::Diagnostics,
        persistence::{test_support::environment, Database},
    },
};
use std::sync::Arc;

pub(crate) struct Fixture {
    pub directory: tempfile::TempDir,
    pub app: Application,
    pub database: Arc<Database>,
    pub store: SiteStore,
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
        let store = SiteStore::new(database.clone());
        let diagnostics = Arc::new(Diagnostics::new(&env));
        let app = composition::compose(env, diagnostics).unwrap();
        Self {
            directory,
            app,
            database,
            store,
        }
    }
    pub(crate) fn site(&self, url: &str, models: &[&str], enabled: bool) -> String {
        let _write = self.app.panel.changes().begin();
        self.store
            .create(
                &CreateSite {
                    name: "同名站点".into(),
                    protocol: Protocol::CustomHttp,
                    base_url: url.into(),
                    secret: "local-panel-test-secret".into(),
                    enabled,
                },
                &models
                    .iter()
                    .map(|model| model.to_string())
                    .collect::<Vec<_>>(),
            )
            .unwrap()
    }
}
