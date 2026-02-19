use anyhow::Result;
use chrono::Utc;
use std::sync::Arc;

use crate::store::MemoryStore;
use crate::types::{MemoryCategory, UserProfileFact};

pub struct UserProfile {
    store: Arc<MemoryStore>,
}

impl UserProfile {
    pub fn new(store: Arc<MemoryStore>) -> Self {
        Self { store }
    }

    pub fn add_fact(&self, fact_type: &str, category: &str, content: &str) -> Result<()> {
        let now = Utc::now();
        let fact = UserProfileFact {
            id: uuid::Uuid::new_v4().to_string(),
            fact_type: fact_type.to_string(),
            category: category.to_string(),
            content: content.to_string(),
            created_at: now,
            updated_at: now,
        };
        self.store.add_profile_fact(&fact)
    }

    pub fn get_facts(&self, fact_type: Option<&str>) -> Result<Vec<UserProfileFact>> {
        self.store.get_profile_facts(fact_type)
    }

    pub fn remove_fact(&self, id: &str) -> Result<bool> {
        self.store.remove_profile_fact(id)
    }

    pub fn format_profile(&self) -> Result<String> {
        let facts = self.store.get_profile_facts(None)?;
        if facts.is_empty() {
            return Ok("No user profile facts recorded yet.".to_string());
        }

        let mut preferences = Vec::new();
        let mut fact_items = Vec::new();
        let mut decisions = Vec::new();
        let mut other = Vec::new();

        for fact in &facts {
            let line = format!("- {}", fact.content);
            match fact.category.as_str() {
                "preference" => preferences.push(line),
                "fact" => fact_items.push(line),
                "decision" => decisions.push(line),
                _ => other.push(line),
            }
        }

        let mut output = String::from("## User Profile\n");

        if !preferences.is_empty() {
            output.push_str("\n### Preferences\n");
            output.push_str(&preferences.join("\n"));
            output.push('\n');
        }
        if !fact_items.is_empty() {
            output.push_str("\n### Facts\n");
            output.push_str(&fact_items.join("\n"));
            output.push('\n');
        }
        if !decisions.is_empty() {
            output.push_str("\n### Decisions\n");
            output.push_str(&decisions.join("\n"));
            output.push('\n');
        }
        if !other.is_empty() {
            output.push_str("\n### Other\n");
            output.push_str(&other.join("\n"));
            output.push('\n');
        }

        Ok(output)
    }

    pub fn extract_facts(text: &str, category: MemoryCategory) -> Vec<String> {
        match category {
            MemoryCategory::Preference | MemoryCategory::Fact | MemoryCategory::Decision => text
                .lines()
                .map(|l| l.trim())
                .filter(|l| !l.is_empty() && l.len() > 10)
                .map(|l| l.to_string())
                .take(3)
                .collect(),
            _ => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (Arc<MemoryStore>, UserProfile) {
        let store = Arc::new(MemoryStore::new_in_memory(64).unwrap());
        let profile = UserProfile::new(store.clone());
        (store, profile)
    }

    #[test]
    fn add_and_get_facts() {
        let (_store, profile) = setup();
        profile
            .add_fact("static", "preference", "Prefers dark mode")
            .unwrap();
        profile
            .add_fact("static", "fact", "Works on uira project")
            .unwrap();

        let facts = profile.get_facts(None).unwrap();
        assert_eq!(facts.len(), 2);

        let static_facts = profile.get_facts(Some("static")).unwrap();
        assert_eq!(static_facts.len(), 2);
    }

    #[test]
    fn remove_fact() {
        let (_store, profile) = setup();
        profile
            .add_fact("static", "preference", "Likes Rust")
            .unwrap();

        let facts = profile.get_facts(None).unwrap();
        assert_eq!(facts.len(), 1);
        let id = facts[0].id.clone();

        let removed = profile.remove_fact(&id).unwrap();
        assert!(removed);

        let facts = profile.get_facts(None).unwrap();
        assert!(facts.is_empty());
    }

    #[test]
    fn format_profile_groups_by_category() {
        let (_store, profile) = setup();
        profile
            .add_fact("static", "preference", "Prefers dark mode")
            .unwrap();
        profile
            .add_fact("static", "fact", "Uses Rust primarily")
            .unwrap();
        profile
            .add_fact("static", "decision", "Chose rusqlite over LanceDB")
            .unwrap();

        let formatted = profile.format_profile().unwrap();
        assert!(formatted.contains("### Preferences"));
        assert!(formatted.contains("### Facts"));
        assert!(formatted.contains("### Decisions"));
        assert!(formatted.contains("Prefers dark mode"));
    }

    #[test]
    fn format_empty_profile() {
        let (_store, profile) = setup();
        let formatted = profile.format_profile().unwrap();
        assert!(formatted.contains("No user profile"));
    }

    #[test]
    fn extract_facts_from_preference() {
        let facts = UserProfile::extract_facts(
            "I prefer dark mode\nI like using Rust for systems programming",
            MemoryCategory::Preference,
        );
        assert_eq!(facts.len(), 2);
    }

    #[test]
    fn extract_facts_from_other_returns_empty() {
        let facts = UserProfile::extract_facts("some random text here", MemoryCategory::Other);
        assert!(facts.is_empty());
    }
}
