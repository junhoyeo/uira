use async_trait::async_trait;
use serde_json::json;
use std::sync::Arc;
use uira_core::{JsonSchema, ToolOutput};

use crate::tools::{Tool, ToolContext, ToolError};
use uira_memory::config::MemoryConfig;
use uira_memory::embeddings::EmbeddingProvider;
use uira_memory::profile::UserProfile;
use uira_memory::search::HybridSearcher;
use uira_memory::store::MemoryStore;
use uira_memory::tools::{
    memory_forget_tool, memory_profile_tool, memory_search_tool, memory_store_tool,
};

pub struct MemoryStoreTool {
    store: Arc<MemoryStore>,
    embedder: Arc<dyn EmbeddingProvider>,
    config: MemoryConfig,
}

impl MemoryStoreTool {
    pub fn new(
        store: Arc<MemoryStore>,
        embedder: Arc<dyn EmbeddingProvider>,
        config: MemoryConfig,
    ) -> Self {
        Self {
            store,
            embedder,
            config,
        }
    }
}

#[async_trait]
impl Tool for MemoryStoreTool {
    fn name(&self) -> &str {
        "memory_store"
    }

    fn description(&self) -> &str {
        "Store information in long-term memory for future recall. Use this to remember user preferences, important facts, decisions, and context that should persist across sessions."
    }

    fn schema(&self) -> JsonSchema {
        JsonSchema::object()
            .with_properties(json!({
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
            }))
            .with_required(vec!["content".to_string()])
    }

    async fn execute(
        &self,
        input: serde_json::Value,
        _ctx: &ToolContext,
    ) -> Result<ToolOutput, ToolError> {
        match memory_store_tool(
            input,
            self.store.clone(),
            self.embedder.clone(),
            &self.config,
        )
        .await
        {
            Ok(result) => Ok(ToolOutput::text(result)),
            Err(e) => Err(ToolError::ExecutionFailed {
                message: e.to_string(),
            }),
        }
    }
}

pub struct MemorySearchTool {
    searcher: Arc<HybridSearcher>,
}

impl MemorySearchTool {
    pub fn new(searcher: Arc<HybridSearcher>) -> Self {
        Self { searcher }
    }
}

#[async_trait]
impl Tool for MemorySearchTool {
    fn name(&self) -> &str {
        "memory_search"
    }

    fn description(&self) -> &str {
        "Search long-term memory for relevant information. Uses hybrid vector + full-text search with temporal decay and diversity reranking."
    }

    fn schema(&self) -> JsonSchema {
        JsonSchema::object()
            .with_properties(json!({
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
            }))
            .with_required(vec!["query".to_string()])
    }

    async fn execute(
        &self,
        input: serde_json::Value,
        _ctx: &ToolContext,
    ) -> Result<ToolOutput, ToolError> {
        match memory_search_tool(input, self.searcher.clone()).await {
            Ok(result) => Ok(ToolOutput::text(result)),
            Err(e) => Err(ToolError::ExecutionFailed {
                message: e.to_string(),
            }),
        }
    }
}

pub struct MemoryForgetTool {
    store: Arc<MemoryStore>,
}

impl MemoryForgetTool {
    pub fn new(store: Arc<MemoryStore>) -> Self {
        Self { store }
    }
}

#[async_trait]
impl Tool for MemoryForgetTool {
    fn name(&self) -> &str {
        "memory_forget"
    }

    fn description(&self) -> &str {
        "Delete specific memories by ID or by search query. Use to remove outdated or incorrect information from memory."
    }

    fn schema(&self) -> JsonSchema {
        JsonSchema::object().with_properties(json!({
            "id": {
                "type": "string",
                "description": "Specific memory ID to delete"
            },
            "query": {
                "type": "string",
                "description": "Search query to find and delete matching memories"
            }
        }))
    }

    async fn execute(
        &self,
        input: serde_json::Value,
        _ctx: &ToolContext,
    ) -> Result<ToolOutput, ToolError> {
        match memory_forget_tool(input, self.store.clone()).await {
            Ok(result) => Ok(ToolOutput::text(result)),
            Err(e) => Err(ToolError::ExecutionFailed {
                message: e.to_string(),
            }),
        }
    }
}

pub struct MemoryProfileTool {
    profile: Arc<UserProfile>,
}

impl MemoryProfileTool {
    pub fn new(profile: Arc<UserProfile>) -> Self {
        Self { profile }
    }
}

#[async_trait]
impl Tool for MemoryProfileTool {
    fn name(&self) -> &str {
        "memory_profile"
    }

    fn description(&self) -> &str {
        "View or manage the user profile. The profile contains learned facts about user preferences, background, and common patterns."
    }

    fn schema(&self) -> JsonSchema {
        JsonSchema::object().with_properties(json!({
            "action": {
                "type": "string",
                "enum": ["view", "add", "remove"],
                "description": "Action to perform on user profile. Default: view"
            },
            "fact": {
                "type": "string",
                "description": "Fact content to add (required for 'add' action)"
            },
            "category": {
                "type": "string",
                "enum": ["preference", "fact", "decision"],
                "description": "Category of the fact (for 'add' action, default: fact)"
            },
            "fact_id": {
                "type": "string",
                "description": "ID of fact to remove (required for 'remove' action)"
            }
        }))
    }

    async fn execute(
        &self,
        input: serde_json::Value,
        _ctx: &ToolContext,
    ) -> Result<ToolOutput, ToolError> {
        match memory_profile_tool(input, self.profile.clone()).await {
            Ok(result) => Ok(ToolOutput::text(result)),
            Err(e) => Err(ToolError::ExecutionFailed {
                message: e.to_string(),
            }),
        }
    }
}
