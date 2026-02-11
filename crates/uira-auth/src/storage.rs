use crate::{AuthError, Result, StoredCredential};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use uira_protocol::atomic_write_secure;

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

        atomic_write_secure(&path, content.as_bytes())
            .map_err(|e| AuthError::StorageError(format!("Failed to write credentials: {}", e)))?;

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
        // Prefer ~/.uira for consistency with other CLI tools, fall back to XDG data dir
        // for environments where HOME is unset (systemd services, containers)
        let base_dir = dirs::home_dir()
            .map(|h| h.join(".uira"))
            .or_else(|| dirs::data_local_dir().map(|d| d.join("uira")))
            .ok_or_else(|| {
                AuthError::StorageError("No home or data directory found".to_string())
            })?;

        Ok(base_dir.join("auth.json"))
    }
}
