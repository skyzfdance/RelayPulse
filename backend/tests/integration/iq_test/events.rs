use super::*;
use crate::{iq_test_support::Fixture, modules::iq_test::IqQuestionInput};
use serde_json::{json, Value};
use std::{sync::Arc, time::Duration};
use tauri::{
    test::{mock_builder, mock_context, noop_assets},
    Listener, Manager, WebviewWindowBuilder,
};

#[tokio::test]
async fn iq_invalidations_contain_only_revision_target_main_and_requery_after_event_observes_the_change(
) {
    let f = Fixture::new();
    let app = mock_builder().build(mock_context(noop_assets())).unwrap();
    let main = WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    let other = WebviewWindowBuilder::new(&app, "other", Default::default())
        .build()
        .unwrap();
    let received = Arc::new(Mutex::new(Vec::<Value>::new()));
    let saved = received.clone();
    main.listen(STATE_CHANGED, move |event| {
        saved
            .lock()
            .unwrap()
            .push(serde_json::from_str(event.payload()).unwrap())
    });
    let elsewhere = Arc::new(Mutex::new(Vec::<String>::new()));
    let saved = elsewhere.clone();
    other.listen(STATE_CHANGED, move |event| {
        saved.lock().unwrap().push(event.payload().into())
    });
    let unexpected = Arc::new(Mutex::new(0));
    let saved = unexpected.clone();
    main.listen("iq-test:completed", move |_| *saved.lock().unwrap() += 1);
    let events = IqEvents::start(app.handle().clone(), &f.app.iq_test);
    f.app
        .iq_test
        .save_question(
            IqQuestionInput {
                content: "event question".into(),
                reference_answer: None,
                enabled: true,
            },
            None,
        )
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        while received.lock().unwrap().is_empty() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let snapshot = f.app.iq_test.snapshot().await.unwrap();
    assert_eq!(snapshot.questions.len(), 1);
    for event in received.lock().unwrap().iter() {
        assert_eq!(event.as_object().unwrap().len(), 1);
        assert!(event["revision"]
            .as_u64()
            .is_some_and(|v| v > 0 && v <= snapshot.revision));
    }
    assert!(elsewhere.lock().unwrap().is_empty());
    assert_eq!(*unexpected.lock().unwrap(), 0);
    events.stop().await;
    f.app.iq_test.stop().await;
}

#[test]
fn clicking_an_iq_notification_navigates_only_main_with_the_selected_run_id() {
    let app = mock_builder()
        .manage(crate::desktop::panel_window::PanelWindow::default())
        .build(mock_context(noop_assets()))
        .unwrap();
    let main = WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    let other = WebviewWindowBuilder::new(&app, "other", Default::default())
        .build()
        .unwrap();
    let received = Arc::new(Mutex::new(Vec::<Value>::new()));
    let saved = received.clone();
    main.listen(crate::desktop::main_window::NAVIGATE, move |event| {
        saved
            .lock()
            .unwrap()
            .push(serde_json::from_str(event.payload()).unwrap())
    });
    let elsewhere = Arc::new(Mutex::new(0));
    let saved = elsewhere.clone();
    other.listen(crate::desktop::main_window::NAVIGATE, move |_| {
        *saved.lock().unwrap() += 1
    });
    crate::desktop::main_window::iq_history(app.handle(), "selected-run_1").unwrap();
    assert_eq!(
        *received.lock().unwrap(),
        vec![json!({"path":"/iq-test/history","query":{"runId":"selected-run_1"}})]
    );
    assert_eq!(*elsewhere.lock().unwrap(), 0);
    assert!(crate::desktop::main_window::iq_history(app.handle(), "../untrusted").is_err());
    assert_eq!(app.webview_windows().len(), 2);
}

#[test]
fn clicking_an_iq_notification_recreates_a_closed_main_with_the_run_in_its_initial_url() {
    let mut context = mock_context(noop_assets());
    context.config_mut().app.windows = vec![tauri::utils::config::WindowConfig {
        label: "main".into(),
        ..Default::default()
    }];
    let app = mock_builder()
        .manage(crate::desktop::panel_window::PanelWindow::default())
        .build(context)
        .unwrap();
    crate::desktop::main_window::iq_history(app.handle(), "selected-run_1").unwrap();
    let url = app.get_webview_window("main").unwrap().url().unwrap();
    assert_eq!(url.path(), "/iq-test/history");
    assert_eq!(url.query(), Some("runId=selected-run_1"));
}
