//! 共用测试夹具；调用方必须传入基于临时目录解析的环境。
use crate::{
    application::composition::Application,
    modules::configuration::{GeneralSettings, Language, ThemeMode},
    shared::environment::EnvironmentContext,
    startup::{initialize, StartupState},
};
use rusqlite::Connection;

pub(super) const DEV: &str = "com.relaypulse.app.dev";
pub(super) const PROD: &str = "com.relaypulse.app";

pub(super) fn ready(environment: EnvironmentContext) -> Application {
    match initialize(Ok(environment)).0 {
        StartupState::Ready(app) => app,
        StartupState::Blocked(error) => panic!("unexpected startup failure: {error}"),
    }
}

pub(super) fn input(theme: ThemeMode, days: u16) -> GeneralSettings {
    GeneralSettings {
        launch_at_startup: true,
        continue_in_background: false,
        show_tray_icon: false,
        theme,
        language: Language::ZhCn,
        history_retention_days: days,
    }
}

pub(super) fn defaults() -> GeneralSettings {
    GeneralSettings {
        launch_at_startup: false,
        continue_in_background: true,
        show_tray_icon: true,
        theme: ThemeMode::System,
        language: Language::ZhCn,
        history_retention_days: 30,
    }
}

pub(super) fn connection(app: &Application) -> Connection {
    Connection::open(
        app.environment
            .paths()
            .data_root()
            .join("relaypulse.sqlite3"),
    )
    .unwrap()
}
