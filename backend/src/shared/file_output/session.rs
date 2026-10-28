use super::{
    filesystem::{self, FileWriter},
    paths, OutputError, Result, SessionLease,
};
use std::{collections::HashMap, path::PathBuf, sync::Arc};
use tokio::sync::Mutex;

struct Files {
    path: PathBuf,
    handles: HashMap<String, FileWriter>,
    failed: bool,
    published: bool,
}
/// 未 publish 的输出始终保留 .partial 标记；Drop 只释放资源，不伪造完成。
pub(crate) struct OutputSession {
    files: Arc<Mutex<Files>>,
    lease: Arc<SessionLease>,
}
impl OutputSession {
    pub(super) fn new(path: PathBuf, lease: Arc<SessionLease>) -> Self {
        Self {
            files: Arc::new(Mutex::new(Files {
                path,
                handles: HashMap::new(),
                failed: false,
                published: false,
            })),
            lease,
        }
    }
    pub(crate) async fn append(&self, name: &str, bytes: &[u8]) -> Result<()> {
        paths::filename(name)?;
        let name = name.to_owned();
        // 先取得可转移的锁再派发 worker；取消等待时 worker 保留锁，后续写入不会超车。
        let mut files = self.files.clone().lock_owned().await;
        let bytes = bytes.to_vec();
        let lease = self.lease.clone();
        self.lease
            .store
            .run(move || {
                let _lease = lease;
                if files.failed || files.published {
                    return Err(OutputError);
                }
                let result = (|| {
                    if !files.handles.contains_key(&name) {
                        filesystem::directory(&files.path, false)?;
                        let file = FileWriter::create_new(&files.path.join(&name))?;
                        files.handles.insert(name.clone(), file);
                    }
                    files
                        .handles
                        .get_mut(&name)
                        .ok_or(OutputError)?
                        .write_all(&bytes)?;
                    Ok(())
                })();
                if result.is_err() {
                    files.failed = true;
                }
                result
            })
            .await
    }
    /// 调用方确认业务结果已提交后发布；所有文件先同步，整个目录再去掉 partial 标记。
    pub(crate) async fn publish(&self) -> Result<()> {
        let mut files = self.files.clone().lock_owned().await;
        let lease = self.lease.clone();
        self.lease
            .store
            .run(move || {
                if files.failed || files.published {
                    return Err(OutputError);
                }
                for file in files.handles.values_mut() {
                    file.sync_all()?;
                }
                files.handles.clear();
                filesystem::directory(&lease.store.0.root, false)?;
                filesystem::directory(&files.path, false)?;
                let destination = lease.store.0.root.join(&lease.id);
                if destination.try_exists()? {
                    return Err(OutputError);
                }
                std::fs::rename(&files.path, &destination)?;
                files.path = destination;
                files.published = true;
                Ok(())
            })
            .await
    }
}
