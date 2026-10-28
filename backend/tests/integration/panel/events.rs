use super::*;
use crate::panel_test_support as fixture;
use crate::{
    commands::register,
    shared::model_client::test_support::{Response, Server},
    startup::StartupState,
};
use serde_json::{json, Value};
use std::sync::Arc;
use tauri::{
    test::{mock_builder, mock_context, noop_assets},
    Listener, WebviewWindowBuilder,
};

#[tokio::test]
async fn real_backend_changes_emit_targeted_tauri_invalidations_and_unsubscribing_does_not_cancel_probes(
) {
    let f = fixture::Fixture::new();
    let server = Server::new(vec![
        Response::raw(200, "text/plain", "OK").delayed(std::time::Duration::from_millis(100))
    ]);
    f.site(&server.url, &["m"], true);
    let service = f.app.panel.clone();
    let configuration = f.app.configuration.clone();
    let fixture::Fixture {
        app: application,
        directory: _directory,
        database: _database,
        store: _store,
    } = f;
    let app = register(mock_builder())
        .manage(StartupState::Ready(application))
        .build(mock_context(noop_assets()))
        .unwrap();
    let main = WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    let panel = WebviewWindowBuilder::new(&app, WINDOW_LABEL, Default::default())
        .build()
        .unwrap();
    let received = Arc::new(Mutex::new(vec![]));
    let captured = received.clone();
    let listener = panel.listen(STATE_CHANGED, move |event| {
        captured
            .lock()
            .unwrap()
            .push(serde_json::from_str::<Value>(event.payload()).unwrap())
    });
    let other = Arc::new(Mutex::new(vec![]));
    let captured = other.clone();
    main.listen(STATE_CHANGED, move |event| {
        captured.lock().unwrap().push(event.payload().to_string())
    });
    let events = PanelEvents::start(app.handle().clone(), &service);
    let task = tauri::async_runtime::spawn({
        let service = service.clone();
        async move { service.refresh().await.unwrap() }
    });
    server.wait_requests(1).await;
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        while received.lock().unwrap().is_empty() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(service.snapshot().await.unwrap().active_probe.is_some());
    assert!(other.lock().unwrap().is_empty());
    assert!(received
        .lock()
        .unwrap()
        .iter()
        .all(|event| event.as_object().unwrap().len() == 1 && event["revision"].is_number()));
    panel.hide().unwrap();
    panel.unlisten(listener);
    let result = task.await.unwrap();
    assert!(result.active_probe.is_none());
    assert_eq!(
        serde_json::to_value(&result).unwrap()["sites"][0]["models"][0]["status"],
        "healthy"
    );
    let before = service.changes().version();
    let mut settings = configuration.load().unwrap();
    settings.continue_in_background = false;
    configuration.save(settings).unwrap();
    assert!(service.changes().version().revision > before.revision);
    assert_eq!(
        serde_json::to_value(service.snapshot().await.unwrap()).unwrap()["schedule"],
        json!({"state":"disabled"})
    );
    events.stop().await;
    assert!(other.lock().unwrap().is_empty());
}

#[tokio::test]
async fn existing_main_window_on_the_panel_route_receives_notifications_without_a_second_window() {
    let f = fixture::Fixture::new();
    let service = f.app.panel.clone();
    let configuration = f.app.configuration.clone();
    let fixture::Fixture {
        app: application,
        directory: _directory,
        database: _database,
        store: _store,
    } = f;
    let app = register(mock_builder())
        .manage(StartupState::Ready(application))
        .build(mock_context(noop_assets()))
        .unwrap();
    let main = WebviewWindowBuilder::new(
        &app,
        "main",
        tauri::WebviewUrl::External("http://localhost:1421/panel".parse().unwrap()),
    )
    .build()
    .unwrap();
    let received = Arc::new(Mutex::new(vec![]));
    let captured = received.clone();
    main.listen(STATE_CHANGED, move |event| {
        captured
            .lock()
            .unwrap()
            .push(serde_json::from_str::<Value>(event.payload()).unwrap())
    });
    let events = PanelEvents::start(app.handle().clone(), &service);
    let mut settings = configuration.load().unwrap();
    settings.theme = crate::modules::configuration::ThemeMode::Dark;
    configuration.save(settings).unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        while received.lock().unwrap().is_empty() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(received.lock().unwrap()[0]["revision"].as_u64().unwrap() > 0);
    assert_eq!(
        service.snapshot().await.unwrap().theme,
        crate::modules::configuration::ThemeMode::Dark
    );
    events.stop().await;
}
