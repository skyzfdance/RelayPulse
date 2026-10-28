use super::*;

fn context(model: &str) -> NotificationContext<'_> {
    NotificationContext {
        model,
        endpoint: "api.example.com",
        finished_at_ms: 1_792_972_800_000,
    }
}

#[test]
fn automatic_notifications_honor_current_switches_and_permission_without_requesting_access() {
    let temp = tempfile::tempdir().unwrap();
    let (service, _) = service(temp.path());
    let platform = FakePlatform::new(NotificationPermission::Granted);
    service
        .send_rule(FAILED, &context("model"), &platform)
        .unwrap();
    assert!(platform.sent.lock().unwrap().is_empty());
    let mut settings = service.load(&platform).unwrap();
    settings.notifications_enabled = true;
    settings
        .rules
        .iter_mut()
        .find(|rule| rule.id == FAILED)
        .unwrap()
        .status = false;
    service.save(settings.clone(), &platform).unwrap();
    service
        .send_rule(FAILED, &context("model"), &platform)
        .unwrap();
    assert!(platform.sent.lock().unwrap().is_empty());
    settings
        .rules
        .iter_mut()
        .find(|rule| rule.id == FAILED)
        .unwrap()
        .status = true;
    service.save(settings, &platform).unwrap();
    for permission in [
        NotificationPermission::Denied,
        NotificationPermission::NotRequested,
        NotificationPermission::Unknown,
        NotificationPermission::Unsupported,
    ] {
        *platform.permission.lock().unwrap() = permission;
        service
            .send_rule(FAILED, &context("model"), &platform)
            .unwrap();
    }
    assert!(platform.sent.lock().unwrap().is_empty());
    assert_eq!(*platform.requested.lock().unwrap(), 0);
    *platform.permission.lock().unwrap() = NotificationPermission::Granted;
    service
        .send_rule(FAILED, &context("model"), &platform)
        .unwrap();
    service
        .send_rule(RECOVERED, &context("model"), &platform)
        .unwrap();
    let sent = platform.sent.lock().unwrap();
    assert_eq!(sent.len(), 2);
    assert_eq!(sent[0].0, "⚠️ model 探测故障");
    assert_eq!(sent[1].0, "✅ model 已恢复");
    assert_eq!(
        *platform.actions.lock().unwrap(),
        vec![Some(NotificationAction::OpenProbeRecords); 2]
    );
}

#[test]
fn automatic_templates_render_actual_values_once_and_limit_long_unicode_values() {
    use unicode_segmentation::UnicodeSegmentation;
    let temp = tempfile::tempdir().unwrap();
    let (service, _) = service(temp.path());
    let platform = FakePlatform::new(NotificationPermission::Granted);
    let mut settings = service.load(&platform).unwrap();
    settings.notifications_enabled = true;
    service.save(settings, &platform).unwrap();
    service
        .save_template(
            FAILED,
            TemplateUpdate {
                template_type: TemplateMode::Custom,
                marker_id: "none".into(),
                title_template: "{{app}}: {{model}}".into(),
                body_template: "{{endpoint}}\n{{model}}\n{{time}}".into(),
            },
        )
        .unwrap();
    service
        .send_rule(FAILED, &context("{{app}}\n原样"), &platform)
        .unwrap();
    let sent = platform.sent.lock().unwrap();
    assert_eq!(sent[0].0, "RelayPulse: {{app}} 原样");
    assert!(sent[0].1.starts_with("api.example.com\n{{app}} 原样\n"));
    assert!(!sent[0].1.contains("{{time}}"));
    drop(sent);
    service
        .send_rule(FAILED, &context(&"👨‍👩‍👧‍👦".repeat(600)), &platform)
        .unwrap();
    let sent = platform.sent.lock().unwrap();
    assert_eq!(sent[1].0.graphemes(true).count(), 120);
    assert_eq!(sent[1].1.graphemes(true).count(), 500);
    assert!(sent[1].0.ends_with('…'));
    assert!(sent[1].1.ends_with('…'));
}
