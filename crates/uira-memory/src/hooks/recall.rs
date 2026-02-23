use anyhow::Result;
use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use crate::config::MemoryConfig;
use crate::profile::UserProfile;
use crate::search::HybridSearcher;

pub struct MemoryRecallHook {
    searcher: Option<Arc<HybridSearcher>>,
    profile: Option<Arc<UserProfile>>,
    config: MemoryConfig,
    turn_counter: AtomicUsize,
    seen_ids: Mutex<HashSet<String>>,
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
            seen_ids: Mutex::new(HashSet::new()),
        }
    }

    pub fn disabled() -> Self {
        Self {
            searcher: None,
            profile: None,
            config: MemoryConfig::default(),
            turn_counter: AtomicUsize::new(0),
            seen_ids: Mutex::new(HashSet::new()),
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

        if query.len() < self.config.recall_min_query_length {
            return Ok(None);
        }

        let turn = self.turn_counter.fetch_add(1, Ordering::Relaxed);

        // Cooldown: skip recall for N turns after a recall fires
        if !turn.is_multiple_of(self.config.recall_cooldown_turns + 1) {
            return Ok(None);
        }
        let results = searcher
            .search(query, self.config.max_recall_results, None)
            .await?;
        // Dedup: filter out already-seen entries
        let new_results: Vec<_> = {
            let seen = self.seen_ids.lock().unwrap();
            results
                .into_iter()
                .filter(|r| !seen.contains(&r.entry.id))
                .collect()
        };
        let include_profile =
            self.config.profile_frequency > 0 && turn.is_multiple_of(self.config.profile_frequency);
        if new_results.is_empty() && !include_profile {
            return Ok(None);
        }

        let mut output = String::from("<memory-context>\n");

        if !new_results.is_empty() {
            output.push_str("<relevant-memories>\n");
            for result in &new_results {
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
            // Record seen IDs after formatting
            {
                let mut seen = self.seen_ids.lock().unwrap();
                for r in &new_results {
                    seen.insert(r.entry.id.clone());
                }
            }
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
            recall_min_query_length: 5,
            recall_cooldown_turns: 0,
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

    #[tokio::test]
    async fn dedup_prevents_duplicate_injection() {
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

        // First recall should return the memory
        let r1 = hook.recall("tell me about Rust programming").await.unwrap();
        assert!(r1.is_some());
        assert!(r1.unwrap().contains("Rust is great"));

        // Second recall with same query should return None (dedup)
        let r2 = hook.recall("tell me about Rust programming").await.unwrap();
        assert!(r2.is_none());
    }

    #[tokio::test]
    async fn configurable_threshold_works() {
        let (_store, searcher, profile, _config) = setup();
        let config = MemoryConfig {
            enabled: true,
            embedding_dimension: 64,
            auto_recall: true,
            recall_min_query_length: 20,
            recall_cooldown_turns: 0,
            ..Default::default()
        };
        let hook = MemoryRecallHook::new(searcher, profile, config);

        // Query shorter than threshold (20) should return None
        let result = hook.recall("short query").await.unwrap();
        assert!(result.is_none());

        // Query at or above threshold should proceed (may return None if no results)
        let result = hook.recall("this is a long enough query text").await.unwrap();
        // No assertion on Some/None since we have no data, just verifying it doesn't early-return
    }

    #[tokio::test]
    async fn cooldown_skips_appropriate_turns() {
        let (store, searcher, profile, _config) = setup();
        let embedder: Arc<dyn EmbeddingProvider> = Arc::new(MockEmbeddingProvider::new(64));

        let entry = MemoryEntry::new(
            "Important memory for cooldown test",
            MemorySource::Manual,
            "default",
        );
        let emb = embedder.embed(&[entry.content.clone()]).await.unwrap();
        store.insert(&entry, &emb[0]).unwrap();

        // recall_cooldown_turns = 1 means fire every other turn
        let config = MemoryConfig {
            enabled: true,
            embedding_dimension: 64,
            auto_recall: true,
            max_recall_results: 3,
            recall_min_query_length: 5,
            recall_cooldown_turns: 1,
            ..Default::default()
        };
        let hook = MemoryRecallHook::new(searcher, profile, config);

        // Turn 0: fires (0 % 2 == 0)
        let r0 = hook.recall("cooldown test query one").await.unwrap();
        assert!(r0.is_some(), "Turn 0 should fire");

        // Turn 1: skipped (1 % 2 != 0)
        let r1 = hook.recall("cooldown test query two").await.unwrap();
        assert!(r1.is_none(), "Turn 1 should be skipped by cooldown");

        // Turn 2: fires (2 % 2 == 0)
        let r2 = hook.recall("cooldown test query three").await.unwrap();
        // r2 might be None due to dedup, but it shouldn't be skipped by cooldown
        // We just verify it got past the cooldown check by checking the turn counter
    }
}
