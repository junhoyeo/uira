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
    async fn embed_and_store(
        store: Arc<MemoryStore>,
        embedder: Arc<dyn EmbeddingProvider>,
        stored_chunks: Vec<(i64, String)>,
    ) -> Result<()> {
        let texts: Vec<String> = stored_chunks.iter().map(|(_, text)| text.clone()).collect();
        let embeddings = embedder.embed(&texts).await?;

        for ((row_id, text), embedding) in stored_chunks.iter().zip(embeddings.iter()) {
            if let Err(err) = store.update_embedding(*row_id, embedding) {
                tracing::warn!(
                    error = %err,
                    row_id,
                    "failed to update memory embedding"
                );
                continue;
            }

            let hash = content_hash(text);
            if let Err(err) = store.cache_embedding(&hash, embedding, embedder.model_name()) {
                tracing::warn!(
                    error = %err,
                    row_id,
                    "failed to cache memory embedding"
                );
            }
        }

        if embeddings.len() != stored_chunks.len() {
            tracing::warn!(
                expected = stored_chunks.len(),
                actual = embeddings.len(),
                "embedding count mismatch for captured chunks"
            );
        }

        Ok(())
    }

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

        let mut stored = 0;
        let mut stored_chunks = Vec::new();

        for chunk in &chunks {
            let mut entry = MemoryEntry::new(
                chunk.clone(),
                MemorySource::Conversation,
                &self.config.container_tag,
            )
            .with_category(category);

            if let Some(sid) = session_id {
                entry = entry.with_session_id(sid);
            }

            let row_id = store.store_text_only(&entry)?;
            stored_chunks.push((row_id, chunk.clone()));

            stored += 1;
        }

        if !stored_chunks.is_empty() {
            let store = Arc::clone(store);
            let embedder = Arc::clone(embedder);

            if let Ok(handle) = tokio::runtime::Handle::try_current() {
                let _embedding_task = handle.spawn(async move {
                    if let Err(err) = Self::embed_and_store(store, embedder, stored_chunks).await {
                        tracing::warn!(error = %err, "failed to embed captured memories");
                    }
                });
            } else if let Err(err) = Self::embed_and_store(store, embedder, stored_chunks).await {
                tracing::warn!(
                    error = %err,
                    "failed to embed captured memories in sync fallback"
                );
            }
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
    use async_trait::async_trait;
    use std::time::{Duration, Instant};

    struct SlowEmbeddingProvider {
        dimension: usize,
        delay: Duration,
    }

    #[async_trait]
    impl EmbeddingProvider for SlowEmbeddingProvider {
        async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
            tokio::time::sleep(self.delay).await;
            Ok(texts.iter().map(|_| vec![0.5; self.dimension]).collect())
        }

        fn dimension(&self) -> usize {
            self.dimension
        }

        fn model_name(&self) -> &str {
            "slow-mock"
        }
    }

    struct FailingEmbeddingProvider {
        dimension: usize,
    }

    #[async_trait]
    impl EmbeddingProvider for FailingEmbeddingProvider {
        async fn embed(&self, _texts: &[String]) -> Result<Vec<Vec<f32>>> {
            anyhow::bail!("embedding failure")
        }

        fn dimension(&self) -> usize {
            self.dimension
        }

        fn model_name(&self) -> &str {
            "failing-mock"
        }
    }

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

    #[tokio::test]
    async fn capture_returns_before_embedding_finishes() {
        let (store, _embedder, profile, config) = setup();
        let slow_embedder: Arc<dyn EmbeddingProvider> = Arc::new(SlowEmbeddingProvider {
            dimension: 64,
            delay: Duration::from_millis(200),
        });
        let hook = MemoryCaptureHook::new(store.clone(), slow_embedder, profile, config);

        let start = Instant::now();
        let count = hook
            .capture(
                "I prefer asynchronous pipelines that avoid data loss on crashes",
                "Great idea - we can store text first and embed in the background.",
                Some("ses_async"),
            )
            .await
            .unwrap();
        let elapsed = start.elapsed();

        assert!(count > 0);
        assert!(elapsed < Duration::from_millis(150));
        assert!(store.count().unwrap() >= count);
    }

    #[tokio::test]
    async fn embedding_failure_does_not_block_text_storage() {
        let (store, _embedder, profile, config) = setup();
        let failing_embedder: Arc<dyn EmbeddingProvider> =
            Arc::new(FailingEmbeddingProvider { dimension: 64 });
        let hook = MemoryCaptureHook::new(store.clone(), failing_embedder, profile, config);

        let count = hook
            .capture(
                "I want robust persistence even when embedding providers fail",
                "We'll persist text immediately and tolerate embedding failures.",
                Some("ses_failure"),
            )
            .await
            .unwrap();

        assert!(count > 0);
        assert!(store.count().unwrap() >= count);
    }
}
