use std::{
    sync::{mpsc, Arc, Mutex},
    thread::{self, JoinHandle},
};

use tokio::sync::oneshot;

use crate::domain::{EntityId, SecretValue};

use super::{CredentialError, CredentialStore, Result};

type Job = Box<dyn FnOnce(&dyn CredentialStore) + Send + 'static>;

struct CredentialServiceInner {
    sender: Mutex<Option<mpsc::Sender<Job>>>,
    worker: Mutex<Option<JoinHandle<()>>>,
}

impl Drop for CredentialServiceInner {
    fn drop(&mut self) {
        self.sender
            .get_mut()
            .expect("credential sender mutex poisoned")
            .take();
        if let Some(worker) = self
            .worker
            .get_mut()
            .expect("credential worker mutex poisoned")
            .take()
        {
            let _ = worker.join();
        }
    }
}

#[derive(Clone)]
pub struct CredentialService {
    inner: Arc<CredentialServiceInner>,
}

impl CredentialService {
    pub fn start(store: impl CredentialStore + 'static) -> Result<Self> {
        let (sender, receiver) = mpsc::channel::<Job>();
        let worker = thread::Builder::new()
            .name("relaypulse-credentials".into())
            .spawn(move || {
                while let Ok(job) = receiver.recv() {
                    job(&store);
                }
            })
            .map_err(|_| CredentialError::WorkerUnavailable)?;

        Ok(Self {
            inner: Arc::new(CredentialServiceInner {
                sender: Mutex::new(Some(sender)),
                worker: Mutex::new(Some(worker)),
            }),
        })
    }

    pub async fn put(&self, reference: EntityId, secret: SecretValue) -> Result<()> {
        self.call(move |store| store.put(&reference, secret)).await
    }

    pub async fn get(&self, reference: EntityId) -> Result<SecretValue> {
        self.call(move |store| store.get(&reference)).await
    }

    pub async fn exists(&self, reference: EntityId) -> Result<bool> {
        self.call(move |store| store.exists(&reference)).await
    }

    pub async fn delete(&self, reference: EntityId) -> Result<()> {
        self.call(move |store| store.delete(&reference)).await
    }

    async fn call<T, F>(&self, operation: F) -> Result<T>
    where
        T: Send + 'static,
        F: FnOnce(&dyn CredentialStore) -> Result<T> + Send + 'static,
    {
        let (result_sender, result_receiver) = oneshot::channel();
        let job = Box::new(move |store: &dyn CredentialStore| {
            let _ = result_sender.send(operation(store));
        });
        let sender = self
            .inner
            .sender
            .lock()
            .map_err(|_| CredentialError::WorkerUnavailable)?
            .clone()
            .ok_or(CredentialError::WorkerUnavailable)?;
        sender
            .send(job)
            .map_err(|_| CredentialError::WorkerUnavailable)?;
        result_receiver
            .await
            .map_err(|_| CredentialError::WorkerStopped)?
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::credentials::InMemoryCredentialStore;

    #[tokio::test(flavor = "current_thread")]
    async fn serializes_secret_operations_on_the_named_worker() {
        struct RecordingStore {
            names: Arc<Mutex<Vec<String>>>,
            inner: InMemoryCredentialStore,
        }

        impl CredentialStore for RecordingStore {
            fn put(&self, reference: &EntityId, secret: SecretValue) -> Result<()> {
                self.names
                    .lock()
                    .expect("names mutex poisoned")
                    .push(thread::current().name().unwrap_or_default().to_owned());
                self.inner.put(reference, secret)
            }

            fn get(&self, reference: &EntityId) -> Result<SecretValue> {
                self.names
                    .lock()
                    .expect("names mutex poisoned")
                    .push(thread::current().name().unwrap_or_default().to_owned());
                self.inner.get(reference)
            }

            fn exists(&self, reference: &EntityId) -> Result<bool> {
                self.names
                    .lock()
                    .expect("names mutex poisoned")
                    .push(thread::current().name().unwrap_or_default().to_owned());
                self.inner.exists(reference)
            }

            fn delete(&self, reference: &EntityId) -> Result<()> {
                self.names
                    .lock()
                    .expect("names mutex poisoned")
                    .push(thread::current().name().unwrap_or_default().to_owned());
                self.inner.delete(reference)
            }
        }

        let names = Arc::new(Mutex::new(Vec::new()));
        let memory = InMemoryCredentialStore::default();
        let service = CredentialService::start(RecordingStore {
            names: Arc::clone(&names),
            inner: memory.clone(),
        })
        .expect("service starts");
        let reference = EntityId::parse("credential-a").expect("id is valid");
        service
            .put(
                reference.clone(),
                SecretValue::new("secret-marker").expect("secret is valid"),
            )
            .await
            .expect("secret stored");
        assert!(memory.matches(&reference, "secret-marker"));
        let secret = service
            .get(reference.clone())
            .await
            .expect("secret can be read on the worker");
        assert_eq!(secret.expose(), "secret-marker");
        assert!(service
            .exists(reference.clone())
            .await
            .expect("existence read"));
        service
            .delete(reference.clone())
            .await
            .expect("secret deleted");
        assert!(!service.exists(reference).await.expect("existence read"));

        assert!(names
            .lock()
            .expect("names mutex poisoned")
            .iter()
            .all(|name| name == "relaypulse-credentials"));
        assert!(!memory.matches(
            &EntityId::parse("credential-a").expect("id is valid"),
            "secret-marker"
        ));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn propagates_classified_store_failures() {
        let memory = InMemoryCredentialStore::default();
        memory.fail_next(CredentialError::Unavailable);
        let service = CredentialService::start(memory).expect("service starts");

        assert_eq!(
            service
                .exists(EntityId::parse("credential-a").expect("id is valid"))
                .await,
            Err(CredentialError::Unavailable)
        );
    }
}
