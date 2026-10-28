//! Crate 入口：只接入已迁移能力，对外提供最小启动门面。

// 只声明新的模块入口，旧业务树仍不接入。
mod application;
mod commands;
mod desktop;
mod modules;
mod shared;
mod startup;

#[cfg(test)]
#[path = "../tests/support/panel.rs"]
mod panel_test_support;

// 第 2 步：向 main 提供唯一的启动入口。
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // 第 3 步：创建窗口并进入桌面事件循环。
    desktop::run();
}
