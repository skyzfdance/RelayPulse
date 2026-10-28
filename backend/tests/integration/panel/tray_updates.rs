use super::*;
use crate::{application::sites::Operation, panel_test_support::Fixture};
use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};
use tokio::sync::mpsc;

struct FakeTray {
    applied: mpsc::UnboundedSender<Result<TrayState, ()>>,
    fail: Arc<AtomicBool>,
    dropped: Arc<AtomicBool>,
}
impl Platform for FakeTray {
    fn apply(&mut self, state: TrayState) -> tauri::Result<()> {
        if self.fail.swap(false, Ordering::SeqCst) {
            self.applied.send(Err(())).unwrap();
            return Err(tauri::Error::FailedToReceiveMessage);
        }
        self.applied.send(Ok(state)).unwrap();
        Ok(())
    }
}
impl Drop for FakeTray {
    fn drop(&mut self) {
        self.dropped.store(true, Ordering::SeqCst);
    }
}

async fn next(
    receiver: &mut mpsc::UnboundedReceiver<Result<TrayState, ()>>,
) -> Result<TrayState, ()> {
    tokio::time::timeout(Duration::from_secs(2), receiver.recv())
        .await
        .unwrap()
        .unwrap()
}

#[tokio::test]
async fn committed_settings_and_site_changes_update_one_tray_without_unchanged_state_repaints() {
    let f = Fixture::new();
    let (send, mut received) = mpsc::unbounded_channel();
    let dropped = Arc::new(AtomicBool::new(false));
    let task = tauri::async_runtime::spawn(watch(
        f.app.tray.clone(),
        FakeTray {
            applied: send,
            fail: Arc::new(AtomicBool::new(false)),
            dropped: dropped.clone(),
        },
    ));
    assert_eq!(next(&mut received).await, Ok(TrayState::Default));
    let id = f.site("http://localhost:1", &["m"], true);
    assert_eq!(next(&mut received).await, Ok(TrayState::Active));
    let mut settings = f.app.configuration.load().unwrap();
    settings.continue_in_background = false;
    f.app.configuration.save(settings.clone()).unwrap();
    f.app
        .sites
        .execute(Operation::Enabled(id.clone(), false))
        .await
        .unwrap();
    assert_eq!(next(&mut received).await, Ok(TrayState::Default));
    settings.show_tray_icon = false;
    f.app.configuration.save(settings.clone()).unwrap();
    assert_eq!(next(&mut received).await, Ok(TrayState::Hidden));
    f.app
        .sites
        .execute(Operation::Enabled(id, true))
        .await
        .unwrap();
    settings.show_tray_icon = true;
    f.app.configuration.save(settings).unwrap();
    assert_eq!(next(&mut received).await, Ok(TrayState::Active));
    task.abort();
    let _ = task.await;
    assert!(dropped.load(Ordering::SeqCst));
}

#[tokio::test]
async fn a_platform_failure_does_not_cache_a_state_that_was_never_displayed() {
    let f = Fixture::new();
    let (send, mut received) = mpsc::unbounded_channel();
    let task = tauri::async_runtime::spawn(watch(
        f.app.tray.clone(),
        FakeTray {
            applied: send,
            fail: Arc::new(AtomicBool::new(true)),
            dropped: Arc::new(AtomicBool::new(false)),
        },
    ));
    assert_eq!(next(&mut received).await, Err(()));
    let settings = f.app.configuration.load().unwrap();
    f.app.configuration.save(settings).unwrap();
    assert_eq!(next(&mut received).await, Ok(TrayState::Default));
    task.abort();
    let _ = task.await;
}
