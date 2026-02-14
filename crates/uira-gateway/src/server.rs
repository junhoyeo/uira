use std::sync::Arc;
use std::time::Instant;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::IntoResponse;
use axum::{Json, Router};
use serde::Serialize;
use tokio::net::TcpListener;

use crate::error::GatewayError;
use crate::protocol::{GatewayMessage, GatewayResponse, SessionInfoResponse};
use crate::session_manager::SessionManager;

struct AppState {
    session_manager: Arc<SessionManager>,
    auth_token: Option<String>,
    start_time: Instant,
}

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: &'static str,
    uptime_secs: u64,
    active_sessions: usize,
    version: &'static str,
}

pub struct GatewayServer {
    session_manager: Arc<SessionManager>,
    auth_token: Option<String>,
}

impl GatewayServer {
    pub fn new(max_sessions: usize) -> Self {
        Self {
            session_manager: Arc::new(SessionManager::new(max_sessions)),
            auth_token: None,
        }
    }

    /// Set an authentication token. When set, WebSocket connections must
    /// provide a matching `Authorization: Bearer <token>` header.
    pub fn with_auth_token(mut self, token: Option<String>) -> Self {
        self.auth_token = token;
        self
    }

    pub fn router(&self) -> Router {
        let state = Arc::new(AppState {
            session_manager: self.session_manager.clone(),
            auth_token: self.auth_token.clone(),
            start_time: Instant::now(),
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

        axum::serve(listener, app)
            .await
            .map_err(|e| GatewayError::ServerError(e.to_string()))?;

        Ok(())
    }
}

async fn health_handler(State(state): State<Arc<AppState>>) -> Json<HealthResponse> {
    let active_sessions = state.session_manager.session_count().await;
    let uptime_secs = state.start_time.elapsed().as_secs();
    Json(HealthResponse {
        status: "ok",
        uptime_secs,
        active_sessions,
        version: env!("CARGO_PKG_VERSION"),
    })
}

async fn ws_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    if let Some(expected_token) = &state.auth_token {
        let auth_header = headers
            .get("authorization")
            .and_then(|v| v.to_str().ok());
        match auth_header {
            Some(value) if value.starts_with("Bearer ") => {
                let token = &value[7..];
                if token != expected_token.as_str() {
                    return axum::http::StatusCode::UNAUTHORIZED.into_response();
                }
            }
            _ => return axum::http::StatusCode::UNAUTHORIZED.into_response(),
        }
    }
    ws.on_upgrade(move |socket| handle_socket(socket, state.session_manager.clone()))
        .into_response()
}

async fn handle_socket(mut socket: WebSocket, session_manager: Arc<SessionManager>) {
    while let Some(msg) = socket.recv().await {
        let text = match msg {
            Ok(Message::Text(text)) => text.to_string(),
            Ok(Message::Close(_)) => break,
            Err(_) => break,
            _ => continue,
        };

        let response = match serde_json::from_str::<GatewayMessage>(&text) {
            Ok(gateway_msg) => handle_message(gateway_msg, &session_manager).await,
            Err(e) => GatewayResponse::Error {
                message: format!("Invalid JSON: {}", e),
            },
        };

        let response_json = serde_json::to_string(&response).unwrap_or_default();
        if socket
            .send(Message::text(response_json))
            .await
            .is_err()
        {
            break;
        }
    }
}

async fn handle_message(msg: GatewayMessage, manager: &SessionManager) -> GatewayResponse {
    match msg {
        GatewayMessage::CreateSession { config } => match manager.create_session(config).await {
            Ok(id) => GatewayResponse::SessionCreated { session_id: id },
            Err(e) => GatewayResponse::Error {
                message: e.to_string(),
            },
        },
        GatewayMessage::ListSessions => {
            let sessions = manager.list_sessions().await;
            GatewayResponse::SessionsList {
                sessions: sessions
                    .into_iter()
                    .map(|s| SessionInfoResponse {
                        id: s.id,
                        status: format!("{:?}", s.status),
                        created_at: s.created_at.to_rfc3339(),
                    })
                    .collect(),
            }
        }
        GatewayMessage::SendMessage {
            session_id,
            content,
        } => match manager.send_message(&session_id, content).await {
            Ok(()) => GatewayResponse::MessageSent { session_id },
            Err(e) => GatewayResponse::Error {
                message: e.to_string(),
            },
        },
        GatewayMessage::DestroySession { session_id } => {
            match manager.destroy_session(&session_id).await {
                Ok(()) => GatewayResponse::SessionDestroyed { session_id },
                Err(e) => GatewayResponse::Error {
                    message: e.to_string(),
                },
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::{SinkExt, StreamExt};
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

    async fn connect(url: &str) -> tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>> {
        let (ws_stream, _) = tokio_tungstenite::connect_async(format!("{}/ws", url))
            .await
            .unwrap();
        ws_stream
    }

    async fn send_and_recv(
        ws: &mut tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
        msg: &str,
    ) -> serde_json::Value {
        ws.send(tungstenite::Message::Text(msg.into()))
            .await
            .unwrap();
        let resp = ws.next().await.unwrap().unwrap();
        let text = resp.into_text().unwrap();
        serde_json::from_str(&text).unwrap()
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
        let mut request = url.parse::<tungstenite::http::Uri>().unwrap().into_client_request().unwrap();
        if let Some(t) = token {
            request.headers_mut().insert(
                "Authorization",
                format!("Bearer {}", t).parse().unwrap(),
            );
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
        let (mut ws, _) =
            tokio_tungstenite::connect_async(format!("{}/ws", ws_url))
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
