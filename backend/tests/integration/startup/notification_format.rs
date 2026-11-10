use crate::shared::persistence::{test_support::environment, Database};
use crate::startup::{initialize, StartupError, StartupState};
#[test]
fn invalid_notification_storage_blocks_startup_without_repair() {
    for sql in [
        "DELETE FROM notification_settings",
        "PRAGMA ignore_check_constraints=ON; UPDATE notification_settings SET enabled=2",
        "INSERT INTO notification_rules VALUES ('probe.model_failed',1,'{}')",
    ] {
        let temp = tempfile::tempdir().unwrap();
        let env = environment(temp.path(), "com.relaypulse.app.dev");
        let (state, _) = initialize(Ok(env));
        assert!(matches!(state, StartupState::Ready(_)));
        drop(state);
        let baseline = [
            crate::modules::configuration::SCHEMA,
            crate::modules::notifications::SCHEMA,
            crate::modules::sites::SCHEMA,
            crate::modules::monitoring::SCHEMA,
            crate::modules::operation_logs::SCHEMA,
            crate::modules::iq_test::SCHEMA,
        ];
        let db = Database::open(
            environment(temp.path(), "com.relaypulse.app.dev").paths(),
            &baseline,
        )
        .unwrap();
        db.with_connection(|c| c.execute_batch(sql)).unwrap();
        drop(db);
        assert!(matches!(
            initialize(Ok(environment(temp.path(), "com.relaypulse.app.dev"))).0,
            StartupState::Blocked(StartupError::Notifications(_))
        ));
    }
}
