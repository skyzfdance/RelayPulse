use crate::shared::environment::AppEnvironment;
#[test]
fn configured_identities_match_the_host_policy() {
    let dev: serde_json::Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tauri.conf.json"
    )))
    .unwrap();
    let production: serde_json::Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tauri.production.conf.json"
    )))
    .unwrap();
    assert_eq!(
        super::identify(dev["identifier"].as_str().unwrap()).unwrap(),
        AppEnvironment::Development
    );
    assert_eq!(
        super::identify(production["identifier"].as_str().unwrap()).unwrap(),
        AppEnvironment::Production
    );
}

#[test]
fn unknown_host_identity_is_rejected_before_directory_access() {
    let temp = tempfile::tempdir().unwrap();
    assert!(super::resolve("unknown", temp.path(), &temp.path().join("unknown")).is_err());
    assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
}

#[cfg(debug_assertions)]
#[test]
fn migration_rehearsal_is_isolated_from_both_real_data_directories() {
    let temp = tempfile::tempdir().unwrap();
    let system = temp.path().canonicalize().unwrap();
    let identifier = "com.relaypulse.app.migration-test";
    let rehearsal = system.join(identifier);
    let environment = super::resolve(identifier, &system, &rehearsal).unwrap();
    assert_eq!(environment.paths().data_root(), rehearsal);
    for real_identifier in ["com.relaypulse.app", "com.relaypulse.app.dev"] {
        let real = system.join(real_identifier);
        assert!(environment.protected_roots().contains(&real));
        assert!(super::resolve(identifier, &system, &real).is_err());
    }
    assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
}

#[tokio::test]
async fn composition_uses_injected_host_version_for_real_requests_and_diagnostic_export() {
    use crate::shared::model_client::test_support::{Response, Server};
    let f = crate::panel_test_support::Fixture::new();
    let metadata = f.app.environment.application();
    assert_eq!(metadata.version(), "99.7.3");
    assert_ne!(metadata.version(), env!("CARGO_PKG_VERSION"));
    let server = Server::new(vec![Response::raw(200, "text/plain", "OK")]);
    let site = f.site(&server.url, &["host-version-model"], true);
    f.app.monitoring.run(site, None).await.unwrap();
    assert!(server.requests()[0].head.contains("FixtureHost/99.7.3 ("));
    let diagnostics = crate::shared::diagnostics::Diagnostics::new(&f.app.environment);
    let export = f.directory.path().join("metadata.json");
    diagnostics.export(&export).unwrap();
    let package: serde_json::Value =
        serde_json::from_slice(&std::fs::read(export).unwrap()).unwrap();
    assert_eq!(package["appName"], "FixtureHost");
    assert_eq!(package["appVersion"], "99.7.3");
    // The host injects both protected environments, even if this one has not been opened.
    for root in f.app.environment.protected_roots() {
        assert!(diagnostics.export(root).is_err());
        std::fs::create_dir_all(root).unwrap();
        let sentinel = root.join("preserve.json");
        std::fs::write(&sentinel, b"keep").unwrap();
        assert!(diagnostics.export(&sentinel).is_err());
        assert_eq!(std::fs::read(sentinel).unwrap(), b"keep");
    }
}
