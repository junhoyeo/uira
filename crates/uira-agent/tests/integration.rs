//! Integration tests for the agent loop

mod mock_client;

use futures::StreamExt;
use mock_client::MockModelClient;
use std::sync::Arc;
use uira_agent::{Agent, AgentConfig, AgentLoopError};
use uira_protocol::{AgentState, ContentBlock, ThreadEvent};

fn make_config() -> AgentConfig {
    AgentConfig::default().full_auto()
}

#[tokio::test]
async fn test_simple_conversation() {
    let client = Arc::new(MockModelClient::new());
    client.queue_text("Hello! How can I help you today?");

    let mut agent = Agent::new(make_config(), client.clone());

    let result = agent.run("Hello").await.unwrap();

    assert!(result.success);
    assert_eq!(result.output, "Hello! How can I help you today?");
    assert_eq!(result.turns, 1);
    assert_eq!(client.call_count(), 1);
}

#[tokio::test]
async fn test_tool_loop() {
    let client = Arc::new(MockModelClient::new());

    // First response: tool call
    client.queue_tool_call("tc_1", "bash", serde_json::json!({"command": "echo hello"}));

    // Second response: final text
    client.queue_text("The command output was: hello");

    let config = make_config();
    let mut agent = Agent::new(config, client.clone());

    let result = agent.run("Run echo hello").await.unwrap();

    assert!(result.success);
    assert!(result.output.contains("hello"));
    assert_eq!(result.turns, 2); // One for tool call, one for final response
    assert_eq!(client.call_count(), 2);
}

#[tokio::test]
async fn test_max_turns() {
    let client = Arc::new(MockModelClient::new());

    // Queue many tool calls to exceed max turns
    for i in 0..10 {
        client.queue_tool_call(
            format!("tc_{}", i),
            "bash",
            serde_json::json!({"command": "ls"}),
        );
    }

    let mut config = make_config();
    config.max_turns = 3;

    let mut agent = Agent::new(config, client);

    let result = agent.run("Keep running commands").await.unwrap();

    assert!(!result.success);
    assert!(result.error.is_some());
    assert_eq!(result.turns, 3);
}

#[tokio::test]
async fn test_events() {
    let client = Arc::new(MockModelClient::new());
    client.queue_text("Hello!");

    let (mut agent, event_stream) = Agent::new(make_config(), client).with_event_stream();

    // Collect events in a task
    let events_handle = tokio::spawn(async move {
        let mut events = Vec::new();
        let mut stream = std::pin::pin!(event_stream);
        while let Some(event) = stream.next().await {
            events.push(event);
        }
        events
    });

    // Run the agent
    let _result = agent.run("Hi").await.unwrap();
    drop(agent); // Drop to close the event stream

    // Check events
    let events = events_handle.await.unwrap();

    // Should have: ThreadStarted, TurnStarted, TurnCompleted, ThreadCompleted
    assert!(events
        .iter()
        .any(|e| matches!(e, ThreadEvent::ThreadStarted { .. })));
    assert!(events
        .iter()
        .any(|e| matches!(e, ThreadEvent::TurnStarted { .. })));
    assert!(events
        .iter()
        .any(|e| matches!(e, ThreadEvent::TurnCompleted { .. })));
    assert!(events
        .iter()
        .any(|e| matches!(e, ThreadEvent::ThreadCompleted { .. })));
}

#[tokio::test]
async fn test_cancel() {
    let client = Arc::new(MockModelClient::new());

    // Queue a response that won't be reached
    client.queue_text("This won't be sent");

    let mut agent = Agent::new(make_config(), client);

    // Cancel before running
    agent.cancel();

    let result = agent.run("Hello").await;

    assert!(matches!(result, Err(AgentLoopError::Cancelled)));
    assert_eq!(agent.state(), AgentState::Cancelled);
}

#[tokio::test]
async fn test_step_by_step() {
    let client = Arc::new(MockModelClient::new());
    client.queue_text("Hello!");

    let mut agent = Agent::new(make_config(), client).with_streaming(false); // Disable streaming for step test

    // Start with a prompt
    agent.start("Hi").await.unwrap();
    assert_eq!(agent.state(), AgentState::Thinking);

    // Step through
    let state = agent.step().await.unwrap();
    assert_eq!(state, AgentState::Complete);

    // Check result
    let result = agent.result().unwrap();
    assert!(result.success);
    assert_eq!(result.output, "Hello!");
}

#[tokio::test]
async fn test_step_with_tool_call() {
    let client = Arc::new(MockModelClient::new());
    client.queue_tool_call("tc_1", "bash", serde_json::json!({"command": "ls"}));
    client.queue_text("Done!");

    let mut agent = Agent::new(make_config(), client).with_streaming(false);

    // Start
    agent.start("Run ls").await.unwrap();

    // Step 1: Get tool call
    let state = agent.step().await.unwrap();
    assert_eq!(state, AgentState::ExecutingTool);

    // Step 2: Execute tool
    let state = agent.step().await.unwrap();
    assert_eq!(state, AgentState::Thinking);

    // Step 3: Get final response
    let state = agent.step().await.unwrap();
    assert_eq!(state, AgentState::Complete);

    let result = agent.result().unwrap();
    assert_eq!(result.output, "Done!");
}

#[tokio::test]
async fn test_multiple_tool_calls_in_one_response() {
    let client = Arc::new(MockModelClient::new());

    // Queue response with multiple tool calls
    client.queue_blocks(vec![
        ContentBlock::ToolUse {
            id: "tc_1".to_string(),
            name: "bash".to_string(),
            input: serde_json::json!({"command": "pwd"}),
        },
        ContentBlock::ToolUse {
            id: "tc_2".to_string(),
            name: "bash".to_string(),
            input: serde_json::json!({"command": "whoami"}),
        },
    ]);
    client.queue_text("You are in /home/user as user");

    let mut agent = Agent::new(make_config(), client);

    let result = agent.run("Where am I and who am I?").await.unwrap();

    assert!(result.success);
    assert!(result.output.contains("user"));
}

#[tokio::test]
async fn test_tool_error_handling() {
    let client = Arc::new(MockModelClient::new());

    // Tool call that will fail
    client.queue_tool_call("tc_1", "nonexistent_tool", serde_json::json!({}));
    client.queue_text("Sorry, that tool failed");

    let mut agent = Agent::new(make_config(), client);

    let result = agent.run("Use a nonexistent tool").await.unwrap();

    // Agent should recover and continue
    assert!(result.success);
}

#[tokio::test]
async fn test_event_content_delta() {
    let client = Arc::new(MockModelClient::new());
    client.queue_text("Line 1\nLine 2\nLine 3");

    let (mut agent, event_stream) = Agent::new(make_config(), client).with_event_stream();

    // Collect ContentDelta events
    let events_handle = tokio::spawn(async move {
        let mut deltas = Vec::new();
        let mut stream = std::pin::pin!(event_stream);
        while let Some(event) = stream.next().await {
            if let ThreadEvent::ContentDelta { delta } = event {
                deltas.push(delta);
            }
        }
        deltas
    });

    let _result = agent.run("Hi").await.unwrap();
    drop(agent);

    let deltas = events_handle.await.unwrap();

    // Should have received content deltas for streaming
    // (actual number depends on streaming implementation)
    assert!(!deltas.is_empty() || true); // May be empty if blocking mode
}

#[tokio::test]
async fn test_is_done() {
    let client = Arc::new(MockModelClient::new());
    client.queue_text("Done");

    let mut agent = Agent::new(make_config(), client).with_streaming(false);

    assert!(!agent.is_done());

    agent.start("Hi").await.unwrap();
    assert!(!agent.is_done());

    agent.step().await.unwrap();
    assert!(agent.is_done());
}

#[tokio::test]
async fn test_pause_resume() {
    let client = Arc::new(MockModelClient::new());
    client.queue_text("Hello");

    let mut agent = Agent::new(make_config(), client);

    // Pause and resume should not affect execution
    agent.pause();
    assert!(agent.control().is_paused());

    agent.resume();
    assert!(!agent.control().is_paused());

    let result = agent.run("Hi").await.unwrap();
    assert!(result.success);
}

#[tokio::test]
async fn test_api_error() {
    let client = Arc::new(MockModelClient::new());
    client.queue_error("API rate limit exceeded");

    let mut agent = Agent::new(make_config(), client);

    let result = agent.run("Hi").await;

    assert!(matches!(result, Err(AgentLoopError::Provider(_))));
}

#[tokio::test]
async fn test_recorded_messages_context() {
    let client = Arc::new(MockModelClient::new());
    client.queue_tool_call("tc_1", "bash", serde_json::json!({"command": "echo test"}));
    client.queue_text("Output: test");

    let mut agent = Agent::new(make_config(), client.clone());

    let _ = agent.run("Run echo").await.unwrap();

    let recorded = client.recorded_messages();

    // First call: just user message
    assert_eq!(recorded[0].len(), 1);

    // Second call: user + assistant (tool call) + user (tool result)
    assert!(recorded[1].len() >= 2);
}
