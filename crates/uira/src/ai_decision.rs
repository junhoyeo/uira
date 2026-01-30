//! Shared AI decision infrastructure for typos, diagnostics, and comments workflows.

use anyhow::{Context, Result};
use colored::Colorize;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::process::{Command, Stdio};
use std::time::Duration;

// ============ HTTP Types ============

#[derive(Debug, Deserialize)]
pub(crate) struct Session {
    pub id: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct ChatBody {
    #[serde(rename = "modelID")]
    pub model_id: String,
    #[serde(rename = "providerID")]
    pub provider_id: String,
    pub parts: Vec<ChatPart>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<HashMap<String, bool>>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ChatPart {
    #[serde(rename = "type")]
    pub part_type: String,
    pub text: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct MessageInfo {
    #[allow(dead_code)]
    pub info: MessageMeta,
    pub parts: Vec<MessagePart>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct MessageMeta {
    #[allow(dead_code)]
    pub id: String,
    #[serde(default)]
    #[allow(dead_code)]
    pub role: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct MessagePart {
    #[serde(rename = "type")]
    pub part_type: String,
    pub text: Option<String>,
}

// ============ Configuration ============

/// Configuration for AI-assisted workflows
#[derive(Debug, Clone)]
pub struct AiWorkflowConfig {
    pub host: String,
    pub port: u16,
    pub model: String,
    pub provider: String,
    pub disable_tools: bool,
    pub disable_mcp: bool,
    pub timeout_secs: u64,
}

impl Default for AiWorkflowConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 4096,
            model: "claude-sonnet-4-20250514".to_string(),
            provider: "anthropic".to_string(),
            disable_tools: true,
            disable_mcp: true,
            timeout_secs: 120,
        }
    }
}

// ============ Decision Types ============

/// Generic decision from AI
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Apply,
    Ignore,
    Skip,
}

/// Extended decision for diagnostics with confidence levels
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticsDecision {
    FixHigh,
    FixLow,
    Skip,
}

/// Extended decision for comments
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommentDecision {
    Remove,
    Keep,
    AllowPragma,
    Skip,
}

// ============ Trait for AI Prompts ============

/// Trait for items that can be formatted into AI prompts
pub trait IntoAiPrompt {
    fn to_prompt_entry(&self, index: usize) -> String;
}

// ============ AI Decision Client ============

/// Shared AI decision client for all workflows
pub struct AiDecisionClient {
    client: reqwest::blocking::Client,
    session_id: Option<String>,
    server_was_started: bool,
    config: AiWorkflowConfig,
    pub modified_files: HashSet<String>,
    pub auto_stage: bool,
}

impl AiDecisionClient {
    pub fn new(config: AiWorkflowConfig) -> Self {
        Self {
            client: reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(config.timeout_secs))
                .build()
                .unwrap(),
            session_id: None,
            server_was_started: false,
            config,
            modified_files: HashSet::new(),
            auto_stage: false,
        }
    }

    // ============ Server Management ============

    pub fn ensure_server(&mut self) -> Result<()> {
        if self.is_server_running() {
            println!("  {} OpenCode server already running", "→".dimmed());
            return Ok(());
        }

        println!("  {} Starting OpenCode server...", "→".dimmed());
        self.start_server()?;
        self.server_was_started = true;

        for _ in 0..30 {
            std::thread::sleep(Duration::from_millis(500));
            if self.is_server_running() {
                println!("  {} OpenCode server ready", "✓".green());
                return Ok(());
            }
        }

        anyhow::bail!("Timeout waiting for OpenCode server to start")
    }

    pub fn is_server_running(&self) -> bool {
        self.client
            .get(format!(
                "http://{}:{}/health",
                self.config.host, self.config.port
            ))
            .send()
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    }

    fn start_server(&self) -> Result<()> {
        Command::new("opencode")
            .args(["serve"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .context("Failed to start OpenCode server. Is opencode installed?")?;
        Ok(())
    }

    pub fn cleanup(&mut self) -> Result<()> {
        if let Some(session_id) = &self.session_id {
            let _ = self
                .client
                .delete(format!(
                    "http://{}:{}/session/{}",
                    self.config.host, self.config.port, session_id
                ))
                .send();
        }

        if self.server_was_started {
            println!("  {} Stopping OpenCode server...", "→".dimmed());
            let _ = Command::new("pkill")
                .args(["-f", "opencode.*--server"])
                .status();
        }

        Ok(())
    }

    // ============ Session Management ============

    pub fn get_or_create_session(&mut self) -> Result<String> {
        if let Some(id) = &self.session_id {
            return Ok(id.clone());
        }

        let resp: Session = self
            .client
            .post(format!(
                "http://{}:{}/session",
                self.config.host, self.config.port
            ))
            .send()
            .context("Failed to create session")?
            .json()
            .context("Failed to parse session response")?;

        self.session_id = Some(resp.id.clone());
        Ok(resp.id)
    }

    // ============ Message Handling ============

    pub fn build_tools_config(&self) -> Option<HashMap<String, bool>> {
        if !self.config.disable_tools && !self.config.disable_mcp {
            return None;
        }

        let mut tools = HashMap::new();

        if self.config.disable_tools {
            tools.insert("bash".to_string(), false);
            tools.insert("edit".to_string(), false);
            tools.insert("write".to_string(), false);
            tools.insert("read".to_string(), false);
            tools.insert("patch".to_string(), false);
            tools.insert("glob".to_string(), false);
            tools.insert("grep".to_string(), false);
            tools.insert("webfetch".to_string(), false);
        }

        if self.config.disable_mcp {
            tools.insert("mcp_*".to_string(), false);
        }

        Some(tools)
    }

    pub fn send_prompt(&mut self, prompt: &str) -> Result<String> {
        let session_id = self.get_or_create_session()?;

        let body = ChatBody {
            model_id: self.config.model.clone(),
            provider_id: self.config.provider.clone(),
            parts: vec![ChatPart {
                part_type: "text".to_string(),
                text: prompt.to_string(),
            }],
            tools: self.build_tools_config(),
        };

        let resp = self
            .client
            .post(format!(
                "http://{}:{}/session/{}/message",
                self.config.host, self.config.port, session_id
            ))
            .json(&body)
            .send()
            .context("Failed to send message")?;

        let message: MessageInfo = resp.json().context("Failed to parse AI response")?;
        Ok(self.extract_text_from_message(&message))
    }

    fn extract_text_from_message(&self, message: &MessageInfo) -> String {
        let mut text = String::new();
        for part in &message.parts {
            if part.part_type == "text" {
                if let Some(t) = &part.text {
                    text.push_str(t);
                    text.push('\n');
                }
            }
        }
        text
    }

    // ============ Decision Parsing ============

    pub fn parse_decisions(&self, ai_text: &str, expected_count: usize) -> Vec<Decision> {
        let mut decisions = Vec::with_capacity(expected_count);
        let upper = ai_text.to_uppercase();

        for line in upper.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            let decision = if trimmed.contains("APPLY") {
                Decision::Apply
            } else if trimmed.contains("IGNORE") {
                Decision::Ignore
            } else if trimmed.contains("SKIP") {
                Decision::Skip
            } else {
                continue;
            };

            decisions.push(decision);

            if decisions.len() >= expected_count {
                break;
            }
        }

        while decisions.len() < expected_count {
            decisions.push(Decision::Skip);
        }

        decisions
    }

    pub fn parse_diagnostics_decisions(
        &self,
        ai_text: &str,
        expected_count: usize,
    ) -> Vec<DiagnosticsDecision> {
        let mut decisions = Vec::with_capacity(expected_count);
        let upper = ai_text.to_uppercase();

        for line in upper.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            let decision = if trimmed.contains("FIX:HIGH") || trimmed.contains("FIX HIGH") {
                DiagnosticsDecision::FixHigh
            } else if trimmed.contains("FIX:LOW") || trimmed.contains("FIX LOW") {
                DiagnosticsDecision::FixLow
            } else if trimmed.contains("SKIP") {
                DiagnosticsDecision::Skip
            } else {
                continue;
            };

            decisions.push(decision);

            if decisions.len() >= expected_count {
                break;
            }
        }

        while decisions.len() < expected_count {
            decisions.push(DiagnosticsDecision::Skip);
        }

        decisions
    }

    pub fn parse_comment_decisions(
        &self,
        ai_text: &str,
        expected_count: usize,
    ) -> Vec<CommentDecision> {
        let mut decisions = Vec::with_capacity(expected_count);
        let upper = ai_text.to_uppercase();

        for line in upper.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            let decision = if trimmed.contains("REMOVE") {
                CommentDecision::Remove
            } else if trimmed.contains("PRAGMA") {
                CommentDecision::AllowPragma
            } else if trimmed.contains("KEEP") {
                CommentDecision::Keep
            } else if trimmed.contains("SKIP") {
                CommentDecision::Skip
            } else {
                continue;
            };

            decisions.push(decision);

            if decisions.len() >= expected_count {
                break;
            }
        }

        while decisions.len() < expected_count {
            decisions.push(CommentDecision::Skip);
        }

        decisions
    }

    // ============ File Staging ============

    pub fn stage_modified_files(&self) -> Result<()> {
        if self.modified_files.is_empty() {
            return Ok(());
        }

        let files: Vec<&str> = self.modified_files.iter().map(|s| s.as_str()).collect();
        println!(
            "  {} Staging {} modified file(s)...",
            "→".dimmed(),
            files.len()
        );

        let status = Command::new("git")
            .arg("add")
            .args(&files)
            .status()
            .context("Failed to run git add")?;

        if status.success() {
            for file in &files {
                println!("    {} {}", "✓".green(), file);
            }
        } else {
            anyhow::bail!("git add failed");
        }

        Ok(())
    }

    // ============ Context Helpers ============

    pub fn get_file_context(path: &str, line_num: u32) -> Result<String> {
        let content = std::fs::read_to_string(path)?;
        let lines: Vec<&str> = content.lines().collect();

        let start = (line_num as usize).saturating_sub(3);
        let end = (line_num as usize + 2).min(lines.len());

        let mut context = String::new();
        for (i, line) in lines[start..end].iter().enumerate() {
            let actual_line = start + i + 1;
            let marker = if actual_line == line_num as usize {
                ">"
            } else {
                " "
            };
            context.push_str(&format!("{} {:4} | {}\n", marker, actual_line, line));
        }

        Ok(context)
    }

    pub fn get_line(path: &str, line_num: u32) -> Result<String> {
        let content = std::fs::read_to_string(path)?;
        content
            .lines()
            .nth(line_num as usize - 1)
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow::anyhow!("Line not found"))
    }
}

impl Default for AiDecisionClient {
    fn default() -> Self {
        Self::new(AiWorkflowConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = AiWorkflowConfig::default();
        assert_eq!(config.host, "127.0.0.1");
        assert_eq!(config.port, 4096);
        assert!(config.disable_tools);
        assert!(config.disable_mcp);
    }

    #[test]
    fn test_parse_model_string() {
        fn parse_model(model: &str) -> (String, String) {
            if let Some((provider, model)) = model.split_once('/') {
                (provider.to_string(), model.to_string())
            } else {
                ("anthropic".to_string(), model.to_string())
            }
        }

        let (provider, model) = parse_model("anthropic/claude-sonnet-4-20250514");
        assert_eq!(provider, "anthropic");
        assert_eq!(model, "claude-sonnet-4-20250514");

        let (provider, model) = parse_model("gpt-4");
        assert_eq!(provider, "anthropic");
        assert_eq!(model, "gpt-4");
    }

    #[test]
    fn test_parse_decisions() {
        let client = AiDecisionClient::default();

        let text = "APPLY\nIGNORE\nSKIP";
        let decisions = client.parse_decisions(text, 3);
        assert_eq!(decisions.len(), 3);
        assert_eq!(decisions[0], Decision::Apply);
        assert_eq!(decisions[1], Decision::Ignore);
        assert_eq!(decisions[2], Decision::Skip);
    }

    #[test]
    fn test_parse_decisions_padding() {
        let client = AiDecisionClient::default();

        let text = "APPLY";
        let decisions = client.parse_decisions(text, 3);
        assert_eq!(decisions.len(), 3);
        assert_eq!(decisions[0], Decision::Apply);
        assert_eq!(decisions[1], Decision::Skip);
        assert_eq!(decisions[2], Decision::Skip);
    }

    #[test]
    fn test_parse_diagnostics_decisions() {
        let client = AiDecisionClient::default();

        let text = "FIX:HIGH\nFIX:LOW\nSKIP";
        let decisions = client.parse_diagnostics_decisions(text, 3);
        assert_eq!(decisions.len(), 3);
        assert_eq!(decisions[0], DiagnosticsDecision::FixHigh);
        assert_eq!(decisions[1], DiagnosticsDecision::FixLow);
        assert_eq!(decisions[2], DiagnosticsDecision::Skip);
    }

    #[test]
    fn test_parse_comment_decisions() {
        let client = AiDecisionClient::default();

        let text = "REMOVE\nKEEP\nPRAGMA\nSKIP";
        let decisions = client.parse_comment_decisions(text, 4);
        assert_eq!(decisions.len(), 4);
        assert_eq!(decisions[0], CommentDecision::Remove);
        assert_eq!(decisions[1], CommentDecision::Keep);
        assert_eq!(decisions[2], CommentDecision::AllowPragma);
        assert_eq!(decisions[3], CommentDecision::Skip);
    }

    #[test]
    fn test_tools_config() {
        let client = AiDecisionClient::default();
        let tools = client.build_tools_config().unwrap();
        assert_eq!(tools.get("bash"), Some(&false));
        assert_eq!(tools.get("mcp_*"), Some(&false));

        let config = AiWorkflowConfig {
            disable_tools: false,
            disable_mcp: false,
            ..Default::default()
        };
        let client = AiDecisionClient::new(config);
        assert!(client.build_tools_config().is_none());
    }
}
