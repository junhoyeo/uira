use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FrecencyEntry {
    pub uses: u64,
    pub last_used_unix: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FrecencyStore {
    pub entries: HashMap<String, FrecencyEntry>,
}

impl FrecencyStore {
    pub fn touch(&mut self, key: impl Into<String>) {
        let key = key.into();
        let now = now_unix();
        let entry = self.entries.entry(key).or_default();
        entry.uses = entry.uses.saturating_add(1);
        entry.last_used_unix = now;
    }

    pub fn score(&self, key: &str) -> f64 {
        let Some(entry) = self.entries.get(key) else {
            return 0.0;
        };

        let age_secs = now_unix().saturating_sub(entry.last_used_unix);
        let age_hours = (age_secs as f64 / 3600.0).max(0.0);
        let recency = (-age_hours / 72.0).exp() * 1000.0;
        (entry.uses as f64 * 10.0) + recency
    }
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
