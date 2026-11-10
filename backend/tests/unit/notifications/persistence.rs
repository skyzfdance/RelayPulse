use super::*;
#[test]
fn custom_templates_switches_and_reopening_preserve_configuration() {
    let temp = tempfile::tempdir().unwrap();
    let (service, db) = service(temp.path());
    let platform = FakePlatform::new(NotificationPermission::Denied);
    let mut settings = service.load(&platform).unwrap();
    settings.notifications_enabled = true;
    settings
        .rules
        .iter_mut()
        .find(|r| r.id == FAILED)
        .unwrap()
        .status = false;
    service.save(settings.clone(), &platform).unwrap();
    let detail = service.save_template(FAILED, custom()).unwrap();
    service.save(settings, &platform).unwrap();
    assert_eq!(service.template(FAILED).unwrap(), detail);
    drop(service);
    drop(db);
    let (service, _) = super::service(temp.path());
    let settings = service.load(&platform).unwrap();
    assert!(settings.notifications_enabled);
    assert!(!service.template(FAILED).unwrap().summary.status);
    assert_eq!(service.template(FAILED).unwrap(), detail);
    let mut update = custom();
    update.template_type = TemplateMode::Default;
    service.save_template(FAILED, update).unwrap();
    assert_eq!(
        service.template(FAILED).unwrap().summary.template_type,
        TemplateMode::Default
    );
}
#[test]
fn partial_catalog_updates_preserve_unknown_provider_settings_and_reject_duplicate_ids() {
    let temp = tempfile::tempdir().unwrap();
    let (service, db) = service(temp.path());
    let platform = FakePlatform::new(NotificationPermission::Granted);
    db.with_connection(|c| {
        c.execute(
            "INSERT INTO notification_rules VALUES ('future.event',0,NULL)",
            [],
        )?;
        Ok(())
    })
    .unwrap();
    let mut settings = service.load(&platform).unwrap();
    settings.rules.truncate(1);
    service.save(settings.clone(), &platform).unwrap();
    settings.rules.push(settings.rules[0].clone());
    assert!(service.save(settings, &platform).is_err());
    assert_eq!(
        db.with_connection(|c| c.query_row(
            "SELECT status FROM notification_rules WHERE id='future.event'",
            [],
            |r| r.get::<_, i64>(0)
        ))
        .unwrap(),
        0
    );
}
#[test]
fn malformed_templates_and_storage_corruption_are_not_repaired() {
    let temp = tempfile::tempdir().unwrap();
    let (service, db) = service(temp.path());
    let platform = FakePlatform::new(NotificationPermission::Granted);
    let mut invalid = custom();
    invalid.title_template = "{{unknown}}".into();
    assert!(service.save_template(FAILED, invalid).is_err());
    service.save_template(FAILED, custom()).unwrap();
    let original = service.load(&platform).unwrap();
    db.with_connection(|c|c.execute_batch("PRAGMA ignore_check_constraints=ON; UPDATE notification_rules SET custom_template='broken';")).unwrap();
    assert!(service.check_ready().is_err());
    assert!(service.save(original, &platform).is_err());
    assert!(service.save_template(FAILED, custom()).is_err());
}
#[test]
fn failed_transactions_preserve_all_settings_and_templates() {
    let temp = tempfile::tempdir().unwrap();
    let (service, db) = service(temp.path());
    let platform = FakePlatform::new(NotificationPermission::Granted);
    let original = service.load(&platform).unwrap();
    db.with_connection(|c|c.execute_batch("CREATE TRIGGER reject_rule BEFORE INSERT ON notification_rules BEGIN SELECT RAISE(ABORT,'fixture'); END;")).unwrap();
    let mut update = original.clone();
    update.notifications_enabled = true;
    assert!(service.save(update, &platform).is_err());
    assert_eq!(service.load(&platform).unwrap(), original);
    assert!(service.save_template(FAILED, custom()).is_err());
}
#[test]
fn concurrent_settings_and_templates_preserve_each_others_fields() {
    let temp = tempfile::tempdir().unwrap();
    let (service, _) = service(temp.path());
    let service = Arc::new(service);
    let platform = FakePlatform::new(NotificationPermission::Granted);
    let mut settings = service.load(&platform).unwrap();
    settings.notifications_enabled = true;
    settings
        .rules
        .iter_mut()
        .find(|r| r.id == RECOVERED)
        .unwrap()
        .status = false;
    std::thread::scope(|scope| {
        let a = service.clone();
        scope.spawn(move || a.save(settings, &platform).unwrap());
        let b = service.clone();
        scope.spawn(move || b.save_template(FAILED, custom()).unwrap());
    });
    assert!(!service.template(RECOVERED).unwrap().summary.status);
    assert_eq!(
        service.template(FAILED).unwrap().title_template,
        custom().title_template
    );
}
