use super::*;

#[test]
fn exclusive_creation_and_reopened_append_preserve_bytes_and_recover_after_removal() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("output.data");
    let mut first = FileWriter::create_new(&path).unwrap();
    first.write_all(b"first\0\xff").unwrap();
    first.sync_all().unwrap();
    drop(first);
    assert!(FileWriter::create_new(&path).is_err());
    {
        let mut appended = FileWriter::open_append(&path).unwrap();
        appended.write_all(b"second\n").unwrap();
        appended.flush().unwrap();
    }
    assert_eq!(fs::read(&path).unwrap(), b"first\0\xffsecond\n");
    fs::remove_file(&path).unwrap();
    {
        let mut reopened = FileWriter::open_append(&path).unwrap();
        reopened.write_all(b"fresh").unwrap();
    }
    assert_eq!(fs::read(&path).unwrap(), b"fresh");
    assert!(FileWriter::create_new(&temp.path().join("missing/file")).is_err());
    assert!(!temp.path().join("missing").exists());
}

#[test]
fn directory_creation_is_explicit_concurrent_and_rejects_non_directories() {
    let temp = tempfile::tempdir().unwrap();
    let nested = temp.path().join("missing/nested");
    assert!(!directory(&nested, false).unwrap());
    assert!(directory(&nested, true).is_err()); // 单层创建不悄悄创建父目录。
    assert!(!nested.parent().unwrap().exists());
    std::thread::scope(|scope| {
        for _ in 0..4 {
            scope.spawn(|| ensure_directory_tree(&nested).unwrap());
        }
    });
    assert!(directory(&nested, false).unwrap());
    assert!(create_directory(&nested).is_err()); // 独占创建不复用现有目录。
    assert!(matches!(regular_file(&nested), Err(FileError::UnsafePath)));
    let missing = nested.join("missing");
    assert!(regular_file(&missing).unwrap().is_none());
    assert!(!missing.exists());
    let occupied = temp.path().join("occupied");
    fs::write(&occupied, b"keep").unwrap();
    assert!(directory(&occupied, true).is_err());
    assert!(ensure_directory_tree(&occupied.join("nested")).is_err());
    assert_eq!(fs::read(occupied).unwrap(), b"keep");
}

#[cfg(unix)]
#[test]
fn new_files_and_directories_are_private_but_existing_permissions_are_preserved() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("new");
    let nested = root.join("nested");
    ensure_directory_tree(&nested).unwrap();
    for directory in [&root, &nested] {
        assert_eq!(
            fs::metadata(directory).unwrap().permissions().mode() & 0o777,
            0o700
        );
    }
    let created = nested.join("created");
    let appended = nested.join("appended");
    drop(FileWriter::create_new(&created).unwrap());
    drop(FileWriter::open_append(&appended).unwrap());
    for path in [&created, &appended] {
        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    fs::set_permissions(&root, fs::Permissions::from_mode(0o750)).unwrap();
    fs::set_permissions(&appended, fs::Permissions::from_mode(0o640)).unwrap();
    ensure_directory_tree(&root).unwrap();
    drop(FileWriter::open_append(&appended).unwrap());
    assert_eq!(
        fs::metadata(root).unwrap().permissions().mode() & 0o777,
        0o750
    );
    assert_eq!(
        fs::metadata(appended).unwrap().permissions().mode() & 0o777,
        0o640
    );
}

#[cfg(unix)]
#[test]
fn links_and_invalid_targets_cannot_be_opened_for_writing() {
    use std::os::unix::fs::symlink;
    let temp = tempfile::tempdir().unwrap();
    let outside = temp.path().join("outside");
    fs::write(&outside, b"keep").unwrap();
    let link = temp.path().join("link");
    for hardlink in [false, true] {
        if hardlink {
            fs::hard_link(&outside, &link).unwrap();
        } else {
            symlink(&outside, &link).unwrap();
        }
        assert!(FileWriter::open_append(&link).is_err());
        assert!(FileWriter::create_new(&link).is_err());
        assert!(regular_file(&link).is_err());
        fs::remove_file(&link).unwrap();
    }
    let missing = temp.path().join("missing");
    symlink(&missing, &link).unwrap();
    assert!(FileWriter::open_append(&link).is_err());
    assert!(FileWriter::create_new(&link).is_err());
    assert!(!missing.exists());
    fs::remove_file(&link).unwrap();
    symlink(temp.path(), &link).unwrap();
    assert!(directory(&link, true).is_err());
    assert!(ensure_directory_tree(&link).is_err());
    assert!(FileWriter::open_append(&link.join("escape")).is_err());
    assert!(FileWriter::create_new(&link.join("escape")).is_err());
    assert!(!temp.path().join("escape").exists());
    assert!(FileWriter::open_append(temp.path()).is_err());
    assert_eq!(fs::read(outside).unwrap(), b"keep");
}
