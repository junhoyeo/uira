use anyhow::Result;
use std::sync::Arc;

use crate::chunker::TextChunker;
use crate::config::MemoryConfig;
use crate::embeddings::{content_hash, EmbeddingProvider};
use crate::profile::UserProfile;
use crate::store::MemoryStore;
use crate::types::{MemoryCategory, MemoryEntry, MemorySource};

pub struct MemoryCaptureHook {
    store: Option<Arc<MemoryStore>>,
    embedder: Option<Arc<dyn EmbeddingProvider>>,
    chunker: TextChunker,
    profile: Option<Arc<UserProfile>>,
    config: MemoryConfig,
}

impl MemoryCaptureHook {
    pub fn new(
        store: Arc<MemoryStore>,
        embedder: Arc<dyn EmbeddingProvider>,
        profile: Arc<UserProfile>,
        config: MemoryConfig,
    ) -> Self {
        let chunker = TextChunker::new(config.chunk_size, config.chunk_overlap);
        Self {
            store: Some(store),
            embedder: Some(embedder),
            chunker,
            profile: Some(profile),
            config,
        }
    }

    pub fn disabled() -> Self {
        Self {
            store: None,
            embedder: None,
            chunker: TextChunker::default(),
            profile: None,
            config: MemoryConfig::default(),
        }
    }

    pub async fn capture(
        &self,
        user_prompt: &str,
        assistant_response: &str,
        session_id: Option<&str>,
    ) -> Result<usize> {
        if !self.config.enabled || !self.config.auto_capture {
            return Ok(0);
        }

        let (store, embedder, profile) = match (&self.store, &self.embedder, &self.profile) {
            (Some(s), Some(e), Some(p)) => (s, e, p),
            _ => return Ok(0),
        };

        if user_prompt.len() < self.config.min_capture_length
            && assistant_response.len() < self.config.min_capture_length
        {
            return Ok(0);
        }

        let combined = format!("User: {user_prompt}\n\nAssistant: {assistant_response}");
        let category = MemoryCategory::detect(&combined);
        let chunks = self.chunker.chunk(&combined);

        let texts: Vec<String> = chunks.clone();
        let embeddings = embedder.embed(&texts).await?;

        let mut stored = 0;
        for (chunk, embedding) in chunks.iter().zip(embeddings.iter()) {
            let mut entry = MemoryEntry::new(
                chunk.clone(),
                MemorySource::Conversation,
                &self.config.container_tag,
            )
            .with_category(category);

            if let Some(sid) = session_id {
                entry = entry.with_session_id(sid);
            }

            store.insert(&entry, embedding)?;

            let hash = content_hash(chunk);
            store.cache_embedding(&hash, embedding, embedder.model_name())?;

            stored += 1;
        }

        if matches!(
            category,
            MemoryCategory::Preference | MemoryCategory::Fact | MemoryCategory::Decision
        ) {
            let facts = UserProfile::extract_facts(&combined, category);
            for fact in facts {
                profile.add_fact("dynamic", category.as_str(), &fact)?;
            }
        }

        Ok(stored)
    }
}

impl Default for MemoryCaptureHook {
    fn default() -> Self {
        Self::disabled()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::embeddings::MockEmbeddingProvider;

    fn setup() -> (
        Arc<MemoryStore>,
        Arc<dyn EmbeddingProvider>,
        Arc<UserProfile>,
        MemoryConfig,
    ) {
        let config = MemoryConfig {
            enabled: true,
            embedding_dimension: 64,
            auto_capture: true,
            min_capture_length: 10,
            chunk_size: 2000,
            chunk_overlap: 200,
            ..Default::default()
        };
        let store = Arc::new(MemoryStore::new_in_memory(64).unwrap());
        let embedder: Arc<dyn EmbeddingProvider> = Arc::new(MockEmbeddingProvider::new(64));
        let profile = Arc::new(UserProfile::new(store.clone()));
        (store, embedder, profile, config)
    }

    #[tokio::test]
    async fn disabled_returns_zero() {
        let hook = MemoryCaptureHook::disabled();
        let count = hook
            .capture("hello world test", "response text here", None)
            .await
            .unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn short_text_skipped() {
        let (store, embedder, profile, config) = setup();
        let hook = MemoryCaptureHook::new(store.clone(), embedder, profile, config);
        let count = hook.capture("hi", "ok", None).await.unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn captures_conversation() {
        let (store, embedder, profile, config) = setup();
        let hook = MemoryCaptureHook::new(store.clone(), embedder, profile, config);

        let count = hook
            .capture(
                "I prefer using dark mode in all my editors",
                "I'll remember that you prefer dark mode for your editing environment.",
                Some("ses_test"),
            )
            .await
            .unwrap();

        assert!(count > 0);
        assert!(store.count().unwrap() > 0);
    }

    #[tokio::test]
    async fn extracts_profile_facts_for_preferences() {
        let (store, embedder, profile, config) = setup();
        let hook = MemoryCaptureHook::new(store.clone(), embedder, profile.clone(), config);

        hook.capture(
            "I prefer using Rust for all my systems programming projects",
            "Noted! Rust is a great choice for systems programming.",
            None,
        )
        .await
        .unwrap();

        let facts = profile.get_facts(None).unwrap();
        assert!(!facts.is_empty());
    }
}
