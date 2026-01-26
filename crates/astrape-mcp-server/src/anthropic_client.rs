use claude_agent_sdk_rs::{query as claude_query, ContentBlock, Message};
use serde_json::json;

pub async fn query(prompt: &str, _model: &str) -> Result<String, String> {
    let messages = claude_query(prompt, None)
        .await
        .map_err(|e| format!("API request failed: {}", e))?;

    let mut result_texts = Vec::new();
    for message in messages {
        if let Message::Assistant(msg) = message {
            for block in msg.message.content {
                if let ContentBlock::Text(text) = block {
                    result_texts.push(text.text);
                }
            }
        }
    }

    let combined_text = result_texts.join("\n");

    Ok(json!({"result": combined_text}).to_string())
}
