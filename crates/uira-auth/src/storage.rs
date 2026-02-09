use crate::{AuthError, Result, StoredCredential};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use uira_core::atomic_write_secure;

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
        let home_dir = dirs::home_dir()
            .ok_or_else(|| AuthError::StorageError("No home directory found".to_string()))?;

        Ok(home_dir.join(".uira").join("auth.json"))
    }
}
