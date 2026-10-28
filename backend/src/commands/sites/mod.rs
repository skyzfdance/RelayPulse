//! 站点命令模块入口：声明内部文件，向统一注册入口导出命令。
mod args;
mod error;
mod handlers;
#[cfg(test)]
#[path = "../../../tests/integration/commands/sites.rs"]
mod tests;

// 同时导出 Tauri 为命令生成的注册宏，供 commands/mod.rs 使用。
pub(super) use handlers::*;
