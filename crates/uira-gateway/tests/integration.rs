//! End-to-end integration tests for uira-gateway.
//!
//! Tests cover:
//! 1. Full WebSocket lifecycle: create → list → send → destroy
//! 2. Channel bridge message routing with session affinity
//! 3. Multiple channels routing simultaneously to different sessions

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::Utc;
use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite;

use uira_gateway::channels::{
    Channel, ChannelCapabilities, ChannelError, ChannelMessage, ChannelResponse, ChannelType,
};
use uira_gateway::{ChannelBridge, GatewayServer, SessionManager};

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

async fn start_test_server() -> String {
    let server = GatewayServer::new(10);
    let app = server.router();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    format!("ws://127.0.0.1:{}", addr.port())
}

async fn connect(
    url: &str,
) -> tokio_tungstenite::WebSocketStream<
    tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
> {
    let (ws_stream, _) = tokio_tungstenite::connect_async(format!("{}/ws", url))
        .await
        .unwrap();
    ws_stream
}

async fn send_and_recv(
    ws: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    msg: &str,
) -> serde_json::Value {
    ws.send(tungstenite::Message::Text(msg.into()))
        .await
        .unwrap();
    let resp = ws.next().await.unwrap().unwrap();
    let text = resp.into_text().unwrap();
    serde_json::from_str(&text).unwrap()
}

// ---------------------------------------------------------------------------
// MockChannel for channel-bridge integration tests
// ---------------------------------------------------------------------------

struct MockChannel {
    channel_type: ChannelType,
    started: bool,
    message_tx: Option<mpsc::Sender<ChannelMessage>>,
    message_rx: Option<mpsc::Receiver<ChannelMessage>>,
    sent_messages: Arc<Mutex<Vec<ChannelResponse>>>,
}

impl MockChannel {
    fn new(channel_type: ChannelType) -> Self {
        let (tx, rx) = mpsc::channel(32);
        Self {
            channel_type,
            started: false,
            message_tx: Some(tx),
            message_rx: Some(rx),
            sent_messages: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn sender(&self) -> mpsc::Sender<ChannelMessage> {
        self.message_tx.clone().expect("sender already taken")
    }
}

#[async_trait]
impl Channel for MockChannel {
    fn channel_type(&self) -> ChannelType {
        self.channel_type.clone()
    }

    fn capabilities(&self) -> ChannelCapabilities {
        ChannelCapabilities {
            max_message_length: 4096,
            supports_markdown: true,
        }
    }

    async fn start(&mut self) -> Result<(), ChannelError> {
        self.started = true;
        Ok(())
    }

    async fn stop(&mut self) -> Result<(), ChannelError> {
        self.started = false;
        self.message_tx.take();
        Ok(())
    }

    async fn send_message(&self, response: ChannelResponse) -> Result<(), ChannelError> {
        self.sent_messages.lock().unwrap().push(response);
        Ok(())
    }

    fn take_message_receiver(&mut self) -> Option<mpsc::Receiver<ChannelMessage>> {
        self.message_rx.take()
    }
}

fn make_channel_message(
    sender: &str,
    content: &str,
    channel_type: ChannelType,
) -> ChannelMessage {
    ChannelMessage {
        sender: sender.to_string(),
        content: content.to_string(),
        channel_type,
        channel_id: "test-channel".to_string(),
        timestamp: Utc::now(),
        metadata: Default::default(),
    }
}

// ---------------------------------------------------------------------------
// Test 1: Full WebSocket lifecycle
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_websocket_full_lifecycle() {
    let url = start_test_server().await;
    let mut ws = connect(&url).await;

    // 1. Create session
    let resp = send_and_recv(&mut ws, r#"{"type": "create_session"}"#).await;
    assert_eq!(resp["type"], "session_created");
    let session_id = resp["session_id"].as_str().unwrap().to_string();
    assert!(session_id.starts_with("gw_ses_"));

    // 2. List sessions — expect 1
    let resp = send_and_recv(&mut ws, r#"{"type": "list_sessions"}"#).await;
    assert_eq!(resp["type"], "sessions_list");
    let sessions = resp["sessions"].as_array().unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0]["id"], session_id);

    // 3. Send message
    let msg = format!(
        r#"{{"type": "send_message", "session_id": "{}", "content": "hello"}}"#,
        session_id
    );
    let resp = send_and_recv(&mut ws, &msg).await;
    assert_eq!(resp["type"], "message_sent");
    assert_eq!(resp["session_id"], session_id.as_str());

    // 4. Destroy session
    let msg = format!(
        r#"{{"type": "destroy_session", "session_id": "{}"}}"#,
        session_id
    );
    let resp = send_and_recv(&mut ws, &msg).await;
    assert_eq!(resp["type"], "session_destroyed");
    assert_eq!(resp["session_id"], session_id.as_str());

    // 5. List sessions — expect 0
    let resp = send_and_recv(&mut ws, r#"{"type": "list_sessions"}"#).await;
    assert_eq!(resp["type"], "sessions_list");
    assert_eq!(resp["sessions"].as_array().unwrap().len(), 0);
}

// ---------------------------------------------------------------------------
// Test 2: Channel bridge routing with session affinity
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_channel_bridge_routing_and_affinity() {
    let sm = Arc::new(SessionManager::new(100));
    let mut bridge = ChannelBridge::new(sm.clone());

    // Create a Telegram mock channel and grab sender before registering
    let channel = MockChannel::new(ChannelType::Telegram);
    let tx = channel.sender();
    bridge.register_channel(Box::new(channel)).await.unwrap();

    // Send first message from user1
    tx.send(make_channel_message(
        "user1",
        "hello from user1",
        ChannelType::Telegram,
    ))
    .await
    .unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Assert: session was auto-created for user1
    let session_id = bridge
        .get_session_for_sender("telegram", "user1")
        .await
        .expect("user1 should have a session");
    assert!(session_id.starts_with("gw_ses_"));
    assert_eq!(sm.session_count().await, 1);

    // Send second message from same user1 — must reuse the same session
    tx.send(make_channel_message(
        "user1",
        "second message",
        ChannelType::Telegram,
    ))
    .await
    .unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Still only 1 session (affinity)
    assert_eq!(sm.session_count().await, 1);
    let same_session = bridge
        .get_session_for_sender("telegram", "user1")
        .await
        .unwrap();
    assert_eq!(session_id, same_session);

    // Send message from a different sender — user2
    tx.send(make_channel_message(
        "user2",
        "hello from user2",
        ChannelType::Telegram,
    ))
    .await
    .unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Now 2 sessions
    assert_eq!(sm.session_count().await, 2);
    let user2_session = bridge
        .get_session_for_sender("telegram", "user2")
        .await
        .expect("user2 should have a session");
    assert_ne!(session_id, user2_session);

    bridge.stop().await;
}

// ---------------------------------------------------------------------------
// Test 3: Multiple channels simultaneously routing to different sessions
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_multiple_channels_simultaneous_routing() {
    let sm = Arc::new(SessionManager::new(100));
    let mut bridge = ChannelBridge::new(sm.clone());

    // Create Telegram channel
    let tg_channel = MockChannel::new(ChannelType::Telegram);
    let tg_tx = tg_channel.sender();
    bridge.register_channel(Box::new(tg_channel)).await.unwrap();

    // Create Slack channel
    let slack_channel = MockChannel::new(ChannelType::Slack);
    let slack_tx = slack_channel.sender();
    bridge
        .register_channel(Box::new(slack_channel))
        .await
        .unwrap();

    // Send from Telegram user "alice"
    tg_tx
        .send(make_channel_message(
            "alice",
            "telegram hello",
            ChannelType::Telegram,
        ))
        .await
        .unwrap();

    // Send from Slack user "alice" (same name, different channel type → separate session)
    slack_tx
        .send(make_channel_message(
            "alice",
            "slack hello",
            ChannelType::Slack,
        ))
        .await
        .unwrap();

    // Send from Slack user "bob"
    slack_tx
        .send(make_channel_message(
            "bob",
            "slack hey",
            ChannelType::Slack,
        ))
        .await
        .unwrap();

    tokio::time::sleep(tokio::time::Duration::from_millis(150)).await;

    // 3 distinct sessions: telegram/alice, slack/alice, slack/bob
    assert_eq!(sm.session_count().await, 3);

    // Each sender has the correct session
    let tg_alice = bridge
        .get_session_for_sender("telegram", "alice")
        .await
        .expect("telegram/alice should have a session");
    let slack_alice = bridge
        .get_session_for_sender("slack", "alice")
        .await
        .expect("slack/alice should have a session");
    let slack_bob = bridge
        .get_session_for_sender("slack", "bob")
        .await
        .expect("slack/bob should have a session");

    // All sessions are distinct
    assert_ne!(tg_alice, slack_alice);
    assert_ne!(slack_alice, slack_bob);
    assert_ne!(tg_alice, slack_bob);

    // All session IDs are well-formed
    assert!(tg_alice.starts_with("gw_ses_"));
    assert!(slack_alice.starts_with("gw_ses_"));
    assert!(slack_bob.starts_with("gw_ses_"));

    bridge.stop().await;
}
