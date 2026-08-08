use tauri::AppHandle;
#[cfg(not(target_os = "macos"))]
use tauri_plugin_notification::{NotificationExt, PermissionState};

use super::{
    DispatchFailure, NotificationChannelSnapshot, NotificationDelivery, NotificationError,
    NotificationPermission, SYSTEM_CHANNEL_ID,
};

const TEST_TITLE: &str = "RelayPulse 测试通知";
const TEST_BODY: &str = "系统通知渠道已成功接受这条测试消息。";

pub fn system_channel_snapshot(app: &AppHandle) -> NotificationChannelSnapshot {
    let supported = cfg!(any(target_os = "macos", windows, target_os = "linux"));
    let permission = if !supported {
        NotificationPermission::Unsupported
    } else {
        system_permission(app)
    };
    NotificationChannelSnapshot {
        id: SYSTEM_CHANNEL_ID.to_owned(),
        display_name: "系统通知".to_owned(),
        description: "通过当前操作系统显示 RelayPulse 通知".to_owned(),
        permission,
        supported,
    }
}

pub fn request_system_notification_permission(
    app: &AppHandle,
    channel_id: &str,
) -> Result<NotificationChannelSnapshot, NotificationError> {
    if channel_id != SYSTEM_CHANNEL_ID {
        return Err(NotificationError::ChannelNotFound);
    }

    let channel = system_channel_snapshot(app);
    if !channel.supported {
        return Err(NotificationError::ChannelUnsupported);
    }
    if channel.permission == NotificationPermission::NotRequested {
        request_permission(app)?;
    }

    Ok(system_channel_snapshot(app))
}

pub fn send_test_system_notification(
    app: &AppHandle,
    channel_id: &str,
) -> Result<(), NotificationError> {
    send_system_notification(app, channel_id, TEST_TITLE, TEST_BODY)
}

pub(crate) fn send_system_notification(
    app: &AppHandle,
    channel_id: &str,
    title: &str,
    body: &str,
) -> Result<(), NotificationError> {
    if channel_id != SYSTEM_CHANNEL_ID {
        return Err(NotificationError::ChannelNotFound);
    }
    let channel = system_channel_snapshot(app);
    if !channel.supported {
        return Err(NotificationError::ChannelUnsupported);
    }
    match channel.permission {
        NotificationPermission::Unknown => return Err(NotificationError::PermissionUnknown),
        NotificationPermission::NotRequested => {
            return Err(NotificationError::PermissionNotRequested)
        }
        NotificationPermission::Denied => return Err(NotificationError::PermissionDenied),
        NotificationPermission::Unsupported => return Err(NotificationError::ChannelUnsupported),
        NotificationPermission::Granted => {}
    }
    show(app, title, body).map_err(|error| {
        if error.retryable {
            NotificationError::DispatchRetryable
        } else {
            NotificationError::DispatchFailed
        }
    })
}

pub(crate) fn system_dispatch(
    app: &AppHandle,
    delivery: &NotificationDelivery,
) -> Result<(), DispatchFailure> {
    if delivery.channel_id != SYSTEM_CHANNEL_ID {
        return Err(DispatchFailure {
            code: "notification_channel_not_found",
            retryable: false,
        });
    }
    let channel = system_channel_snapshot(app);
    match channel.permission {
        NotificationPermission::Granted if channel.supported => {
            show(app, &delivery.title, &delivery.body)
        }
        NotificationPermission::Unknown => Err(DispatchFailure {
            code: "notification_permission_unknown",
            retryable: true,
        }),
        NotificationPermission::NotRequested => Err(DispatchFailure {
            code: "notification_permission_not_requested",
            retryable: false,
        }),
        NotificationPermission::Denied => Err(DispatchFailure {
            code: "notification_permission_denied",
            retryable: false,
        }),
        NotificationPermission::Unsupported | NotificationPermission::Granted => {
            Err(DispatchFailure {
                code: "notification_channel_unsupported",
                retryable: false,
            })
        }
    }
}

fn show(_app: &AppHandle, title: &str, body: &str) -> Result<(), DispatchFailure> {
    #[cfg(target_os = "macos")]
    {
        show_macos(title, body)
    }

    #[cfg(not(target_os = "macos"))]
    {
        _app.notification()
            .builder()
            .title(title)
            .body(body)
            .show()
            .map_err(|_| DispatchFailure {
                code: "notification_dispatch_retryable",
                retryable: true,
            })
    }
}

#[cfg(target_os = "macos")]
fn show_macos(title: &str, body: &str) -> Result<(), DispatchFailure> {
    mac_usernotifications::Notification::new()
        .title(title)
        .message(body)
        .send_blocking()
        .map(|_| ())
        .map_err(|_| DispatchFailure {
            code: "notification_dispatch_retryable",
            retryable: true,
        })
}

#[cfg(target_os = "macos")]
fn system_permission(_app: &AppHandle) -> NotificationPermission {
    use mac_usernotifications::{blocking, AuthorizationStatus};

    match blocking::get_notification_settings().map(|settings| settings.authorization_status) {
        Ok(AuthorizationStatus::NotDetermined) => NotificationPermission::NotRequested,
        Ok(AuthorizationStatus::Denied) => NotificationPermission::Denied,
        Ok(
            AuthorizationStatus::Authorized
            | AuthorizationStatus::Provisional
            | AuthorizationStatus::Ephemeral,
        ) => NotificationPermission::Granted,
        Ok(AuthorizationStatus::Unknown) | Err(_) => NotificationPermission::Unknown,
    }
}

#[cfg(not(target_os = "macos"))]
fn system_permission(app: &AppHandle) -> NotificationPermission {
    match app.notification().permission_state() {
        Ok(PermissionState::Granted) => NotificationPermission::Granted,
        Ok(PermissionState::Denied) => NotificationPermission::Denied,
        Ok(PermissionState::Prompt | PermissionState::PromptWithRationale) => {
            NotificationPermission::NotRequested
        }
        Err(_) => NotificationPermission::Unknown,
    }
}

#[cfg(target_os = "macos")]
fn request_permission(_app: &AppHandle) -> Result<(), NotificationError> {
    mac_usernotifications::blocking::request_auth()
        .map(|_| ())
        .map_err(|_| NotificationError::PermissionUnknown)
}

#[cfg(not(target_os = "macos"))]
fn request_permission(app: &AppHandle) -> Result<(), NotificationError> {
    app.notification()
        .request_permission()
        .map(|_| ())
        .map_err(|_| NotificationError::PermissionUnknown)
}
