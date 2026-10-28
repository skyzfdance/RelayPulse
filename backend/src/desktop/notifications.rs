//! 宿主平台连接：macOS 使用原生 API，不能使用桌面插件恒定的 Granted 返回值。
use crate::modules::notifications::{
    NotificationAction, NotificationError, NotificationPermission, NotificationPlatform,
};

pub(super) struct SystemNotifications {
    #[cfg(target_os = "macos")]
    open_records: std::sync::Arc<dyn Fn() + Send + Sync>,
    #[cfg(target_os = "macos")]
    responses: std::sync::Mutex<Option<tokio::task::JoinSet<()>>>,
}

impl SystemNotifications {
    pub(super) fn new<R: tauri::Runtime>(_app: tauri::AppHandle<R>) -> Self {
        Self {
            #[cfg(target_os = "macos")]
            open_records: std::sync::Arc::new(move || {
                let app = _app.clone();
                if _app
                    .run_on_main_thread(move || {
                        if super::main_window::records(&app).is_err() {
                            eprintln!("[notifications] open probe records failed");
                        }
                    })
                    .is_err()
                {
                    eprintln!("[notifications] dispatch click action failed");
                }
            }),
            #[cfg(target_os = "macos")]
            responses: std::sync::Mutex::new(Some(tokio::task::JoinSet::new())),
        }
    }

    #[cfg(target_os = "macos")]
    fn watch_response(
        &self,
        response: impl std::future::Future<
                Output = Result<
                    mac_usernotifications::NotificationResponse,
                    mac_usernotifications::Error,
                >,
            > + Send
            + 'static,
    ) {
        let mut responses = self
            .responses
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let Some(responses) = responses.as_mut() else {
            return;
        };
        while responses.try_join_next().is_some() {}
        let open_records = self.open_records.clone();
        responses.spawn_on(
            async move {
                match response.await {
                    Ok(response)
                        if response.is_default_action() && response.close_reason.is_none() =>
                    {
                        open_records();
                    }
                    Ok(_) => {}
                    Err(_) => eprintln!("[notifications] receive click response failed"),
                }
            },
            tauri::async_runtime::handle().inner(),
        );
    }

    pub(super) async fn stop(&self) {
        #[cfg(target_os = "macos")]
        {
            let responses = self
                .responses
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .take();
            if let Some(mut responses) = responses {
                responses.shutdown().await;
            }
        }
    }
}

#[cfg(target_os = "macos")]
fn native_error(operation: &'static str, error: mac_usernotifications::Error) -> NotificationError {
    match error {
        mac_usernotifications::Error::NoBundleIdentifier => NotificationError::ApplicationIdentity,
        _ => NotificationError::Platform(operation),
    }
}

#[cfg(target_os = "macos")]
fn permission(status: mac_usernotifications::AuthorizationStatus) -> NotificationPermission {
    use mac_usernotifications::AuthorizationStatus::*;
    match status {
        NotDetermined => NotificationPermission::NotRequested,
        Denied => NotificationPermission::Denied,
        Authorized | Provisional | Ephemeral => NotificationPermission::Granted,
        Unknown => NotificationPermission::Unknown,
    }
}

#[cfg(target_os = "macos")]
impl NotificationPlatform for SystemNotifications {
    fn permission(&self) -> Result<NotificationPermission, NotificationError> {
        mac_usernotifications::blocking::get_notification_settings()
            .map(|settings| permission(settings.authorization_status))
            .map_err(|error| native_error("permission_query", error))
    }

    fn request_permission(&self) -> Result<NotificationPermission, NotificationError> {
        // 已决定的授权由系统设置维护；未知／首次授权才请求，结果重新向系统读取。
        match self.permission()? {
            NotificationPermission::NotRequested | NotificationPermission::Unknown => {
                mac_usernotifications::blocking::request_auth()
                    .map_err(|error| native_error("permission_request", error))?;
                self.permission()
            }
            permission => Ok(permission),
        }
    }

    fn send(
        &self,
        title: &str,
        body: &str,
        action: Option<NotificationAction>,
    ) -> Result<(), NotificationError> {
        let notification = mac_usernotifications::Notification::new()
            .title(title)
            .message(body);
        match action {
            None => mac_usernotifications::blocking::send(notification)
                .map(|_| ())
                .map_err(|error| native_error("send", error)),
            Some(NotificationAction::OpenProbeRecords) => {
                // 只等待系统接受请求；交互异步等待，不占用探测 worker 或主线程。
                // Tauri 驱动主线程 RunLoop，不使用库对 is_waiting 的瞬时检查。
                let handle = mac_usernotifications::block_on(notification.send())
                    .map_err(|error| native_error("send", error))?;
                self.watch_response(handle.response());
                Ok(())
            }
        }
    }

    fn open_settings(&self) -> Result<(), NotificationError> {
        // 固定系统 URL，不拼接客户端输入，不通过 shell 执行。
        let status = std::process::Command::new("/usr/bin/open")
            .arg("x-apple.systempreferences:com.apple.Notifications-Settings.extension")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map_err(|_| NotificationError::Platform("open_settings"))?;
        if status.success() {
            Ok(())
        } else {
            Err(NotificationError::Platform("open_settings"))
        }
    }
}

#[cfg(not(target_os = "macos"))]
impl NotificationPlatform for SystemNotifications {
    fn permission(&self) -> Result<NotificationPermission, NotificationError> {
        Ok(NotificationPermission::Unsupported)
    }
    fn request_permission(&self) -> Result<NotificationPermission, NotificationError> {
        Ok(NotificationPermission::Unsupported)
    }
    fn send(
        &self,
        _: &str,
        _: &str,
        _: Option<NotificationAction>,
    ) -> Result<(), NotificationError> {
        Err(NotificationError::Permission(
            NotificationPermission::Unsupported,
        ))
    }
    fn open_settings(&self) -> Result<(), NotificationError> {
        Err(NotificationError::Permission(
            NotificationPermission::Unsupported,
        ))
    }
}

#[cfg(all(test, target_os = "macos"))]
#[path = "../../tests/unit/desktop/notifications.rs"]
mod tests;
