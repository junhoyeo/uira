pub mod chunker;
pub mod config;
pub mod embeddings;
pub mod hooks;
pub mod profile;
pub mod search;
pub mod store;
pub mod tools;
pub mod types;

use anyhow::Result;
use std::sync::Arc;

pub use chunker::TextChunker;
pub use config::MemoryConfig;
pub use embeddings::{EmbeddingProvider, MockEmbeddingProvider, OpenAIEmbeddingProvider};
pub use hooks::{MemoryCaptureHook, MemoryRecallHook};
pub use profile::UserProfile;
pub use search::HybridSearcher;
pub use store::MemoryStore;
pub use tools::MemoryTools;

pub struct MemorySystem {
    pub store: Arc<MemoryStore>,
    pub embedder: Arc<dyn EmbeddingProvider>,
    pub searcher: Arc<HybridSearcher>,
    pub profile: Arc<UserProfile>,
    pub recall_hook: MemoryRecallHook,
    pub capture_hook: MemoryCaptureHook,
}

impl MemorySystem {
    pub fn new(config: &MemoryConfig, embedder: Arc<dyn EmbeddingProvider>) -> Result<Self> {
        let store = Arc::new(MemoryStore::new(config)?);
        let searcher = Arc::new(HybridSearcher::new(store.clone(), embedder.clone(), config));
        let profile = Arc::new(UserProfile::new(store.clone()));

        let recall_hook = MemoryRecallHook::new(searcher.clone(), profile.clone(), config.clone());
        let capture_hook = MemoryCaptureHook::new(
            store.clone(),
            embedder.clone(),
            profile.clone(),
            config.clone(),
        );

        Ok(Self {
            store,
            embedder,
            searcher,
            profile,
            recall_hook,
            capture_hook,
        })
    }

    pub fn new_in_memory(
        config: &MemoryConfig,
        embedder: Arc<dyn EmbeddingProvider>,
    ) -> Result<Self> {
        let store = Arc::new(MemoryStore::new_in_memory(config.embedding_dimension)?);
        let searcher = Arc::new(HybridSearcher::new(store.clone(), embedder.clone(), config));
        let profile = Arc::new(UserProfile::new(store.clone()));

        let recall_hook = MemoryRecallHook::new(searcher.clone(), profile.clone(), config.clone());
        let capture_hook = MemoryCaptureHook::new(
            store.clone(),
            embedder.clone(),
            profile.clone(),
            config.clone(),
        );

        Ok(Self {
            store,
            embedder,
            searcher,
            profile,
            recall_hook,
            capture_hook,
        })
    }

    pub fn tools(&self) -> MemoryTools {
        MemoryTools::new(
            self.store.clone(),
            self.searcher.clone(),
            self.embedder.clone(),
            self.profile.clone(),
            MemoryConfig::default(),
        )
    }
}
