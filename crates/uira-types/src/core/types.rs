use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreToolUseInput {
    pub session_id: String,
    #[serde(default)]
    pub transcript_path: Option<String>,
    pub cwd: String,
    #[serde(default)]
    pub permission_mode: Option<PermissionMode>,
    pub tool_name: String,
    pub tool_input: serde_json::Value,
    #[serde(default)]
    pub tool_use_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostToolUseInput {
    pub session_id: String,
    #[serde(default)]
    pub transcript_path: Option<String>,
    pub cwd: String,
    #[serde(default)]
    pub permission_mode: Option<PermissionMode>,
    pub tool_name: String,
    pub tool_input: serde_json::Value,
    pub tool_response: ToolResponse,
    #[serde(default)]
    pub tool_use_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResponse {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub output: Option<String>,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserPromptSubmitInput {
    pub session_id: String,
    pub cwd: String,
    #[serde(default)]
    pub permission_mode: Option<PermissionMode>,
    pub prompt: String,
    #[serde(default)]
    pub session: Option<SessionInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StopInput {
    pub session_id: String,
    #[serde(default)]
    pub transcript_path: Option<String>,
    pub cwd: String,
    #[serde(default)]
    pub permission_mode: Option<PermissionMode>,
    #[serde(default)]
    pub stop_hook_active: bool,
    #[serde(default)]
    pub todo_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreCompactInput {
    pub session_id: String,
    pub cwd: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PermissionMode {
    Default,
    Plan,
    AcceptEdits,
    BypassPermissions,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PermissionDecision {
    Allow,
    Deny,
    Ask,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookContext {
    pub cwd: String,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub agent: Option<String>,
}

impl Default for HookContext {
    fn default() -> Self {
        Self {
            cwd: std::env::current_dir()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| ".".to_string()),
            env: HashMap::new(),
            session_id: None,
            agent: None,
        }
    }
}

impl HookContext {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_env(mut self, key: &str, value: &str) -> Self {
        self.env.insert(key.to_string(), value.to_string());
        self
    }

    pub fn with_session(mut self, session_id: &str) -> Self {
        self.session_id = Some(session_id.to_string());
        self
    }

    pub fn with_agent(mut self, agent: &str) -> Self {
        self.agent = Some(agent.to_string());
        self
    }

    pub fn set_env(&mut self, key: &str, value: &str) {
        self.env.insert(key.to_string(), value.to_string());
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HookMatcher {
    #[serde(default)]
    pub matcher: Option<String>,
    #[serde(default)]
    pub run: Option<String>,
    #[serde(default)]
    pub commands: Vec<HookCommand>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookCommand {
    #[serde(default)]
    pub name: Option<String>,
    pub run: String,
    #[serde(default)]
    pub on_fail: OnFail,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OnFail {
    #[default]
    Continue,
    Stop,
    Warn,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hook_context() {
        let ctx = HookContext::new()
            .with_env("FILE", "test.rs")
            .with_session("sess-123")
            .with_agent("prometheus");

        assert_eq!(ctx.env.get("FILE"), Some(&"test.rs".to_string()));
        assert_eq!(ctx.session_id, Some("sess-123".to_string()));
        assert_eq!(ctx.agent, Some("prometheus".to_string()));
    }

    #[test]
    fn test_deserialize_user_prompt() {
        let json = r#"{
            "session_id": "sess-123",
            "cwd": "/home/user",
            "prompt": "hello world"
        }"#;

        let input: UserPromptSubmitInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.session_id, "sess-123");
        assert_eq!(input.prompt, "hello world");
    }
}
