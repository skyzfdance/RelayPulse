use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use crate::domain::{EntityId, SecretValue};

use super::{CredentialError, CredentialStore, Result};

#[derive(Clone, Default)]
pub struct InMemoryCredentialStore {
    inner: Arc<Inner>,
}

#[derive(Default)]
struct Inner {
    secrets: Mutex<HashMap<EntityId, SecretValue>>,
    next_error: Mutex<Option<CredentialError>>,
    next_delete_error: Mutex<Option<CredentialError>>,
    next_put_after_write_error: Mutex<Option<CredentialError>>,
    next_delete_after_delete_error: Mutex<Option<CredentialError>>,
}

impl InMemoryCredentialStore {
    pub fn fail_next(&self, error: CredentialError) {
        *self
            .inner
            .next_error
            .lock()
            .expect("credential failure mutex poisoned") = Some(error);
    }

    pub fn fail_next_delete(&self, error: CredentialError) {
        *self
            .inner
            .next_delete_error
            .lock()
            .expect("credential delete failure mutex poisoned") = Some(error);
    }

    #[cfg(test)]
    pub(crate) fn fail_next_put_after_write(&self, error: CredentialError) {
        *self
            .inner
            .next_put_after_write_error
            .lock()
            .expect("credential post-write failure mutex poisoned") = Some(error);
    }

    #[cfg(test)]
    pub(crate) fn fail_next_delete_after_delete(&self, error: CredentialError) {
        *self
            .inner
            .next_delete_after_delete_error
            .lock()
            .expect("credential post-delete failure mutex poisoned") = Some(error);
    }

    #[cfg(test)]
    pub(crate) fn matches(&self, reference: &EntityId, expected: &str) -> bool {
        self.inner
            .secrets
            .lock()
            .expect("credential secrets mutex poisoned")
            .get(reference)
            .is_some_and(|secret| secret.expose() == expected)
    }

    fn take_error(&self) -> Result<()> {
        match self
            .inner
            .next_error
            .lock()
            .map_err(|_| CredentialError::Unavailable)?
            .take()
        {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}

impl CredentialStore for InMemoryCredentialStore {
    fn put(&self, reference: &EntityId, secret: SecretValue) -> Result<()> {
        self.take_error()?;
        self.inner
            .secrets
            .lock()
            .map_err(|_| CredentialError::Unavailable)?
            .insert(reference.clone(), secret);
        match self
            .inner
            .next_put_after_write_error
            .lock()
            .map_err(|_| CredentialError::Unavailable)?
            .take()
        {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    fn get(&self, reference: &EntityId) -> Result<SecretValue> {
        self.take_error()?;
        self.inner
            .secrets
            .lock()
            .map_err(|_| CredentialError::Unavailable)?
            .get(reference)
            .map(|secret| SecretValue::new(secret.expose()))
            .transpose()
            .map_err(|_| CredentialError::Unavailable)?
            .ok_or(CredentialError::NotFound)
    }

    fn exists(&self, reference: &EntityId) -> Result<bool> {
        self.take_error()?;
        Ok(self
            .inner
            .secrets
            .lock()
            .map_err(|_| CredentialError::Unavailable)?
            .contains_key(reference))
    }

    fn delete(&self, reference: &EntityId) -> Result<()> {
        if let Some(error) = self
            .inner
            .next_delete_error
            .lock()
            .map_err(|_| CredentialError::Unavailable)?
            .take()
        {
            return Err(error);
        }
        self.take_error()?;
        self.inner
            .secrets
            .lock()
            .map_err(|_| CredentialError::Unavailable)?
            .remove(reference);
        match self
            .inner
            .next_delete_after_delete_error
            .lock()
            .map_err(|_| CredentialError::Unavailable)?
            .take()
        {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}
