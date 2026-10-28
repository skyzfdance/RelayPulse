use std::{fs, path::Path};

use super::{AppEnvironment, EnvironmentContext, EnvironmentError};

const DEV: &str = "com.relaypulse.app.dev";
const PROD: &str = "com.relaypulse.app";

fn resolve(identifier: &str, base: &Path) -> Result<EnvironmentContext, EnvironmentError> {
    EnvironmentContext::resolve(identifier, base, &base.join(identifier))
}

#[test]
fn first_run_resolves_distinct_roots_without_creating_them() {
    let temp = tempfile::tempdir().unwrap();
    let base = temp.path().join("尚未创建的 data directory");
    let dev = resolve(DEV, &base).unwrap();
    let prod = resolve(PROD, &base).unwrap();

    assert_eq!(dev.environment(), AppEnvironment::Development);
    assert_eq!(prod.environment(), AppEnvironment::Production);
    let physical_base = fs::canonicalize(temp.path())
        .unwrap()
        .join(base.file_name().unwrap());
    assert_eq!(dev.paths().data_root(), physical_base.join(DEV));
    assert_eq!(prod.paths().data_root(), physical_base.join(PROD));
    assert_ne!(dev.paths().data_root(), prod.paths().data_root());
    assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 0);
}

#[test]
fn rejects_unknown_identities_instead_of_falling_back() {
    let temp = tempfile::tempdir().unwrap();
    for identifier in [
        "",
        "dev",
        "com.relaypulse.app.production",
        "com.relaypulse.app.dev ",
    ] {
        assert!(matches!(
            resolve(identifier, temp.path()),
            Err(EnvironmentError::UnknownApplicationIdentity(_))
        ));
    }
    assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 0);
}

#[test]
fn rejects_cross_environment_path_pairs() {
    let temp = tempfile::tempdir().unwrap();
    for (identifier, other) in [(DEV, PROD), (PROD, DEV)] {
        assert!(matches!(
            EnvironmentContext::resolve(identifier, temp.path(), &temp.path().join(other)),
            Err(EnvironmentError::DataDirectoryMismatch),
        ));
    }
    assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 0);
}

#[test]
fn rejects_relative_paths_and_parent_traversal() {
    assert!(matches!(
        resolve(DEV, Path::new("relative")),
        Err(EnvironmentError::DataDirectoryMismatch)
    ));
    let temp = tempfile::tempdir().unwrap();
    let base = temp.path().join("child/..");
    assert!(matches!(
        resolve(DEV, &base),
        Err(EnvironmentError::DataDirectoryMismatch)
    ));
    let escaped_app = temp.path().join(PROD).join("..").join(DEV);
    assert!(matches!(
        EnvironmentContext::resolve(DEV, temp.path(), &escaped_app),
        Err(EnvironmentError::DataDirectoryMismatch)
    ));
}

#[test]
fn existing_environment_files_are_left_unchanged() {
    let temp = tempfile::tempdir().unwrap();
    for identifier in [DEV, PROD] {
        let root = temp.path().join(identifier);
        fs::create_dir(&root).unwrap();
        // 无效数据库与凭据作为哨兵：解析路径不能尝试打开、修复或迁移它们。
        fs::write(
            root.join("relaypulse.sqlite3"),
            format!("untouched {identifier}"),
        )
        .unwrap();
        fs::write(root.join("credentials.json"), "not real credentials").unwrap();
    }
    for identifier in [DEV, PROD] {
        let context = resolve(identifier, temp.path()).unwrap();
        assert_eq!(
            context.paths().data_root(),
            fs::canonicalize(temp.path().join(identifier)).unwrap()
        );
    }
    for identifier in [DEV, PROD] {
        let root = temp.path().join(identifier);
        assert_eq!(fs::read_dir(&root).unwrap().count(), 2);
        assert_eq!(
            fs::read_to_string(root.join("relaypulse.sqlite3")).unwrap(),
            format!("untouched {identifier}")
        );
        assert_eq!(
            fs::read_to_string(root.join("credentials.json")).unwrap(),
            "not real credentials"
        );
    }
}

#[test]
fn rejects_a_file_at_either_environment_root() {
    for identifier in [DEV, PROD] {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join(identifier);
        fs::write(&file, b"preserve this file").unwrap();
        assert!(matches!(
            resolve(DEV, temp.path()),
            Err(EnvironmentError::DataDirectoryConflict { .. })
        ));
        assert_eq!(fs::read(file).unwrap(), b"preserve this file");
    }
}

#[test]
fn a_file_in_the_parent_path_is_not_treated_as_a_missing_directory() {
    let temp = tempfile::tempdir().unwrap();
    let base = temp.path().join("occupied");
    fs::write(&base, b"preserve parent").unwrap();
    assert!(resolve(DEV, &base).is_err());
    assert_eq!(fs::read(base).unwrap(), b"preserve parent");
}

#[test]
fn configured_identities_match_the_runtime_policy() {
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
        AppEnvironment::from_identifier(dev["identifier"].as_str().unwrap()).unwrap(),
        AppEnvironment::Development
    );
    assert_eq!(
        AppEnvironment::from_identifier(production["identifier"].as_str().unwrap()).unwrap(),
        AppEnvironment::Production
    );
}

#[cfg(unix)]
mod unix {
    use std::os::unix::fs::symlink;

    use super::*;

    #[test]
    fn rejects_current_root_links_even_when_dangling() {
        for create_target in [true, false] {
            let temp = tempfile::tempdir().unwrap();
            let target = temp.path().join(PROD);
            if create_target {
                fs::create_dir(&target).unwrap();
            }
            symlink(&target, temp.path().join(DEV)).unwrap();
            assert!(matches!(
                resolve(DEV, temp.path()),
                Err(EnvironmentError::DataDirectoryConflict { .. })
            ));
            assert_eq!(target.exists(), create_target);
        }
    }

    #[test]
    fn rejects_other_environment_aliasing_or_containing_current_root() {
        for link_to_parent in [true, false] {
            let temp = tempfile::tempdir().unwrap();
            let current = temp.path().join(DEV);
            fs::create_dir(&current).unwrap();
            let target = if link_to_parent {
                temp.path()
            } else {
                &current
            };
            symlink(target, temp.path().join(PROD)).unwrap();
            assert!(matches!(
                resolve(DEV, temp.path()),
                Err(EnvironmentError::DataDirectoryConflict { .. })
            ));
        }
    }

    #[test]
    fn allows_a_shared_system_parent_link_and_normalizes_it() {
        let temp = tempfile::tempdir().unwrap();
        let actual = temp.path().join("actual");
        let alias = temp.path().join("alias");
        fs::create_dir(&actual).unwrap();
        symlink(&actual, &alias).unwrap();
        let context = resolve(DEV, &alias).unwrap();
        assert_eq!(
            context.paths().data_root(),
            fs::canonicalize(&actual).unwrap().join(DEV)
        );
        assert_eq!(fs::read_dir(actual).unwrap().count(), 0);
    }

    #[test]
    fn reports_a_broken_system_parent_link_instead_of_creating_directories() {
        let temp = tempfile::tempdir().unwrap();
        let missing = temp.path().join("missing");
        let alias = temp.path().join("alias");
        symlink(&missing, &alias).unwrap();
        assert!(matches!(
            resolve(DEV, &alias),
            Err(EnvironmentError::DataDirectoryInspectionFailed { .. })
        ));
        assert!(!missing.exists());
    }
}
