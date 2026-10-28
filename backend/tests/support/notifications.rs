use super::*;

pub(super) const DEV: &str = "com.relaypulse.app.dev";
pub(super) const FAILED: &str = "probe.model_failed";
pub(super) const RECOVERED: &str = "probe.model_recovered";

pub(super) fn service(root: &std::path::Path) -> (NotificationService, Arc<Database>) {
    let database = Arc::new(
        Database::open(
            environment(root, DEV).paths(),
            &[configuration::SCHEMA, SCHEMA],
        )
        .unwrap(),
    );
    (
        NotificationService::new(SqliteNotificationStore::new(database.clone())),
        database,
    )
}

pub(super) fn custom() -> TemplateUpdate {
    TemplateUpdate {
        template_type: TemplateMode::Custom,
        marker_id: "warning".into(),
        title_template: "{{model}} 🚀".into(),
        body_template: "{{app}}\n{{endpoint}}\r\n{{time}}".into(),
    }
}

pub(crate) struct FakePlatform {
    pub permission: Mutex<NotificationPermission>,
    pub fail: Mutex<Option<&'static str>>,
    pub sent: Mutex<Vec<(String, String)>>,
    pub actions: Mutex<Vec<Option<NotificationAction>>>,
    pub requested: Mutex<usize>,
    pub opened: Mutex<usize>,
}
impl FakePlatform {
    pub fn new(permission: NotificationPermission) -> Self {
        Self {
            permission: Mutex::new(permission),
            fail: Mutex::new(None),
            sent: Mutex::new(vec![]),
            actions: Mutex::new(vec![]),
            requested: Mutex::new(0),
            opened: Mutex::new(0),
        }
    }
    fn check(&self, operation: &'static str) -> Result<(), NotificationError> {
        if *self.fail.lock().unwrap() == Some(operation) {
            Err(NotificationError::Platform(operation))
        } else {
            Ok(())
        }
    }
}
impl NotificationPlatform for FakePlatform {
    fn permission(&self) -> Result<NotificationPermission, NotificationError> {
        self.check("permission_query")?;
        Ok(*self.permission.lock().unwrap())
    }
    fn request_permission(&self) -> Result<NotificationPermission, NotificationError> {
        *self.requested.lock().unwrap() += 1;
        self.check("permission_request")?;
        self.permission()
    }
    fn send(
        &self,
        title: &str,
        body: &str,
        action: Option<NotificationAction>,
    ) -> Result<(), NotificationError> {
        self.check("send")?;
        self.sent.lock().unwrap().push((title.into(), body.into()));
        self.actions.lock().unwrap().push(action);
        Ok(())
    }
    fn open_settings(&self) -> Result<(), NotificationError> {
        self.check("open_settings")?;
        *self.opened.lock().unwrap() += 1;
        Ok(())
    }
}
