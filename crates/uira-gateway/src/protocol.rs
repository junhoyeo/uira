use serde::{Deserialize, Serialize};

use crate::config::SessionConfig;

/// Inbound messages from WebSocket clients
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum GatewayMessage {
    CreateSession {
        #[serde(default)]
        config: SessionConfig,
    },
    SendMessage {
        session_id: String,
        content: String,
    },
    ListSessions,
    DestroySession {
        session_id: String,
    },
    SendOutbound {
        channel_type: String,
        recipient: String,
        text: String,
    },
}

/// Outbound messages to WebSocket clients
#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum GatewayResponse {
    SessionCreated {
        session_id: String,
    },
    SessionsList {
        sessions: Vec<SessionInfoResponse>,
    },
    MessageSent {
        session_id: String,
    },
    SessionDestroyed {
        session_id: String,
    },
    OutboundSent {
        channel_type: String,
        recipient: String,
    },
    Error {
        message: String,
    },
}

#[derive(Debug, Serialize)]
pub struct SessionInfoResponse {
    pub id: String,
    pub status: String,
    pub created_at: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialize_create_session() {
        let json = r#"{"type": "create_session"}"#;
        let msg: GatewayMessage = serde_json::from_str(json).unwrap();
        assert!(matches!(msg, GatewayMessage::CreateSession { .. }));
    }

    #[test]
    fn test_deserialize_create_session_with_config() {
        let json = r#"{"type": "create_session", "config": {"model": "gpt-4"}}"#;
        let msg: GatewayMessage = serde_json::from_str(json).unwrap();
        match msg {
            GatewayMessage::CreateSession { config } => {
                assert_eq!(config.model, Some("gpt-4".to_string()));
            }
            _ => panic!("Expected CreateSession"),
        }
    }

    #[test]
    fn test_deserialize_send_message() {
        let json = r#"{"type": "send_message", "session_id": "abc", "content": "hello"}"#;
        let msg: GatewayMessage = serde_json::from_str(json).unwrap();
        match msg {
            GatewayMessage::SendMessage {
                session_id,
                content,
            } => {
                assert_eq!(session_id, "abc");
                assert_eq!(content, "hello");
            }
            _ => panic!("Expected SendMessage"),
        }
    }

    #[test]
    fn test_deserialize_list_sessions() {
        let json = r#"{"type": "list_sessions"}"#;
        let msg: GatewayMessage = serde_json::from_str(json).unwrap();
        assert!(matches!(msg, GatewayMessage::ListSessions));
    }

    #[test]
    fn test_deserialize_destroy_session() {
        let json = r#"{"type": "destroy_session", "session_id": "abc"}"#;
        let msg: GatewayMessage = serde_json::from_str(json).unwrap();
        match msg {
            GatewayMessage::DestroySession { session_id } => {
                assert_eq!(session_id, "abc");
            }
            _ => panic!("Expected DestroySession"),
        }
    }

    #[test]
    fn test_serialize_session_created() {
        let resp = GatewayResponse::SessionCreated {
            session_id: "abc".to_string(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"type\":\"session_created\""));
        assert!(json.contains("\"session_id\":\"abc\""));
    }

    #[test]
    fn test_serialize_error() {
        let resp = GatewayResponse::Error {
            message: "bad request".to_string(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"type\":\"error\""));
        assert!(json.contains("\"message\":\"bad request\""));
    }

    #[test]
    fn test_serialize_sessions_list() {
        let resp = GatewayResponse::SessionsList {
            sessions: vec![SessionInfoResponse {
                id: "s1".to_string(),
                status: "Active".to_string(),
                created_at: "2025-01-01T00:00:00Z".to_string(),
            }],
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"type\":\"sessions_list\""));
        assert!(json.contains("\"id\":\"s1\""));
    }
}
