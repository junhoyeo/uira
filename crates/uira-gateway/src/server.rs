use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::Response;
use axum::Router;
use tokio::net::TcpListener;

use crate::error::GatewayError;
use crate::protocol::{GatewayMessage, GatewayResponse, SessionInfoResponse};
use crate::session_manager::SessionManager;

pub struct GatewayServer {
    session_manager: Arc<SessionManager>,
}

impl GatewayServer {
    pub fn new(max_sessions: usize) -> Self {
        Self {
            session_manager: Arc::new(SessionManager::new(max_sessions)),
        }
    }

    pub fn router(&self) -> Router {
        Router::new()
            .route("/ws", axum::routing::any(ws_handler))
            .with_state(self.session_manager.clone())
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

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(session_manager): State<Arc<SessionManager>>,
) -> Response {
    ws.on_upgrade(move |socket| handle_socket(socket, session_manager))
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
        if socket.send(Message::text(response_json)).await.is_err() {
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
