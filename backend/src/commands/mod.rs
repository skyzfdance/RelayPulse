//! Tauri 命令目录与统一注册入口；具体协议适配按业务领域拆分。
mod configuration;
mod diagnostics;
mod error;
mod notifications;
mod panel;
mod records;
mod sites;

pub(crate) fn register<R: tauri::Runtime>(builder: tauri::Builder<R>) -> tauri::Builder<R> {
    builder.invoke_handler(tauri::generate_handler![
        panel::get_panel_snapshot,
        panel::refresh_panel,
        panel::sync_panel_window,
        panel::open_panel_settings,
        panel::open_panel_sites,
        records::get_probe_record_options,
        records::get_probe_records,
        records::get_probe_token_stats,
        records::get_probe_record_detail,
        records::get_probe_record_request,
        records::get_probe_record_response,
        records::open_probe_record_log_directory,
        records::delete_probe_record,
        records::clear_probe_records,
        sites::get_site_protocols,
        sites::get_sites,
        sites::get_site,
        sites::create_site,
        sites::save_site,
        sites::update_site_connection,
        sites::delete_site,
        sites::reorder_sites,
        sites::set_site_enabled,
        sites::add_site_model,
        sites::delete_site_model,
        sites::set_site_model_enabled,
        sites::sync_site_models,
        sites::apply_site_model_sync,
        sites::test_model_probe,
        sites::update_site_probe_policy,
        sites::run_site_probe,
        diagnostics::get_log_statistics,
        diagnostics::clear_diagnostic_logs,
        diagnostics::open_log_directory,
        diagnostics::export_diagnostics,
        configuration::get_general_settings,
        configuration::update_general_settings,
        notifications::get_notification_settings,
        notifications::update_notification_settings,
        notifications::request_notification_permission,
        notifications::send_test_notification,
        notifications::open_notification_settings,
        notifications::get_notification_template,
        notifications::update_notification_template,
        notifications::get_notification_markers,
        notifications::get_notification_variables,
    ])
}
