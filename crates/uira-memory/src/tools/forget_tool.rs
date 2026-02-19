use anyhow::Result;
use serde_json::json;
use std::sync::Arc;

use crate::store::MemoryStore;

pub fn memory_forget_tool_schema() -> serde_json::Value {
    json!({
        "type": "object",
        "properties": {
            "id": {
                "type": "string",
                "description": "Specific memory ID to delete"
            },
            "query": {
                "type": "string",
                "description": "Search query to find and delete matching memories"
            }
        }
    })
}

pub async fn memory_forget_tool(
    input: serde_json::Value,
    store: Arc<MemoryStore>,
) -> Result<String> {
    if let Some(id) = input.get("id").and_then(|v| v.as_str()) {
        let deleted = store.delete(id)?;
        if deleted {
            return Ok(format!("Deleted memory with ID: {id}"));
        } else {
            return Ok(format!("No memory found with ID: {id}"));
        }
    }

    if let Some(query) = input.get("query").and_then(|v| v.as_str()) {
        let matches = store.fts_search(query, 10)?;
        if matches.is_empty() {
            return Ok(format!("No memories found matching query: {query}"));
        }

        let ids: Vec<String> = matches.into_iter().map(|(id, _)| id).collect();
        let count = store.delete_by_ids(&ids)?;
        return Ok(format!("Deleted {count} memories matching query: {query}"));
    }

    Err(anyhow::anyhow!(
        "Please provide either 'id' or 'query' to specify which memories to forget."
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::embeddings::{EmbeddingProvider, MockEmbeddingProvider};
    use crate::types::{MemoryEntry, MemorySource};

    #[tokio::test]
    async fn forget_by_id() {
        let store = Arc::new(MemoryStore::new_in_memory(64).unwrap());
        let embedder = MockEmbeddingProvider::new(64);

        let entry = MemoryEntry::new("test memory", MemorySource::Manual, "default");
        let id = entry.id.clone();
        let emb = embedder.embed(&[entry.content.clone()]).await.unwrap();
        store.insert(&entry, &emb[0]).unwrap();

        let input = serde_json::json!({ "id": id });
        let result = memory_forget_tool(input, store.clone()).await.unwrap();
        assert!(result.contains("Deleted memory"));
        assert_eq!(store.count().unwrap(), 0);
    }

    #[tokio::test]
    async fn forget_by_query() {
        let store = Arc::new(MemoryStore::new_in_memory(64).unwrap());
        let embedder = MockEmbeddingProvider::new(64);

        let e1 = MemoryEntry::new("rust programming language", MemorySource::Manual, "default");
        let e2 = MemoryEntry::new("python scripting", MemorySource::Manual, "default");
        let emb1 = embedder.embed(&[e1.content.clone()]).await.unwrap();
        let emb2 = embedder.embed(&[e2.content.clone()]).await.unwrap();
        store.insert(&e1, &emb1[0]).unwrap();
        store.insert(&e2, &emb2[0]).unwrap();

        let input = serde_json::json!({ "query": "rust" });
        let result = memory_forget_tool(input, store.clone()).await.unwrap();
        assert!(result.contains("Deleted 1"));
        assert_eq!(store.count().unwrap(), 1);
    }

    #[tokio::test]
    async fn forget_no_args() {
        let store = Arc::new(MemoryStore::new_in_memory(64).unwrap());
        let input = serde_json::json!({});
        let result = memory_forget_tool(input, store).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Please provide"));
    }
}
