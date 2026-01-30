use anyhow::{Context, Result};
use colored::Colorize;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::process::Command;

use crate::ai_decision::{AiDecisionClient, AiWorkflowConfig, Decision};
use crate::hooks::{AiHookExecutor, HookContext, HookEvent, HooksConfig};
use uira_config::TyposSettings;

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

pub struct TyposChecker {
    ai_client: AiDecisionClient,
    hook_executor: Option<AiHookExecutor>,
}

impl TyposChecker {
    #[allow(dead_code)]
    pub fn new(config: Option<TyposSettings>) -> Self {
        Self::with_hooks(config, None)
    }

    pub fn with_auto_stage(mut self, auto_stage: bool) -> Self {
        self.ai_client.auto_stage = auto_stage;
        self
    }

    pub fn with_hooks(config: Option<TyposSettings>, hooks_config: Option<HooksConfig>) -> Self {
        let config = config.unwrap_or_default();
        let (provider, model) = config.ai.parse_model();

        let ai_config = AiWorkflowConfig {
            host: config.ai.host.clone(),
            port: config.ai.port,
            model,
            provider,
            disable_tools: config.ai.disable_tools,
            disable_mcp: config.ai.disable_mcp,
            timeout_secs: 120,
        };

        let hook_executor = hooks_config.map(AiHookExecutor::new);

        Self {
            ai_client: AiDecisionClient::new(ai_config),
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

        self.ai_client.ensure_server()?;

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

        self.ai_client.cleanup()?;

        if self.ai_client.auto_stage && !self.ai_client.modified_files.is_empty() {
            self.ai_client.stage_modified_files()?;
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
            let context = AiDecisionClient::get_file_context(&typo.path, typo.line_num)?;
            let line_content = AiDecisionClient::get_line(&typo.path, typo.line_num)?;

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
        let ai_text = self.ai_client.send_prompt(&prompt)?;
        let decisions = self.ai_client.parse_decisions(&ai_text, typos.len());

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
        self.ai_client.modified_files.insert(typo.path.clone());
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
        self.ai_client
            .modified_files
            .insert(config_path.to_string());

        println!("    {} Added '{}' to {}", "✓".yellow(), word, config_path);
        Ok(())
    }
}

impl Default for TyposChecker {
    fn default() -> Self {
        Self::with_hooks(None, None)
    }
}

impl From<TyposSettings> for TyposChecker {
    fn from(config: TyposSettings) -> Self {
        Self::with_hooks(Some(config), None)
    }
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
    use uira_config::TyposAiSettings;

    #[test]
    fn test_typos_checker_creation() {
        let checker = TyposChecker::new(None);
        assert!(!checker.ai_client.auto_stage);

        let config = TyposSettings {
            ai: TyposAiSettings {
                model: "openai/gpt-4o".to_string(),
                disable_tools: false,
                disable_mcp: true,
                ..Default::default()
            },
        };
        let (provider, model) = config.ai.parse_model();
        assert_eq!(provider, "openai");
        assert_eq!(model, "gpt-4o");

        let checker_with_config = TyposChecker::new(Some(config));
        assert!(!checker_with_config.ai_client.auto_stage);
    }

    #[test]
    fn test_parse_model() {
        let config = TyposSettings::default();
        let (provider, model) = config.ai.parse_model();
        assert_eq!(provider, "anthropic");
        assert_eq!(model, "claude-sonnet-4-20250514");

        let config = TyposSettings {
            ai: TyposAiSettings {
                model: "anthropic/claude-opus-4-5-high".to_string(),
                ..Default::default()
            },
        };
        let (provider, model) = config.ai.parse_model();
        assert_eq!(provider, "anthropic");
        assert_eq!(model, "claude-opus-4-5-high");
    }

    #[test]
    fn test_tools_config() {
        let checker = TyposChecker::default();
        let tools = checker.ai_client.build_tools_config().unwrap();
        assert_eq!(tools.get("bash"), Some(&false));
        assert_eq!(tools.get("mcp_*"), Some(&false));

        let config = TyposSettings {
            ai: TyposAiSettings {
                disable_tools: false,
                disable_mcp: false,
                ..Default::default()
            },
        };
        let checker_no_disable = TyposChecker::new(Some(config));
        assert!(checker_no_disable.ai_client.build_tools_config().is_none());
    }
}
