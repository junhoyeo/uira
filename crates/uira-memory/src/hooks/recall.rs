use anyhow::Result;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use crate::config::MemoryConfig;
use crate::profile::UserProfile;
use crate::search::HybridSearcher;

pub struct MemoryRecallHook {
    searcher: Option<Arc<HybridSearcher>>,
    profile: Option<Arc<UserProfile>>,
    config: MemoryConfig,
    turn_counter: AtomicUsize,
}

impl MemoryRecallHook {
    pub fn new(
        searcher: Arc<HybridSearcher>,
        profile: Arc<UserProfile>,
        config: MemoryConfig,
    ) -> Self {
        Self {
            searcher: Some(searcher),
            profile: Some(profile),
            config,
            turn_counter: AtomicUsize::new(0),
        }
    }

    pub fn disabled() -> Self {
        Self {
            searcher: None,
            profile: None,
            config: MemoryConfig::default(),
            turn_counter: AtomicUsize::new(0),
        }
    }

    pub async fn recall(&self, query: &str) -> Result<Option<String>> {
        if !self.config.enabled || !self.config.auto_recall {
            return Ok(None);
        }

        let (searcher, profile) = match (&self.searcher, &self.profile) {
            (Some(s), Some(p)) => (s, p),
            _ => return Ok(None),
        };

        if query.len() < 5 {
            return Ok(None);
        }

        let turn = self.turn_counter.fetch_add(1, Ordering::Relaxed);

        let results = searcher
            .search(query, self.config.max_recall_results, None)
            .await?;

        let include_profile =
            self.config.profile_frequency > 0 && turn.is_multiple_of(self.config.profile_frequency);

        if results.is_empty() && !include_profile {
            return Ok(None);
        }

        let mut output = String::from("<memory-context>\n");

        if !results.is_empty() {
            output.push_str("<relevant-memories>\n");
            for result in &results {
                let score = format!("{:.2}", result.final_score);
                let category = result.entry.category.as_str();
                let age = chrono::Utc::now() - result.entry.created_at;
                let age_str = if age.num_days() > 0 {
                    format!("{}d ago", age.num_days())
                } else if age.num_hours() > 0 {
                    format!("{}h ago", age.num_hours())
                } else {
                    "just now".to_string()
                };
                output.push_str(&format!(
                    "- [{}] {} ({}, {})\n",
                    score, result.entry.content, category, age_str
                ));
            }
            output.push_str("</relevant-memories>\n");
        }

        if include_profile {
            let profile_text = profile.format_profile()?;
            output.push_str(&format!(
                "<user-profile>\n{profile_text}\n</user-profile>\n"
            ));
        }

        output.push_str("</memory-context>");

        Ok(Some(output))
    }
}

impl Default for MemoryRecallHook {
    fn default() -> Self {
        Self::disabled()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::embeddings::{EmbeddingProvider, MockEmbeddingProvider};
    use crate::store::MemoryStore;
    use crate::types::{MemoryEntry, MemorySource};

    fn setup() -> (
        Arc<MemoryStore>,
        Arc<HybridSearcher>,
        Arc<UserProfile>,
        MemoryConfig,
    ) {
        let config = MemoryConfig {
            enabled: true,
            embedding_dimension: 64,
            auto_recall: true,
            max_recall_results: 3,
            profile_frequency: 2,
            ..Default::default()
        };
        let store = Arc::new(MemoryStore::new_in_memory(64).unwrap());
        let embedder: Arc<dyn crate::embeddings::EmbeddingProvider> =
            Arc::new(MockEmbeddingProvider::new(64));
        let searcher = Arc::new(HybridSearcher::new(
            store.clone(),
            embedder.clone(),
            &config,
        ));
        let profile = Arc::new(UserProfile::new(store.clone()));
        (store, searcher, profile, config)
    }

    #[tokio::test]
    async fn disabled_returns_none() {
        let hook = MemoryRecallHook::disabled();
        let result = hook.recall("test query").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn short_query_returns_none() {
        let (_store, searcher, profile, config) = setup();
        let hook = MemoryRecallHook::new(searcher, profile, config);
        let result = hook.recall("hi").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn recall_with_memories() {
        let (store, searcher, profile, config) = setup();
        let embedder: Arc<dyn EmbeddingProvider> = Arc::new(MockEmbeddingProvider::new(64));

        let entry = MemoryEntry::new(
            "Rust is great for systems programming",
            MemorySource::Manual,
            "default",
        );
        let emb = embedder.embed(&[entry.content.clone()]).await.unwrap();
        store.insert(&entry, &emb[0]).unwrap();

        let hook = MemoryRecallHook::new(searcher, profile, config);
        let result = hook.recall("tell me about Rust programming").await.unwrap();
        assert!(result.is_some());
        let text = result.unwrap();
        assert!(text.contains("<memory-context>"));
        assert!(text.contains("</memory-context>"));
    }

    #[tokio::test]
    async fn profile_injected_at_frequency() {
        let (_store, searcher, profile, config) = setup();
        profile
            .add_fact("static", "preference", "Prefers dark mode")
            .unwrap();

        let hook = MemoryRecallHook::new(searcher, profile, config);

        let r0 = hook.recall("first query here").await.unwrap();
        if let Some(text) = &r0 {
            assert!(text.contains("<user-profile>"));
        }

        let _r1 = hook.recall("second query here").await.unwrap();
        let r2 = hook.recall("third query here").await.unwrap();
        if let Some(text) = &r2 {
            assert!(text.contains("<user-profile>"));
        }
    }
}
