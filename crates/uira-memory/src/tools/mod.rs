pub mod forget_tool;
pub mod profile_tool;
pub mod search_tool;
pub mod store_tool;

pub use forget_tool::memory_forget_tool;
pub use profile_tool::memory_profile_tool;
pub use search_tool::memory_search_tool;
pub use store_tool::memory_store_tool;

use std::sync::Arc;

use crate::config::MemoryConfig;
use crate::embeddings::EmbeddingProvider;
use crate::profile::UserProfile;
use crate::search::HybridSearcher;
use crate::store::MemoryStore;

pub struct MemoryTools {
    pub store: Arc<MemoryStore>,
    pub searcher: Arc<HybridSearcher>,
    pub embedder: Arc<dyn EmbeddingProvider>,
    pub profile: Arc<UserProfile>,
    pub config: MemoryConfig,
}

impl MemoryTools {
    pub fn new(
        store: Arc<MemoryStore>,
        searcher: Arc<HybridSearcher>,
        embedder: Arc<dyn EmbeddingProvider>,
        profile: Arc<UserProfile>,
        config: MemoryConfig,
    ) -> Self {
        Self {
            store,
            searcher,
            embedder,
            profile,
            config,
        }
    }
}
