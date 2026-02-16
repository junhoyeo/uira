//! Tests for session persistence.

use std::path::PathBuf;
use tempfile::TempDir;
use uira_agent::session::{
    extract_messages, get_last_turn, get_total_usage, EventWrapper, SessionItem, SessionMessage,
    SessionMetaLine, SessionRecorder,
};
use uira_core::{Message, ThreadEvent, TokenUsage};

fn make_test_meta() -> SessionMetaLine {
    SessionMetaLine::new(
        "test-session-123",
        "claude-3-opus",
        "anthropic",
        PathBuf::from("/test/project"),
        "workspace-write",
    )
}

#[test]
fn test_session_item_serialization() {
    // Test SessionMeta
    let meta = make_test_meta();
    let item = SessionItem::SessionMeta(meta.clone());
    let json = serde_json::to_string(&item).unwrap();
    assert!(json.contains("\"type\":\"session_meta\""));
    assert!(json.contains("test-session-123"));

    // Test ToolCall
    let tool_item = SessionItem::ToolCall {
        id: "tc_123".to_string(),
        name: "read_file".to_string(),
        input: serde_json::json!({"path": "/tmp/test.txt"}),
    };
    let json = serde_json::to_string(&tool_item).unwrap();
    assert!(json.contains("\"type\":\"tool_call\""));
    assert!(json.contains("read_file"));

    // Test TurnContext
    let turn_item = SessionItem::TurnContext {
        turn: 5,
        usage: TokenUsage {
            input_tokens: 100,
            output_tokens: 50,
            cache_read_tokens: 10,
            cache_creation_tokens: 0,
        },
    };
    let json = serde_json::to_string(&turn_item).unwrap();
    assert!(json.contains("\"turn\":5"));
    assert!(json.contains("\"input_tokens\":100"));
}

#[test]
fn test_session_save_load() {
    // Create a temp directory for tests
    let temp_dir = TempDir::new().unwrap();
    let meta = SessionMetaLine {
        thread_id: "test-save-load".to_string(),
        timestamp: chrono::Utc::now(),
        model: "test-model".to_string(),
        provider: "test-provider".to_string(),
        cwd: temp_dir.path().to_path_buf(),
        sandbox_policy: "test-policy".to_string(),
        git_commit: None,
        git_branch: None,
        turns: 0,
        total_usage: TokenUsage::default(),
        parent_id: None,
        forked_from_message: None,
        fork_count: 0,
    };

    let session_path = temp_dir.path().join("test-session.jsonl");
    let mut file = std::fs::File::create(&session_path).unwrap();

    // Write items manually
    use std::io::Write;
    writeln!(
        file,
        "{}",
        serde_json::to_string(&SessionItem::SessionMeta(meta)).unwrap()
    )
    .unwrap();
    writeln!(
        file,
        "{}",
        serde_json::to_string(&SessionItem::Message(SessionMessage::new(Message::user(
            "Hello"
        ))))
        .unwrap()
    )
    .unwrap();
    writeln!(
        file,
        "{}",
        serde_json::to_string(&SessionItem::ToolCall {
            id: "tc_1".to_string(),
            name: "bash".to_string(),
            input: serde_json::json!({"command": "ls"}),
        })
        .unwrap()
    )
    .unwrap();
    writeln!(
        file,
        "{}",
        serde_json::to_string(&SessionItem::TurnContext {
            turn: 1,
            usage: TokenUsage {
                input_tokens: 100,
                output_tokens: 50,
                ..Default::default()
            },
        })
        .unwrap()
    )
    .unwrap();

    // Load and verify
    let items = SessionRecorder::load(&session_path).unwrap();
    assert_eq!(items.len(), 4);
    assert!(matches!(items[0], SessionItem::SessionMeta(_)));
    assert!(matches!(items[1], SessionItem::Message(_)));
    assert!(matches!(items[2], SessionItem::ToolCall { .. }));
    assert!(matches!(items[3], SessionItem::TurnContext { .. }));
}

#[test]
fn test_extract_metadata() {
    let temp_dir = TempDir::new().unwrap();
    let session_path = temp_dir.path().join("test-meta.jsonl");

    let meta = SessionMetaLine {
        thread_id: "extract-meta-test".to_string(),
        timestamp: chrono::Utc::now(),
        model: "claude-3".to_string(),
        provider: "anthropic".to_string(),
        cwd: PathBuf::from("/test"),
        sandbox_policy: "full-access".to_string(),
        git_commit: Some("abc123".to_string()),
        git_branch: Some("main".to_string()),
        turns: 3,
        total_usage: TokenUsage::default(),
        parent_id: None,
        forked_from_message: None,
        fork_count: 0,
    };

    // Write metadata
    let mut file = std::fs::File::create(&session_path).unwrap();
    use std::io::Write;
    writeln!(
        file,
        "{}",
        serde_json::to_string(&SessionItem::SessionMeta(meta)).unwrap()
    )
    .unwrap();
    writeln!(
        file,
        "{}",
        serde_json::to_string(&SessionItem::Message(SessionMessage::new(Message::user(
            "test"
        ))))
        .unwrap()
    )
    .unwrap();

    // Extract metadata (should only read first line)
    let extracted = SessionRecorder::extract_metadata(&session_path)
        .unwrap()
        .unwrap();

    assert_eq!(extracted.thread_id, "extract-meta-test");
    assert_eq!(extracted.model, "claude-3");
    assert_eq!(extracted.git_commit, Some("abc123".to_string()));
    assert_eq!(extracted.git_branch, Some("main".to_string()));
}

#[test]
fn test_extract_messages() {
    let items = vec![
        SessionItem::SessionMeta(make_test_meta()),
        SessionItem::Message(SessionMessage::new(Message::user("Hello"))),
        SessionItem::ToolCall {
            id: "tc_1".to_string(),
            name: "test".to_string(),
            input: serde_json::Value::Null,
        },
        SessionItem::Message(SessionMessage::new(Message::assistant("Hi there!"))),
        SessionItem::TurnContext {
            turn: 1,
            usage: TokenUsage::default(),
        },
    ];

    let messages = extract_messages(&items);

    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].content.as_text(), Some("Hello"));
    assert_eq!(messages[1].content.as_text(), Some("Hi there!"));
}

#[test]
fn test_get_last_turn() {
    let items = vec![
        SessionItem::TurnContext {
            turn: 1,
            usage: TokenUsage::default(),
        },
        SessionItem::TurnContext {
            turn: 2,
            usage: TokenUsage::default(),
        },
        SessionItem::TurnContext {
            turn: 5,
            usage: TokenUsage::default(),
        },
        SessionItem::TurnContext {
            turn: 3,
            usage: TokenUsage::default(),
        },
    ];

    assert_eq!(get_last_turn(&items), 5);
    assert_eq!(get_last_turn(&[]), 0);
}

#[test]
fn test_get_total_usage() {
    let items = vec![
        SessionItem::TurnContext {
            turn: 1,
            usage: TokenUsage {
                input_tokens: 100,
                output_tokens: 50,
                cache_read_tokens: 10,
                cache_creation_tokens: 5,
            },
        },
        SessionItem::TurnContext {
            turn: 2,
            usage: TokenUsage {
                input_tokens: 200,
                output_tokens: 100,
                cache_read_tokens: 20,
                cache_creation_tokens: 10,
            },
        },
    ];

    let usage = get_total_usage(&items);

    assert_eq!(usage.input_tokens, 300);
    assert_eq!(usage.output_tokens, 150);
    assert_eq!(usage.cache_read_tokens, 30);
    assert_eq!(usage.cache_creation_tokens, 15);
}

#[test]
fn test_event_serialization() {
    let event = ThreadEvent::TurnCompleted {
        turn_number: 3,
        usage: TokenUsage {
            input_tokens: 100,
            output_tokens: 50,
            ..Default::default()
        },
    };

    let wrapper = EventWrapper::from(event);
    let item = SessionItem::Event { event: wrapper };
    let json = serde_json::to_string(&item).unwrap();

    assert!(json.contains("\"type\":\"event\""));
    assert!(json.contains("turn_completed"));

    // Can deserialize back
    let parsed: SessionItem = serde_json::from_str(&json).unwrap();
    match parsed {
        SessionItem::Event { event: _ } => {}
        _ => panic!("Expected Event variant"),
    }
}

#[test]
fn test_tool_result_serialization() {
    let item = SessionItem::ToolResult {
        id: "tc_123".to_string(),
        output: "file.txt\ndata.csv".to_string(),
        is_error: false,
    };

    let json = serde_json::to_string(&item).unwrap();
    assert!(json.contains("\"type\":\"tool_result\""));
    assert!(json.contains("tc_123"));

    let error_item = SessionItem::ToolResult {
        id: "tc_456".to_string(),
        output: "File not found".to_string(),
        is_error: true,
    };

    let json = serde_json::to_string(&error_item).unwrap();
    assert!(json.contains("\"is_error\":true"));
}
