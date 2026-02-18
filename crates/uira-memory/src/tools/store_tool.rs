use anyhow::Result;
use serde_json::json;
use std::sync::Arc;

use crate::chunker::TextChunker;
use crate::config::MemoryConfig;
use crate::embeddings::{content_hash, EmbeddingProvider};
use crate::store::MemoryStore;
use crate::types::{MemoryCategory, MemoryEntry, MemorySource};

pub fn memory_store_tool_schema() -> serde_json::Value {
    json!({
        "type": "object",
        "properties": {
            "content": {
                "type": "string",
                "description": "The text content to store in memory"
            },
            "container_tag": {
                "type": "string",
                "description": "Optional namespace/container tag for organizing memories"
            },
            "category": {
                "type": "string",
                "enum": ["preference", "fact", "decision", "entity", "other"],
                "description": "Optional category override. Auto-detected if not specified."
            }
        },
        "required": ["content"]
    })
}

pub async fn memory_store_tool(
    input: serde_json::Value,
    store: Arc<MemoryStore>,
    embedder: Arc<dyn EmbeddingProvider>,
    config: &MemoryConfig,
) -> Result<String> {
    let content = input
        .get("content")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing required field 'content'"))?;

    let container_tag = input
        .get("container_tag")
        .and_then(|v| v.as_str())
        .unwrap_or(&config.container_tag);

    let category = input
        .get("category")
        .and_then(|v| v.as_str())
        .map(MemoryCategory::from_str_lossy);

    let chunker = TextChunker::new(config.chunk_size, config.chunk_overlap);
    let chunks = chunker.chunk(content);

    let texts: Vec<String> = chunks.clone();
    let embeddings = embedder.embed(&texts).await?;

    let mut ids = Vec::new();
    for (chunk, embedding) in chunks.iter().zip(embeddings.iter()) {
        let mut entry = MemoryEntry::new(chunk.clone(), MemorySource::Manual, container_tag);
        if let Some(cat) = category {
            entry = entry.with_category(cat);
        }

        store.insert(&entry, embedding)?;
        let hash = content_hash(chunk);
        store.cache_embedding(&hash, embedding, embedder.model_name())?;
        ids.push(entry.id);
    }

    Ok(format!(
        "Stored {} memory chunk(s) with IDs: {}",
        ids.len(),
        ids.join(", ")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::embeddings::MockEmbeddingProvider;
    use serde_json::json;

    #[tokio::test]
    async fn store_simple_content() {
        let store = Arc::new(MemoryStore::new_in_memory(64).unwrap());
        let embedder: Arc<dyn EmbeddingProvider> = Arc::new(MockEmbeddingProvider::new(64));
        let config = MemoryConfig {
            embedding_dimension: 64,
            ..Default::default()
        };

        let input = json!({ "content": "I prefer using Rust for systems programming" });
        let result = memory_store_tool(input, store.clone(), embedder, &config)
            .await
            .unwrap();
        assert!(result.contains("Stored 1 memory chunk"));
        assert_eq!(store.count().unwrap(), 1);
    }

    #[tokio::test]
    async fn store_with_custom_category() {
        let store = Arc::new(MemoryStore::new_in_memory(64).unwrap());
        let embedder: Arc<dyn EmbeddingProvider> = Arc::new(MockEmbeddingProvider::new(64));
        let config = MemoryConfig {
            embedding_dimension: 64,
            ..Default::default()
        };

        let input = json!({
            "content": "some content here",
            "category": "decision",
            "container_tag": "work"
        });
        let result = memory_store_tool(input, store.clone(), embedder, &config)
            .await
            .unwrap();
        assert!(result.contains("Stored 1"));

        let entries = store.list(Some("work"), 10).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].category, MemoryCategory::Decision);
    }
}
