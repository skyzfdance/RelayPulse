use std::{
    path::{Path, PathBuf},
    sync::{mpsc, Arc, Mutex},
    thread::{self, JoinHandle},
    time::Duration,
};

use rusqlite::Connection;
use tokio::sync::oneshot;

use super::{migrations::migrate_to_latest, DatabaseError, Result};

type Job = Box<dyn FnOnce(&mut Connection) + Send + 'static>;

enum Location {
    File(PathBuf),
    Memory,
}

impl Location {
    fn open(self) -> rusqlite::Result<Connection> {
        match self {
            Self::File(path) => Connection::open(path),
            Self::Memory => Connection::open_in_memory(),
        }
    }

    fn uses_file(&self) -> bool {
        matches!(self, Self::File(_))
    }
}

struct DatabaseInner {
    sender: Mutex<Option<mpsc::Sender<Job>>>,
    worker: Mutex<Option<JoinHandle<()>>>,
}

impl Drop for DatabaseInner {
    fn drop(&mut self) {
        self.sender
            .get_mut()
            .expect("database sender mutex poisoned")
            .take();

        if let Some(worker) = self
            .worker
            .get_mut()
            .expect("database worker mutex poisoned")
            .take()
        {
            let _ = worker.join();
        }
    }
}

#[derive(Clone)]
pub struct Database {
    inner: Arc<DatabaseInner>,
}

impl Database {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::start(Location::File(path.as_ref().to_path_buf()))
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::start(Location::Memory)
    }

    fn start(location: Location) -> Result<Self> {
        let (sender, receiver) = mpsc::channel::<Job>();
        let (ready_sender, ready_receiver) = mpsc::sync_channel(1);
        let worker = thread::Builder::new()
            .name("relaypulse-database".into())
            .spawn(move || {
                let result = open_and_prepare(location);
                match result {
                    Ok(mut connection) => {
                        if ready_sender.send(Ok(())).is_err() {
                            return;
                        }
                        while let Ok(job) = receiver.recv() {
                            job(&mut connection);
                        }
                    }
                    Err(error) => {
                        let _ = ready_sender.send(Err(error));
                    }
                }
            })
            .map_err(|_| DatabaseError::WorkerUnavailable)?;

        match ready_receiver.recv() {
            Ok(Ok(())) => Ok(Self {
                inner: Arc::new(DatabaseInner {
                    sender: Mutex::new(Some(sender)),
                    worker: Mutex::new(Some(worker)),
                }),
            }),
            Ok(Err(error)) => {
                worker.join().map_err(|_| DatabaseError::WorkerPanicked)?;
                Err(error)
            }
            Err(_) => {
                worker.join().map_err(|_| DatabaseError::WorkerPanicked)?;
                Err(DatabaseError::WorkerStopped)
            }
        }
    }

    pub async fn call<T, F>(&self, operation: F) -> Result<T>
    where
        T: Send + 'static,
        F: FnOnce(&mut Connection) -> Result<T> + Send + 'static,
    {
        let (result_sender, result_receiver) = oneshot::channel();
        let job = Box::new(move |connection: &mut Connection| {
            let _ = result_sender.send(operation(connection));
        });

        let sender = self
            .inner
            .sender
            .lock()
            .map_err(|_| DatabaseError::WorkerUnavailable)?
            .clone()
            .ok_or(DatabaseError::WorkerUnavailable)?;
        sender
            .send(job)
            .map_err(|_| DatabaseError::WorkerUnavailable)?;

        result_receiver
            .await
            .map_err(|_| DatabaseError::WorkerStopped)?
    }
}

fn open_and_prepare(location: Location) -> Result<Connection> {
    let uses_file = location.uses_file();
    let mut connection = location.open()?;
    connection.busy_timeout(Duration::from_secs(5))?;
    connection.pragma_update(None, "foreign_keys", "ON")?;
    connection.pragma_update(None, "synchronous", "NORMAL")?;
    if uses_file {
        connection.pragma_update(None, "journal_mode", "WAL")?;
    }
    migrate_to_latest(&mut connection)?;
    Ok(connection)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    #[tokio::test(flavor = "current_thread")]
    async fn executes_work_on_the_named_worker() {
        let database = Database::open_in_memory().expect("database opens");
        let worker_name = database
            .call(|_| {
                Ok(thread::current()
                    .name()
                    .expect("worker has a name")
                    .to_owned())
            })
            .await
            .expect("database call succeeds");

        assert_eq!(worker_name, "relaypulse-database");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn serializes_concurrent_writes() {
        let database = Arc::new(Database::open_in_memory().expect("database opens"));
        let mut tasks = Vec::new();

        for _ in 0..32 {
            let database = Arc::clone(&database);
            tasks.push(tokio::spawn(async move {
                database
                    .call(|connection| {
                        connection.execute(
                            "UPDATE app_state SET revision = revision + 1 WHERE singleton = 1",
                            [],
                        )?;
                        Ok(())
                    })
                    .await
            }));
        }

        for task in tasks {
            task.await.expect("task joins").expect("write succeeds");
        }

        let revision = database
            .call(|connection| {
                Ok(connection.query_row(
                    "SELECT revision FROM app_state WHERE singleton = 1",
                    [],
                    |row| row.get::<_, i64>(0),
                )?)
            })
            .await
            .expect("revision can be read");
        assert_eq!(revision, 32);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn persists_data_after_reopening() {
        let directory = tempfile::tempdir().expect("tempdir created");
        let path = directory.path().join("relaypulse.sqlite3");

        {
            let database = Database::open(&path).expect("database opens");
            database
                .call(|connection| {
                    connection
                        .execute("UPDATE app_state SET revision = 7 WHERE singleton = 1", [])?;
                    Ok(())
                })
                .await
                .expect("revision is written");
        }

        let database = Database::open(&path).expect("database reopens");
        let revision = database
            .call(|connection| {
                Ok(connection.query_row(
                    "SELECT revision FROM app_state WHERE singleton = 1",
                    [],
                    |row| row.get::<_, i64>(0),
                )?)
            })
            .await
            .expect("revision can be read");
        assert_eq!(revision, 7);
    }
}
