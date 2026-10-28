use super::*;
use crate::shared::persistence::test_support::environment;
use serde_json::{json, Value};
use std::sync::Arc;

fn diagnostics(temp: &tempfile::TempDir, identity: &str) -> Diagnostics {
    Diagnostics::new(&environment(temp.path(), identity))
}
const DEV: &str = "com.relaypulse.app.dev";
const PROD: &str = "com.relaypulse.app";

#[test]
fn write_statistics_clear_and_resume_preserve_other_data_and_environment() {
    let temp = tempfile::tempdir().unwrap();
    let dev = diagnostics(&temp, DEV);
    let prod = diagnostics(&temp, PROD);
    assert_eq!(dev.statistics().unwrap(), LogStatistics::default());
    assert_eq!(dev.clear().unwrap(), LogStatistics::default());
    dev.record("startup", Outcome::Started).unwrap();
    prod.record("startup", Outcome::Succeeded).unwrap();
    let logs = dev.log_directory().unwrap();
    fs::write(logs.join("relaypulse.log"), b"legacy").unwrap();
    fs::write(logs.join("relaypulse.log.1"), b"legacy rotated").unwrap();
    fs::write(logs.join("unrelated.txt"), b"keep").unwrap();
    fs::create_dir(logs.join("nested")).unwrap();
    fs::write(dev.root.join("relaypulse.sqlite3"), b"database sentinel").unwrap();
    fs::write(dev.root.join("credentials.json"), b"credential sentinel").unwrap();
    let statistics = dev.statistics().unwrap();
    assert_eq!(statistics.file_count, 3);
    assert_eq!(
        statistics.total_bytes,
        fs::metadata(logs.join(files::EVENTS)).unwrap().len() + 6 + 14
    );
    assert_eq!(
        serde_json::to_value(&statistics)
            .unwrap()
            .as_object()
            .unwrap()
            .len(),
        2
    );
    let prod_before = prod.package().unwrap();
    assert_eq!(dev.clear().unwrap(), LogStatistics::default());
    assert_eq!(fs::read(logs.join("unrelated.txt")).unwrap(), b"keep");
    assert!(logs.join("nested").is_dir());
    assert_eq!(
        fs::read(dev.root.join("relaypulse.sqlite3")).unwrap(),
        b"database sentinel"
    );
    assert_eq!(
        fs::read(dev.root.join("credentials.json")).unwrap(),
        b"credential sentinel"
    );
    assert!(prod.statistics().unwrap().total_bytes > 0);
    let before: Value = serde_json::from_slice(&prod_before).unwrap();
    let after: Value = serde_json::from_slice(&prod.package().unwrap()).unwrap();
    assert_eq!(before["events"], after["events"]);
    dev.record("configuration_save", Outcome::Succeeded)
        .unwrap();
    assert_eq!(dev.statistics().unwrap().file_count, 1);
}

#[test]
fn package_contains_only_typed_events_and_explicit_unavailable_probe_summary() {
    let temp = tempfile::tempdir().unwrap();
    let diagnostics = diagnostics(&temp, DEV);
    diagnostics
        .record("startup", Outcome::Failed(Failure::Persistence))
        .unwrap();
    let directory = diagnostics.log_directory().unwrap();
    let secret = "credential=SECRET token=TOKEN requestBody=PRIVATE responseBody=RESPONSE";
    fs::write(directory.join("relaypulse.log"), secret).unwrap();
    let bytes = diagnostics.package().unwrap();
    let package: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(package["appVersion"], env!("CARGO_PKG_VERSION"));
    assert_eq!(package["environment"], "development");
    assert_eq!(
        package["startup"],
        json!({"status":"failed", "error":"persistence"})
    );
    assert_eq!(package["logStatistics"]["fileCount"], 2);
    assert_eq!(
        package["probeSummary"],
        json!({"available":false,"reason":"probe_summary_not_implemented"})
    );
    let text = String::from_utf8(bytes).unwrap();
    for secret in [
        "SECRET",
        "TOKEN",
        "PRIVATE",
        "RESPONSE",
        "requestBody",
        "responseBody",
        "credential",
    ] {
        assert!(!text.contains(secret));
    }
    // 含额外字段或损坏的事件不允许透传到诊断包，也不伪装成成功导出。
    fs::write(directory.join(files::EVENTS), b"{\"timestampMs\":1,\"operation\":\"startup\",\"outcome\":{\"status\":\"started\"},\"token\":\"SECRET\"}\n").unwrap();
    assert!(matches!(
        diagnostics.package(),
        Err(DiagnosticsError::InvalidData(_))
    ));
}

#[test]
fn export_is_complete_and_failures_preserve_destination() {
    let temp = tempfile::tempdir().unwrap();
    let diagnostics = diagnostics(&temp, DEV);
    diagnostics.record("startup", Outcome::Succeeded).unwrap();
    let destination = temp.path().join("export.json");
    diagnostics.export(&destination).unwrap();
    let before = fs::read(&destination).unwrap();
    let package: Value = serde_json::from_slice(&before).unwrap();
    assert_eq!(package["events"].as_array().unwrap().len(), 1);
    assert!(diagnostics
        .export(&diagnostics.root.join("relaypulse.sqlite3"))
        .is_err());
    assert!(diagnostics
        .export(&temp.path().join("missing/export.json"))
        .is_err());
    fs::write(
        diagnostics.log_directory().unwrap().join(files::EVENTS),
        b"broken",
    )
    .unwrap();
    assert!(diagnostics.export(&destination).is_err());
    assert_eq!(fs::read(destination).unwrap(), before);
    assert!(!fs::read_dir(temp.path()).unwrap().any(|entry| entry
        .unwrap()
        .file_name()
        .to_string_lossy()
        .ends_with(".tmp")));
}

#[test]
fn invalid_paths_fail_without_touching_other_files() {
    let temp = tempfile::tempdir().unwrap();
    let diagnostics = diagnostics(&temp, DEV);
    fs::create_dir_all(&diagnostics.root).unwrap();
    fs::write(diagnostics.root.join("logs"), b"keep").unwrap();
    assert!(diagnostics.statistics().is_err());
    assert!(diagnostics.clear().is_err());
    assert!(diagnostics.record("startup", Outcome::Started).is_err());
    assert_eq!(fs::read(diagnostics.root.join("logs")).unwrap(), b"keep");
}

#[cfg(unix)]
#[test]
fn symlinks_and_hardlinks_cannot_escape_log_boundary() {
    use std::os::unix::fs::symlink;
    let temp = tempfile::tempdir().unwrap();
    let diagnostics = diagnostics(&temp, DEV);
    let directory = diagnostics.log_directory().unwrap();
    let other = temp.path().join("outside");
    fs::write(&other, b"keep").unwrap();
    for hardlink in [false, true] {
        let log = directory.join(files::EVENTS);
        if hardlink {
            fs::hard_link(&other, &log).unwrap();
        } else {
            symlink(&other, &log).unwrap();
        }
        assert!(diagnostics.record("startup", Outcome::Started).is_err());
        assert!(diagnostics.statistics().is_err());
        assert!(diagnostics.clear().is_err());
        assert!(diagnostics.package().is_err());
        fs::remove_file(log).unwrap();
    }
    fs::remove_dir(&directory).unwrap();
    symlink(temp.path(), &directory).unwrap();
    assert!(diagnostics.clear().is_err());
    assert!(diagnostics.record("startup", Outcome::Started).is_err());
    assert_eq!(fs::read(other).unwrap(), b"keep");
}

#[test]
fn concurrent_write_statistics_clear_and_package_are_consistent() {
    let temp = tempfile::tempdir().unwrap();
    let diagnostics = Arc::new(diagnostics(&temp, DEV));
    std::thread::scope(|scope| {
        for worker in 0..4 {
            let diagnostics = diagnostics.clone();
            scope.spawn(move || {
                for _ in 0..50 {
                    match worker {
                        0 | 1 => diagnostics
                            .record("configuration_save", Outcome::Succeeded)
                            .unwrap(),
                        2 => assert_eq!(diagnostics.clear().unwrap(), LogStatistics::default()),
                        _ => {
                            let package: Value =
                                serde_json::from_slice(&diagnostics.package().unwrap()).unwrap();
                            let count = package["events"].as_array().unwrap().len();
                            assert_eq!(package["logStatistics"]["fileCount"], u64::from(count > 0));
                            if count == 0 {
                                assert_eq!(package["logStatistics"]["totalBytes"], 0);
                            }
                            assert!(diagnostics.statistics().unwrap().file_count <= 1);
                        }
                    }
                }
            });
        }
    });
    diagnostics.record("startup", Outcome::Succeeded).unwrap();
    assert!(diagnostics.statistics().unwrap().total_bytes > 0);
}

#[cfg(unix)]
#[test]
fn logs_and_export_use_private_file_creation_without_output_sessions() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let diagnostics = diagnostics(&temp, DEV);
    diagnostics.record("startup", Outcome::Started).unwrap();
    let directory = diagnostics.log_directory().unwrap();
    let destination = temp.path().join("export.json");
    diagnostics.export(&destination).unwrap();
    for path in [&diagnostics.root, &directory] {
        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o700
        );
    }
    for path in [directory.join(files::EVENTS), destination.clone()] {
        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    assert_eq!(diagnostics.statistics().unwrap().file_count, 1);
    assert!(!diagnostics.root.join("probe-records").exists());
    assert!(!fs::read_dir(&directory).unwrap().any(|entry| entry
        .unwrap()
        .file_name()
        .to_string_lossy()
        .ends_with(".partial")));
    let original = fs::read(&destination).unwrap();
    diagnostics.clear().unwrap();
    diagnostics
        .record("configuration_save", Outcome::Succeeded)
        .unwrap();
    let next: Value = serde_json::from_slice(&diagnostics.package().unwrap()).unwrap();
    assert_eq!(next["events"].as_array().unwrap().len(), 1);
    assert_eq!(next["events"][0]["operation"], "configuration_save");
    assert_eq!(fs::read(destination).unwrap(), original);
}
