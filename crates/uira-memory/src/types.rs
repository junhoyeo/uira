use chrono::{DateTime, Utc};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::LazyLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MemoryCategory {
    Preference,
    Fact,
    Decision,
    Entity,
    Other,
}

static PREFERENCE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(prefer|like|want|favorite|rather|love|enjoy|wish|choose to|opt for)\b")
        .unwrap()
});
static DECISION_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)\b(decided|chose|going with|will use|picked|settled on|switched to|migrated to)\b",
    )
    .unwrap()
});
static FACT_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(is|are|was|has|have|knows? that|works? (on|at|with)|uses?|built with)\b")
        .unwrap()
});
static ENTITY_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b[A-Z][a-z]+(?:\s+[A-Z][a-z]+)+\b").unwrap());

impl MemoryCategory {
    pub fn detect(text: &str) -> Self {
        if PREFERENCE_RE.is_match(text) {
            Self::Preference
        } else if DECISION_RE.is_match(text) {
            Self::Decision
        } else if ENTITY_RE.is_match(text) {
            Self::Entity
        } else if FACT_RE.is_match(text) {
            Self::Fact
        } else {
            Self::Other
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Preference => "preference",
            Self::Fact => "fact",
            Self::Decision => "decision",
            Self::Entity => "entity",
            Self::Other => "other",
        }
    }

    pub fn from_str_lossy(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "preference" => Self::Preference,
            "fact" => Self::Fact,
            "decision" => Self::Decision,
            "entity" => Self::Entity,
            _ => Self::Other,
        }
    }
}

impl std::fmt::Display for MemoryCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MemorySource {
    Manual,
    Conversation,
    Session,
}

impl MemorySource {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::Conversation => "conversation",
            Self::Session => "session",
        }
    }

    pub fn from_str_lossy(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "conversation" => Self::Conversation,
            "session" => Self::Session,
            _ => Self::Manual,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub id: String,
    pub content: String,
    pub source: MemorySource,
    pub category: MemoryCategory,
    pub container_tag: String,
    pub metadata: HashMap<String, serde_json::Value>,
    pub session_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl MemoryEntry {
    pub fn new(
        content: impl Into<String>,
        source: MemorySource,
        container_tag: impl Into<String>,
    ) -> Self {
        let content = content.into();
        let category = MemoryCategory::detect(&content);
        let now = Utc::now();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            content,
            source,
            category,
            container_tag: container_tag.into(),
            metadata: HashMap::new(),
            session_id: None,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn with_category(mut self, category: MemoryCategory) -> Self {
        self.category = category;
        self
    }

    pub fn with_session_id(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }

    pub fn with_metadata(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.metadata.insert(key.into(), value);
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub entry: MemoryEntry,
    pub vector_score: Option<f32>,
    pub fts_score: Option<f64>,
    pub combined_score: f64,
    pub final_score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserProfileFact {
    pub id: String,
    pub fact_type: String,
    pub category: String,
    pub content: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MemoryStats {
    pub total_memories: usize,
    pub total_by_category: HashMap<String, usize>,
    pub total_by_container: HashMap<String, usize>,
    pub db_size_bytes: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_preference() {
        assert_eq!(
            MemoryCategory::detect("I prefer dark mode"),
            MemoryCategory::Preference
        );
        assert_eq!(
            MemoryCategory::detect("I like using Rust"),
            MemoryCategory::Preference
        );
        assert_eq!(
            MemoryCategory::detect("I want fast builds"),
            MemoryCategory::Preference
        );
        assert_eq!(
            MemoryCategory::detect("My favorite editor is Neovim"),
            MemoryCategory::Preference
        );
    }

    #[test]
    fn detect_decision() {
        assert_eq!(
            MemoryCategory::detect("We decided to use SQLite"),
            MemoryCategory::Decision
        );
        assert_eq!(
            MemoryCategory::detect("I chose rusqlite over LanceDB"),
            MemoryCategory::Decision
        );
        assert_eq!(
            MemoryCategory::detect("Going with the hybrid approach"),
            MemoryCategory::Decision
        );
    }

    #[test]
    fn detect_entity() {
        assert_eq!(
            MemoryCategory::detect("John Smith works here"),
            MemoryCategory::Entity
        );
        assert_eq!(
            MemoryCategory::detect("Junho Yeo created uira"),
            MemoryCategory::Entity
        );
    }

    #[test]
    fn detect_fact() {
        assert_eq!(
            MemoryCategory::detect("The project is built with Rust"),
            MemoryCategory::Fact
        );
        assert_eq!(
            MemoryCategory::detect("It uses tokio for async"),
            MemoryCategory::Fact
        );
    }

    #[test]
    fn detect_other() {
        assert_eq!(MemoryCategory::detect("hello"), MemoryCategory::Other);
        assert_eq!(MemoryCategory::detect("1234"), MemoryCategory::Other);
    }

    #[test]
    fn memory_entry_auto_categorizes() {
        let entry = MemoryEntry::new("I prefer dark mode", MemorySource::Manual, "default");
        assert_eq!(entry.category, MemoryCategory::Preference);
        assert!(!entry.id.is_empty());
    }

    #[test]
    fn memory_entry_builder() {
        let entry = MemoryEntry::new("test", MemorySource::Conversation, "default")
            .with_category(MemoryCategory::Fact)
            .with_session_id("ses_123")
            .with_metadata("key", serde_json::json!("value"));
        assert_eq!(entry.category, MemoryCategory::Fact);
        assert_eq!(entry.session_id, Some("ses_123".to_string()));
        assert_eq!(entry.metadata.get("key"), Some(&serde_json::json!("value")));
    }

    #[test]
    fn category_roundtrip() {
        for cat in [
            MemoryCategory::Preference,
            MemoryCategory::Fact,
            MemoryCategory::Decision,
            MemoryCategory::Entity,
            MemoryCategory::Other,
        ] {
            assert_eq!(MemoryCategory::from_str_lossy(cat.as_str()), cat);
        }
    }

    #[test]
    fn source_roundtrip() {
        for src in [
            MemorySource::Manual,
            MemorySource::Conversation,
            MemorySource::Session,
        ] {
            assert_eq!(MemorySource::from_str_lossy(src.as_str()), src);
        }
    }
}
