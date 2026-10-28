use super::*;
use crate::shared::persistence::{test_support::environment, Database};
use serde_json::json;
use std::sync::Arc;
fn fixture(root: &std::path::Path) -> (SiteStore, Arc<Database>) {
    let db = Arc::new(
        Database::open(
            environment(root, "com.relaypulse.app.dev").paths(),
            &[crate::modules::configuration::SCHEMA, SCHEMA],
        )
        .unwrap(),
    );
    (SiteStore::new(db.clone()), db)
}
fn input() -> CreateSite {
    CreateSite {
        name: "  本地站点  ".into(),
        base_url: "https://example.test".into(),
        protocol: Protocol::OpenaiCompatible,
        secret: "original-secret".into(),
        enabled: false,
    }
}
#[test]
fn isolated_saves_credentials_persistence_and_unique_models() {
    let tmp = tempfile::tempdir().unwrap();
    let (store, db) = fixture(tmp.path());
    let id = store.create(&input(), &["model-name".into()]).unwrap();
    let original = store.get(&id).unwrap();
    assert_ne!(original.models[0].id, original.models[0].name);
    assert_eq!(original.name, "本地站点");
    assert!(!serde_json::to_string(&original).unwrap().contains("secret"));
    let custom = CustomRequest {
        version: 2,
        endpoint: "/custom".into(),
        parameters: vec![],
    };
    store
        .save_policy(&id, Some(&original.models[0].id), None, Some(&custom))
        .unwrap();
    let mut connection = original.connection.clone();
    connection.interval_seconds = 1;
    connection.headers.push(RequestHeader {
        key: "X-Empty".into(),
        value: "".into(),
    });
    store.save_connection(&id, &connection).unwrap();
    let draft = SiteDraft {
        name: "renamed".into(),
        protocol: original.protocol,
        base_url: original.base_url.clone(),
        credential_action: CredentialAction::Keep,
        secret: "must-be-ignored".into(),
    };
    let saved = store.save_basic(&id, &draft).unwrap();
    assert_eq!(saved.connection, connection);
    assert_eq!(saved.models[0].custom_request, Some(custom));
    assert_eq!(
        store
            .execution_snapshot(&id)
            .unwrap()
            .target
            .secret
            .as_str(),
        "original-secret"
    );
    let mut replacement = draft;
    replacement.credential_action = CredentialAction::Replace;
    replacement.secret = "replacement".into();
    store.save_basic(&id, &replacement).unwrap();
    assert!(matches!(
        store.add_model(&id, " model-name "),
        Err(SiteConfigError::Duplicate)
    ));
    drop(store);
    drop(db);
    let (store, _) = fixture(tmp.path());
    assert_eq!(
        store
            .execution_snapshot(&id)
            .unwrap()
            .target
            .secret
            .as_str(),
        "replacement"
    );
    assert_eq!(store.get(&id).unwrap().models[0].id, original.models[0].id);
    store.delete(&id).unwrap();
    assert!(store.list().unwrap().is_empty());
}
#[test]
fn readback_errors_roll_back_all_changes_and_sync_validates_full_sets() {
    let tmp = tempfile::tempdir().unwrap();
    let (store, db) = fixture(tmp.path());
    let id = store
        .create(&input(), &["keep".into(), "remove".into()])
        .unwrap();
    let before = store.get(&id).unwrap();
    let apply = ApplySync {
        models: vec!["keep".into(), "remove".into()],
        added: vec!["added".into()],
        removed: vec![before.models[1].id.clone()],
    };
    let mut invalid = apply.clone();
    invalid.removed.push("not-this-site".into());
    assert!(store.apply_sync(&id, before.revision, &invalid).is_err());
    assert_eq!(store.get(&id).unwrap(), before);
    db.with_connection(|c|c.execute_batch("CREATE TRIGGER reject_model BEFORE INSERT ON site_models WHEN NEW.name='added' BEGIN SELECT RAISE(ABORT,'test'); END;")).unwrap();
    assert!(store.apply_sync(&id, before.revision, &apply).is_err());
    assert_eq!(store.get(&id).unwrap(), before);
    db.with_connection(|c| c.execute_batch("DROP TRIGGER reject_model;"))
        .unwrap();
    store.apply_sync(&id, before.revision, &apply).unwrap();
    assert!(store.apply_sync(&id, before.revision, &apply).is_err());
    assert_eq!(store.get(&id).unwrap().models[0], before.models[0]);
    db.with_connection(|c|c.execute_batch("CREATE TRIGGER break_site AFTER UPDATE ON sites BEGIN DELETE FROM sites WHERE id=NEW.id; END;")).unwrap();
    let before = store.get(&id).unwrap();
    assert!(store.set_enabled(&id, true).is_err());
    assert_eq!(store.get(&id).unwrap(), before);
}
#[test]
fn parameter_types_paths_and_frontend_validation_boundaries() {
    let mut connection = Connection::for_protocol(Protocol::OpenaiCompatible);
    for n in [0, 86401] {
        connection.interval_seconds = n;
        assert!(validation::connection(&connection, "https://example.test").is_err());
    }
    connection.interval_seconds = 86400;
    for n in [0, 101] {
        connection.slow_first_token_seconds = n;
        assert!(validation::connection(&connection, "https://example.test").is_err());
    }
    connection.slow_first_token_seconds = 100;
    connection.slow_total_seconds = 500;
    connection.failure_threshold = 10;
    connection.recovery_threshold = 10;
    assert!(validation::connection(&connection, "https://example.test").is_ok());
    for endpoint in ["", " /x", "/x ", "/x\ny", "https://u:p@example.test/x"] {
        connection.models_path = endpoint.into();
        assert!(validation::connection(&connection, "https://example.test").is_err());
    }
    for key in ["", "__proto__.x", "a.constructor.x", "a..b", "a. b"] {
        let parameter = Parameter::new(key, "1", ParameterType::Number);
        assert!(validation::parameters(&[parameter]).is_err());
    }
    for (text, expected) in [
        ("1e2", json!(100)),
        ("64.0", json!(64)),
        ("-0", json!(0)),
        ("9007199254740993", json!(9007199254740992u64)),
        ("0xff", json!(255)),
        (" 0b10 ", json!(2)),
    ] {
        assert_eq!(
            crate::shared::model_client::parameter_value(&Parameter::new(
                "n",
                text,
                ParameterType::Number
            ))
            .unwrap(),
            expected
        );
    }
    for text in ["", " ", "NaN", "Infinity", "1e999", "no"] {
        assert!(
            crate::shared::model_client::parameter_value(&Parameter::new(
                "n",
                text,
                ParameterType::Number
            ))
            .is_err()
        );
    }
    assert_eq!(
        validation::endpoint("https://example.test/v1", "images", true)
            .unwrap()
            .as_str(),
        "https://example.test/v1/images"
    );
    assert_eq!(
        validation::endpoint("https://example.test/v1", "/images", true)
            .unwrap()
            .as_str(),
        "https://example.test/images"
    );
    for endpoint in ["//evil.test/x", "http://example.test/x", "/x\ty"] {
        assert!(validation::endpoint("https://example.test/v1", endpoint, true).is_err());
    }
}

#[test]
fn schema_extension_preserves_existing_settings_and_isolates_environments() {
    use crate::modules::{configuration, monitoring, notifications};
    let temp = tempfile::tempdir().unwrap();
    let env = environment(temp.path(), "com.relaypulse.app.dev");
    let old = Arc::new(
        Database::open(env.paths(), &[configuration::SCHEMA, notifications::SCHEMA]).unwrap(),
    );
    old.with_connection(|c|c.execute_batch("UPDATE general_settings SET history_retention_days=77; UPDATE notification_settings SET enabled=1; UPDATE notification_rules SET status=0 WHERE id='probe.model_failed';")).unwrap();
    drop(old);
    let migrations = [
        configuration::SCHEMA,
        notifications::SCHEMA,
        SCHEMA,
        monitoring::SCHEMA,
    ];
    let db = Arc::new(Database::open(env.paths(), &migrations).unwrap());
    db.with_connection(|c| {
        assert_eq!(
            c.query_row(
                "SELECT history_retention_days FROM general_settings",
                [],
                |r| r.get::<_, i64>(0)
            )?,
            77
        );
        assert_eq!(
            c.query_row("SELECT enabled FROM notification_settings", [], |r| r
                .get::<_, i64>(0))?,
            1
        );
        assert_eq!(
            c.query_row(
                "SELECT status FROM notification_rules WHERE id='probe.model_failed'",
                [],
                |r| r.get::<_, i64>(0)
            )?,
            0
        );
        Ok(())
    })
    .unwrap();
    let store = SiteStore::new(db);
    let id = store.create(&input(), &["m".into()]).unwrap();
    let prod = Arc::new(
        Database::open(
            environment(temp.path(), "com.relaypulse.app").paths(),
            &migrations,
        )
        .unwrap(),
    );
    assert!(SiteStore::new(prod).list().unwrap().is_empty());
    let reopened = SiteStore::new(Arc::new(Database::open(env.paths(), &migrations).unwrap()));
    assert_eq!(reopened.get(&id).unwrap().name, "本地站点");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(env.paths().data_root())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert_eq!(
            std::fs::metadata(env.paths().data_root().join("relaypulse.sqlite3"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
}
#[test]
fn failed_site_schema_extension_rolls_back_new_tables_without_erasing_existing_data() {
    use crate::modules::{configuration, notifications};
    let temp = tempfile::tempdir().unwrap();
    let env = environment(temp.path(), "com.relaypulse.app.dev");
    let db = Database::open(env.paths(), &[configuration::SCHEMA, notifications::SCHEMA]).unwrap();
    db.with_connection(|c|c.execute_batch("UPDATE general_settings SET history_retention_days=91; CREATE TABLE site_models (existing TEXT); INSERT INTO site_models VALUES ('keep');")).unwrap();
    drop(db);
    assert!(Database::open(
        env.paths(),
        &[configuration::SCHEMA, notifications::SCHEMA, SCHEMA]
    )
    .is_err());
    let db = Database::open(env.paths(), &[configuration::SCHEMA, notifications::SCHEMA]).unwrap();
    db.with_connection(|c| {
        assert_eq!(
            c.query_row("SELECT COUNT(*) FROM schema_migrations", [], |r| r
                .get::<_, i64>(0))?,
            2
        );
        assert_eq!(
            c.query_row(
                "SELECT COUNT(*) FROM sqlite_schema WHERE name='sites'",
                [],
                |r| r.get::<_, i64>(0)
            )?,
            0
        );
        assert_eq!(
            c.query_row("SELECT existing FROM site_models", [], |r| r
                .get::<_, String>(0))?,
            "keep"
        );
        assert_eq!(
            c.query_row(
                "SELECT history_retention_days FROM general_settings",
                [],
                |r| r.get::<_, i64>(0)
            )?,
            91
        );
        Ok(())
    })
    .unwrap();
}

#[test]
fn request_parameter_table_round_trips_disabled_rows_empty_objects_and_empty_tables() {
    let tmp = tempfile::tempdir().unwrap();
    let (store, db) = fixture(tmp.path());
    let id = store.create(&input(), &["model".into()]).unwrap();
    let policy: ProbePolicy = serde_json::from_value(json!({"version":2,"parameters":[
        {"key":"model","value":"","type":"string","enabled":false,"binding":"model"},
        {"key":"reasoning","value":"{}","type":"object","enabled":false},
        {"key":"reasoning.effort","value":"{","type":"json","enabled":true},
        {"key":"empty","value":"{}","type":"object","enabled":true},
        {"key":"max_tokens","value":"8","type":"number","enabled":true}
    ]}))
    .unwrap();
    let saved = store.save_policy(&id, None, Some(&policy), None).unwrap();
    assert_eq!(saved.policy, policy);
    let dto = serde_json::to_value(&saved.policy).unwrap();
    assert_eq!(dto.as_object().unwrap().len(), 2);
    assert!(dto.get("prompt").is_none());
    assert!(dto.get("maxTokens").is_none());
    let model = saved.models[0].id.clone();
    let empty = CustomRequest {
        version: 2,
        endpoint: "/custom".into(),
        parameters: vec![],
    };
    store
        .save_policy(&id, Some(&model), None, Some(&empty))
        .unwrap();
    drop(store);
    drop(db);
    let (store, _) = fixture(tmp.path());
    let reopened = store.get(&id).unwrap();
    assert_eq!(reopened.policy, policy);
    assert_eq!(reopened.models[0].custom_request, Some(empty));
    let blank = ProbePolicy {
        version: 2,
        parameters: vec![],
    };
    store.save_policy(&id, None, Some(&blank), None).unwrap();
    assert_eq!(store.get(&id).unwrap().policy, blank);
}

#[test]
fn legacy_json_is_expanded_once_and_preserved_when_switching_protocols() {
    let tmp = tempfile::tempdir().unwrap();
    let (store, db) = fixture(tmp.path());
    let mut initial = input();
    initial.protocol = Protocol::OpenaiResponses;
    let id = store
        .create(&initial, &["independent".into(), "custom".into()])
        .unwrap();
    let original = store.get(&id).unwrap();
    let legacy = json!({"prompt":"legacy prompt","maxTokens":24,"parameters":[
        {"key":"reasoning","type":"json","value":"{\"budget\":10}"},
        {"key":"reasoning.effort","type":"string","value":"high"},
        {"key":"metadata","type":"json","value":"{\"literal.key\":1}"}
    ]})
    .to_string();
    let custom = json!({"endpoint":"/legacy","parameters":[]}).to_string();
    db.with_connection(|c| {
        c.execute(
            "UPDATE sites SET policy=?1 WHERE id=?2",
            rusqlite::params![legacy, id],
        )?;
        c.execute(
            "UPDATE site_models SET policy=?1 WHERE id=?2",
            rusqlite::params![legacy, original.models[0].id],
        )?;
        c.execute(
            "UPDATE site_models SET custom_request=?1 WHERE id=?2",
            rusqlite::params![custom, original.models[1].id],
        )?;
        Ok(())
    })
    .unwrap();
    let expanded = store.get(&id).unwrap();
    assert_eq!(expanded.policy.version, 2);
    assert_eq!(
        expanded
            .policy
            .parameters
            .iter()
            .find(|p| p.key == "input")
            .unwrap()
            .value,
        "legacy prompt"
    );
    assert_eq!(
        expanded
            .policy
            .parameters
            .iter()
            .find(|p| p.key == "max_output_tokens")
            .unwrap()
            .value,
        "24"
    );
    assert_eq!(
        expanded
            .policy
            .parameters
            .iter()
            .find(|p| p.key == "reasoning")
            .unwrap()
            .kind,
        ParameterType::Object
    );
    assert_eq!(
        expanded
            .policy
            .parameters
            .iter()
            .find(|p| p.key == "reasoning.budget")
            .unwrap()
            .value,
        "10"
    );
    assert_eq!(
        expanded
            .policy
            .parameters
            .iter()
            .find(|p| p.key == "reasoning.effort")
            .unwrap()
            .value,
        "high"
    );
    let metadata = expanded
        .policy
        .parameters
        .iter()
        .find(|p| p.key == "metadata")
        .unwrap();
    assert_eq!(metadata.kind, ParameterType::Json);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&metadata.value).unwrap(),
        json!({"literal.key":1})
    );
    assert_eq!(
        expanded.models[1]
            .custom_request
            .as_ref()
            .unwrap()
            .parameters,
        vec![Parameter::model()]
    );
    let changed = store
        .save_basic(
            &id,
            &SiteDraft {
                name: expanded.name.clone(),
                protocol: Protocol::Anthropic,
                base_url: expanded.base_url.clone(),
                credential_action: CredentialAction::Keep,
                secret: String::new(),
            },
        )
        .unwrap();
    assert_eq!(changed.policy, expanded.policy);
    assert_eq!(changed.models, expanded.models);
    drop(store);
    drop(db);
    let (store, _) = fixture(tmp.path());
    assert_eq!(store.get(&id).unwrap().policy, expanded.policy);
    assert_eq!(store.get(&id).unwrap().models, expanded.models);
}
