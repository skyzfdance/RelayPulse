use std::{panic::AssertUnwindSafe, time::Duration};

use futures_util::FutureExt;
use tauri::AppHandle;

use crate::diagnostics::Diagnostics;

use super::{system_dispatch, NotificationService};

const DISPATCH_INTERVAL: Duration = Duration::from_secs(2);
const CLEANUP_INTERVAL: Duration = Duration::from_secs(60 * 60);
const MAX_BATCH_SIZE: usize = 16;

pub fn start_dispatcher(
    app: AppHandle,
    service: NotificationService,
    diagnostics: Option<Diagnostics>,
) {
    let cleanup_service = service.clone();
    let cleanup_diagnostics = diagnostics.clone();
    tauri::async_runtime::spawn(async move {
        let task = async {
            let mut interval = tokio::time::interval(CLEANUP_INTERVAL);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                interval.tick().await;
                if cleanup_service
                    .cleanup_terminal_notifications()
                    .await
                    .is_err()
                {
                    if let Some(diagnostics) = &cleanup_diagnostics {
                        diagnostics.record_background_error(
                            "notification-cleanup",
                            "notification_settings_unavailable",
                        );
                    }
                }
            }
        };
        if AssertUnwindSafe(task).catch_unwind().await.is_err() {
            if let Some(diagnostics) = &cleanup_diagnostics {
                diagnostics.record_background_error(
                    "notification-cleanup",
                    "notification_cleanup_panicked",
                );
            }
        }
    });

    tauri::async_runtime::spawn(async move {
        let task = async {
            let mut interval = tokio::time::interval(DISPATCH_INTERVAL);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                interval.tick().await;
                for _ in 0..MAX_BATCH_SIZE {
                    match service
                        .dispatch_next(|delivery| system_dispatch(&app, delivery))
                        .await
                    {
                        Ok(Some(report)) => {
                            if let Some(diagnostics) = &diagnostics {
                                diagnostics.record_notification_dispatch(&report);
                            }
                        }
                        Ok(None) => break,
                        Err(_) => {
                            if let Some(diagnostics) = &diagnostics {
                                diagnostics.record_background_error(
                                    "notification-dispatcher",
                                    "notification_settings_unavailable",
                                );
                            }
                            break;
                        }
                    }
                }
            }
        };
        if AssertUnwindSafe(task).catch_unwind().await.is_err() {
            if let Some(diagnostics) = &diagnostics {
                diagnostics.record_background_error(
                    "notification-dispatcher",
                    "notification_dispatcher_panicked",
                );
            }
        }
    });
}
