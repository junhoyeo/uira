use std::collections::HashMap;

use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::{debug, info, warn};
use uira_core::schema::SlackChannelConfig;

use super::channel::Channel;
use super::error::ChannelError;
use super::types::{ChannelCapabilities, ChannelMessage, ChannelResponse, ChannelType};

const SLACK_MAX_MESSAGE_LENGTH: usize = 4000;
const CONNECTIONS_OPEN_URL: &str = "https://slack.com/api/apps.connections.open";
const CHAT_POST_MESSAGE_URL: &str = "https://slack.com/api/chat.postMessage";

pub struct SlackChannel {
    config: SlackChannelConfig,
    message_tx: Option<mpsc::Sender<ChannelMessage>>,
    message_rx: Option<mpsc::Receiver<ChannelMessage>>,
    ws_handle: Option<JoinHandle<()>>,
    http_client: reqwest::Client,
}

impl SlackChannel {
    pub fn new(config: SlackChannelConfig) -> Self {
        let (tx, rx) = mpsc::channel(256);
        Self {
            config,
            message_tx: Some(tx),
            message_rx: Some(rx),
            ws_handle: None,
            http_client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl Channel for SlackChannel {
    fn channel_type(&self) -> ChannelType {
        ChannelType::Slack
    }

    fn capabilities(&self) -> ChannelCapabilities {
        ChannelCapabilities {
            max_message_length: SLACK_MAX_MESSAGE_LENGTH,
            supports_markdown: true,
        }
    }

    async fn start(&mut self) -> Result<(), ChannelError> {
        let ws_url = request_socket_mode_url(&self.http_client, &self.config.app_token).await?;

        let tx = self
            .message_tx
            .clone()
            .ok_or_else(|| ChannelError::Other("Message sender already taken".into()))?;
        let allowed_channels = self.config.allowed_channels.clone();
        let http_client = self.http_client.clone();
        let app_token = self.config.app_token.clone();

        let handle = tokio::spawn(async move {
            let mut backoff_secs = 1u64;
            const MAX_BACKOFF_SECS: u64 = 60;

            let mut current_url = ws_url;

            loop {
                match run_socket_mode_loop(&current_url, tx.clone(), &allowed_channels).await {
                    Ok(()) => {
                        info!("Socket Mode loop ended; reconnecting");
                    }
                    Err(e) => {
                        warn!("Socket Mode loop exited with error: {e}");
                    }
                }

                if tx.is_closed() {
                    info!("Message receiver closed, stopping Slack reconnect loop");
                    break;
                }

                info!(
                    backoff_secs,
                    "Reconnecting to Slack Socket Mode after backoff"
                );
                tokio::time::sleep(tokio::time::Duration::from_secs(backoff_secs)).await;

                if tx.is_closed() {
                    info!("Message receiver closed during backoff, stopping Slack reconnect loop");
                    break;
                }

                match request_socket_mode_url(&http_client, &app_token).await {
                    Ok(url) => {
                        current_url = url;
                        backoff_secs = 1;
                    }
                    Err(e) => {
                        warn!("Failed to request new Socket Mode URL: {e}");
                        backoff_secs = (backoff_secs * 2).min(MAX_BACKOFF_SECS);
                    }
                }
            }
        });

        self.ws_handle = Some(handle);
        info!("Slack Socket Mode channel started");
        Ok(())
    }

    async fn stop(&mut self) -> Result<(), ChannelError> {
        if let Some(handle) = self.ws_handle.take() {
            handle.abort();
        }
        self.message_tx.take();
        info!("Slack channel stopped");
        Ok(())
    }

    async fn send_message(&self, response: ChannelResponse) -> Result<(), ChannelError> {
        let chunks = chunk_message(&response.content, SLACK_MAX_MESSAGE_LENGTH);

        for chunk in chunks {
            let body = serde_json::json!({
                "channel": response.recipient,
                "text": chunk,
            });

            let resp = self
                .http_client
                .post(CHAT_POST_MESSAGE_URL)
                .bearer_auth(&self.config.bot_token)
                .json(&body)
                .send()
                .await
                .map_err(|e| ChannelError::SendFailed(e.to_string()))?;

            let status = resp.status();
            let resp_body: serde_json::Value = resp
                .json()
                .await
                .map_err(|e| ChannelError::SendFailed(e.to_string()))?;

            if !status.is_success() || resp_body.get("ok") != Some(&serde_json::Value::Bool(true))
            {
                let err_msg = resp_body["error"]
                    .as_str()
                    .unwrap_or("unknown error")
                    .to_string();
                return Err(ChannelError::SendFailed(err_msg));
            }
        }

        Ok(())
    }

    fn take_message_receiver(&mut self) -> Option<mpsc::Receiver<ChannelMessage>> {
        self.message_rx.take()
    }
}

async fn request_socket_mode_url(
    client: &reqwest::Client,
    app_token: &str,
) -> Result<String, ChannelError> {
    let resp = client
        .post(CONNECTIONS_OPEN_URL)
        .bearer_auth(app_token)
        .send()
        .await
        .map_err(|e| ChannelError::ConnectionFailed(e.to_string()))?;

    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| ChannelError::ConnectionFailed(e.to_string()))?;

    if body.get("ok") != Some(&serde_json::Value::Bool(true)) {
        let err = body["error"]
            .as_str()
            .unwrap_or("unknown error")
            .to_string();
        return Err(ChannelError::AuthError(err));
    }

    body["url"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| ChannelError::ConnectionFailed("No URL in response".into()))
}

async fn run_socket_mode_loop(
    ws_url: &str,
    tx: mpsc::Sender<ChannelMessage>,
    allowed_channels: &[String],
) -> Result<(), ChannelError> {
    let (ws_stream, _) = tokio_tungstenite::connect_async(ws_url)
        .await
        .map_err(|e| ChannelError::ConnectionFailed(e.to_string()))?;

    let (mut ws_sink, mut ws_stream_rx) = ws_stream.split();
    info!("Connected to Slack Socket Mode WebSocket");

    while let Some(msg) = ws_stream_rx.next().await {
        let msg = match msg {
            Ok(m) => m,
            Err(e) => {
                warn!("WebSocket error: {e}");
                break;
            }
        };

        let text = match msg {
            tokio_tungstenite::tungstenite::Message::Text(t) => t,
            tokio_tungstenite::tungstenite::Message::Ping(payload) => {
                if let Err(e) = ws_sink.send(tokio_tungstenite::tungstenite::Message::Pong(payload)).await {
                    warn!("Failed to send Pong: {e}");
                    break;
                }
                continue;
            }
            tokio_tungstenite::tungstenite::Message::Close(_) => {
                info!("WebSocket closed by server");
                break;
            }
            _ => continue,
        };

        let envelope: serde_json::Value = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(e) => {
                warn!("Failed to parse Socket Mode envelope: {e}");
                continue;
            }
        };

        if let Some(envelope_id) = envelope["envelope_id"].as_str() {
            let ack = build_envelope_ack(envelope_id);
            if let Err(e) = ws_sink
                .send(tokio_tungstenite::tungstenite::Message::Text(ack.into()))
                .await
            {
                warn!("Failed to send envelope ack: {e}");
            }
        }

        if let Some(channel_msg) = parse_socket_mode_event(&envelope, allowed_channels) {
            debug!("Received message from {}: {}", channel_msg.sender, channel_msg.content);
            if tx.send(channel_msg).await.is_err() {
                info!("Message receiver dropped, stopping Socket Mode loop");
                break;
            }
        }
    }

    Ok(())
}

fn build_envelope_ack(envelope_id: &str) -> String {
    serde_json::json!({ "envelope_id": envelope_id }).to_string()
}

fn parse_socket_mode_event(
    envelope: &serde_json::Value,
    allowed_channels: &[String],
) -> Option<ChannelMessage> {
    let envelope_type = envelope["type"].as_str()?;
    if envelope_type != "events_api" {
        return None;
    }

    let event = &envelope["payload"]["event"];
    let event_type = event["type"].as_str()?;
    if event_type != "message" {
        return None;
    }

    if event.get("subtype").is_some() {
        return None;
    }

    let channel_id = event["channel"].as_str()?;
    if !is_channel_allowed(channel_id, allowed_channels) {
        debug!("Filtering out message from disallowed channel: {channel_id}");
        return None;
    }

    let sender = event["user"].as_str().unwrap_or("unknown").to_string();
    let content = event["text"].as_str().unwrap_or("").to_string();
    let ts = event["ts"].as_str().unwrap_or("0");

    let timestamp = parse_slack_timestamp(ts);

    let mut metadata = HashMap::new();
    metadata.insert("ts".to_string(), ts.to_string());
    if let Some(team) = event["team"].as_str() {
        metadata.insert("team".to_string(), team.to_string());
    }

    Some(ChannelMessage {
        sender,
        content,
        channel_type: ChannelType::Slack,
        channel_id: channel_id.to_string(),
        timestamp,
        metadata,
    })
}

fn is_channel_allowed(channel_id: &str, allowed_channels: &[String]) -> bool {
    allowed_channels.is_empty() || allowed_channels.iter().any(|c| c == channel_id)
}

fn parse_slack_timestamp(ts: &str) -> chrono::DateTime<chrono::Utc> {
    // Slack timestamps are "EPOCH.SEQUENCE" format (e.g., "1234567890.123456")
    // Parse as two parts to avoid f64 precision loss
    let parts: Vec<&str> = ts.split('.').collect();
    
    let secs_i64 = parts
        .first()
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(0);
    
    let nanos = parts
        .get(1)
        .and_then(|s| {
            // Pad or truncate to 9 digits for nanoseconds
            let padded = format!("{:0<9}", s);
            padded[..9].parse::<u32>().ok()
        })
        .unwrap_or(0);
    
    chrono::DateTime::from_timestamp(secs_i64, nanos).unwrap_or_default()
}

fn chunk_message(content: &str, max_len: usize) -> Vec<&str> {
    if content.len() <= max_len {
        return vec![content];
    }

    let mut chunks = Vec::new();
    let mut remaining = content;

    while !remaining.is_empty() {
        if remaining.len() <= max_len {
            chunks.push(remaining);
            break;
        }

        let split_at = find_split_point(remaining, max_len);
        let (chunk, rest) = remaining.split_at(split_at);
        chunks.push(chunk);
        remaining = rest;
    }

    chunks
}

fn floor_char_boundary(text: &str, max_len: usize) -> usize {
    if max_len >= text.len() {
        return text.len();
    }

    let mut i = max_len;
    while i > 0 && !text.is_char_boundary(i) {
        i -= 1;
    }
    i
}

fn find_split_point(text: &str, max_len: usize) -> usize {
    let safe_max = floor_char_boundary(text, max_len);
    let prefix = &text[..safe_max];

    if let Some(pos) = prefix.rfind('\n') {
        return pos + 1;
    }
    if let Some(pos) = prefix.rfind(' ') {
        return pos + 1;
    }
    safe_max
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_socket_mode_message_event() {
        let envelope: serde_json::Value = serde_json::json!({
            "envelope_id": "abc123",
            "type": "events_api",
            "payload": {
                "event": {
                    "type": "message",
                    "text": "Hello bot",
                    "user": "U12345",
                    "channel": "C12345",
                    "ts": "1234567890.123456"
                }
            }
        });

        let msg = parse_socket_mode_event(&envelope, &[]).unwrap();
        assert_eq!(msg.sender, "U12345");
        assert_eq!(msg.content, "Hello bot");
        assert_eq!(msg.channel_id, "C12345");
        assert_eq!(msg.channel_type, ChannelType::Slack);
        assert_eq!(msg.metadata.get("ts").unwrap(), "1234567890.123456");
    }

    #[test]
    fn test_parse_ignores_non_events_api() {
        let envelope: serde_json::Value = serde_json::json!({
            "envelope_id": "abc123",
            "type": "hello",
            "payload": {}
        });

        assert!(parse_socket_mode_event(&envelope, &[]).is_none());
    }

    #[test]
    fn test_parse_ignores_non_message_events() {
        let envelope: serde_json::Value = serde_json::json!({
            "envelope_id": "abc123",
            "type": "events_api",
            "payload": {
                "event": {
                    "type": "reaction_added",
                    "user": "U12345",
                    "reaction": "thumbsup"
                }
            }
        });

        assert!(parse_socket_mode_event(&envelope, &[]).is_none());
    }

    #[test]
    fn test_parse_ignores_message_subtypes() {
        let envelope: serde_json::Value = serde_json::json!({
            "envelope_id": "abc123",
            "type": "events_api",
            "payload": {
                "event": {
                    "type": "message",
                    "subtype": "bot_message",
                    "text": "Bot reply",
                    "channel": "C12345",
                    "ts": "1234567890.000000"
                }
            }
        });

        assert!(parse_socket_mode_event(&envelope, &[]).is_none());
    }

    #[test]
    fn test_allowed_channels_filtering() {
        let envelope: serde_json::Value = serde_json::json!({
            "envelope_id": "abc123",
            "type": "events_api",
            "payload": {
                "event": {
                    "type": "message",
                    "text": "Hello",
                    "user": "U12345",
                    "channel": "C99999",
                    "ts": "1234567890.000000"
                }
            }
        });

        let allowed = vec!["C12345".to_string(), "C67890".to_string()];
        assert!(parse_socket_mode_event(&envelope, &allowed).is_none());

        let allowed_match = vec!["C99999".to_string()];
        assert!(parse_socket_mode_event(&envelope, &allowed_match).is_some());
    }

    #[test]
    fn test_empty_allowed_channels_allows_all() {
        assert!(is_channel_allowed("C_ANY", &[]));
        assert!(is_channel_allowed("C12345", &["C12345".to_string()]));
        assert!(!is_channel_allowed("C99999", &["C12345".to_string()]));
    }

    #[test]
    fn test_envelope_ack_format() {
        let ack = build_envelope_ack("envelope-xyz-123");
        let parsed: serde_json::Value = serde_json::from_str(&ack).unwrap();
        assert_eq!(parsed["envelope_id"], "envelope-xyz-123");
    }

    #[test]
    fn test_chunk_message_short() {
        let msg = "Hello world";
        let chunks = chunk_message(msg, 4000);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], "Hello world");
    }

    #[test]
    fn test_chunk_message_exact_limit() {
        let msg = "a".repeat(4000);
        let chunks = chunk_message(&msg, 4000);
        assert_eq!(chunks.len(), 1);
    }

    #[test]
    fn test_chunk_message_over_limit() {
        let msg = "a".repeat(8500);
        let chunks = chunk_message(&msg, 4000);
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].len(), 4000);
        assert_eq!(chunks[1].len(), 4000);
        assert_eq!(chunks[2].len(), 500);
    }

    #[test]
    fn test_chunk_message_splits_at_newline() {
        let mut msg = String::new();
        msg.push_str(&"a".repeat(3990));
        msg.push('\n');
        msg.push_str(&"b".repeat(3990));

        let chunks = chunk_message(&msg, 4000);
        assert_eq!(chunks.len(), 2);
        assert!(chunks[0].ends_with('\n'));
    }

    #[test]
    fn test_chunk_message_splits_at_space() {
        let mut msg = String::new();
        msg.push_str(&"a".repeat(3990));
        msg.push(' ');
        msg.push_str(&"b".repeat(3990));

        let chunks = chunk_message(&msg, 4000);
        assert_eq!(chunks.len(), 2);
    }

    #[test]
    fn test_chunk_message_unicode() {
        let text = "🎉".repeat(2000);
        let chunks = chunk_message(&text, 4000);

        for chunk in &chunks {
            assert!(chunk.len() <= 4000);
        }

        let rejoined: String = chunks.concat();
        assert_eq!(rejoined, text);
    }

    #[test]
    fn test_chunk_message_cjk() {
        let text = "你".repeat(2000);
        let chunks = chunk_message(&text, 4000);

        for chunk in &chunks {
            assert!(chunk.len() <= 4000);
        }

        let rejoined: String = chunks.concat();
        assert_eq!(rejoined, text);
    }

    #[test]
    fn test_parse_slack_timestamp() {
        let ts = parse_slack_timestamp("1234567890.123456");
        assert_eq!(ts.timestamp(), 1234567890);

        let ts_zero = parse_slack_timestamp("invalid");
        assert_eq!(ts_zero.timestamp(), 0);
    }

    #[test]
    fn test_parse_slack_timestamp_short_fraction() {
        let ts = parse_slack_timestamp("1234567890.1");
        assert_eq!(ts.timestamp(), 1234567890);
        assert_eq!(ts.timestamp_subsec_nanos(), 100000000); // "1" → "100000000"
    }

    #[test]
    fn test_message_metadata_includes_team() {
        let envelope: serde_json::Value = serde_json::json!({
            "envelope_id": "abc123",
            "type": "events_api",
            "payload": {
                "event": {
                    "type": "message",
                    "text": "Hello",
                    "user": "U12345",
                    "channel": "C12345",
                    "ts": "1234567890.000000",
                    "team": "T12345"
                }
            }
        });

        let msg = parse_socket_mode_event(&envelope, &[]).unwrap();
        assert_eq!(msg.metadata.get("team").unwrap(), "T12345");
    }

    #[test]
    fn test_slack_channel_type() {
        let config = SlackChannelConfig {
            account_id: "default".to_string(),
            bot_token: "xoxb-test".to_string(),
            app_token: "xapp-test".to_string(),
            allowed_channels: vec![],
            active_skills: vec![],
        };
        let channel = SlackChannel::new(config);
        assert_eq!(channel.channel_type(), ChannelType::Slack);
    }

    #[test]
    fn test_slack_capabilities() {
        let config = SlackChannelConfig {
            account_id: "default".to_string(),
            bot_token: "xoxb-test".to_string(),
            app_token: "xapp-test".to_string(),
            allowed_channels: vec![],
            active_skills: vec![],
        };
        let channel = SlackChannel::new(config);
        let caps = channel.capabilities();
        assert_eq!(caps.max_message_length, 4000);
        assert!(caps.supports_markdown);
    }

    #[test]
    fn test_take_message_receiver() {
        let config = SlackChannelConfig {
            account_id: "default".to_string(),
            bot_token: "xoxb-test".to_string(),
            app_token: "xapp-test".to_string(),
            allowed_channels: vec![],
            active_skills: vec![],
        };
        let mut channel = SlackChannel::new(config);
        assert!(channel.take_message_receiver().is_some());
        assert!(channel.take_message_receiver().is_none());
    }
}
