use std::sync::Arc;

use serde_json::json;
use uira_memory::tools::{
    memory_forget_tool, memory_profile_tool, memory_search_tool, memory_store_tool,
};
use uira_memory::{
    EmbeddingProvider, MemoryConfig, MemorySystem, MemoryTools, MockEmbeddingProvider,
};

fn test_config() -> MemoryConfig {
    MemoryConfig {
        enabled: true,
        embedding_dimension: 64,
        auto_recall: true,
        auto_capture: true,
        min_capture_length: 10,
        max_recall_results: 5,
        profile_frequency: 2,
        chunk_size: 2000,
        chunk_overlap: 200,
        ..Default::default()
    }
}

fn test_system() -> MemorySystem {
    let config = test_config();
    let embedder: Arc<dyn EmbeddingProvider> = Arc::new(MockEmbeddingProvider::new(64));
    MemorySystem::new_in_memory(&config, embedder).unwrap()
}

#[tokio::test]
async fn test_system_initialization() {
    let system = test_system();
    let _tools: MemoryTools = system.tools();
    assert_eq!(system.store.count().unwrap(), 0);
}

#[tokio::test]
async fn test_store_and_search_cycle() {
    let system = test_system();
    let config = test_config();

    let stored = memory_store_tool(
        json!({
            "content": "I prefer writing Rust services with Tokio for async workloads"
        }),
        system.store.clone(),
        system.embedder.clone(),
        &config,
    )
    .await
    .unwrap();
    assert!(stored.contains("Stored 1 memory chunk"));

    let searched = memory_search_tool(
        json!({
            "query": "Rust async workloads",
            "limit": 5
        }),
        system.searcher.clone(),
    )
    .await
    .unwrap();

    assert!(searched.contains("Found"));
    assert!(searched.contains("Tokio"));
}

#[tokio::test]
async fn test_recall_hook_returns_context() {
    let system = test_system();
    let config = test_config();

    memory_store_tool(
        json!({
            "content": "I am planning to migrate my memory retrieval pipeline to Rust"
        }),
        system.store.clone(),
        system.embedder.clone(),
        &config,
    )
    .await
    .unwrap();

    let recalled = system
        .recall_hook
        .recall("How should I evolve my memory retrieval pipeline")
        .await
        .unwrap();

    let context = recalled.expect("expected recall context");
    assert!(context.contains("<memory-context>"));
    assert!(context.contains("memory retrieval pipeline"));
}

#[tokio::test]
async fn test_recall_hook_empty_db() {
    let system = test_system();

    let recalled = system
        .recall_hook
        .recall("What should I remember about this session")
        .await
        .unwrap();

    match recalled {
        None => {}
        Some(context) => {
            assert!(context.contains("<memory-context>"));
            assert!(context.contains("<user-profile>"));
            assert!(context.contains("No user profile facts"));
        }
    }
}

#[tokio::test]
async fn test_capture_hook_stores_memories() {
    let system = test_system();
    let before = system.store.count().unwrap();

    let captured = system
        .capture_hook
        .capture(
            "Please remember that I am working on memory ranking for my Rust agent.",
            "Understood. I will keep your memory ranking work in context.",
            Some("session_capture_1"),
        )
        .await
        .unwrap();

    let after = system.store.count().unwrap();
    assert!(captured > 0);
    assert!(after > before);
}

#[tokio::test]
async fn test_capture_hook_extracts_profile_facts() {
    let system = test_system();

    system
        .capture_hook
        .capture(
            "I prefer concise commit messages and Rust-first implementation patterns.",
            "Noted. I will keep those preferences in mind.",
            Some("session_capture_2"),
        )
        .await
        .unwrap();

    let facts = system.profile.get_facts(None).unwrap();
    assert!(!facts.is_empty());
}

#[tokio::test]
async fn test_forget_tool() {
    let system = test_system();
    let config = test_config();

    memory_store_tool(
        json!({
            "content": "Temporary memory that should be forgotten"
        }),
        system.store.clone(),
        system.embedder.clone(),
        &config,
    )
    .await
    .unwrap();

    let before = system.store.count().unwrap();
    let id = system.store.list(None, 10).unwrap()[0].id.clone();

    let output = memory_forget_tool(json!({ "id": id }), system.store.clone())
        .await
        .unwrap();
    let after = system.store.count().unwrap();

    assert!(output.contains("Deleted memory"));
    assert!(after < before);
}

#[tokio::test]
async fn test_profile_tool_add_and_view() {
    let system = test_system();

    let add_output = memory_profile_tool(
        json!({
            "action": "add",
            "category": "preference",
            "fact": "Prefers strongly typed APIs"
        }),
        system.profile.clone(),
    )
    .await
    .unwrap();
    assert!(add_output.contains("Added profile fact"));

    let view_output = memory_profile_tool(json!({ "action": "view" }), system.profile.clone())
        .await
        .unwrap();
    assert!(view_output.contains("Prefers strongly typed APIs"));
}

#[tokio::test]
async fn test_full_lifecycle() {
    let system = test_system();
    let config = test_config();

    memory_store_tool(
        json!({
            "content": "Use deterministic embeddings in memory integration tests"
        }),
        system.store.clone(),
        system.embedder.clone(),
        &config,
    )
    .await
    .unwrap();
    let stored_id = system.store.list(None, 10).unwrap()[0].id.clone();

    let search_output = memory_search_tool(
        json!({ "query": "deterministic embeddings" }),
        system.searcher.clone(),
    )
    .await
    .unwrap();
    assert!(search_output.contains("deterministic embeddings"));

    let recall_output = system
        .recall_hook
        .recall("What do we remember about embeddings in tests")
        .await
        .unwrap();
    let recall_context = recall_output.expect("expected recall context");
    assert!(recall_context.contains("<memory-context>"));

    let captured = system
        .capture_hook
        .capture(
            "I prefer fast test suites with in-memory databases.",
            "Noted. We'll favor in-memory systems for speed.",
            Some("session_full_lifecycle"),
        )
        .await
        .unwrap();
    assert!(captured > 0);

    let profile_output = memory_profile_tool(json!({ "action": "view" }), system.profile.clone())
        .await
        .unwrap();
    assert!(profile_output.contains("User Profile") || profile_output.contains("No user profile"));

    let forget_output = memory_forget_tool(json!({ "id": stored_id }), system.store.clone())
        .await
        .unwrap();
    assert!(forget_output.contains("Deleted memory"));
}

#[tokio::test]
async fn test_multiple_memories_search_ranking() {
    let system = test_system();
    let config = test_config();

    let memories = [
        "Rust async runtime with Tokio for high-throughput services",
        "Rust memory safety and ownership rules prevent many bugs",
        "A grocery list with apples and oranges",
        "Advanced Rust trait bounds for async abstractions",
    ];

    for content in memories {
        memory_store_tool(
            json!({ "content": content }),
            system.store.clone(),
            system.embedder.clone(),
            &config,
        )
        .await
        .unwrap();
    }

    let results = system
        .searcher
        .search("rust async services", 4, None)
        .await
        .unwrap();
    assert!(results.len() >= 3);

    for pair in results.windows(2) {
        assert!(pair[0].final_score >= pair[1].final_score);
    }
}
