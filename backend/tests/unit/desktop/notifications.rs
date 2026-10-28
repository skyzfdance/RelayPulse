use super::*;
#[test]
fn maps_all_native_authorization_states_and_preserves_failures() {
    use mac_usernotifications::AuthorizationStatus as Native;
    for (input, expected) in [
        (Native::NotDetermined, NotificationPermission::NotRequested),
        (Native::Denied, NotificationPermission::Denied),
        (Native::Authorized, NotificationPermission::Granted),
        (Native::Provisional, NotificationPermission::Granted),
        (Native::Ephemeral, NotificationPermission::Granted),
        (Native::Unknown, NotificationPermission::Unknown),
    ] {
        assert_eq!(permission(input), expected);
    }
    assert!(matches!(
        native_error(
            "permission_query",
            mac_usernotifications::Error::NoBundleIdentifier
        ),
        NotificationError::ApplicationIdentity
    ));
    assert!(matches!(
        native_error("send", mac_usernotifications::Error::NotificationRejected),
        NotificationError::Platform("send")
    ));
}

fn response(
    action: &str,
    close_reason: Option<mac_usernotifications::CloseReason>,
) -> mac_usernotifications::NotificationResponse {
    mac_usernotifications::NotificationResponse {
        notification_id: "probe-notification".into(),
        action_identifier: action.into(),
        reply_text: None,
        close_reason,
    }
}

#[tokio::test]
async fn only_clicking_the_notification_body_opens_probe_records() {
    use mac_usernotifications::CloseReason;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    };
    let opened = Arc::new(AtomicUsize::new(0));
    let counter = opened.clone();
    let platform = SystemNotifications {
        open_records: Arc::new(move || {
            counter.fetch_add(1, Ordering::SeqCst);
        }),
        responses: Mutex::new(Some(tokio::task::JoinSet::new())),
    };
    for response in [
        response("com.apple.UNNotificationDefaultActionIdentifier", None),
        response(
            "com.apple.UNNotificationDismissActionIdentifier",
            Some(CloseReason::Dismissed),
        ),
        response("", Some(CloseReason::Expired)),
        response("unrecognized-action", None),
    ] {
        platform.watch_response(std::future::ready(Ok(response)));
    }
    let mut tasks = platform.responses.lock().unwrap().take().unwrap();
    while let Some(result) = tasks.join_next().await {
        result.unwrap();
    }
    assert_eq!(opened.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn exit_cancels_pending_responses_without_waiting_for_a_click() {
    use std::sync::{Arc, Mutex};
    let platform = SystemNotifications {
        open_records: Arc::new(|| panic!("退出后不能打开窗口")),
        responses: Mutex::new(Some(tokio::task::JoinSet::new())),
    };
    let (send, receive) = tokio::sync::oneshot::channel();
    platform.watch_response(async move { receive.await.unwrap() });
    tokio::time::timeout(std::time::Duration::from_secs(1), platform.stop())
        .await
        .unwrap();
    assert!(send
        .send(Ok(response(
            "com.apple.UNNotificationDefaultActionIdentifier",
            None
        )))
        .is_err());
    assert!(platform.responses.lock().unwrap().is_none());
    platform.watch_response(std::future::ready(Ok(response(
        "com.apple.UNNotificationDefaultActionIdentifier",
        None,
    ))));
    platform.stop().await;
}
