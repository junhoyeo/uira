//! Approval caching for tool execution
//!
//! Caches user approval decisions to avoid repeated prompts for similar operations.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CacheDecision {
    ApproveOnce,
    ApproveForSession,
    ApproveForPattern,
    DenyOnce,
    DenyForSession,
}

impl CacheDecision {
    pub fn is_approve(&self) -> bool {
        matches!(
            self,
            CacheDecision::ApproveOnce
                | CacheDecision::ApproveForSession
                | CacheDecision::ApproveForPattern
        )
    }

    pub fn should_cache(&self) -> bool {
        matches!(
            self,
            CacheDecision::ApproveForSession
                | CacheDecision::ApproveForPattern
                | CacheDecision::DenyForSession
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalKey {
    pub tool: String,
    pub pattern: String,
    pub key_hash: String,
}

impl ApprovalKey {
    pub fn new(tool: &str, pattern: &str) -> Self {
        use sha2::{Digest, Sha256};

        let mut hasher = Sha256::new();
        hasher.update(tool.as_bytes());
        hasher.update(b"|");
        hasher.update(pattern.as_bytes());
        let hash = hasher.finalize();

        Self {
            tool: tool.to_string(),
            pattern: pattern.to_string(),
            key_hash: hex::encode(hash),
        }
    }

    pub fn from_tool_and_path(tool: &str, path: &str) -> Self {
        let pattern = Self::path_to_pattern(path);
        Self::new(tool, &pattern)
    }

    pub fn for_bash_command(command: &str, working_dir: &str) -> Self {
        use sha2::{Digest, Sha256};

        let mut hasher = Sha256::new();
        hasher.update(b"bash|");
        hasher.update(command.as_bytes());
        hasher.update(b"|");
        hasher.update(working_dir.as_bytes());
        let hash = hasher.finalize();
        let hash_hex = hex::encode(hash);

        Self {
            tool: "Bash".to_string(),
            pattern: format!("cmd:{}", &hash_hex[..16]),
            key_hash: hash_hex,
        }
    }

    fn path_to_pattern(path: &str) -> String {
        if let Some(parent) = std::path::Path::new(path).parent() {
            let parent_str = parent.display().to_string();
            if parent_str.is_empty() || parent_str == "." {
                path.to_string()
            } else {
                format!("{}/**", parent_str)
            }
        } else {
            path.to_string()
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedApproval {
    pub key: ApprovalKey,
    pub decision: CacheDecision,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
}

impl CachedApproval {
    pub fn new(key: ApprovalKey, decision: CacheDecision) -> Self {
        Self {
            key,
            decision,
            created_at: Utc::now(),
            expires_at: None,
        }
    }

    pub fn with_ttl(mut self, duration: chrono::Duration) -> Self {
        self.expires_at = Some(self.created_at + duration);
        self
    }

    pub fn is_expired(&self) -> bool {
        self.expires_at.is_some_and(|exp| Utc::now() > exp)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalCacheFile {
    pub version: u32,
    pub session_id: String,
    pub approvals: Vec<CachedApproval>,
}

impl ApprovalCacheFile {
    pub fn new(session_id: String) -> Self {
        Self {
            version: 1,
            session_id,
            approvals: Vec::new(),
        }
    }
}

#[derive(Debug, Default)]
pub struct ApprovalCache {
    session_id: String,
    cache: HashMap<String, CachedApproval>,
    cache_dir: Option<PathBuf>,
}

impl ApprovalCache {
    pub fn new(session_id: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
            cache: HashMap::new(),
            cache_dir: None,
        }
    }

    pub fn with_persistence(mut self, cache_dir: PathBuf) -> Self {
        self.cache_dir = Some(cache_dir);
        self
    }

    pub fn lookup(&self, tool: &str, path: &str) -> Option<CacheDecision> {
        let key = ApprovalKey::from_tool_and_path(tool, path);
        self.lookup_by_key(&key)
    }

    pub fn lookup_bash(&self, command: &str, working_dir: &str) -> Option<CacheDecision> {
        let key = ApprovalKey::for_bash_command(command, working_dir);
        self.lookup_by_key(&key)
    }

    fn lookup_by_key(&self, key: &ApprovalKey) -> Option<CacheDecision> {
        self.cache.get(&key.key_hash).and_then(|cached| {
            if cached.is_expired() {
                None
            } else {
                Some(cached.decision)
            }
        })
    }

    pub fn insert(&mut self, key: ApprovalKey, decision: CacheDecision) {
        if decision.should_cache() {
            let ttl = match decision {
                CacheDecision::ApproveForSession | CacheDecision::DenyForSession => {
                    Some(chrono::Duration::hours(8))
                }
                _ => None,
            };

            let cached = if let Some(duration) = ttl {
                CachedApproval::new(key.clone(), decision).with_ttl(duration)
            } else {
                CachedApproval::new(key.clone(), decision)
            };
            self.cache.insert(key.key_hash.clone(), cached);
        }
    }

    pub fn insert_with_ttl(
        &mut self,
        key: ApprovalKey,
        decision: CacheDecision,
        ttl: chrono::Duration,
    ) {
        if decision.should_cache() {
            let cached = CachedApproval::new(key.clone(), decision).with_ttl(ttl);
            self.cache.insert(key.key_hash.clone(), cached);
        }
    }

    pub fn clear_expired(&mut self) {
        self.cache.retain(|_, v| !v.is_expired());
    }

    pub fn save(&self) -> std::io::Result<()> {
        let Some(cache_dir) = &self.cache_dir else {
            return Ok(());
        };

        std::fs::create_dir_all(cache_dir)?;
        let path = cache_dir.join(format!("{}.json", self.session_id));

        let file = ApprovalCacheFile {
            version: 1,
            session_id: self.session_id.clone(),
            approvals: self.cache.values().cloned().collect(),
        };

        let content = serde_json::to_string_pretty(&file)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(path, content)
    }

    pub fn load(session_id: &str, cache_dir: &Path) -> std::io::Result<Self> {
        let path = cache_dir.join(format!("{}.json", session_id));
        if !path.exists() {
            return Ok(Self::new(session_id).with_persistence(cache_dir.to_path_buf()));
        }

        let content = std::fs::read_to_string(&path)?;
        let file: ApprovalCacheFile = serde_json::from_str(&content)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        let mut cache = HashMap::new();
        for approval in file.approvals {
            if !approval.is_expired() {
                cache.insert(approval.key.key_hash.clone(), approval);
            }
        }

        Ok(Self {
            session_id: session_id.to_string(),
            cache,
            cache_dir: Some(cache_dir.to_path_buf()),
        })
    }

    pub fn len(&self) -> usize {
        self.cache.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_approval_cache_lookup() {
        let mut cache = ApprovalCache::new("test_session");
        let key = ApprovalKey::from_tool_and_path("edit", "src/main.rs");

        assert!(cache.lookup("edit", "src/main.rs").is_none());

        cache.insert(key.clone(), CacheDecision::ApproveForSession);

        assert_eq!(
            cache.lookup("edit", "src/main.rs"),
            Some(CacheDecision::ApproveForSession)
        );
    }

    #[test]
    fn test_approval_key_hash() {
        let key1 = ApprovalKey::new("edit", "src/**");
        let key2 = ApprovalKey::new("edit", "src/**");
        let key3 = ApprovalKey::new("bash", "src/**");

        assert_eq!(key1.key_hash, key2.key_hash);
        assert_ne!(key1.key_hash, key3.key_hash);
    }

    #[test]
    fn test_cache_decision_properties() {
        assert!(CacheDecision::ApproveOnce.is_approve());
        assert!(CacheDecision::ApproveForSession.is_approve());
        assert!(!CacheDecision::DenyOnce.is_approve());

        assert!(!CacheDecision::ApproveOnce.should_cache());
        assert!(CacheDecision::ApproveForSession.should_cache());
        assert!(CacheDecision::ApproveForPattern.should_cache());
    }

    #[test]
    fn test_cached_approval_expiry() {
        let key = ApprovalKey::new("test", "pattern");
        let approval = CachedApproval::new(key, CacheDecision::ApproveForSession);
        assert!(!approval.is_expired());

        let expired = CachedApproval::new(
            ApprovalKey::new("test", "pattern"),
            CacheDecision::ApproveForSession,
        )
        .with_ttl(chrono::Duration::seconds(-1));
        assert!(expired.is_expired());
    }
}
