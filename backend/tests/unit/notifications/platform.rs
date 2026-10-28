use super::*;

fn sample() -> TestNotification {
    TestNotification {
        title: "⚠️ {{already_rendered}}".into(),
        body: "完成的正文\n第二行".into(),
        marker_id: Some("warning".into()),
    }
}

#[test]
fn test_notification_requires_enabled_and_real_permission_and_is_sent_unchanged() {
    let temp = tempfile::tempdir().unwrap();
    let (service, _) = service(temp.path());
    let platform = FakePlatform::new(NotificationPermission::Granted);
    assert!(matches!(
        service.send_test(sample(), &platform),
        Err(NotificationError::Disabled)
    ));
    let mut settings = service.load(&platform).unwrap();
    settings.notifications_enabled = true;
    service.save(settings, &platform).unwrap();
    for permission in [
        NotificationPermission::Unknown,
        NotificationPermission::NotRequested,
        NotificationPermission::Denied,
        NotificationPermission::Unsupported,
    ] {
        *platform.permission.lock().unwrap() = permission;
        assert!(
            matches!(service.send_test(sample(), &platform), Err(NotificationError::Permission(p)) if p == permission)
        );
        assert_eq!(service.request_permission(&platform).unwrap(), permission);
    }
    assert!(platform.sent.lock().unwrap().is_empty());
    *platform.permission.lock().unwrap() = NotificationPermission::Granted;
    service.send_test(sample(), &platform).unwrap();
    assert_eq!(
        *platform.sent.lock().unwrap(),
        vec![(sample().title, sample().body)]
    );
    assert_eq!(*platform.actions.lock().unwrap(), vec![None]);
}

#[test]
fn platform_failures_propagate_and_query_failure_does_not_persist_a_save() {
    let temp = tempfile::tempdir().unwrap();
    let (service, _) = service(temp.path());
    let platform = FakePlatform::new(NotificationPermission::Granted);
    let mut settings = service.load(&platform).unwrap();
    settings.notifications_enabled = true;
    service.save(settings.clone(), &platform).unwrap();
    for operation in [
        "permission_query",
        "permission_request",
        "send",
        "open_settings",
    ] {
        *platform.fail.lock().unwrap() = Some(operation);
        let result = match operation {
            "permission_query" => {
                settings.notifications_enabled = false;
                service.save(settings.clone(), &platform).map(|_| ())
            }
            "permission_request" => service.request_permission(&platform).map(|_| ()),
            "send" => service.send_test(sample(), &platform),
            _ => service.open_settings(&platform),
        };
        assert!(matches!(result, Err(NotificationError::Platform(value)) if value == operation));
    }
    *platform.fail.lock().unwrap() = None;
    assert!(service.load(&platform).unwrap().notifications_enabled);
    assert!(platform.sent.lock().unwrap().is_empty());
}
