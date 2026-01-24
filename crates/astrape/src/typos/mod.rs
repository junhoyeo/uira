use anyhow::{Context, Result};
use colored::Colorize;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

use crate::config::AiConfig;
use crate::hooks::{AiHookExecutor, HookContext, HookEvent, HooksConfig};

const DEFAULT_OPENCODE_PORT: u16 = 4096;
const DEFAULT_OPENCODE_HOST: &str = "127.0.0.1";

#[derive(Debug, Deserialize)]
pub struct TypoEntry {
    #[serde(rename = "type")]
    pub entry_type: String,
    pub path: String,
    pub line_num: u32,
    #[allow(dead_code)]
    pub byte_offset: u32,
    pub typo: String,
    pub corrections: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct Session {
    id: String,
}

#[derive(Debug, Serialize)]
struct ChatBody {
    #[serde(rename = "modelID")]
    model_id: String,

    #[serde(rename = "providerID")]
    provider_id: String,

    parts: Vec<ChatPart>,

    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<HashMap<String, bool>>,
}

#[derive(Debug, Serialize)]
struct ChatPart {
    #[serde(rename = "type")]
    part_type: String,
    text: String,
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    id: String,
}

#[derive(Debug, Deserialize)]
struct MessageInfo {
    info: MessageMeta,
    parts: Vec<MessagePart>,
}

#[derive(Debug, Deserialize)]
struct MessageMeta {
    #[allow(dead_code)]
    id: String,
    #[serde(default)]
    role: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MessagePart {
    #[serde(rename = "type")]
    part_type: String,
    text: Option<String>,
}

pub struct TyposChecker {
    client: reqwest::blocking::Client,
    session_id: Option<String>,
    server_was_started: bool,
    config: AiConfig,
    host: String,
    port: u16,
    hook_executor: Option<AiHookExecutor>,
}

impl TyposChecker {
    #[allow(dead_code)]
    pub fn new(config: Option<AiConfig>) -> Self {
        Self::with_hooks(config, None)
    }

    pub fn with_hooks(config: Option<AiConfig>, hooks_config: Option<HooksConfig>) -> Self {
        let config = config.unwrap_or_default();
        let host = config
            .host
            .clone()
            .unwrap_or_else(|| DEFAULT_OPENCODE_HOST.to_string());
        let port = config.port.unwrap_or(DEFAULT_OPENCODE_PORT);

        let hook_executor = hooks_config.map(AiHookExecutor::new);

        Self {
            client: reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(120))
                .build()
                .unwrap(),
            session_id: None,
            server_was_started: false,
            config,
            host,
            port,
            hook_executor,
        }
    }

    fn run_hook(&self, event: HookEvent, context: &HookContext) -> Result<bool> {
        if let Some(executor) = &self.hook_executor {
            let result = executor.execute(event, context)?;
            if !result.should_continue {
                if let Some(msg) = result.message {
                    eprintln!("  {} Hook stopped: {}", "!".yellow(), msg);
                }
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn build_tools_config(&self) -> Option<HashMap<String, bool>> {
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

    pub fn run(&mut self, files: &[String]) -> Result<bool> {
        let pre_check_ctx = HookContext::new();
        if !self.run_hook(HookEvent::PreCheck, &pre_check_ctx)? {
            return Ok(false);
        }

        let typos = self.find_typos(files)?;

        if typos.is_empty() {
            println!("{} No typos found", "✓".green().bold());
            return Ok(true);
        }

        let mut post_check_ctx = HookContext::new();
        post_check_ctx.set_env("TYPO_COUNT", &typos.len().to_string());
        if !self.run_hook(HookEvent::PostCheck, &post_check_ctx)? {
            return Ok(false);
        }

        println!(
            "{} Found {} potential typo(s)",
            "!".yellow().bold(),
            typos.len()
        );

        self.ensure_opencode_server()?;

        let mut applied = 0;
        let mut ignored = 0;
        let mut errors = 0;

        for typo in &typos {
            match self.process_typo(typo) {
                Ok(Decision::Apply) => {
                    self.apply_fix(typo)?;
                    applied += 1;
                }
                Ok(Decision::Ignore) => {
                    self.add_to_ignore_list(&typo.typo)?;
                    ignored += 1;
                }
                Ok(Decision::Skip) => {}
                Err(e) => {
                    eprintln!("  {} Error processing typo: {}", "✗".red(), e);
                    errors += 1;
                }
            }
        }

        self.cleanup_server()?;

        println!();
        println!(
            "{} Applied: {}, Ignored: {}, Errors: {}",
            "→".cyan().bold(),
            applied,
            ignored,
            errors
        );

        Ok(errors == 0)
    }

    fn find_typos(&self, files: &[String]) -> Result<Vec<TypoEntry>> {
        let mut cmd = Command::new("typos");
        cmd.args(["--format", "json"]);

        if files.is_empty() {
            cmd.arg(".");
        } else {
            cmd.args(files);
        }

        let output = cmd
            .output()
            .context("Failed to run typos. Is it installed? Run: cargo install typos-cli")?;

        let mut typos = Vec::new();
        for line in output.stdout.split(|&b| b == b'\n') {
            if line.is_empty() {
                continue;
            }
            if let Ok(entry) = serde_json::from_slice::<TypoEntry>(line) {
                if entry.entry_type == "typo" {
                    typos.push(entry);
                }
            }
        }

        Ok(typos)
    }

    fn ensure_opencode_server(&mut self) -> Result<()> {
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

    fn is_server_running(&self) -> bool {
        self.client
            .get(format!("http://{}:{}/health", self.host, self.port))
            .send()
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    }

    fn start_server(&self) -> Result<()> {
        Command::new("opencode")
            .args(["--server"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .context("Failed to start OpenCode server. Is opencode installed?")?;
        Ok(())
    }

    fn cleanup_server(&mut self) -> Result<()> {
        if let Some(session_id) = &self.session_id {
            let _ = self
                .client
                .delete(format!(
                    "http://{}:{}/session/{}",
                    self.host, self.port, session_id
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

    fn get_or_create_session(&mut self) -> Result<String> {
        if let Some(id) = &self.session_id {
            return Ok(id.clone());
        }

        let resp: Session = self
            .client
            .post(format!("http://{}:{}/session", self.host, self.port))
            .send()
            .context("Failed to create session")?
            .json()
            .context("Failed to parse session response")?;

        self.session_id = Some(resp.id.clone());
        Ok(resp.id)
    }

    fn process_typo(&mut self, typo: &TypoEntry) -> Result<Decision> {
        let corrections = typo.corrections.join(", ");

        let mut pre_ai_ctx = HookContext::new();
        pre_ai_ctx.set_env("FILE", &typo.path);
        pre_ai_ctx.set_env("LINE", &typo.line_num.to_string());
        pre_ai_ctx.set_env("TYPO", &typo.typo);
        pre_ai_ctx.set_env("CORRECTIONS", &corrections);
        if !self.run_hook(HookEvent::PreAi, &pre_ai_ctx)? {
            return Ok(Decision::Skip);
        }

        let context = self.get_file_context(&typo.path, typo.line_num)?;

        println!();
        println!(
            "  {} {}:{} - '{}' → '{}'",
            "?".yellow().bold(),
            typo.path.dimmed(),
            typo.line_num,
            typo.typo.red(),
            corrections.green()
        );

        let session_id = self.get_or_create_session()?;
        let prompt = format!(
            r#"I found a potential typo in code. Analyze if this should be fixed or ignored.

File: {}
Line {}: {}

Typo: "{}"
Suggested corrections: {}

Context (surrounding lines):
```
{}
```

Respond with EXACTLY one of these:
- "APPLY" if this is a real typo that should be fixed
- "IGNORE" if this is intentional (variable name, technical term, proper noun, abbreviation)
- "SKIP" if you're unsure

Just respond with the single word, nothing else."#,
            typo.path,
            typo.line_num,
            self.get_line(&typo.path, typo.line_num)?,
            typo.typo,
            corrections,
            context
        );

        let (provider_id, model_id) = self.config.parse_model();

        let body = ChatBody {
            model_id,
            provider_id,
            parts: vec![ChatPart {
                part_type: "text".to_string(),
                text: prompt,
            }],
            tools: self.build_tools_config(),
        };

        let resp = self
            .client
            .post(format!(
                "http://{}:{}/session/{}/message",
                self.host, self.port, session_id
            ))
            .json(&body)
            .send()
            .context("Failed to send message")?;

        let chat_resp: ChatResponse = resp.json().context("Failed to parse chat response")?;

        let ai_text = self.wait_for_response(&session_id, &chat_resp.id)?;

        let decision = if ai_text.contains("APPLY") {
            println!("    {} AI: Apply fix", "→".green());
            Decision::Apply
        } else if ai_text.contains("IGNORE") {
            println!("    {} AI: Ignore (adding to config)", "→".yellow());
            Decision::Ignore
        } else {
            println!("    {} AI: Skip (uncertain)", "→".dimmed());
            Decision::Skip
        };

        let mut post_ai_ctx = HookContext::new();
        post_ai_ctx.set_env("FILE", &typo.path);
        post_ai_ctx.set_env("TYPO", &typo.typo);
        post_ai_ctx.set_env("DECISION", &format!("{:?}", decision));
        let _ = self.run_hook(HookEvent::PostAi, &post_ai_ctx);

        Ok(decision)
    }

    fn find_assistant_response(
        &self,
        messages: &[MessageInfo],
        user_message_id: &str,
    ) -> Option<String> {
        let mut passed_user_message = false;
        for msg in messages {
            if msg.info.id == user_message_id {
                passed_user_message = true;
                continue;
            }
            if !passed_user_message {
                continue;
            }
            let is_assistant = msg.info.role.as_deref().map_or(true, |r| r == "assistant");
            if !is_assistant {
                continue;
            }
            for part in &msg.parts {
                if part.part_type == "text" {
                    if let Some(text) = &part.text {
                        let trimmed = text.trim();
                        if !trimmed.is_empty() {
                            return Some(trimmed.to_uppercase());
                        }
                    }
                }
            }
        }
        None
    }

    fn wait_for_response(&self, session_id: &str, user_message_id: &str) -> Result<String> {
        for _ in 0..60 {
            std::thread::sleep(Duration::from_millis(500));

            let resp = self
                .client
                .get(format!(
                    "http://{}:{}/session/{}/message",
                    self.host, self.port, session_id
                ))
                .send()
                .context("Failed to get messages")?;

            let messages: Vec<MessageInfo> = resp.json().context("Failed to parse messages")?;

            if let Some(assistant_response) =
                self.find_assistant_response(&messages, user_message_id)
            {
                return Ok(assistant_response);
            }
        }

        anyhow::bail!("Timeout waiting for AI response")
    }

    fn get_file_context(&self, path: &str, line_num: u32) -> Result<String> {
        let content = fs::read_to_string(path)?;
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

    fn get_line(&self, path: &str, line_num: u32) -> Result<String> {
        let content = fs::read_to_string(path)?;
        content
            .lines()
            .nth(line_num as usize - 1)
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow::anyhow!("Line not found"))
    }

    fn apply_fix(&self, typo: &TypoEntry) -> Result<()> {
        if typo.corrections.is_empty() {
            return Ok(());
        }

        let correction = &typo.corrections[0];

        let mut pre_fix_ctx = HookContext::new();
        pre_fix_ctx.set_env("FILE", &typo.path);
        pre_fix_ctx.set_env("LINE", &typo.line_num.to_string());
        pre_fix_ctx.set_env("TYPO", &typo.typo);
        pre_fix_ctx.set_env("CORRECTION", correction);
        if !self.run_hook(HookEvent::PreFix, &pre_fix_ctx)? {
            println!("    {} Fix skipped by hook", "→".dimmed());
            return Ok(());
        }

        let content = fs::read_to_string(&typo.path)?;
        let mut lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();

        if let Some(line) = lines.get_mut(typo.line_num as usize - 1) {
            *line = line.replace(&typo.typo, correction);
        }

        fs::write(&typo.path, lines.join("\n") + "\n")?;
        println!("    {} Fixed: {} → {}", "✓".green(), typo.typo, correction);

        let mut post_fix_ctx = HookContext::new();
        post_fix_ctx.set_env("FILE", &typo.path);
        post_fix_ctx.set_env("TYPO", &typo.typo);
        post_fix_ctx.set_env("CORRECTION", correction);
        let _ = self.run_hook(HookEvent::PostFix, &post_fix_ctx);

        Ok(())
    }

    fn add_to_ignore_list(&self, word: &str) -> Result<()> {
        let config_path = "_typos.toml";
        let mut config: TyposConfig = if Path::new(config_path).exists() {
            let content = fs::read_to_string(config_path)?;
            toml::from_str(&content).unwrap_or_default()
        } else {
            TyposConfig::default()
        };

        config
            .default
            .extend_words
            .insert(word.to_lowercase(), word.to_lowercase());

        let toml_str = toml::to_string_pretty(&config)?;
        fs::write(config_path, toml_str)?;

        println!("    {} Added '{}' to {}", "✓".yellow(), word, config_path);
        Ok(())
    }
}

impl Default for TyposChecker {
    fn default() -> Self {
        Self::with_hooks(None, None)
    }
}

impl From<AiConfig> for TyposChecker {
    fn from(config: AiConfig) -> Self {
        Self::with_hooks(Some(config), None)
    }
}

#[derive(Debug)]
enum Decision {
    Apply,
    Ignore,
    Skip,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct TyposConfig {
    #[serde(default)]
    default: TyposDefaultConfig,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct TyposDefaultConfig {
    #[serde(default, rename = "extend-words")]
    extend_words: HashMap<String, String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_typos_checker_creation() {
        let checker = TyposChecker::new(None);
        assert!(checker.session_id.is_none());
        assert!(!checker.server_was_started);
        assert!(checker.config.disable_tools);
        assert!(checker.config.disable_mcp);

        let config = AiConfig {
            model: Some("openai/gpt-4o".to_string()),
            disable_tools: false,
            disable_mcp: true,
            ..Default::default()
        };
        let checker_with_config = TyposChecker::new(Some(config));
        let (provider, model) = checker_with_config.config.parse_model();
        assert_eq!(provider, "openai");
        assert_eq!(model, "gpt-4o");
        assert!(!checker_with_config.config.disable_tools);
        assert!(checker_with_config.config.disable_mcp);
    }

    #[test]
    fn test_parse_model() {
        let config = AiConfig::default();
        let (provider, model) = config.parse_model();
        assert_eq!(provider, "anthropic");
        assert_eq!(model, "claude-sonnet-4-20250514");

        let config = AiConfig {
            model: Some("anthropic/claude-opus-4-5-high".to_string()),
            ..Default::default()
        };
        let (provider, model) = config.parse_model();
        assert_eq!(provider, "anthropic");
        assert_eq!(model, "claude-opus-4-5-high");
    }

    #[test]
    fn test_tools_config() {
        let checker = TyposChecker::default();
        let tools = checker.build_tools_config().unwrap();
        assert_eq!(tools.get("bash"), Some(&false));
        assert_eq!(tools.get("mcp_*"), Some(&false));

        let config = AiConfig {
            disable_tools: false,
            disable_mcp: false,
            ..Default::default()
        };
        let checker_no_disable = TyposChecker::new(Some(config));
        assert!(checker_no_disable.build_tools_config().is_none());
    }
}
