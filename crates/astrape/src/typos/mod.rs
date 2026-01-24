use anyhow::{Context, Result};
use colored::Colorize;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
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
struct MessageInfo {
    #[allow(dead_code)]
    info: MessageMeta,
    parts: Vec<MessagePart>,
}

#[derive(Debug, Deserialize)]
struct MessageMeta {
    #[allow(dead_code)]
    id: String,
    #[serde(default)]
    #[allow(dead_code)]
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
    modified_files: HashSet<String>,
    auto_stage: bool,
}

impl TyposChecker {
    #[allow(dead_code)]
    pub fn new(config: Option<AiConfig>) -> Self {
        Self::with_hooks(config, None)
    }

    pub fn with_auto_stage(mut self, auto_stage: bool) -> Self {
        self.auto_stage = auto_stage;
        self
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
            modified_files: HashSet::new(),
            auto_stage: false,
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

        let grouped = self.group_typos_by_keyword(&typos);
        let unique_count = grouped.len();

        println!(
            "{} Found {} typo(s) ({} unique)",
            "!".yellow().bold(),
            typos.len(),
            unique_count
        );

        self.ensure_opencode_server()?;

        let unique_typos: Vec<&TypoEntry> = grouped.values().map(|v| v[0]).collect();
        let decisions = self.process_typos_batch_unique(&unique_typos)?;

        let decision_map: HashMap<String, Decision> = unique_typos
            .iter()
            .zip(decisions)
            .map(|(t, d)| (t.typo.clone(), d))
            .collect();

        let mut applied = 0;
        let mut ignored = 0;
        let mut errors = 0;

        println!();
        for (typo_word, occurrences) in &grouped {
            let decision = decision_map.get(typo_word).unwrap_or(&Decision::Skip);
            let corrections = occurrences[0].corrections.join(", ");

            let status = match decision {
                Decision::Apply => format!("{}", "APPLY".green()),
                Decision::Ignore => format!("{}", "IGNORE".yellow()),
                Decision::Skip => format!("{}", "SKIP".dimmed()),
            };

            println!(
                "  {} '{}' → '{}' ({} occurrences) [{}]",
                "→".cyan(),
                typo_word.red(),
                corrections.green(),
                occurrences.len(),
                status
            );

            for occ in occurrences {
                println!("      {}:{}", occ.path.dimmed(), occ.line_num);

                match decision {
                    Decision::Apply => match self.apply_fix(occ) {
                        Ok(()) => applied += 1,
                        Err(e) => {
                            eprintln!("        {} {}", "✗".red(), e);
                            errors += 1;
                        }
                    },
                    Decision::Ignore => {}
                    Decision::Skip => {}
                }
            }

            if matches!(decision, Decision::Ignore) {
                match self.add_to_ignore_list(typo_word) {
                    Ok(()) => ignored += 1,
                    Err(e) => {
                        eprintln!("    {} {}", "✗".red(), e);
                        errors += 1;
                    }
                }
            }
        }

        self.cleanup_server()?;

        if self.auto_stage && !self.modified_files.is_empty() {
            self.stage_modified_files()?;
        }

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

    fn group_typos_by_keyword<'a>(
        &self,
        typos: &'a [TypoEntry],
    ) -> HashMap<String, Vec<&'a TypoEntry>> {
        let mut grouped: HashMap<String, Vec<&TypoEntry>> = HashMap::new();
        for typo in typos {
            grouped.entry(typo.typo.clone()).or_default().push(typo);
        }
        grouped
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
            .args(["serve"])
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

    fn process_typos_batch_unique(&mut self, typos: &[&TypoEntry]) -> Result<Vec<Decision>> {
        for typo in typos {
            let corrections = typo.corrections.join(", ");
            let mut pre_ai_ctx = HookContext::new();
            pre_ai_ctx.set_env("FILE", &typo.path);
            pre_ai_ctx.set_env("LINE", &typo.line_num.to_string());
            pre_ai_ctx.set_env("TYPO", &typo.typo);
            pre_ai_ctx.set_env("CORRECTIONS", &corrections);
            let _ = self.run_hook(HookEvent::PreAi, &pre_ai_ctx);
        }

        let mut typo_list = String::new();
        for (i, typo) in typos.iter().enumerate() {
            let corrections = typo.corrections.join(", ");
            let context = self.get_file_context(&typo.path, typo.line_num)?;
            let line_content = self.get_line(&typo.path, typo.line_num)?;

            typo_list.push_str(&format!(
                r#"
[TYPO {}]
File: {}
Line {}: {}
Typo: "{}"
Suggested corrections: {}
Context:
```
{}
```
"#,
                i + 1,
                typo.path,
                typo.line_num,
                line_content,
                typo.typo,
                corrections,
                context
            ));
        }

        let session_id = self.get_or_create_session()?;
        let prompt = format!(
            r#"I found {} unique potential typos in code. For EACH typo, analyze if it should be fixed or ignored.

{}

Respond with EXACTLY {} lines, one decision per typo in order.
Each line must be EXACTLY one of: APPLY, IGNORE, or SKIP
- APPLY: Real typo that should be fixed
- IGNORE: Intentional (variable name, technical term, proper noun, abbreviation)
- SKIP: Unsure

Example response for 3 typos:
APPLY
IGNORE
SKIP

Your response (one word per line, {} lines total):"#,
            typos.len(),
            typo_list,
            typos.len(),
            typos.len()
        );

        println!(
            "  {} Analyzing {} unique typo(s) with AI...",
            "→".dimmed(),
            typos.len()
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

        let message: MessageInfo = resp.json().context("Failed to parse AI response")?;

        let ai_text = self.extract_text_from_message(&message);
        let decisions = self.parse_batch_decisions(&ai_text, typos.len());

        for (typo, decision) in typos.iter().zip(decisions.iter()) {
            let mut post_ai_ctx = HookContext::new();
            post_ai_ctx.set_env("FILE", &typo.path);
            post_ai_ctx.set_env("TYPO", &typo.typo);
            post_ai_ctx.set_env("DECISION", &format!("{:?}", decision));
            let _ = self.run_hook(HookEvent::PostAi, &post_ai_ctx);
        }

        println!("  {} AI analysis complete", "✓".green());

        Ok(decisions)
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

    fn parse_batch_decisions(&self, ai_text: &str, expected_count: usize) -> Vec<Decision> {
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

    fn apply_fix(&mut self, typo: &TypoEntry) -> Result<()> {
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
        self.modified_files.insert(typo.path.clone());
        println!("    {} Fixed: {} → {}", "✓".green(), typo.typo, correction);

        let mut post_fix_ctx = HookContext::new();
        post_fix_ctx.set_env("FILE", &typo.path);
        post_fix_ctx.set_env("TYPO", &typo.typo);
        post_fix_ctx.set_env("CORRECTION", correction);
        let _ = self.run_hook(HookEvent::PostFix, &post_fix_ctx);

        Ok(())
    }

    fn add_to_ignore_list(&mut self, word: &str) -> Result<()> {
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
        self.modified_files.insert(config_path.to_string());

        println!("    {} Added '{}' to {}", "✓".yellow(), word, config_path);
        Ok(())
    }

    fn stage_modified_files(&self) -> Result<()> {
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
