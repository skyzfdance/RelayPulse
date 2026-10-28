//! 进程入口：只负责启动桌面应用。

// 第 1 步：Windows 正式构建隐藏额外的控制台窗口。
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // 第 2 步：交给库的启动门面；目前没有辅助进程或业务分支。
    relaypulse_lib::run();
}
