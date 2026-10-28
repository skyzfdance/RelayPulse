use super::*;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};

fn settings() -> GeneralSettings {
    GeneralSettings {
        launch_at_startup: false,
        continue_in_background: true,
        show_tray_icon: true,
        theme: ThemeMode::System,
        language: Language::ZhCn,
        history_retention_days: 30,
    }
}

#[derive(Clone)]
struct MemoryStore(Arc<Mutex<(GeneralSettings, usize)>>);
impl SettingsStore for MemoryStore {
    fn load(&self) -> Result<GeneralSettings, StorageError> {
        Ok(self.0.lock().unwrap().0.clone())
    }
    fn save(&self, settings: &GeneralSettings) -> Result<GeneralSettings, StorageError> {
        let mut state = self.0.lock().unwrap();
        state.0 = settings.clone();
        state.1 += 1;
        Ok(state.0.clone())
    }
}

#[test]
fn validates_before_saving_and_accepts_retention_boundaries() {
    let state = Arc::new(Mutex::new((settings(), 0)));
    let service = ConfigurationService::new(MemoryStore(Arc::clone(&state)));
    for days in [0, 3651, u16::MAX] {
        let mut input = settings();
        input.history_retention_days = days;
        assert!(matches!(
            service.save(input),
            Err(ConfigurationError::InvalidInput {
                field: "history_retention_days"
            })
        ));
        assert_eq!(*state.lock().unwrap(), (settings(), 0));
    }
    for days in [1, 3650] {
        let mut input = settings();
        input.history_retention_days = days;
        assert_eq!(service.save(input.clone()).unwrap(), input);
        assert_eq!(service.load().unwrap(), input);
    }
}

#[test]
fn rejects_invalid_stored_settings_without_overwriting() {
    let mut invalid = settings();
    invalid.history_retention_days = 0;
    let state = Arc::new(Mutex::new((invalid.clone(), 0)));
    let service = ConfigurationService::new(MemoryStore(Arc::clone(&state)));
    assert!(matches!(
        service.load(),
        Err(ConfigurationError::InvalidInput { .. })
    ));
    assert_eq!(*state.lock().unwrap(), (invalid, 0));
}

#[test]
fn serialized_contract_contains_exactly_the_six_frontend_fields() {
    let expected = json!({
        "launchAtStartup": false, "continueInBackground": true, "showTrayIcon": true,
        "theme": "system", "language": "zh-CN", "historyRetentionDays": 30
    });
    assert_eq!(serde_json::to_value(settings()).unwrap(), expected);
    assert_eq!(
        serde_json::from_value::<GeneralSettings>(expected).unwrap(),
        settings()
    );
    for (theme, value) in [
        (ThemeMode::System, "system"),
        (ThemeMode::Light, "light"),
        (ThemeMode::Dark, "dark"),
    ] {
        assert_eq!(serde_json::to_value(theme).unwrap(), value);
    }
}

#[test]
fn full_settings_contract_rejects_missing_unknown_and_invalid_fields() {
    let valid = serde_json::to_value(settings()).unwrap();
    for field in valid.as_object().unwrap().keys() {
        let mut incomplete = valid.clone();
        incomplete.as_object_mut().unwrap().remove(field);
        assert!(
            serde_json::from_value::<GeneralSettings>(incomplete).is_err(),
            "missing {field}"
        );
    }
    for (field, value) in [
        ("theme", json!("blue")),
        ("language", json!("en-US")),
        ("showTrayIcon", json!("panel")),
        ("launchAtStartup", json!(1)),
        ("continueInBackground", Value::Null),
        ("historyRetentionDays", json!(-1)),
        ("historyRetentionDays", json!(1.5)),
        ("historyRetentionDays", json!(65536)),
        ("trayAction", json!("panel")),
        ("diagnosticCaptureEnabled", json!(true)),
        ("updatedAt", json!(0)),
    ] {
        let mut invalid = valid.clone();
        invalid[field] = value;
        assert!(
            serde_json::from_value::<GeneralSettings>(invalid).is_err(),
            "invalid {field}"
        );
    }
}

struct FailingStore;
impl SettingsStore for FailingStore {
    fn load(&self) -> Result<GeneralSettings, StorageError> {
        Err(std::io::Error::other("read failed").into())
    }
    fn save(&self, _: &GeneralSettings) -> Result<GeneralSettings, StorageError> {
        Err(std::io::Error::other("write failed").into())
    }
}

#[test]
fn propagates_storage_failures() {
    let service = ConfigurationService::new(FailingStore);
    assert!(matches!(
        service.load(),
        Err(ConfigurationError::Storage(_))
    ));
    assert!(matches!(
        service.save(settings()),
        Err(ConfigurationError::Storage(_))
    ));
}

#[test]
fn consumers_are_notified_only_after_a_successful_save() {
    let state = Arc::new(Mutex::new((settings(), 0)));
    let service = ConfigurationService::new(MemoryStore(state));
    let mut changes = service.subscribe();
    service.load().unwrap();
    assert!(!changes.has_changed().unwrap());

    let mut invalid = settings();
    invalid.history_retention_days = 0;
    assert!(service.save(invalid).is_err());
    assert!(!changes.has_changed().unwrap());

    let mut saved = settings();
    saved.continue_in_background = false;
    service.save(saved.clone()).unwrap();
    assert!(changes.has_changed().unwrap());
    changes.borrow_and_update();
    assert_eq!(service.load().unwrap(), saved);
    assert!(!changes.has_changed().unwrap());

    let failing = ConfigurationService::new(FailingStore);
    let changes = failing.subscribe();
    assert!(failing.save(settings()).is_err());
    assert!(!changes.has_changed().unwrap());
}
