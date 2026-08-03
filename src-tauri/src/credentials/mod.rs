mod local;
mod memory;
mod service;

pub use local::LocalCredentialStore;
pub use memory::InMemoryCredentialStore;
pub use service::CredentialService;

use thiserror::Error;

use crate::domain::{EntityId, SecretValue};

#[derive(Debug, Clone, Copy, Error, PartialEq, Eq)]
pub enum CredentialError {
    #[error("credential store is unavailable")]
    Unavailable,
    #[error("credential was not found")]
    NotFound,
    #[error("credential worker is unavailable")]
    WorkerUnavailable,
    #[error("credential worker stopped before returning a result")]
    WorkerStopped,
    #[error("credential worker thread panicked")]
    WorkerPanicked,
}

pub type Result<T> = std::result::Result<T, CredentialError>;

pub trait CredentialStore: Send + Sync {
    fn put(&self, reference: &EntityId, secret: SecretValue) -> Result<()>;
    fn get(&self, reference: &EntityId) -> Result<SecretValue>;
    fn exists(&self, reference: &EntityId) -> Result<bool>;
    fn delete(&self, reference: &EntityId) -> Result<()>;
}
