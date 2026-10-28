//! 桌面宿主：加载前端根入口，持有业务及诊断资源并注册命令。

pub(crate) mod diagnostics;
pub(crate) mod directories;
mod environment;
pub(crate) mod main_window;
mod notifications;
pub(crate) mod panel;
#[cfg(target_os = "macos")]
mod panel_macos;
pub(crate) mod panel_window;
mod tray;

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tauri::Manager;
struct Background {
    lifecycle: Arc<crate::application::lifecycle::Lifecycle>,
    panel_events: Arc<panel::PanelEvents>,
    tray: Arc<tray::TrayRuntime>,
    notifications: Arc<notifications::SystemNotifications>,
    stopping: AtomicBool,
    stopped: Arc<AtomicBool>,
}

use crate::{
    commands,
    startup::{self, StartupState},
};

pub(crate) fn run() {
    // 第 1 步：沿用 Tauri 配置中的应用身份、窗口标题和尺寸。
    // 开发/生产环境隔离规则保持不变。
    // 第 2 步：使用 Tauri 默认前端根入口，具体页面导航交给 Vue Router。
    let context = tauri::generate_context!();

    // 第 3 步：在宿主可用后解析环境，把明确的阶段结果注册为应用状态。
    // 第 4 步：注册命令；业务就绪后由 lifecycle 持有站点周期任务。
    // 在业务初始化、窗口和调度器之前拦截重复进程；开发与正式身份各自独立。
    // 显示及导航继续由 Dock Reopen 和真实通知的点击回调处理。
    let builder =
        tauri::Builder::default().plugin(tauri_plugin_single_instance::init(|_, _, _| {}));
    let builder = commands::register(builder);
    #[cfg(target_os = "macos")]
    let builder = builder.plugin(tauri_nspanel::init());
    let app = builder
        .plugin(tauri_plugin_dialog::init())
        .on_menu_event(tray::on_menu_event)
        .setup(|app| {
            app.manage(panel_window::PanelWindow::default());
            let notifications = Arc::new(notifications::SystemNotifications::new(
                app.handle().clone(),
            ));
            app.manage(crate::modules::notifications::NotificationPlatformState(
                notifications.clone(),
            ));
            let (state, diagnostics_state) =
                startup::initialize(environment::resolve(app.handle()));
            app.manage(diagnostics_state);
            app.manage(diagnostics::DiagnosticsDesktop(std::sync::Arc::new(
                diagnostics::SystemDiagnostics(app.handle().clone()),
            )));
            app.manage(directories::DirectoryDesktop(Arc::new(
                directories::SystemDirectories,
            )));
            match &state {
                StartupState::Ready(application) => eprintln!(
                    "[startup] configuration, notifications and sites ready: {:?}; data root: {}",
                    application.environment.environment(),
                    application.environment.paths().data_root().display(),
                ),
                StartupState::Blocked(error) => eprintln!("[startup] blocked: {error}"),
            }
            // 宿主持有服务及连接；失败保留阶段错误，命令返回明确的不可用原因。
            if let StartupState::Ready(application) = &state {
                application
                    .monitoring
                    .set_health_observer(Arc::new(
                        crate::application::notifications::HealthNotifications::new(
                            application.notifications.clone(),
                            app.state::<crate::modules::notifications::NotificationPlatformState>()
                                .0
                                .clone(),
                        ),
                    ))
                    .map_err(|_| std::io::Error::other("health notifications already connected"))?;
                let panel_events = Arc::new(panel::PanelEvents::start(
                    app.handle().clone(),
                    &application.panel,
                ));
                let tray = Arc::new(tray::TrayRuntime::start(
                    app.handle().clone(),
                    application.tray.clone(),
                ));
                let lifecycle = tauri::async_runtime::block_on(async {
                    crate::application::lifecycle::Lifecycle::start(application.monitoring.clone())
                })?;
                app.manage(Background {
                    lifecycle: Arc::new(lifecycle),
                    panel_events,
                    tray,
                    notifications,
                    stopping: AtomicBool::new(false),
                    stopped: Arc::new(AtomicBool::new(false)),
                });
            }
            app.manage(state);
            Ok(())
        })
        .build(context)
        .expect("failed to build the desktop shell");
    app.run(|handle, event| {
        #[cfg(target_os = "macos")]
        if let tauri::RunEvent::Reopen { .. } = &event {
            if main_window::show(handle, None).is_err() {
                eprintln!("[desktop] main window reopen failed");
            }
        }
        if let tauri::RunEvent::ExitRequested { api, code, .. } = event {
            if let Some(background) = handle.try_state::<Background>() {
                if background.stopped.load(Ordering::SeqCst) {
                    return;
                }
                api.prevent_exit();
                if !background.stopping.swap(true, Ordering::SeqCst) {
                    let lifecycle = background.lifecycle.clone();
                    let panel_events = background.panel_events.clone();
                    let tray = background.tray.clone();
                    let notifications = background.notifications.clone();
                    let stopped = background.stopped.clone();
                    let handle = handle.clone();
                    tauri::async_runtime::spawn(async move {
                        handle.state::<panel_window::PanelWindow>().stop().await;
                        tray.stop().await;
                        lifecycle.stop().await;
                        notifications.stop().await;
                        panel_events.stop().await;
                        stopped.store(true, Ordering::SeqCst);
                        handle.exit(code.unwrap_or(0));
                    });
                }
            }
        }
    });
}
