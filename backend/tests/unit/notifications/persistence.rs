use super::*;

#[test]
fn extends_configuration_preserves_settings_and_reopens_notifications() {
    let temp = tempfile::tempdir().unwrap();
    let env = environment(temp.path(), DEV);
    let old_database = Database::open(env.paths(), &[configuration::SCHEMA]).unwrap();
    let configuration = configuration::ConfigurationService::new(
        configuration::SqliteSettingsStore::new(old_database),
    );
    let mut old = configuration.load().unwrap();
    old.history_retention_days = 77;
    old.theme = configuration::ThemeMode::Dark;
    configuration.save(old.clone()).unwrap();
    drop(configuration);
    let (service, database) = service(temp.path());
    let platform = FakePlatform::new(NotificationPermission::Denied);
    let original = service.load(&platform).unwrap();
    assert!(!original.notifications_enabled);
    assert!(original
        .rules
        .iter()
        .all(|r| r.status && r.template_type == TemplateMode::Default));
    let mut input = original;
    input.notifications_enabled = true;
    input.rules[0].status = false;
    input.authorization = NotificationPermission::Granted;
    for rule in &mut input.rules {
        rule.title = "伪造".into();
        rule.sub_title = "伪造".into();
        rule.channel = "伪造".into();
        rule.template_type = TemplateMode::Custom;
    }
    let saved = service.save(input, &platform).unwrap();
    assert_eq!(saved.authorization, NotificationPermission::Denied);
    assert!(saved
        .rules
        .iter()
        .all(|r| r.channel == "系统通知" && r.template_type == TemplateMode::Default));
    let detail = service.save_template(FAILED, custom()).unwrap();
    assert_eq!(detail.title_template, custom().title_template);
    assert_eq!(
        service.load(&platform).unwrap().rules[0].template_type,
        TemplateMode::Custom
    );
    // 旧页面列表保存不能覆盖刚保存的模板。
    service.save(saved, &platform).unwrap();
    assert_eq!(service.template(FAILED).unwrap(), detail);
    let general = configuration::ConfigurationService::new(
        configuration::SqliteSettingsStore::new(database.clone()),
    );
    assert_eq!(general.load().unwrap(), old);
    drop(general);
    drop(service);
    drop(database);
    let (reopened, _) = super::service(temp.path());
    assert!(reopened.load(&platform).unwrap().notifications_enabled);
    assert!(!reopened.load(&platform).unwrap().rules[0].status);
    assert_eq!(reopened.template(FAILED).unwrap(), detail);
    let mut defaults = custom();
    defaults.template_type = TemplateMode::Default;
    defaults.marker_id = "forged".into();
    defaults.title_template = "forged".into();
    let restored = reopened.save_template(FAILED, defaults).unwrap();
    assert_eq!(restored.title_template, restored.default_title_template);
    assert_eq!(restored.marker_id, restored.default_marker_id);
    assert_eq!(restored.summary.template_type, TemplateMode::Default);
}

#[test]
fn rejects_bad_rules_templates_and_corruption_without_repairing() {
    let temp = tempfile::tempdir().unwrap();
    let (service, database) = service(temp.path());
    let platform = FakePlatform::new(NotificationPermission::Granted);
    let original = service.load(&platform).unwrap();
    for kind in 0..4 {
        let mut input = original.clone();
        input.notifications_enabled = true;
        match kind {
            0 => {
                input.rules.pop();
            }
            1 => input.rules[0].id = "unknown".into(),
            2 => input.rules[1].id = input.rules[0].id.clone(),
            _ => input.rules.push(input.rules[0].clone()),
        }
        assert!(service.save(input, &platform).is_err());
        assert_eq!(service.load(&platform).unwrap(), original);
    }
    assert!(matches!(
        service.template("unknown"),
        Err(NotificationError::RuleNotFound)
    ));
    let mut invalid = custom();
    invalid.title_template = "{{secret}}".into();
    assert!(service.save_template(FAILED, invalid).is_err());
    assert_eq!(
        service.template(FAILED).unwrap().summary.template_type,
        TemplateMode::Default
    );
    database.with_connection(|c| c.execute_batch("PRAGMA ignore_check_constraints=ON; UPDATE notification_rules SET custom_template='broken' WHERE id='probe.model_failed';")).unwrap();
    assert!(service.check_ready().is_err());
    assert!(service.save(original, &platform).is_err());
    assert!(service.save_template(FAILED, custom()).is_err());
    database
        .with_connection(|c| {
            assert_eq!(
                c.query_row(
                    "SELECT custom_template FROM notification_rules WHERE id='probe.model_failed'",
                    [],
                    |r| r.get::<_, String>(0)
                )?,
                "broken"
            );
            Ok(())
        })
        .unwrap();
}

#[test]
fn failed_readback_rolls_back_settings_and_template_transactions() {
    let temp = tempfile::tempdir().unwrap();
    let (service, database) = service(temp.path());
    let platform = FakePlatform::new(NotificationPermission::Granted);
    let original = service.load(&platform).unwrap();
    database.with_connection(|c| c.execute_batch("CREATE TRIGGER break_rules AFTER UPDATE ON notification_rules BEGIN DELETE FROM notification_rules WHERE id='probe.model_recovered'; END;")).unwrap();
    let mut update = original.clone();
    update.notifications_enabled = true;
    update.rules[0].status = false;
    assert!(service.save(update, &platform).is_err());
    assert_eq!(service.load(&platform).unwrap(), original);
    assert!(service.save_template(FAILED, custom()).is_err());
    assert_eq!(
        service.template(FAILED).unwrap().summary.template_type,
        TemplateMode::Default
    );
    assert!(service.template(RECOVERED).is_ok());
}

#[test]
fn concurrent_settings_and_template_updates_preserve_each_others_fields() {
    let temp = tempfile::tempdir().unwrap();
    let (service, _) = service(temp.path());
    let service = Arc::new(service);
    let platform = FakePlatform::new(NotificationPermission::Granted);
    let mut input = service.load(&platform).unwrap();
    input.notifications_enabled = true;
    input.rules[1].status = false;
    std::thread::scope(|scope| {
        let settings_service = service.clone();
        scope.spawn(move || settings_service.save(input, &platform).unwrap());
        let service = service.clone();
        scope.spawn(move || service.save_template(FAILED, custom()).unwrap());
    });
    assert!(
        service
            .load(&FakePlatform::new(NotificationPermission::Granted))
            .unwrap()
            .notifications_enabled
    );
    assert!(!service.template(RECOVERED).unwrap().summary.status);
    assert_eq!(
        service.template(FAILED).unwrap().title_template,
        custom().title_template
    );
}

#[test]
fn failed_notification_schema_extension_preserves_configuration_and_history() {
    let temp = tempfile::tempdir().unwrap();
    let env = environment(temp.path(), DEV);
    let database = Database::open(env.paths(), &[configuration::SCHEMA]).unwrap();
    database.with_connection(|c| c.execute_batch("UPDATE general_settings SET history_retention_days=123; CREATE TABLE notification_rules (existing TEXT); INSERT INTO notification_rules VALUES ('preserve');")).unwrap();
    drop(database);
    assert!(Database::open(env.paths(), &[configuration::SCHEMA, SCHEMA]).is_err());
    let database = Database::open(env.paths(), &[configuration::SCHEMA]).unwrap();
    database
        .with_connection(|c| {
            assert_eq!(
                c.query_row("SELECT COUNT(*) FROM schema_migrations", [], |r| r
                    .get::<_, i64>(0))?,
                1
            );
            assert_eq!(
                c.query_row(
                    "SELECT COUNT(*) FROM sqlite_schema WHERE name='notification_settings'",
                    [],
                    |r| r.get::<_, i64>(0)
                )?,
                0
            );
            assert_eq!(
                c.query_row(
                    "SELECT history_retention_days FROM general_settings",
                    [],
                    |r| r.get::<_, i64>(0)
                )?,
                123
            );
            assert_eq!(
                c.query_row("SELECT existing FROM notification_rules", [], |r| r
                    .get::<_, String>(0))?,
                "preserve"
            );
            Ok(())
        })
        .unwrap();
}

#[test]
fn missing_or_invalid_notifications_block_startup_without_repairing() {
    use crate::startup::{initialize, StartupError, StartupState};
    for sql in [
        "DELETE FROM notification_settings",
        "DELETE FROM notification_rules WHERE id='probe.model_failed'",
        "PRAGMA ignore_check_constraints=ON; UPDATE notification_settings SET enabled=2",
        "PRAGMA ignore_check_constraints=ON; UPDATE notification_rules SET status=2",
        "PRAGMA ignore_check_constraints=ON; UPDATE notification_rules SET id='unknown' WHERE id='probe.model_failed'",
        "UPDATE notification_rules SET custom_template='{}' WHERE id='probe.model_failed'",
    ] {
        let temp = tempfile::tempdir().unwrap();
        let (service, database) = service(temp.path());
        database.with_connection(|c| c.execute_batch(sql)).unwrap();
        drop(service); drop(database);
        assert!(matches!(initialize(Ok(environment(temp.path(), DEV))).0, StartupState::Blocked(StartupError::Notifications(_))), "{sql}");
        // 再次打开仍是相同错误，启动没有用默认值回填或修复。
        assert!(matches!(initialize(Ok(environment(temp.path(), DEV))).0, StartupState::Blocked(StartupError::Notifications(_))));
    }
}

#[test]
fn notification_environments_remain_isolated_in_temporary_directories() {
    let temp = tempfile::tempdir().unwrap();
    let platform = FakePlatform::new(NotificationPermission::Granted);
    let (dev, _) = service(temp.path());
    let env = environment(temp.path(), "com.relaypulse.app");
    let database = Arc::new(Database::open(env.paths(), &[configuration::SCHEMA, SCHEMA]).unwrap());
    let other = NotificationService::new(SqliteNotificationStore::new(database));
    let mut settings = dev.load(&platform).unwrap();
    settings.notifications_enabled = true;
    dev.save(settings, &platform).unwrap();
    dev.save_template(FAILED, custom()).unwrap();
    assert!(!other.load(&platform).unwrap().notifications_enabled);
    assert_eq!(
        other.template(FAILED).unwrap().summary.template_type,
        TemplateMode::Default
    );
}
