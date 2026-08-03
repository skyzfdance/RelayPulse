pub mod application;
pub mod commands;
pub mod credentials;
pub mod domain;
pub mod persistence;
pub mod startup;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let state = match app.path().app_data_dir() {
                Ok(data_directory) => startup::BackendState::initialize(data_directory),
                Err(_) => startup::BackendState::blocked(startup::StartupFailure::data_directory()),
            };
            let credential_saga = state.credential_saga().cloned();
            app.manage(state);
            if let Some(credential_saga) = credential_saga {
                tauri::async_runtime::spawn(async move {
                    let _ = credential_saga.reconcile().await;
                });
            }
            Ok(())
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running RelayPulse");
}
