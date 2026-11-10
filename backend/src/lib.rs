//! Crate 入口：只接入已迁移能力，对外提供最小启动门面。

// 只声明新的模块入口，旧业务树仍不接入。
mod application;
mod commands;
mod desktop;
mod modules;
mod shared;
mod startup;

#[cfg(test)]
#[path = "../tests/support/iq_test.rs"]
mod iq_test_support;
#[cfg(test)]
#[path = "../tests/support/panel.rs"]
mod panel_test_support;

// 第 2 步：向 main 提供唯一的启动入口。
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // 第 3 步：创建窗口并进入桌面事件循环。
    desktop::run();
}

#[cfg(test)]
#[path = "../tests/integration/startup/notification_format.rs"]
mod notification_format_tests;

#[cfg(test)]
#[path = "../tests/support/notification_definitions.rs"]
mod notification_test_definitions;
#[cfg(test)]
#[path = "../tests/support/action_definitions.rs"]
mod test_actions;

#[cfg(test)]
#[path = "../tests/integration/iq_test/mod.rs"]
mod iq_integration_tests;

#[cfg(test)]
#[path = "../tests/support/notification_host.rs"]
mod notification_host;

/// Run the explicit RelayPulse offline conversion command without starting the desktop app.
pub fn run_data_transfer() {
    application::data_transfer::run();
}
