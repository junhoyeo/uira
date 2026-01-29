use crate::{AuthError, Result, StoredCredential};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct CredentialStore {
    credentials: HashMap<String, StoredCredential>,
}

impl CredentialStore {
    pub fn load() -> Result<Self> {
        let path = Self::storage_path()?;

        if !path.exists() {
            return Ok(Self::default());
        }

        let content = std::fs::read_to_string(&path)
            .map_err(|e| AuthError::StorageError(format!("Failed to read: {}", e)))?;

        let store: Self = serde_json::from_str(&content)?;
        Ok(store)
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::storage_path()?;

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| AuthError::StorageError(format!("Failed to create dir: {}", e)))?;
        }

        let content = serde_json::to_string_pretty(self)?;

        std::fs::write(&path, content)
            .map_err(|e| AuthError::StorageError(format!("Failed to write: {}", e)))?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&path)?.permissions();
            perms.set_mode(0o600);
            std::fs::set_permissions(&path, perms)?;
        }

        Ok(())
    }

    pub fn get(&self, provider: &str) -> Option<&StoredCredential> {
        self.credentials.get(provider)
    }

    pub fn insert(&mut self, provider: String, credential: StoredCredential) {
        self.credentials.insert(provider, credential);
    }

    pub fn remove(&mut self, provider: &str) -> Option<StoredCredential> {
        self.credentials.remove(provider)
    }

    pub fn providers(&self) -> Vec<&str> {
        self.credentials.keys().map(|s| s.as_str()).collect()
    }

    pub fn len(&self) -> usize {
        self.credentials.len()
    }

    pub fn is_empty(&self) -> bool {
        self.credentials.is_empty()
    }

    fn storage_path() -> Result<PathBuf> {
        let data_dir = dirs::data_local_dir()
            .ok_or_else(|| AuthError::StorageError("No data directory found".to_string()))?;

        Ok(data_dir.join("uira").join("auth.json"))
    }
}
