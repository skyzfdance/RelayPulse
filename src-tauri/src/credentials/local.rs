use std::{
    collections::HashMap,
    fs::{self, OpenOptions},
    io::Write,
    path::PathBuf,
    sync::Mutex,
};

#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

use serde::{Deserialize, Serialize};

use crate::domain::{EntityId, SecretValue};

use super::{CredentialError, CredentialStore, Result};

#[derive(Debug)]
pub struct LocalCredentialStore {
    path: PathBuf,
    gate: Mutex<()>,
}

impl LocalCredentialStore {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            gate: Mutex::new(()),
        }
    }

    fn read(&self) -> Result<CredentialFile> {
        match fs::read(&self.path) {
            Ok(bytes) => {
                let file: CredentialFile =
                    serde_json::from_slice(&bytes).map_err(|_| CredentialError::Unavailable)?;
                if file.version != credential_file_version() {
                    return Err(CredentialError::Unavailable);
                }
                Ok(file)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Ok(CredentialFile::default())
            }
            Err(_) => Err(CredentialError::Unavailable),
        }
    }

    fn write(&self, credentials: &CredentialFile) -> Result<()> {
        let parent = self.path.parent().ok_or(CredentialError::Unavailable)?;
        fs::create_dir_all(parent).map_err(|_| CredentialError::Unavailable)?;
        let bytes =
            serde_json::to_vec_pretty(credentials).map_err(|_| CredentialError::Unavailable)?;
        let mut options = OpenOptions::new();
        options.create(true).truncate(true).write(true);
        #[cfg(unix)]
        options.mode(0o600);
        let mut file = options
            .open(&self.path)
            .map_err(|_| CredentialError::Unavailable)?;
        write_file(&mut file, &bytes)?;
        #[cfg(unix)]
        fs::set_permissions(&self.path, fs::Permissions::from_mode(0o600))
            .map_err(|_| CredentialError::Unavailable)?;
        Ok(())
    }
}

fn write_file(file: &mut fs::File, bytes: &[u8]) -> Result<()> {
    file.write_all(bytes)
        .and_then(|()| file.write_all(b"\n"))
        .and_then(|()| file.sync_all())
        .map_err(|_| CredentialError::Unavailable)
}

impl CredentialStore for LocalCredentialStore {
    fn put(&self, reference: &EntityId, secret: SecretValue) -> Result<()> {
        let _guard = self.gate.lock().map_err(|_| CredentialError::Unavailable)?;
        let mut file = self.read()?;
        file.credentials
            .insert(reference.to_string(), secret.expose().to_owned());
        self.write(&file)
    }

    fn get(&self, reference: &EntityId) -> Result<SecretValue> {
        let _guard = self.gate.lock().map_err(|_| CredentialError::Unavailable)?;
        let file = self.read()?;
        let secret = file
            .credentials
            .get(reference.as_str())
            .ok_or(CredentialError::NotFound)?;
        SecretValue::new(secret).map_err(|_| CredentialError::Unavailable)
    }

    fn exists(&self, reference: &EntityId) -> Result<bool> {
        let _guard = self.gate.lock().map_err(|_| CredentialError::Unavailable)?;
        Ok(self.read()?.credentials.contains_key(reference.as_str()))
    }

    fn delete(&self, reference: &EntityId) -> Result<()> {
        let _guard = self.gate.lock().map_err(|_| CredentialError::Unavailable)?;
        let mut file = self.read()?;
        if file.credentials.remove(reference.as_str()).is_some() {
            self.write(&file)?;
        }
        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct CredentialFile {
    #[serde(default = "credential_file_version")]
    version: u8,
    #[serde(default)]
    credentials: HashMap<String, String>,
}

impl Default for CredentialFile {
    fn default() -> Self {
        Self {
            version: credential_file_version(),
            credentials: HashMap::new(),
        }
    }
}

fn credential_file_version() -> u8 {
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credentials_round_trip_after_reopening_without_exposing_them_in_debug_output() {
        let directory = tempfile::tempdir().expect("tempdir created");
        let path = directory.path().join("credentials.json");
        let reference = EntityId::parse("credential-a").expect("reference is valid");
        let store = LocalCredentialStore::new(path.clone());
        store
            .put(
                &reference,
                SecretValue::new("fixture-secret").expect("secret is valid"),
            )
            .expect("credential stored");

        let reopened = LocalCredentialStore::new(path.clone());
        assert!(reopened
            .exists(&reference)
            .expect("credential existence read"));
        assert_eq!(
            reopened.get(&reference).expect("credential read").expose(),
            "fixture-secret"
        );
        reopened
            .put(
                &reference,
                SecretValue::new("replacement-secret").expect("replacement is valid"),
            )
            .expect("credential replaced");
        assert_eq!(
            LocalCredentialStore::new(path.clone())
                .get(&reference)
                .expect("replacement reads")
                .expose(),
            "replacement-secret"
        );
        assert!(!format!("{reopened:?}").contains("fixture-secret"));

        #[cfg(unix)]
        assert_eq!(
            fs::metadata(&path)
                .expect("credential metadata reads")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );

        reopened.delete(&reference).expect("credential deleted");
        assert!(!reopened
            .exists(&reference)
            .expect("credential absence read"));
    }
}
