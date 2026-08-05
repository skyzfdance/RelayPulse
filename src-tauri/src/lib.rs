pub mod application;
pub mod commands;
pub mod credentials;
pub mod desktop;
pub mod diagnostics;
pub mod domain;
pub mod persistence;
pub mod startup;

use tauri::{Manager, WindowEvent};
use tauri_plugin_autostart::MacosLauncher;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            let _ = desktop::show_main_window(app, "sites");
        }))
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let state = match app.path().app_data_dir() {
                Ok(data_directory) => startup::BackendState::initialize(data_directory),
                Err(_) => startup::BackendState::blocked(startup::StartupFailure::data_directory()),
            };
            let credential_saga = state.credential_saga().cloned();
            let application_service = state.application_service().cloned();
            app.manage(state);
            if let Some(credential_saga) = credential_saga {
                tauri::async_runtime::spawn(async move {
                    let _ = credential_saga.reconcile().await;
                });
            }
            if let Some(application_service) = application_service {
                let scheduler_app = app.handle().clone();
                let scheduler =
                    application::start_scheduler(application_service.clone(), move |snapshot| {
                        commands::publish_background_snapshot(&scheduler_app, &snapshot);
                    });
                if let Some(state) = app.try_state::<startup::BackendState>() {
                    state.set_scheduler(scheduler);
                }
                let app_handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    if let Ok(snapshot) = application_service.snapshot().await {
                        if let Some(state) = app_handle.try_state::<startup::BackendState>() {
                            state.set_continue_in_background(
                                snapshot.general_settings.continue_in_background,
                            );
                            let _ = desktop::set_tray_enabled(
                                &app_handle,
                                snapshot.general_settings.tray_action == domain::TrayAction::Panel,
                            );
                            let succeeded = desktop::set_autostart(
                                &app_handle,
                                snapshot.general_settings.launch_at_startup,
                            )
                            .is_ok();
                            if let Some(diagnostics) = state.diagnostics() {
                                diagnostics.record_autostart_reconciled(
                                    snapshot.general_settings.launch_at_startup,
                                    succeeded,
                                );
                            }
                            let _ = desktop::update_status_projection(&app_handle, &snapshot);
                        }
                    }
                });
            }
            Ok(())
        })
        .on_window_event(|window, event| match event {
            WindowEvent::CloseRequested { api, .. } if window.label() == "main" => {
                if window
                    .try_state::<startup::BackendState>()
                    .is_some_and(|state| state.continue_in_background())
                {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
            WindowEvent::CloseRequested { api, .. } if window.label() == "status-panel" => {
                api.prevent_close();
                let _ = window.hide();
            }
            WindowEvent::Focused(false) if window.label() == "status-panel" => {
                let _ = window.hide();
            }
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_snapshot,
            commands::get_site,
            commands::create_site,
            commands::update_site,
            commands::delete_site,
            commands::reorder_sites,
            commands::set_site_enabled,
            commands::create_model,
            commands::delete_model,
            commands::set_model_enabled,
            commands::update_site_advanced,
            commands::update_site_probe_settings,
            commands::update_model_probe_settings,
            commands::update_model_custom_probe_settings,
            commands::update_general_settings,
            commands::refresh_probe_records,
            commands::clear_probe_records,
            commands::refresh_models,
            commands::run_immediate_probe,
            commands::run_site_probe,
            commands::run_model_probe,
            commands::open_log_directory,
            commands::export_diagnostics,
            commands::open_main_window,
        ])
        .run(tauri::generate_context!())
        .expect("error while running RelayPulse");
}
