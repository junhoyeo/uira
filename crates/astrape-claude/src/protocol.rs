use astrape_core::{
    HookOutput, PostToolUseInput, PreToolUseInput, StopInput, UserPromptSubmitInput,
};
use serde::{Deserialize, Serialize};
use std::io::{self, Read, Write};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum HookInput {
    UserPromptSubmit(UserPromptSubmitInput),
    Stop(StopInput),
    PreToolUse(PreToolUseInput),
    PostToolUse(PostToolUseInput),
    Generic(serde_json::Value),
}

pub fn read_input() -> io::Result<HookInput> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;

    serde_json::from_str(&input).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

pub fn write_output(output: &HookOutput) -> io::Result<()> {
    let json = serde_json::to_string(output)?;
    io::stdout().write_all(json.as_bytes())?;
    io::stdout().flush()?;
    Ok(())
}

pub fn extract_prompt(input: &HookInput) -> Option<String> {
    match input {
        HookInput::UserPromptSubmit(data) => Some(data.prompt.clone()),
        HookInput::Generic(value) => {
            if let Some(prompt) = value.get("prompt").and_then(|v| v.as_str()) {
                return Some(prompt.to_string());
            }
            if let Some(content) = value
                .get("message")
                .and_then(|m| m.get("content"))
                .and_then(|v| v.as_str())
            {
                return Some(content.to_string());
            }
            if let Some(parts) = value.get("parts").and_then(|p| p.as_array()) {
                let texts: Vec<&str> = parts
                    .iter()
                    .filter_map(|p| {
                        if p.get("type").and_then(|t| t.as_str()) == Some("text") {
                            p.get("text").and_then(|t| t.as_str())
                        } else {
                            None
                        }
                    })
                    .collect();
                if !texts.is_empty() {
                    return Some(texts.join(" "));
                }
            }
            None
        }
        _ => None,
    }
}

pub fn extract_session_id(input: &HookInput) -> Option<String> {
    match input {
        HookInput::UserPromptSubmit(data) => Some(data.session_id.clone()),
        HookInput::Stop(data) => Some(data.session_id.clone()),
        HookInput::PreToolUse(data) => Some(data.session_id.clone()),
        HookInput::PostToolUse(data) => Some(data.session_id.clone()),
        HookInput::Generic(value) => value
            .get("session_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_prompt_from_user_submit() {
        let input = HookInput::UserPromptSubmit(UserPromptSubmitInput {
            session_id: "sess-123".to_string(),
            cwd: "/home".to_string(),
            permission_mode: None,
            prompt: "hello world".to_string(),
            session: None,
        });

        assert_eq!(extract_prompt(&input), Some("hello world".to_string()));
    }

    #[test]
    fn test_extract_prompt_from_generic() {
        let value = serde_json::json!({
            "prompt": "test prompt"
        });
        let input = HookInput::Generic(value);
        assert_eq!(extract_prompt(&input), Some("test prompt".to_string()));
    }

    #[test]
    fn test_extract_prompt_from_parts() {
        let value = serde_json::json!({
            "parts": [
                {"type": "text", "text": "hello"},
                {"type": "text", "text": "world"}
            ]
        });
        let input = HookInput::Generic(value);
        assert_eq!(extract_prompt(&input), Some("hello world".to_string()));
    }
}
