use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::IntoResponse;
use axum::{Json, Router};
use futures_util::stream::SplitSink;
use futures_util::{SinkExt, StreamExt};
use serde::Serialize;
use tokio::net::TcpListener;
use tokio::sync::broadcast;
use tokio::sync::mpsc;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tracing::Instrument;
use uira_core::schema::GatewaySettings;

use crate::channels::{Channel, ChannelResponse};
use crate::error::GatewayError;
use crate::protocol::{GatewayMessage, GatewayResponse, SessionInfoResponse};
use crate::session_manager::SessionManager;

/// Maximum size (in bytes) for a single WS frame payload.
const MAX_WS_FRAME_SIZE: usize = 128 * 1024; // 128 KB
const MAX_MESSAGE_CONTENT_SIZE: usize = 64 * 1024;

struct AppState {
    session_manager: Arc<SessionManager>,
    channels: Arc<RwLock<HashMap<String, Arc<dyn Channel>>>>,
    auth_token: Option<String>,
    start_time: Instant,
    next_conn_id: AtomicU64,
}

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: &'static str,
    uptime_secs: u64,
    active_sessions: usize,
    max_sessions: usize,
    version: &'static str,
}

pub struct GatewayServer {
    session_manager: Arc<SessionManager>,
    channels: Arc<RwLock<HashMap<String, Arc<dyn Channel>>>>,
    auth_token: Option<String>,
}

impl GatewayServer {
    pub fn new(max_sessions: usize) -> Self {
        Self {
            session_manager: Arc::new(SessionManager::new(max_sessions)),
            channels: Arc::new(RwLock::new(HashMap::new())),
            auth_token: None,
        }
    }

    pub fn new_with_settings(settings: GatewaySettings) -> Self {
        let session_manager = Arc::new(SessionManager::new_with_settings(
            settings.max_sessions,
            settings.clone(),
        ));
        Self {
            session_manager,
            channels: Arc::new(RwLock::new(HashMap::new())),
            auth_token: settings.auth_token,
        }
    }

    /// Shared handle to the session manager backing this server.
    pub fn session_manager(&self) -> Arc<SessionManager> {
        self.session_manager.clone()
    }

    /// Set an authentication token. When set, WebSocket connections must
    /// provide a matching `Authorization: Bearer <token>` header.
    pub fn with_auth_token(mut self, token: Option<String>) -> Self {
        self.auth_token = token;
        self
    }

    /// Set a shared channel registry for outbound messaging.
    pub fn with_channels(
        mut self,
        channels: Arc<RwLock<HashMap<String, Arc<dyn Channel>>>>,
    ) -> Self {
        self.channels = channels;
        self
    }

    pub fn router(&self) -> Router {
        let state = Arc::new(AppState {
            session_manager: self.session_manager.clone(),
            channels: self.channels.clone(),
            auth_token: self.auth_token.clone(),
            start_time: Instant::now(),
            next_conn_id: AtomicU64::new(1),
        });
        Router::new()
            .route("/ws", axum::routing::any(ws_handler))
            .route("/health", axum::routing::get(health_handler))
            .with_state(state)
    }

    pub async fn start(&self, host: &str, port: u16) -> Result<(), GatewayError> {
        let app = self.router();
        let addr = format!("{}:{}", host, port);
        let listener = TcpListener::bind(&addr)
            .await
            .map_err(|e| GatewayError::ServerError(e.to_string()))?;

        tracing::info!("Gateway started on ws://{}", addr);

        let session_manager = self.session_manager.clone();

        axum::serve(listener, app)
            .with_graceful_shutdown(async {
                tokio::signal::ctrl_c().await.ok();
                tracing::info!("Shutdown signal received, stopping gateway...");
            })
            .await
            .map_err(|e| GatewayError::ServerError(e.to_string()))?;

        session_manager.shutdown().await?;
        tracing::info!("Gateway shutdown complete");

        Ok(())
    }
}

async fn health_handler(State(state): State<Arc<AppState>>) -> Json<HealthResponse> {
    let active_sessions = state.session_manager.session_count().await;
    let max_sessions = state.session_manager.max_sessions();
    let uptime_secs = state.start_time.elapsed().as_secs();
    Json(HealthResponse {
        status: "ok",
        uptime_secs,
        active_sessions,
        max_sessions,
        version: env!("CARGO_PKG_VERSION"),
    })
}

async fn ws_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    if let Some(expected_token) = &state.auth_token {
        let auth_header = headers.get("authorization").and_then(|v| v.to_str().ok());
        match auth_header {
            Some(value) if value.starts_with("Bearer ") => {
                let token = &value[7..];
                if !constant_time_eq(token, expected_token.as_str()) {
                    return axum::http::StatusCode::UNAUTHORIZED.into_response();
                }
            }
            _ => return axum::http::StatusCode::UNAUTHORIZED.into_response(),
        }
    }
    let conn_id = state.next_conn_id.fetch_add(1, Ordering::Relaxed);
    ws.on_upgrade(move |socket| {
        let span = tracing::info_span!("ws_conn", conn_id);
        handle_socket(
            socket,
            state.session_manager.clone(),
            state.channels.clone(),
        )
        .instrument(span)
    })
    .into_response()
}

fn constant_time_eq(a: &str, b: &str) -> bool {
    let a_bytes = a.as_bytes();
    let b_bytes = b.as_bytes();
    let max_len = a_bytes.len().max(b_bytes.len());

    let mut diff = a_bytes.len() ^ b_bytes.len();
    for i in 0..max_len {
        let a_byte = *a_bytes.get(i).unwrap_or(&0);
        let b_byte = *b_bytes.get(i).unwrap_or(&0);
        diff |= (a_byte ^ b_byte) as usize;
    }

    diff == 0
}

async fn handle_socket(
    socket: WebSocket,
    session_manager: Arc<SessionManager>,
    channels: Arc<RwLock<HashMap<String, Arc<dyn Channel>>>>,
) {
    tracing::debug!("WebSocket connection established");

    let (ws_sender, mut ws_receiver) = socket.split();
    let (tx, rx) = mpsc::channel::<String>(64);
    let mut event_tasks: HashMap<String, JoinHandle<()>> = HashMap::new();
    let writer_task = tokio::spawn(write_outbound(ws_sender, rx));

    while let Some(inbound) = ws_receiver.next().await {
        let text = match inbound {
            Ok(Message::Text(text)) => text.to_string(),
            Ok(Message::Close(_)) => break,
            Err(_) => break,
            _ => continue,
        };

        match serde_json::from_str::<GatewayMessage>(&text) {
            Ok(GatewayMessage::SubscribeEvents { session_id }) => {
                if event_tasks
                    .get(&session_id)
                    .is_some_and(|t| !t.is_finished())
                {
                    let err = GatewayResponse::Error {
                        message: format!(
                            "Already subscribed to events for session '{}'",
                            session_id
                        ),
                    };
                    if tx.send(serialize_response(&err)).await.is_err() {
                        break;
                    }
                    continue;
                }

                match session_manager.subscribe_events(&session_id).await {
                    Some(mut event_rx) => {
                        let ack = GatewayResponse::EventsSubscribed {
                            session_id: session_id.clone(),
                        };
                        if tx.send(serialize_response(&ack)).await.is_err() {
                            break;
                        }

                        let tx_clone = tx.clone();
                        let stream_session_id = session_id.clone();
                        let task = tokio::spawn(async move {
                            loop {
                                let event_json = match event_rx.recv().await {
                                    Ok(event_json) => event_json,
                                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                                        tracing::warn!(
                                            session_id = %stream_session_id,
                                            skipped,
                                            "WebSocket event subscriber lagged behind; skipping missed events"
                                        );
                                        continue;
                                    }
                                    Err(broadcast::error::RecvError::Closed) => break,
                                };

                                let response = GatewayResponse::AgentEvent {
                                    session_id: stream_session_id.clone(),
                                    event: event_json,
                                };

                                let serialized = serialize_response(&response);
                                let to_send = if serialized.len() > MAX_WS_FRAME_SIZE {
                                    let truncated = GatewayResponse::AgentEvent {
                                        session_id: stream_session_id.clone(),
                                        event: serde_json::json!({
                                            "type": "truncated",
                                            "original_size": serialized.len(),
                                            "message": "Event payload exceeded maximum frame size"
                                        }),
                                    };
                                    serialize_response(&truncated)
                                } else {
                                    serialized
                                };

                                if tx_clone.send(to_send).await.is_err() {
                                    return;
                                }
                            }

                            let ended = GatewayResponse::EventStreamEnded {
                                session_id: stream_session_id,
                            };
                            let _ = tx_clone.send(serialize_response(&ended)).await;
                        });
                        event_tasks.insert(session_id, task);
                    }
                    None => {
                        let err = GatewayResponse::Error {
                            message: format!("Session '{}' not found", session_id),
                        };
                        if tx.send(serialize_response(&err)).await.is_err() {
                            break;
                        }
                    }
                }
            }
            Ok(gateway_msg) => {
                let response = handle_message(gateway_msg, &session_manager, &channels).await;
                if tx.send(serialize_response(&response)).await.is_err() {
                    break;
                }
            }
            Err(e) => {
                let err = GatewayResponse::Error {
                    message: format!("Invalid JSON: {}", e),
                };
                if tx.send(serialize_response(&err)).await.is_err() {
                    break;
                }
            }
        }

        event_tasks.retain(|_, task| !task.is_finished());
    }

    drop(tx);
    for (_, task) in event_tasks {
        task.abort();
    }
    let _ = writer_task.await;
    tracing::debug!("WebSocket connection closed");
}

async fn write_outbound(
    mut ws_sender: SplitSink<WebSocket, Message>,
    mut rx: mpsc::Receiver<String>,
) {
    while let Some(message) = rx.recv().await {
        if ws_sender.send(Message::text(message)).await.is_err() {
            break;
        }
    }
}

fn serialize_response(response: &GatewayResponse) -> String {
    match serde_json::to_string(response) {
        Ok(json) => json,
        Err(_) => r#"{"type":"error","message":"Internal serialization error"}"#.to_string(),
    }
}

async fn handle_message(
    msg: GatewayMessage,
    manager: &SessionManager,
    channels: &Arc<RwLock<HashMap<String, Arc<dyn Channel>>>>,
) -> GatewayResponse {
    match msg {
        GatewayMessage::CreateSession { config } => {
            let mut config = config;
            config.sanitize();
            match manager.create_session(config).await {
                Ok(id) => GatewayResponse::SessionCreated { session_id: id },
                Err(e) => GatewayResponse::Error {
                    message: e.to_string(),
                },
            }
        }
        GatewayMessage::ListSessions => {
            let sessions = manager.list_sessions().await;
            GatewayResponse::SessionsList {
                sessions: sessions
                    .into_iter()
                    .map(|s| SessionInfoResponse {
                        id: s.id,
                        status: s.status.to_string(),
                        created_at: s.created_at.to_rfc3339(),
                        last_message_at: s.last_message_at.to_rfc3339(),
                    })
                    .collect(),
            }
        }
        GatewayMessage::SendMessage {
            session_id,
            content,
        } => {
            if content.len() > MAX_MESSAGE_CONTENT_SIZE {
                return GatewayResponse::Error {
                    message: "Message content exceeds maximum size (64KB)".to_string(),
                };
            }

            match manager.send_message(&session_id, content).await {
                Ok(()) => GatewayResponse::MessageSent { session_id },
                Err(e) => GatewayResponse::Error {
                    message: e.to_string(),
                },
            }
        }
        GatewayMessage::SubscribeEvents { session_id } => GatewayResponse::Error {
            message: format!(
                "subscribe_events must be handled by the WebSocket event stream loop: {}",
                session_id
            ),
        },
        GatewayMessage::DestroySession { session_id } => {
            match manager.destroy_session(&session_id).await {
                Ok(()) => GatewayResponse::SessionDestroyed { session_id },
                Err(e) => GatewayResponse::Error {
                    message: e.to_string(),
                },
            }
        }
        GatewayMessage::SendOutbound {
            channel_type,
            recipient,
            text,
        } => {
            if text.len() > MAX_MESSAGE_CONTENT_SIZE {
                return GatewayResponse::Error {
                    message: "Outbound text exceeds maximum size (64KB)".to_string(),
                };
            }

            let channel = {
                let channels_map = channels.read().await;
                channels_map.get(&channel_type).cloned()
            };

            match channel {
                Some(channel) => {
                    let response = ChannelResponse {
                        content: text,
                        recipient: recipient.clone(),
                    };
                    match channel.send_message(response).await {
                        Ok(()) => GatewayResponse::OutboundSent {
                            channel_type,
                            recipient,
                        },
                        Err(e) => GatewayResponse::Error {
                            message: format!("Failed to send message: {}", e),
                        },
                    }
                }
                None => GatewayResponse::Error {
                    message: format!("Channel '{}' not found", channel_type),
                },
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::{SinkExt, StreamExt};
    use tokio::time::{timeout, Duration};
    use tokio_tungstenite::tungstenite;
    use tungstenite::client::IntoClientRequest;

    async fn start_test_server() -> String {
        unsafe {
            std::env::set_var("ANTHROPIC_API_KEY", "test-key");
        }
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
    ) -> tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>
    {
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

    async fn recv_until_type(
        ws: &mut tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
        expected_type: &str,
    ) -> serde_json::Value {
        for _ in 0..40 {
            let maybe_frame = timeout(Duration::from_millis(500), ws.next()).await;
            let frame = match maybe_frame {
                Ok(Some(Ok(frame))) => frame,
                Ok(Some(Err(error))) => panic!("WebSocket receive error: {}", error),
                Ok(None) => panic!("WebSocket closed before receiving {}", expected_type),
                Err(_) => continue,
            };

            let value: serde_json::Value =
                serde_json::from_str(&frame.into_text().unwrap()).unwrap();
            if value["type"].as_str() == Some(expected_type) {
                return value;
            }
        }

        panic!("Timed out waiting for response type {}", expected_type);
    }

    #[tokio::test]
    async fn test_create_session() {
        let url = start_test_server().await;
        let mut ws = connect(&url).await;

        let resp = send_and_recv(&mut ws, r#"{"type": "create_session"}"#).await;
        assert_eq!(resp["type"], "session_created");
        assert!(resp["session_id"].as_str().unwrap().starts_with("gw_ses_"));
    }

    #[tokio::test]
    async fn test_list_sessions() {
        let url = start_test_server().await;
        let mut ws = connect(&url).await;

        send_and_recv(&mut ws, r#"{"type": "create_session"}"#).await;
        let resp = send_and_recv(&mut ws, r#"{"type": "list_sessions"}"#).await;
        assert_eq!(resp["type"], "sessions_list");
        assert_eq!(resp["sessions"].as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn test_send_message() {
        let url = start_test_server().await;
        let mut ws = connect(&url).await;

        let create_resp = send_and_recv(&mut ws, r#"{"type": "create_session"}"#).await;
        let session_id = create_resp["session_id"].as_str().unwrap();

        let msg = format!(
            r#"{{"type": "send_message", "session_id": "{}", "content": "hello"}}"#,
            session_id
        );
        let resp = send_and_recv(&mut ws, &msg).await;
        assert_eq!(resp["type"], "message_sent");
        assert_eq!(resp["session_id"], session_id);
    }

    #[tokio::test]
    async fn test_subscribe_events_streams_agent_events() {
        let url = start_test_server().await;
        let mut ws = connect(&url).await;

        let create_resp = send_and_recv(&mut ws, r#"{"type": "create_session"}"#).await;
        let session_id = create_resp["session_id"].as_str().unwrap().to_string();

        let subscribe_msg = format!(
            r#"{{"type": "subscribe_events", "session_id": "{}"}}"#,
            session_id
        );
        let subscribe_resp = send_and_recv(&mut ws, &subscribe_msg).await;
        assert_eq!(subscribe_resp["type"], "events_subscribed");
        assert_eq!(
            subscribe_resp["session_id"].as_str(),
            Some(session_id.as_str())
        );

        ws.send(tungstenite::Message::Text(
            r#"{"type": "list_sessions"}"#.to_string().into(),
        ))
        .await
        .unwrap();
        let list_resp = recv_until_type(&mut ws, "sessions_list").await;
        assert_eq!(list_resp["type"], "sessions_list");

        let send_msg = format!(
            r#"{{"type": "send_message", "session_id": "{}", "content": "hello"}}"#,
            session_id
        );
        ws.send(tungstenite::Message::Text(send_msg.into()))
            .await
            .unwrap();

        let mut got_message_sent = false;
        let mut got_agent_event = false;

        for _ in 0..30 {
            let maybe_frame = timeout(Duration::from_millis(500), ws.next()).await;
            let frame = match maybe_frame {
                Ok(Some(Ok(frame))) => frame,
                Ok(Some(Err(error))) => panic!("WebSocket receive error: {}", error),
                Ok(None) => break,
                Err(_) => continue,
            };

            let value: serde_json::Value =
                serde_json::from_str(&frame.into_text().unwrap()).unwrap();
            match value["type"].as_str() {
                Some("message_sent") => {
                    if value["session_id"].as_str() == Some(session_id.as_str()) {
                        got_message_sent = true;
                    }
                }
                Some("agent_event") => {
                    if value["session_id"].as_str() == Some(session_id.as_str()) {
                        got_agent_event = true;
                    }
                }
                _ => {}
            }

            if got_message_sent && got_agent_event {
                break;
            }
        }

        assert!(got_message_sent, "expected message_sent response");
        assert!(
            got_agent_event,
            "expected at least one agent_event response"
        );
    }

    #[tokio::test]
    async fn test_subscribe_events_twice_fails() {
        let url = start_test_server().await;
        let mut ws = connect(&url).await;

        let create_resp = send_and_recv(&mut ws, r#"{"type": "create_session"}"#).await;
        let session_id = create_resp["session_id"].as_str().unwrap().to_string();

        let subscribe_msg = format!(
            r#"{{"type": "subscribe_events", "session_id": "{}"}}"#,
            session_id
        );
        ws.send(tungstenite::Message::Text(subscribe_msg.clone().into()))
            .await
            .unwrap();
        let first = recv_until_type(&mut ws, "events_subscribed").await;
        assert_eq!(first["type"], "events_subscribed");

        ws.send(tungstenite::Message::Text(subscribe_msg.into()))
            .await
            .unwrap();
        let second = recv_until_type(&mut ws, "error").await;
        assert_eq!(second["type"], "error");
        assert!(second["message"]
            .as_str()
            .unwrap()
            .contains("Already subscribed"));
    }

    #[tokio::test]
    async fn test_destroy_session() {
        let url = start_test_server().await;
        let mut ws = connect(&url).await;

        let create_resp = send_and_recv(&mut ws, r#"{"type": "create_session"}"#).await;
        let session_id = create_resp["session_id"].as_str().unwrap();

        let msg = format!(
            r#"{{"type": "destroy_session", "session_id": "{}"}}"#,
            session_id
        );
        let resp = send_and_recv(&mut ws, &msg).await;
        assert_eq!(resp["type"], "session_destroyed");
        assert_eq!(resp["session_id"], session_id);

        // Verify session is gone
        let list_resp = send_and_recv(&mut ws, r#"{"type": "list_sessions"}"#).await;
        assert_eq!(list_resp["sessions"].as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn test_malformed_json() {
        let url = start_test_server().await;
        let mut ws = connect(&url).await;

        let resp = send_and_recv(&mut ws, "not valid json").await;
        assert_eq!(resp["type"], "error");
        assert!(resp["message"].as_str().unwrap().contains("Invalid JSON"));
    }

    #[tokio::test]
    async fn test_send_message_nonexistent_session() {
        let url = start_test_server().await;
        let mut ws = connect(&url).await;

        let resp = send_and_recv(
            &mut ws,
            r#"{"type": "send_message", "session_id": "nope", "content": "hi"}"#,
        )
        .await;
        assert_eq!(resp["type"], "error");
        assert!(resp["message"].as_str().unwrap().contains("not found"));
    }

    #[tokio::test]
    async fn test_send_message_content_too_large() {
        let url = start_test_server().await;
        let mut ws = connect(&url).await;

        let create_resp = send_and_recv(&mut ws, r#"{"type": "create_session"}"#).await;
        let session_id = create_resp["session_id"].as_str().unwrap();
        let content = "a".repeat(MAX_MESSAGE_CONTENT_SIZE + 1);

        let msg = serde_json::json!({
            "type": "send_message",
            "session_id": session_id,
            "content": content,
        })
        .to_string();

        let resp = send_and_recv(&mut ws, &msg).await;
        assert_eq!(resp["type"], "error");
        assert_eq!(
            resp["message"].as_str().unwrap(),
            "Message content exceeds maximum size (64KB)"
        );
    }

    // -- Auth helpers & tests ------------------------------------------------

    async fn start_auth_server(auth_token: Option<String>) -> String {
        let server = GatewayServer::new(10).with_auth_token(auth_token);
        let app = server.router();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        format!("127.0.0.1:{}", addr.port())
    }

    async fn connect_with_auth(
        addr: &str,
        token: Option<&str>,
    ) -> Result<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
        tungstenite::Error,
    > {
        let url = format!("ws://{}/ws", addr);
        let mut request = url
            .parse::<tungstenite::http::Uri>()
            .unwrap()
            .into_client_request()
            .unwrap();
        if let Some(t) = token {
            request
                .headers_mut()
                .insert("Authorization", format!("Bearer {}", t).parse().unwrap());
        }
        let (ws_stream, _) = tokio_tungstenite::connect_async(request).await?;
        Ok(ws_stream)
    }

    #[tokio::test]
    async fn test_auth_rejects_missing_token() {
        let addr = start_auth_server(Some("secret-token".into())).await;
        let result = connect_with_auth(&addr, None).await;
        assert!(result.is_err(), "should reject connection without token");
    }

    #[tokio::test]
    async fn test_auth_rejects_wrong_token() {
        let addr = start_auth_server(Some("secret-token".into())).await;
        let result = connect_with_auth(&addr, Some("wrong-token")).await;
        assert!(result.is_err(), "should reject connection with wrong token");
    }

    #[tokio::test]
    async fn test_auth_accepts_correct_token() {
        let addr = start_auth_server(Some("secret-token".into())).await;
        let mut ws = connect_with_auth(&addr, Some("secret-token"))
            .await
            .expect("should accept connection with correct token");

        let resp = send_and_recv(&mut ws, r#"{"type": "list_sessions"}"#).await;
        assert_eq!(resp["type"], "sessions_list");
    }

    #[tokio::test]
    async fn test_auth_accepts_all_when_no_token_configured() {
        let addr = start_auth_server(None).await;
        let mut ws = connect_with_auth(&addr, None)
            .await
            .expect("should accept connection when no auth configured");

        let resp = send_and_recv(&mut ws, r#"{"type": "list_sessions"}"#).await;
        assert_eq!(resp["type"], "sessions_list");
    }

    #[tokio::test]
    async fn test_new_with_settings_uses_auth_token() {
        let settings = GatewaySettings {
            max_sessions: 10,
            auth_token: Some("test-secret".to_string()),
            ..GatewaySettings::default()
        };
        let server = GatewayServer::new_with_settings(settings);
        let app = server.router();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let bind_addr = format!("127.0.0.1:{}", addr.port());

        let no_auth = connect_with_auth(&bind_addr, None).await;
        assert!(
            no_auth.is_err(),
            "should reject connection without token when auth_token is set"
        );

        let mut ws = connect_with_auth(&bind_addr, Some("test-secret"))
            .await
            .expect("should accept connection with configured token");
        let resp = send_and_recv(&mut ws, r#"{"type": "list_sessions"}"#).await;
        assert_eq!(resp["type"], "sessions_list");
    }

    // -- Health endpoint tests -----------------------------------------------

    async fn start_http_test_server() -> String {
        unsafe {
            std::env::set_var("ANTHROPIC_API_KEY", "test-key");
        }
        let server = GatewayServer::new(10);
        let app = server.router();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        format!("http://127.0.0.1:{}", addr.port())
    }

    #[tokio::test]
    async fn test_health_returns_200_with_correct_fields() {
        let base_url = start_http_test_server().await;
        let client = reqwest::Client::new();
        let resp = client
            .get(format!("{}/health", base_url))
            .send()
            .await
            .unwrap();

        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = resp.json().await.unwrap();
        assert_eq!(body["status"], "ok");
        assert!(body["uptime_secs"].is_u64());
        assert_eq!(body["active_sessions"], 0);
        assert_eq!(body["max_sessions"], 10);
        assert!(body["version"].is_string());
        assert!(!body["version"].as_str().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_health_reflects_session_count() {
        unsafe {
            std::env::set_var("ANTHROPIC_API_KEY", "test-key");
        }
        let server = GatewayServer::new(10);
        let app = server.router();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let base_url = format!("http://127.0.0.1:{}", addr.port());
        let ws_url = format!("ws://127.0.0.1:{}", addr.port());

        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        // Create a session via WebSocket
        let (mut ws, _) = tokio_tungstenite::connect_async(format!("{}/ws", ws_url))
            .await
            .unwrap();

        use futures_util::SinkExt;
        ws.send(tungstenite::Message::Text(
            r#"{"type": "create_session"}"#.into(),
        ))
        .await
        .unwrap();

        let resp_msg = ws.next().await.unwrap().unwrap();
        let resp_json: serde_json::Value =
            serde_json::from_str(&resp_msg.into_text().unwrap()).unwrap();
        assert_eq!(resp_json["type"], "session_created");

        // Health should now show 1 active session
        let client = reqwest::Client::new();
        let resp = client
            .get(format!("{}/health", base_url))
            .send()
            .await
            .unwrap();
        let body: serde_json::Value = resp.json().await.unwrap();
        assert_eq!(body["active_sessions"], 1);
    }

    #[tokio::test]
    async fn test_health_uptime_increases() {
        let base_url = start_http_test_server().await;
        let client = reqwest::Client::new();

        let resp1 = client
            .get(format!("{}/health", base_url))
            .send()
            .await
            .unwrap();
        let body1: serde_json::Value = resp1.json().await.unwrap();
        let uptime1 = body1["uptime_secs"].as_u64().unwrap();

        // Sleep briefly then check uptime hasn't gone backwards
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        let resp2 = client
            .get(format!("{}/health", base_url))
            .send()
            .await
            .unwrap();
        let body2: serde_json::Value = resp2.json().await.unwrap();
        let uptime2 = body2["uptime_secs"].as_u64().unwrap();

        assert!(uptime2 >= uptime1);
    }

    #[tokio::test]
    async fn test_health_no_auth_required() {
        // Start a server WITH auth configured — health should still work
        let server = GatewayServer::new(10).with_auth_token(Some("secret".into()));
        let app = server.router();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let client = reqwest::Client::new();
        let resp = client
            .get(format!("http://127.0.0.1:{}/health", addr.port()))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = resp.json().await.unwrap();
        assert_eq!(body["status"], "ok");
    }

    // -- Outbound messaging tests -------------------------------------------

    async fn start_test_server_with_mock_channel() -> (String, Arc<crate::testing::MockChannel>) {
        unsafe {
            std::env::set_var("ANTHROPIC_API_KEY", "test-key");
        }
        let mock = Arc::new(crate::testing::MockChannel::new(
            crate::channels::types::ChannelType::Telegram,
        ));

        let channels: HashMap<String, Arc<dyn crate::channels::Channel>> = {
            let mut m = HashMap::new();
            m.insert(
                "telegram".to_string(),
                mock.clone() as Arc<dyn crate::channels::Channel>,
            );
            m
        };

        let server = GatewayServer::new(10).with_channels(Arc::new(RwLock::new(channels)));
        let app = server.router();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let url = format!("ws://127.0.0.1:{}", addr.port());
        (url, mock)
    }

    #[tokio::test]
    async fn test_send_outbound_to_registered_channel() {
        let (url, mock_channel) = start_test_server_with_mock_channel().await;
        let mut ws = connect(&url).await;

        let resp = send_and_recv(
            &mut ws,
            r#"{"type": "send_outbound", "channel_type": "telegram", "recipient": "user123", "text": "Hello proactive!"}"#,
        )
        .await;

        assert_eq!(resp["type"], "outbound_sent");
        assert_eq!(resp["channel_type"], "telegram");
        assert_eq!(resp["recipient"], "user123");

        assert_eq!(mock_channel.sent_message_count(), 1);
        let sent = mock_channel.sent_messages();
        assert_eq!(sent[0].content, "Hello proactive!");
        assert_eq!(sent[0].recipient, "user123");
    }

    #[tokio::test]
    async fn test_send_outbound_unknown_channel_returns_error() {
        let (url, _mock_channel) = start_test_server_with_mock_channel().await;
        let mut ws = connect(&url).await;

        let resp = send_and_recv(
            &mut ws,
            r#"{"type": "send_outbound", "channel_type": "nonexistent", "recipient": "user123", "text": "Hello"}"#,
        )
        .await;

        assert_eq!(resp["type"], "error");
        assert_eq!(
            resp["message"].as_str().unwrap(),
            "Channel 'nonexistent' not found"
        );
    }

    #[tokio::test]
    async fn test_send_outbound_content_too_large() {
        let (url, _mock_channel) = start_test_server_with_mock_channel().await;
        let mut ws = connect(&url).await;

        let large_text = "a".repeat(MAX_MESSAGE_CONTENT_SIZE + 1);
        let msg = serde_json::json!({
            "type": "send_outbound",
            "channel_type": "telegram",
            "recipient": "user123",
            "text": large_text,
        })
        .to_string();

        let resp = send_and_recv(&mut ws, &msg).await;
        assert_eq!(resp["type"], "error");
        assert_eq!(
            resp["message"].as_str().unwrap(),
            "Outbound text exceeds maximum size (64KB)"
        );

        assert_eq!(_mock_channel.sent_message_count(), 0);
    }

    #[tokio::test]
    async fn test_send_message_to_shutting_down_session() {
        let url = start_test_server().await;
        let mut ws = connect(&url).await;

        let create_resp = send_and_recv(&mut ws, r#"{"type": "create_session"}"#).await;
        let session_id = create_resp["session_id"].as_str().unwrap().to_string();

        let msg = format!(
            r#"{{"type": "destroy_session", "session_id": "{}"}}"#,
            session_id
        );
        let resp = send_and_recv(&mut ws, &msg).await;
        assert_eq!(resp["type"], "session_destroyed");

        let msg = format!(
            r#"{{"type": "send_message", "session_id": "{}", "content": "hello"}}"#,
            session_id
        );
        let resp = send_and_recv(&mut ws, &msg).await;
        assert_eq!(resp["type"], "error");
        assert!(resp["message"].as_str().unwrap().contains("not found"));
    }

    // -- Existing tests ----------------------------------------------------

    #[tokio::test]
    async fn test_full_lifecycle() {
        let url = start_test_server().await;
        let mut ws = connect(&url).await;

        // Create
        let resp = send_and_recv(&mut ws, r#"{"type": "create_session"}"#).await;
        assert_eq!(resp["type"], "session_created");
        let session_id = resp["session_id"].as_str().unwrap().to_string();

        // List — 1 session
        let resp = send_and_recv(&mut ws, r#"{"type": "list_sessions"}"#).await;
        assert_eq!(resp["sessions"].as_array().unwrap().len(), 1);

        // Send message
        let msg = format!(
            r#"{{"type": "send_message", "session_id": "{}", "content": "test"}}"#,
            session_id
        );
        let resp = send_and_recv(&mut ws, &msg).await;
        assert_eq!(resp["type"], "message_sent");

        // Destroy
        let msg = format!(
            r#"{{"type": "destroy_session", "session_id": "{}"}}"#,
            session_id
        );
        let resp = send_and_recv(&mut ws, &msg).await;
        assert_eq!(resp["type"], "session_destroyed");

        // List — 0 sessions
        let resp = send_and_recv(&mut ws, r#"{"type": "list_sessions"}"#).await;
        assert_eq!(resp["sessions"].as_array().unwrap().len(), 0);
    }
}
