use anyhow::{Context, Result};
use colored::Colorize;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

const OPENCODE_PORT: u16 = 4096;
const OPENCODE_HOST: &str = "127.0.0.1";

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

#[derive(Debug, Serialize)]
struct SessionCreateBody {
    title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    model: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Session {
    id: String,
}

#[derive(Debug, Serialize)]
struct PromptBody {
    parts: Vec<PromptPart>,
}

#[derive(Debug, Serialize)]
struct PromptPart {
    #[serde(rename = "type")]
    part_type: String,
    text: String,
}

#[derive(Debug, Deserialize)]
struct PromptResponse {
    parts: Vec<ResponsePart>,
}

#[derive(Debug, Deserialize)]
struct ResponsePart {
    #[serde(rename = "type")]
    #[allow(dead_code)]
    part_type: String,
    text: Option<String>,
}

pub struct TyposChecker {
    client: reqwest::blocking::Client,
    session_id: Option<String>,
    server_was_started: bool,
    model: Option<String>,
}

impl TyposChecker {
    pub fn new(model: Option<String>) -> Self {
        Self {
            client: reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(120))
                .build()
                .unwrap(),
            session_id: None,
            server_was_started: false,
            model,
        }
    }

    pub fn run(&mut self, files: &[String]) -> Result<bool> {
        let typos = self.find_typos(files)?;

        if typos.is_empty() {
            println!("{} No typos found", "✓".green().bold());
            return Ok(true);
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
            .get(format!("http://{}:{}/health", OPENCODE_HOST, OPENCODE_PORT))
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
                    OPENCODE_HOST, OPENCODE_PORT, session_id
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

        let body = SessionCreateBody {
            title: "astrape-typos".to_string(),
            model: self.model.clone(),
        };

        let resp: Session = self
            .client
            .post(format!(
                "http://{}:{}/session",
                OPENCODE_HOST, OPENCODE_PORT
            ))
            .json(&body)
            .send()
            .context("Failed to create session")?
            .json()
            .context("Failed to parse session response")?;

        self.session_id = Some(resp.id.clone());
        Ok(resp.id)
    }

    fn process_typo(&mut self, typo: &TypoEntry) -> Result<Decision> {
        let context = self.get_file_context(&typo.path, typo.line_num)?;
        let corrections = typo.corrections.join(", ");

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

        let body = PromptBody {
            parts: vec![PromptPart {
                part_type: "text".to_string(),
                text: prompt,
            }],
        };

        let resp = self
            .client
            .post(format!(
                "http://{}:{}/session/{}/prompt",
                OPENCODE_HOST, OPENCODE_PORT, session_id
            ))
            .json(&body)
            .send()
            .context("Failed to send prompt")?;

        let response: PromptResponse = resp.json().context("Failed to parse AI response")?;

        let ai_text = response
            .parts
            .iter()
            .find_map(|p| p.text.as_ref())
            .map(|s| s.trim().to_uppercase())
            .unwrap_or_default();

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

        Ok(decision)
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
        let content = fs::read_to_string(&typo.path)?;
        let mut lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();

        if let Some(line) = lines.get_mut(typo.line_num as usize - 1) {
            *line = line.replace(&typo.typo, correction);
        }

        fs::write(&typo.path, lines.join("\n") + "\n")?;
        println!("    {} Fixed: {} → {}", "✓".green(), typo.typo, correction);

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
        Self::new(None)
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
        assert!(checker.model.is_none());

        let checker_with_model = TyposChecker::new(Some("claude-sonnet-4-20250514".to_string()));
        assert_eq!(
            checker_with_model.model,
            Some("claude-sonnet-4-20250514".to_string())
        );
    }
}
