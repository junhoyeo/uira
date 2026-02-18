use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::PathBuf;
use uira_core::UIRA_DIR;

#[derive(Debug, Clone)]
pub struct KvStore {
    path: PathBuf,
    data: HashMap<String, Value>,
}

impl KvStore {
    pub fn new() -> Self {
        let mut path = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        path.push(UIRA_DIR);
        path.push("ui-state.json");

        let data = fs::read_to_string(&path)
            .ok()
            .and_then(|content| serde_json::from_str::<HashMap<String, Value>>(&content).ok())
            .unwrap_or_default();

        Self { path, data }
    }

    pub fn get<T: DeserializeOwned>(&self, key: &str, default: T) -> T {
        self.data
            .get(key)
            .and_then(|value| serde_json::from_value::<T>(value.clone()).ok())
            .unwrap_or(default)
    }

    pub fn set<T: Serialize>(&mut self, key: &str, value: T) {
        match serde_json::to_value(value) {
            Ok(serialized) => {
                self.data.insert(key.to_string(), serialized);
                if let Err(error) = self.save() {
                    tracing::warn!("Failed to persist UI KV state: {}", error);
                }
            }
            Err(error) => {
                tracing::warn!("Failed to serialize UI KV value for '{}': {}", key, error);
            }
        }
    }

    pub fn save(&self) -> io::Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }

        let content = serde_json::to_string_pretty(&self.data)
            .map_err(|error| io::Error::other(error.to_string()))?;
        let tmp_path = self.path.with_extension("json.tmp");
        fs::write(&tmp_path, content)?;
        fs::rename(tmp_path, &self.path)
    }
}
