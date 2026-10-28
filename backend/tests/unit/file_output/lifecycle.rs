use super::*;
use std::{fs, time::Duration};

#[tokio::test]
async fn incremental_binary_output_publishes_only_explicitly_and_survives_reopen() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("output");
    let store = OutputStore::new(root.clone());
    let session = store.start("record-1".into()).await.unwrap();
    session.append("body.bin", b"first\0\xff").await.unwrap();
    session.append("body.bin", b"second").await.unwrap();
    session.append("events.jsonl", b"{}\n").await.unwrap();
    assert_eq!(
        fs::read(root.join("record-1.partial/body.bin")).unwrap(),
        b"first\0\xffsecond"
    );
    assert!(!root.join("record-1").exists());
    assert!(store.entries().await.unwrap().is_empty());
    assert!(store.start("record-1".into()).await.is_err());
    session.publish().await.unwrap();
    assert!(session.append("body.bin", b"late").await.is_err());
    assert!(session.publish().await.is_err());
    drop(session);
    let reopened = OutputStore::new(root.clone());
    assert!(reopened.start("record-1".into()).await.is_err());
    let entries = reopened.entries().await.unwrap();
    assert_eq!(entries.len(), 1);
    assert!(entries[0].published);
    assert_eq!(
        fs::read(root.join("record-1/body.bin")).unwrap(),
        b"first\0\xffsecond"
    );
    reopened
        .remove(entries.into_iter().next().unwrap())
        .await
        .unwrap();
    assert!(reopened.entries().await.unwrap().is_empty());
}

#[tokio::test]
async fn concurrent_first_sessions_are_isolated_and_dropped_sessions_remain_partial() {
    let temp = tempfile::tempdir().unwrap();
    let store = OutputStore::new(temp.path().join("dev"));
    let production = OutputStore::new(temp.path().join("production"));
    let (a, b) = tokio::join!(store.start("a".into()), store.start("b".into()));
    let (a, b) = (a.unwrap(), b.unwrap());
    let p = production.start("a".into()).await.unwrap();
    a.append("data", b"a").await.unwrap();
    b.append("data", b"b").await.unwrap();
    p.append("data", b"production").await.unwrap();
    store
        .remove(OutputEntry {
            id: "a".into(),
            published: false,
            modified_ms: 0,
        })
        .await
        .unwrap();
    assert_eq!(
        fs::read(temp.path().join("dev/a.partial/data")).unwrap(),
        b"a"
    );
    drop(a);
    drop(b);
    let entries = store.entries().await.unwrap();
    assert_eq!(entries.len(), 2);
    for entry in entries {
        assert!(!entry.published);
        store.remove(entry).await.unwrap();
    }
    assert_eq!(
        fs::read(temp.path().join("production/a.partial/data")).unwrap(),
        b"production"
    );
    drop(p);
}

#[tokio::test]
async fn clear_removes_output_packages_without_touching_unmanaged_files() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("output");
    let store = OutputStore::new(root.clone());
    let published = store.start("published-record".into()).await.unwrap();
    published.append("events.jsonl", b"{}\n").await.unwrap();
    published.publish().await.unwrap();
    drop(published);
    let partial = store.start("unfinished-record".into()).await.unwrap();
    partial.append("events.jsonl", b"{}\n").await.unwrap();
    drop(partial);
    // Finder 浏览目录后留下的元数据不属于输出包，不能阻塞清理或被误删。
    let metadata = root.join(".DS_Store");
    fs::write(&metadata, b"Finder metadata").unwrap();
    let unrelated = root.join("unrelated.directory");
    fs::create_dir(&unrelated).unwrap();
    fs::write(unrelated.join("keep.txt"), b"unmanaged").unwrap();
    let worker = store.clone();
    tokio::task::spawn_blocking(move || worker.remove_all())
        .await
        .unwrap()
        .unwrap();
    assert!(!root.join("published-record").exists());
    assert!(!root.join("unfinished-record.partial").exists());
    assert_eq!(fs::read(&metadata).unwrap(), b"Finder metadata");
    assert_eq!(fs::read(unrelated.join("keep.txt")).unwrap(), b"unmanaged");
    assert!(store.entries().await.unwrap().is_empty());
}

#[tokio::test]
async fn rejected_paths_and_write_failures_do_not_overwrite_existing_files() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("output");
    let store = OutputStore::new(root.clone());
    for name in ["", "../escape", "/absolute", "a/b", "a.partial"] {
        assert!(store.start(name.into()).await.is_err());
    }
    let session = store.start("record".into()).await.unwrap();
    for name in ["../escape", "/absolute", "a/b", ".hidden"] {
        assert!(session.append(name, b"bad").await.is_err());
    }
    let occupied = root.join("record.partial/body");
    fs::write(&occupied, b"original").unwrap();
    assert!(session.append("body", b"replacement").await.is_err());
    assert!(session.append("other", b"late").await.is_err());
    assert!(session.publish().await.is_err());
    assert_eq!(fs::read(occupied).unwrap(), b"original");
    assert!(!root.join("record").exists());
}

#[tokio::test]
async fn shutdown_waits_for_sessions_and_workers_even_when_the_waiter_is_aborted() {
    let temp = tempfile::tempdir().unwrap();
    let store = OutputStore::new(temp.path().join("output"));
    let session = store.start("record".into()).await.unwrap();
    let (started, ready) = tokio::sync::oneshot::channel();
    let (release, blocked) = std::sync::mpsc::channel();
    let worker_store = store.clone();
    let task = tokio::spawn(async move {
        worker_store
            .run(move || {
                started.send(()).unwrap();
                blocked.recv().unwrap();
                Ok(())
            })
            .await
    });
    ready.await.unwrap();
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    store.close();
    assert!(store.start("new".into()).await.is_err());
    drop(session);
    assert!(
        tokio::time::timeout(Duration::from_millis(20), store.drained())
            .await
            .is_err()
    );
    release.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(2), store.drained())
        .await
        .unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn private_permissions_and_link_boundaries_are_enforced() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("output");
    let store = OutputStore::new(root.clone());
    let session = store.start("record".into()).await.unwrap();
    session.append("body", b"private").await.unwrap();
    for path in [&root, &root.join("record.partial")] {
        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o700
        );
    }
    assert_eq!(
        fs::metadata(root.join("record.partial/body"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    drop(session);
    let external = temp.path().join("external");
    fs::write(&external, b"untouched").unwrap();
    let link = root.join("record.partial/link");
    symlink(&external, &link).unwrap();
    assert!(store.remove_all().is_err());
    assert!(store
        .remove(store.entries().await.unwrap().remove(0))
        .await
        .is_err());
    assert!(root.join("record.partial/body").exists());
    fs::remove_file(&link).unwrap();
    fs::hard_link(&external, &link).unwrap();
    assert!(store.remove_all().is_err());
    assert!(store
        .remove(store.entries().await.unwrap().remove(0))
        .await
        .is_err());
    assert_eq!(fs::read(&external).unwrap(), b"untouched");
    let alias = temp.path().join("alias");
    symlink(&root, &alias).unwrap();
    assert!(OutputStore::new(alias)
        .start("escape".into())
        .await
        .is_err());
}
