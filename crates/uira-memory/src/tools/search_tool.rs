use anyhow::Result;
use serde_json::json;
use std::sync::Arc;

use crate::search::HybridSearcher;

pub fn memory_search_tool_schema() -> serde_json::Value {
    json!({
        "type": "object",
        "properties": {
            "query": {
                "type": "string",
                "description": "Search query for finding relevant memories"
            },
            "limit": {
                "type": "integer",
                "description": "Maximum number of results to return (default: 5)"
            },
            "container_tag": {
                "type": "string",
                "description": "Optional container tag to filter results"
            }
        },
        "required": ["query"]
    })
}

pub async fn memory_search_tool(
    input: serde_json::Value,
    searcher: Arc<HybridSearcher>,
) -> Result<String> {
    let query = input
        .get("query")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing required field 'query'"))?;

    let limit = input.get("limit").and_then(|v| v.as_u64()).unwrap_or(5) as usize;

    let container_tag = input.get("container_tag").and_then(|v| v.as_str());

    let results = searcher.search(query, limit, container_tag).await?;

    if results.is_empty() {
        return Ok("No memories found matching your query.".to_string());
    }

    let mut output = format!("Found {} relevant memories:\n\n", results.len());
    for (i, result) in results.iter().enumerate() {
        let age = chrono::Utc::now() - result.entry.created_at;
        let age_str = if age.num_days() > 0 {
            format!("{}d ago", age.num_days())
        } else if age.num_hours() > 0 {
            format!("{}h ago", age.num_hours())
        } else {
            "just now".to_string()
        };

        output.push_str(&format!(
            "{}. [score: {:.3}] [{}] [{}] {}\n   ID: {} | Container: {}\n\n",
            i + 1,
            result.final_score,
            result.entry.category,
            age_str,
            result.entry.content,
            result.entry.id,
            result.entry.container_tag,
        ));
    }

    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::MemoryConfig;
    use crate::embeddings::EmbeddingProvider;
    use crate::embeddings::MockEmbeddingProvider;
    use crate::store::MemoryStore;
    use crate::types::{MemoryEntry, MemorySource};

    #[tokio::test]
    async fn search_empty_returns_no_results() {
        let store = Arc::new(MemoryStore::new_in_memory(64).unwrap());
        let embedder: Arc<dyn EmbeddingProvider> = Arc::new(MockEmbeddingProvider::new(64));
        let config = MemoryConfig {
            embedding_dimension: 64,
            ..Default::default()
        };
        let searcher = Arc::new(crate::search::HybridSearcher::new(store, embedder, &config));

        let input = serde_json::json!({ "query": "anything" });
        let result = memory_search_tool(input, searcher).await.unwrap();
        assert!(result.contains("No memories found"));
    }

    #[tokio::test]
    async fn search_with_results() {
        let store = Arc::new(MemoryStore::new_in_memory(64).unwrap());
        let embedder: Arc<dyn EmbeddingProvider> = Arc::new(MockEmbeddingProvider::new(64));
        let config = MemoryConfig {
            embedding_dimension: 64,
            ..Default::default()
        };

        let entry = MemoryEntry::new("Rust is fast and safe", MemorySource::Manual, "default");
        let emb = embedder.embed(&[entry.content.clone()]).await.unwrap();
        store.insert(&entry, &emb[0]).unwrap();

        let searcher = Arc::new(crate::search::HybridSearcher::new(store, embedder, &config));
        let input = serde_json::json!({ "query": "rust programming", "limit": 3 });
        let result = memory_search_tool(input, searcher).await.unwrap();
        assert!(result.contains("Found"));
    }
}
