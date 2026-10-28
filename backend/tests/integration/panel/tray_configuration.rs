use super::*;
use crate::{application::sites::Operation, panel_test_support::Fixture};

#[tokio::test]
async fn tray_configuration_uses_the_three_switches_on_the_same_site_and_ignores_background_mode() {
    for show in [false, true] {
        for enabled in [false, true] {
            for model_enabled in [false, true] {
                for background in [false, true] {
                    let f = Fixture::new();
                    let id = f.site("http://localhost:1", &["m"], enabled);
                    let model = f.store.get(&id).unwrap().models[0].id.clone();
                    f.app
                        .sites
                        .execute(Operation::ModelEnabled(id, model, model_enabled))
                        .await
                        .unwrap();
                    let mut settings = f.app.configuration.load().unwrap();
                    settings.show_tray_icon = show;
                    settings.continue_in_background = background;
                    f.app.configuration.save(settings).unwrap();
                    let expected = if !show {
                        TrayState::Hidden
                    } else if enabled && model_enabled {
                        TrayState::Active
                    } else {
                        TrayState::Default
                    };
                    assert_eq!(f.app.tray.snapshot().await.unwrap(), expected);
                    assert!(f.app.panel.snapshot().await.unwrap().active_probe.is_none());
                }
            }
        }
    }
}

#[tokio::test]
async fn enabled_models_in_a_hidden_site_cannot_activate_a_different_empty_site() {
    let f = Fixture::new();
    let first = f.site("http://localhost:1", &["disabled"], true);
    let second = f.site("http://localhost:1", &["enabled"], false);
    let model = f.store.get(&first).unwrap().models[0].id.clone();
    f.app
        .sites
        .execute(Operation::ModelEnabled(first, model, false))
        .await
        .unwrap();
    assert_eq!(f.app.tray.snapshot().await.unwrap(), TrayState::Default);
    f.app
        .sites
        .execute(Operation::Enabled(second.clone(), true))
        .await
        .unwrap();
    assert_eq!(f.app.tray.snapshot().await.unwrap(), TrayState::Active);
    f.app
        .sites
        .execute(Operation::Delete(second))
        .await
        .unwrap();
    assert_eq!(f.app.tray.snapshot().await.unwrap(), TrayState::Default);
}

#[tokio::test]
async fn tray_query_waits_for_committed_configuration_and_restores_saved_state_after_reopen() {
    let f = Fixture::new();
    f.site("http://localhost:1", &["m"], true);
    let write = f.app.tray.changes().begin();
    let tray = f.app.tray.clone();
    let read = tokio::spawn(async move { tray.snapshot().await.unwrap() });
    tokio::task::yield_now().await;
    assert!(!read.is_finished());
    let mut settings = f.app.configuration.load().unwrap();
    settings.show_tray_icon = false;
    f.app.configuration.save(settings).unwrap();
    drop(write);
    assert_eq!(read.await.unwrap(), TrayState::Hidden);
    let env = crate::shared::persistence::test_support::environment(
        f.directory.path(),
        "com.relaypulse.app.dev",
    );
    let diagnostics = Arc::new(crate::shared::diagnostics::Diagnostics::new(&env));
    let reopened = crate::application::composition::compose(env, diagnostics).unwrap();
    assert_eq!(reopened.tray.snapshot().await.unwrap(), TrayState::Hidden);
}
