//! 启动组装后的配置集成回归：保存、重开、环境隔离、事务回滚与并发。
use super::support::{connection, defaults, input, ready, DEV, PROD};
use crate::{
    modules::configuration::{ConfigurationError, ThemeMode},
    shared::persistence::test_support::environment,
};
use std::sync::Arc;

#[test]
fn defaults_full_save_reopen_and_environment_isolation() {
    let temp = tempfile::tempdir().unwrap();
    for (identifier, theme, days) in [(DEV, ThemeMode::Dark, 1), (PROD, ThemeMode::Light, 3650)] {
        let app = ready(environment(temp.path(), identifier));
        assert_eq!(app.configuration.load().unwrap(), defaults());
        let expected = input(theme, days);
        assert_eq!(app.configuration.save(expected.clone()).unwrap(), expected);
        assert_eq!(app.configuration.load().unwrap(), expected);
        drop(app);
        let reopened = ready(environment(temp.path(), identifier));
        assert_eq!(reopened.configuration.load().unwrap(), expected);
    }
    let dev = ready(environment(temp.path(), DEV));
    let prod = ready(environment(temp.path(), PROD));
    assert_ne!(
        dev.environment.paths().data_root(),
        prod.environment.paths().data_root()
    );
    assert_eq!(dev.configuration.load().unwrap().theme, ThemeMode::Dark);
    assert_eq!(prod.configuration.load().unwrap().theme, ThemeMode::Light);
    // 再全量保存另一份对象，确保 false/true 两个方向都覆盖，没有字段级合并。
    assert_eq!(dev.configuration.save(defaults()).unwrap(), defaults());
    assert_eq!(dev.configuration.load().unwrap(), defaults());
    assert_eq!(
        prod.configuration.load().unwrap(),
        input(ThemeMode::Light, 3650)
    );
}

#[test]
fn invalid_input_does_not_change_settings() {
    let temp = tempfile::tempdir().unwrap();
    let app = ready(environment(temp.path(), DEV));
    for days in [0, 3651, u16::MAX] {
        assert!(matches!(
            app.configuration.save(input(ThemeMode::Dark, days)),
            Err(ConfigurationError::InvalidInput { .. })
        ));
        assert_eq!(app.configuration.load().unwrap(), defaults());
    }
}

#[test]
fn readback_failure_rolls_back_the_entire_save() {
    let temp = tempfile::tempdir().unwrap();
    let app = ready(environment(temp.path(), DEV));
    let connection = connection(&app);
    // 更新成功后让读回失败，证明事务覆盖“写入 + 读回”，失败不能留下半完成状态。
    connection.execute_batch("CREATE TRIGGER remove_settings AFTER UPDATE ON general_settings BEGIN DELETE FROM general_settings; END;").unwrap();
    assert!(matches!(
        app.configuration.save(input(ThemeMode::Dark, 90)),
        Err(ConfigurationError::Storage(_))
    ));
    assert_eq!(app.configuration.load().unwrap(), defaults());
    connection
        .execute_batch("DROP TRIGGER remove_settings;")
        .unwrap();
    let expected = input(ThemeMode::Light, 90);
    assert_eq!(app.configuration.save(expected.clone()).unwrap(), expected);
}

#[test]
fn each_concurrent_save_returns_its_own_committed_settings() {
    let temp = tempfile::tempdir().unwrap();
    let app = Arc::new(ready(environment(temp.path(), DEV)));
    let threads: Vec<_> = (1..=8)
        .map(|days| {
            let app = Arc::clone(&app);
            std::thread::spawn(move || {
                let expected = input(ThemeMode::Light, days);
                assert_eq!(app.configuration.save(expected.clone()).unwrap(), expected);
            })
        })
        .collect();
    for thread in threads {
        thread.join().unwrap();
    }
}
