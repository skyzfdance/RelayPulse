use super::{
    health::*,
    records::{CompletedRecord, RecordContext},
    sqlite::*,
};
use crate::{
    modules::{
        configuration, notifications,
        sites::{self, CreateSite, Protocol, SiteStore},
    },
    shared::{
        model_client::{
            test_support::request, CheckResult, ResponseKind, ResponseMetadata, TokenUsage,
        },
        persistence::{test_support::environment, Database, Migration},
    },
};
use std::sync::Arc;

const SCHEMAS: [Migration; 4] = [
    configuration::SCHEMA,
    notifications::SCHEMA,
    sites::SCHEMA,
    SCHEMA,
];

fn site(store: &SiteStore) -> String {
    store
        .create(
            &CreateSite {
                name: "original".into(),
                protocol: Protocol::CustomHttp,
                base_url: "https://example.test".into(),
                secret: "private-key".into(),
                enabled: false,
            },
            &["remote-name".into()],
        )
        .unwrap()
}

fn completed(store: &SiteStore, id: &str, finished: u64) -> CompletedRecord {
    let site = store.get(id).unwrap();
    let mut input = request(&site.base_url, site.protocol);
    input.target.secret = "private-key".to_owned().into();
    let context = RecordContext::new(&site, &site.models[0], &input, false, true).unwrap();
    CompletedRecord::new(
        context,
        CheckResult {
            success: true,
            status_code: Some(200),
            first_token_ms: None,
            duration_ms: 2,
            failure: None,
            metadata: ResponseMetadata {
                request_id: Some("provider-request".into()),
                kind: Some(ResponseKind::Other),
                usage: TokenUsage {
                    input_tokens: Some(0),
                    output_tokens: Some(7),
                    total_tokens: None,
                },
                received_bytes: 17,
                body_complete: true,
            },
        },
        "private-key",
        finished,
    )
}

#[test]
fn history_state_and_metadata_commit_together_and_delete_cascades() {
    let tmp = tempfile::tempdir().unwrap();
    let db = Arc::new(
        Database::open(
            environment(tmp.path(), "com.relaypulse.app.dev").paths(),
            &SCHEMAS,
        )
        .unwrap(),
    );
    let config = SiteStore::new(db.clone());
    let storage = MonitoringStore::new(db.clone());
    let id = site(&config);
    let record = completed(&config, &id, 1_000_000_000);
    let model = record.context.model_id.clone();
    let state = HealthState {
        health: Health::Healthy,
        duration_ms: Some(2),
        finished_at_ms: record.finished_at_ms,
        ..Default::default()
    };
    db.with_connection(|c| c.execute_batch("CREATE TRIGGER break_state BEFORE INSERT ON probe_states BEGIN SELECT RAISE(ABORT,'test'); END;")).unwrap();
    assert!(storage.complete(&record, &state, 1).is_err());
    assert!(storage.records().retained_ids().unwrap().is_empty());
    assert!(storage
        .states(std::slice::from_ref(&model))
        .unwrap()
        .is_empty());
    db.with_connection(|c| c.execute_batch("DROP TRIGGER break_state"))
        .unwrap();
    storage.complete(&record, &state, 1).unwrap();
    assert_eq!(
        storage.states(std::slice::from_ref(&model)).unwrap()[&model],
        state
    );
    db.with_connection(|c| {
        let actual = c.query_row("SELECT site_name,model_name,result_kind,input_tokens,output_tokens,total_tokens,body_complete FROM probe_records",[],|r| {
            Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,Option<i64>>(3)?,r.get::<_,Option<i64>>(4)?,r.get::<_,Option<i64>>(5)?,r.get::<_,bool>(6)?))
        })?;
        assert_eq!(actual,("original".into(),"remote-name".into(),"succeeded".into(),Some(0),Some(7),None,true));
        Ok(())
    }).unwrap();
    // 重复记录 ID 必须拒绝，不能推进当前状态。
    let later = HealthState {
        finished_at_ms: state.finished_at_ms + 2 * 86_400_000,
        ..state.clone()
    };
    assert!(storage.complete(&record, &later, 1).is_err());
    assert_eq!(
        storage.states(std::slice::from_ref(&model)).unwrap()[&model],
        state
    );
    storage
        .complete(&completed(&config, &id, later.finished_at_ms), &later, 1)
        .unwrap();
    assert_eq!(storage.records().retained_ids().unwrap().len(), 1);
    drop(config);
    drop(storage);
    drop(db);
    let db = Arc::new(
        Database::open(
            environment(tmp.path(), "com.relaypulse.app.dev").paths(),
            &SCHEMAS,
        )
        .unwrap(),
    );
    let storage = MonitoringStore::new(db.clone());
    let config = SiteStore::new(db.clone());
    db.with_connection(|c| {
        assert_eq!(
            c.query_row("SELECT request_id FROM probe_records", [], |r| r
                .get::<_, String>(0))?,
            "provider-request"
        );
        assert_eq!(
            c.query_row("SELECT output_tokens FROM probe_records", [], |r| r
                .get::<_, i64>(0))?,
            7
        );
        Ok(())
    })
    .unwrap();
    assert_eq!(
        storage.states(std::slice::from_ref(&model)).unwrap()[&model],
        later
    );
    config.delete(&id).unwrap();
    assert!(storage
        .states(std::slice::from_ref(&model))
        .unwrap()
        .is_empty());
    assert!(storage.records().retained_ids().unwrap().is_empty());
}

#[tokio::test]
async fn aborted_waiter_does_not_release_a_running_storage_worker_before_shutdown_drain() {
    let tmp = tempfile::tempdir().unwrap();
    let db = Arc::new(
        Database::open(
            environment(tmp.path(), "com.relaypulse.app.dev").paths(),
            &SCHEMAS,
        )
        .unwrap(),
    );
    let storage = MonitoringStore::new(db);
    let (started, ready) = tokio::sync::oneshot::channel();
    let (release, wait) = std::sync::mpsc::channel();
    let active = storage.clone();
    let task = tokio::spawn(async move {
        active
            .blocking(move || {
                started.send(()).unwrap();
                wait.recv().unwrap();
            })
            .await
    });
    ready.await.unwrap();
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(20), storage.drained())
            .await
            .is_err()
    );
    release.send(()).unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(1), storage.drained())
        .await
        .unwrap();
}

#[test]
fn complete_initial_schema_preserves_records_and_settings_on_reopen() {
    let tmp = tempfile::tempdir().unwrap();
    let env = environment(tmp.path(), "com.relaypulse.app.dev");
    let db = Arc::new(Database::open(env.paths(), &SCHEMAS).unwrap());
    let config = SiteStore::new(db.clone());
    let id = site(&config);
    let record = completed(&config, &id, 1_000_000_000);
    let storage = MonitoringStore::new(db.clone());
    let state = HealthState {
        health: Health::Healthy,
        duration_ms: Some(2),
        finished_at_ms: record.finished_at_ms,
        ..Default::default()
    };
    storage.complete(&record, &state, 30).unwrap();
    db.with_connection(|c| {
        c.execute("UPDATE general_settings SET history_retention_days=77", [])?;
        c.execute("UPDATE notification_settings SET enabled=1", [])?;
        Ok(())
    })
    .unwrap();
    drop(storage);
    drop(config);
    drop(db);
    for _ in 0..2 {
        let db = Database::open(env.paths(), &SCHEMAS).unwrap();
        db.with_connection(|c| {
            let existing: (String, i64, Option<String>, Option<i64>) = c.query_row(
                "SELECT id,duration_ms,site_name,input_tokens FROM probe_records",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )?;
            assert_eq!(
                existing,
                (
                    record.context.id.clone(),
                    2,
                    Some("original".into()),
                    Some(0)
                )
            );
            assert_eq!(
                c.query_row(
                    "SELECT history_retention_days FROM general_settings",
                    [],
                    |r| r.get::<_, i64>(0)
                )?,
                77
            );
            assert!(
                c.query_row("SELECT enabled FROM notification_settings", [], |r| r
                    .get::<_, bool>(0))?
            );
            assert_eq!(
                c.query_row("SELECT COUNT(*) FROM schema_migrations", [], |r| r
                    .get::<_, i64>(0))?,
                4
            );
            Ok(())
        })
        .unwrap();
    }
}

#[test]
fn retention_only_removes_probe_history_and_keeps_current_state_and_other_business_data() {
    let tmp = tempfile::tempdir().unwrap();
    let env = environment(tmp.path(), "com.relaypulse.app.dev");
    let db = Arc::new(Database::open(env.paths(), &SCHEMAS).unwrap());
    let config = SiteStore::new(db.clone());
    let id = site(&config);
    let record = completed(&config, &id, 1_000_000_000);
    let storage = MonitoringStore::new(db.clone());
    let state = HealthState {
        health: Health::Error,
        failures: 3,
        finished_at_ms: record.finished_at_ms,
        duration_ms: Some(2),
        ..Default::default()
    };
    storage.complete(&record, &state, 30).unwrap();
    db.with_connection(|c|c.execute_batch("CREATE TABLE iq_history_sentinel (id TEXT PRIMARY KEY); INSERT INTO iq_history_sentinel VALUES ('keep');")).unwrap();
    storage.records().prune(record.finished_at_ms + 1).unwrap();
    assert!(storage.records().retained_ids().unwrap().is_empty());
    assert_eq!(
        storage
            .states(std::slice::from_ref(&record.context.model_id))
            .unwrap()[&record.context.model_id],
        state
    );
    db.with_connection(|c| {
        assert_eq!(
            c.query_row("SELECT id FROM iq_history_sentinel", [], |r| r
                .get::<_, String>(0))?,
            "keep"
        );
        assert_eq!(
            c.query_row("SELECT COUNT(*) FROM sites", [], |r| r.get::<_, i64>(0))?,
            1
        );
        Ok(())
    })
    .unwrap();
    drop(config);
    drop(storage);
    drop(db);
    let db = Arc::new(Database::open(env.paths(), &SCHEMAS).unwrap());
    assert_eq!(
        MonitoringStore::new(db)
            .states(std::slice::from_ref(&record.context.model_id))
            .unwrap()[&record.context.model_id],
        state
    );
}
