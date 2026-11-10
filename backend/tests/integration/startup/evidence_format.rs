use crate::{
    shared::persistence::test_support::environment,
    startup::{initialize, StartupState},
};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

fn files(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut result = BTreeMap::new();
    for entry in fs::read_dir(root).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            result.extend(files(&path));
        } else {
            result.insert(path.clone(), fs::read(path).unwrap());
        }
    }
    result
}

#[test]
fn unknown_iq_and_probe_formats_are_refused_before_any_startup_write() {
    for (path, body) in [
        (
            "iq-test/runs/run/entries/entry/meta.1.json",
            r#"{"formatVersion":999}"#,
        ),
        (
            "probe-records/record/response.jsonl",
            "{\"event\":\"record_created\",\"formatVersion\":999}\n",
        ),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let env = environment(temp.path(), "com.relaypulse.app.dev");
        let root = env.paths().data_root().to_owned();
        let (state, diagnostics) = initialize(Ok(env));
        assert!(matches!(state, StartupState::Ready(_)));
        drop(state);
        drop(diagnostics);
        let file = root.join(path);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(file, body).unwrap();
        let before = files(&root);
        for _ in 0..2 {
            let (state, diagnostics) =
                initialize(Ok(environment(temp.path(), "com.relaypulse.app.dev")));
            assert!(matches!(state, StartupState::Blocked(_)));
            assert!(diagnostics.0.is_none());
            assert_eq!(files(&root), before);
        }
    }
}

#[test]
fn startup_accepts_all_replayable_legacy_probe_envelopes_without_rewriting_them() {
    for header in [
        "{\"event\":\"record_created\",\"formatVersion\":2}\n",
        "{\"event\":\"record_created\"}\n",
        "",
    ] {
        let temp = tempfile::tempdir().unwrap();
        let env = environment(temp.path(), "com.relaypulse.app.dev");
        let root = env.paths().data_root().to_owned();
        let (state, diagnostics) = initialize(Ok(env));
        assert!(matches!(state, StartupState::Ready(_)));
        drop((state, diagnostics));
        let file = root.join("probe-records/record/response.jsonl");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        let bytes = format!("{header}{{\"event\":\"request_started\",\"method\":\"POST\"}}\n{{\"event\":\"request_finished\",\"bodyComplete\":true}}\n");
        fs::write(&file, &bytes).unwrap();
        let (state, diagnostics) =
            initialize(Ok(environment(temp.path(), "com.relaypulse.app.dev")));
        assert!(matches!(state, StartupState::Ready(_)));
        drop((state, diagnostics));
        assert_eq!(fs::read(&file).unwrap(), bytes.as_bytes());
    }
}

#[test]
fn startup_preserves_interrupted_partial_tail_but_refuses_future_version() {
    for (bytes, ready) in [
        (
            "{\"event\":\"record_created\",\"formatVersion\":2}\n{\"event\":\"json\",\"value\":",
            true,
        ),
        (
            "{\"event\":\"record_created\",\"formatVersion\":999}\n{",
            false,
        ),
        (
            "{\"event\":\"record_created\",\"formatVersion\":2}\n{broken}\n",
            true,
        ),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let env = environment(temp.path(), "com.relaypulse.app.dev");
        let root = env.paths().data_root().to_owned();
        let (state, diagnostics) = initialize(Ok(env));
        drop((state, diagnostics));
        let file = root.join("probe-records/record.partial/response.jsonl");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, bytes).unwrap();
        assert!(
            crate::modules::monitoring::verify_capture_history(&root.join("probe-records"))
                .is_err()
        );
        let (state, diagnostics) =
            initialize(Ok(environment(temp.path(), "com.relaypulse.app.dev")));
        assert_eq!(matches!(state, StartupState::Ready(_)), ready);
        drop((state, diagnostics));
        assert_eq!(fs::read(&file).unwrap(), bytes.as_bytes());
    }
}
