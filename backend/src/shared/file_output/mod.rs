//! 同步文件基础能力与异步任务输出会话；不认识 HTTP、业务记录或保留期。
pub(crate) mod filesystem;
mod managed;
mod paths;
mod session;
#[cfg(test)]
#[path = "../../../tests/unit/file_output/lifecycle.rs"]
mod tests;

pub(crate) use session::OutputSession;
use std::{
    collections::HashSet,
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tokio::sync::watch;

#[derive(Debug, thiserror::Error)]
#[error("本地文件输出失败。")]
pub(crate) struct OutputError;
type Result<T> = std::result::Result<T, OutputError>;
impl From<std::io::Error> for OutputError {
    fn from(_: std::io::Error) -> Self {
        Self
    }
}

impl From<filesystem::FileError> for OutputError {
    fn from(_: filesystem::FileError) -> Self {
        Self
    }
}

#[derive(Clone)]
pub(crate) struct OutputStore(Arc<Inner>);
struct Inner {
    root: PathBuf,
    state: Mutex<State>,
    count: watch::Sender<usize>,
}
#[derive(Default)]
struct State {
    active: HashSet<String>,
    workers: usize,
    closing: bool,
}
struct SessionLease {
    store: OutputStore,
    id: String,
}
struct WorkerLease {
    store: OutputStore,
}
#[derive(Debug)]
pub(crate) struct OutputEntry {
    pub id: String,
    pub published: bool,
    pub modified_ms: u64,
}
impl OutputStore {
    /// root 的父目录由应用环境提供且已经存在；本能力只管理这个专用子目录。
    pub(crate) fn new(root: PathBuf) -> Self {
        Self(Arc::new(Inner {
            root,
            state: Mutex::new(State::default()),
            count: watch::channel(0).0,
        }))
    }
    pub(crate) async fn start(&self, id: String) -> Result<OutputSession> {
        paths::identifier(&id)?;
        let lease = {
            let mut state = self.0.state.lock().map_err(|_| OutputError)?;
            if state.closing || !state.active.insert(id.clone()) {
                return Err(OutputError);
            }
            self.notify(&state);
            Arc::new(SessionLease {
                store: self.clone(),
                id: id.clone(),
            })
        };
        let held = lease.clone();
        let store = self.clone();
        let path = self
            .run(move || {
                let _held = held;
                filesystem::directory(&store.0.root, true)?;
                let pending = store.0.root.join(format!("{id}.partial"));
                if store.0.root.join(&id).try_exists()? {
                    return Err(OutputError);
                }
                filesystem::create_directory(&pending)?;
                Ok(pending)
            })
            .await?;
        Ok(OutputSession::new(path, lease))
    }
    pub(crate) async fn entries(&self) -> Result<Vec<OutputEntry>> {
        let store = self.clone();
        self.run(move || {
            if !filesystem::directory(&store.0.root, false)? {
                return Ok(vec![]);
            }
            let state = store.0.state.lock().map_err(|_| OutputError)?;
            let mut entries = vec![];
            for entry in std::fs::read_dir(&store.0.root)? {
                let entry = entry?;
                let name = entry.file_name().into_string().map_err(|_| OutputError)?;
                let (id, published) = name
                    .strip_suffix(".partial")
                    .map_or((name.as_str(), true), |id| (id, false));
                if paths::identifier(id).is_err() || state.active.contains(id) {
                    continue;
                }
                filesystem::directory(&entry.path(), false)?;
                let modified_ms = entry
                    .metadata()?
                    .modified()?
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_err(|_| OutputError)?
                    .as_millis()
                    .min(u64::MAX as u128) as u64;
                entries.push(OutputEntry {
                    id: id.into(),
                    published,
                    modified_ms,
                });
            }
            Ok(entries)
        })
        .await
    }
    pub(crate) async fn remove(&self, entry: OutputEntry) -> Result<()> {
        let store = self.clone();
        self.run(move || {
            paths::identifier(&entry.id)?;
            let state = store.0.state.lock().map_err(|_| OutputError)?;
            if state.active.contains(&entry.id) {
                return Ok(());
            }
            filesystem::directory(&store.0.root, false)?;
            let name = if entry.published {
                entry.id
            } else {
                format!("{}.partial", entry.id)
            };
            let path = store.0.root.join(name);
            if !filesystem::directory(&path, false)? {
                return Ok(());
            }
            // 输出包只含直接文件；先检查全部条目，再删除，不递归跟随任何链接。
            let files = std::fs::read_dir(&path)?
                .map(|entry| entry.map(|e| e.path()))
                .collect::<std::io::Result<Vec<_>>>()?;
            for file in &files {
                filesystem::regular_file(file)?.ok_or(OutputError)?;
            }
            for file in files {
                std::fs::remove_file(file)?;
            }
            std::fs::remove_dir(path)?;
            Ok(())
        })
        .await
    }
    pub(crate) fn close(&self) {
        if let Ok(mut state) = self.0.state.lock() {
            state.closing = true;
        }
    }
    pub(crate) async fn drained(&self) {
        let mut count = self.0.count.subscribe();
        while *count.borrow_and_update() != 0 {
            if count.changed().await.is_err() {
                break;
            }
        }
    }
    fn notify(&self, state: &State) {
        self.0
            .count
            .send_replace(state.active.len() + state.workers);
    }
    async fn run<T: Send + 'static>(
        &self,
        action: impl FnOnce() -> Result<T> + Send + 'static,
    ) -> Result<T> {
        {
            let mut state = self.0.state.lock().map_err(|_| OutputError)?;
            state.workers += 1;
            self.notify(&state);
        }
        let lease = WorkerLease {
            store: self.clone(),
        };
        tokio::task::spawn_blocking(move || {
            let _lease = lease;
            action()
        })
        .await
        .map_err(|_| OutputError)?
    }
}
impl Drop for SessionLease {
    fn drop(&mut self) {
        if let Ok(mut state) = self.store.0.state.lock() {
            state.active.remove(&self.id);
            self.store.notify(&state);
        }
    }
}
impl Drop for WorkerLease {
    fn drop(&mut self) {
        if let Ok(mut state) = self.store.0.state.lock() {
            state.workers -= 1;
            self.store.notify(&state);
        }
    }
}
