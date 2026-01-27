use claude_agent_sdk_rs::{query as claude_query, ClaudeAgentOptions, ContentBlock, Message};
use serde_json::json;

pub async fn query(
    prompt: &str,
    model: &str,
    allowed_tools: Option<Vec<String>>,
) -> Result<String, String> {
    let options = match allowed_tools {
        Some(tools) if !tools.is_empty() => ClaudeAgentOptions::builder()
            .model(model.to_string())
            .allowed_tools(tools)
            .build(),
        _ => ClaudeAgentOptions::builder()
            .model(model.to_string())
            .build(),
    };

    let messages = claude_query(prompt, Some(options))
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
