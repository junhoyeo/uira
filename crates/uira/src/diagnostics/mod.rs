use anyhow::{Context, Result};
use colored::Colorize;
use std::fs;

use crate::ai_decision::{AiDecisionClient, AiWorkflowConfig, DiagnosticsDecision, IntoAiPrompt};
use uira_config::DiagnosticsSettings;
use uira_oxc::{LintDiagnostic, LintRule, Linter, Severity};

pub struct DiagnosticItem {
    pub file: String,
    pub line: u32,
    pub column: u32,
    pub message: String,
    pub rule: String,
    pub severity: Severity,
    pub suggestion: Option<String>,
    pub context_before: Vec<String>,
    pub context_line: String,
    pub context_after: Vec<String>,
}

impl DiagnosticItem {
    fn from_lint_diagnostic(diag: LintDiagnostic, source_lines: &[&str]) -> Self {
        let line_idx = (diag.line as usize).saturating_sub(1);
        let context_before: Vec<String> = source_lines
            .iter()
            .skip(line_idx.saturating_sub(3))
            .take(3.min(line_idx))
            .map(|s| s.to_string())
            .collect();
        let context_line = source_lines
            .get(line_idx)
            .map(|s| s.to_string())
            .unwrap_or_default();
        let context_after: Vec<String> = source_lines
            .iter()
            .skip(line_idx + 1)
            .take(3)
            .map(|s| s.to_string())
            .collect();

        Self {
            file: diag.file,
            line: diag.line,
            column: diag.column,
            message: diag.message,
            rule: diag.rule,
            severity: diag.severity,
            suggestion: diag.suggestion,
            context_before,
            context_line,
            context_after,
        }
    }
}

impl IntoAiPrompt for DiagnosticItem {
    fn to_prompt_entry(&self, index: usize) -> String {
        let severity_str = match self.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Info => "info",
        };

        let suggestion_str = self
            .suggestion
            .as_deref()
            .unwrap_or("No automatic suggestion available");

        let context_before = self.context_before.join("\n");
        let context_after = self.context_after.join("\n");

        format!(
            r#"[DIAGNOSTIC {}]
File: {}
Line {}, Column {}: {}
Rule: {}
Severity: {}
Suggestion: {}
Context:
```
{}
> {}
{}
```"#,
            index + 1,
            self.file,
            self.line,
            self.column,
            self.message,
            self.rule,
            severity_str,
            suggestion_str,
            context_before,
            self.context_line,
            context_after
        )
    }
}

pub struct DiagnosticsChecker {
    ai_client: AiDecisionClient,
    #[allow(dead_code)]
    config: DiagnosticsSettings,
    linter: Linter,
}

impl DiagnosticsChecker {
    pub fn new(config: Option<DiagnosticsSettings>) -> Self {
        let config = config.unwrap_or_default();
        let (provider, model) = config.ai.parse_model();

        let ai_config = AiWorkflowConfig {
            host: "127.0.0.1".to_string(),
            port: 4096,
            model,
            provider,
            disable_tools: true,
            disable_mcp: true,
            timeout_secs: 120,
        };

        Self {
            ai_client: AiDecisionClient::new(ai_config),
            config,
            linter: Linter::new(LintRule::recommended()),
        }
    }

    pub fn with_auto_stage(mut self, auto_stage: bool) -> Self {
        self.ai_client.auto_stage = auto_stage;
        self
    }

    pub fn run(&mut self, files: &[String], severity_filter: &str) -> Result<bool> {
        let diagnostics = self.detect_diagnostics(files, severity_filter)?;

        if diagnostics.is_empty() {
            println!("{} No diagnostics found", "✓".green().bold());
            return Ok(true);
        }

        println!(
            "{} Found {} diagnostic(s)",
            "!".yellow().bold(),
            diagnostics.len()
        );

        self.ai_client.ensure_server()?;

        let decisions = self.process_diagnostics_batch(&diagnostics)?;

        let mut applied = 0;
        let mut skipped = 0;
        let mut errors = 0;

        println!();
        for (diagnostic, decision) in diagnostics.iter().zip(decisions.iter()) {
            let severity_str = match diagnostic.severity {
                Severity::Error => "error".red(),
                Severity::Warning => "warning".yellow(),
                Severity::Info => "info".blue(),
            };

            let status = match decision {
                DiagnosticsDecision::FixHigh => "FIX:HIGH".green(),
                DiagnosticsDecision::FixLow => "FIX:LOW".yellow(),
                DiagnosticsDecision::Skip => "SKIP".dimmed(),
            };

            println!(
                "  {} [{}] {}:{} - {} [{}]",
                "→".cyan(),
                severity_str,
                diagnostic.file.dimmed(),
                diagnostic.line,
                diagnostic.message,
                status
            );

            match decision {
                DiagnosticsDecision::FixHigh | DiagnosticsDecision::FixLow => {
                    let low_confidence = *decision == DiagnosticsDecision::FixLow;
                    match self.apply_fix(diagnostic, low_confidence) {
                        Ok(()) => {
                            applied += 1;
                            if low_confidence {
                                println!("      {} Applied (low confidence)", "⚠".yellow());
                            } else {
                                println!("      {} Applied", "✓".green());
                            }
                        }
                        Err(e) => {
                            eprintln!("      {} {}", "✗".red(), e);
                            errors += 1;
                        }
                    }
                }
                DiagnosticsDecision::Skip => {
                    skipped += 1;
                }
            }
        }

        self.ai_client.cleanup()?;

        if !self.ai_client.modified_files.is_empty() {
            self.ai_client.stage_modified_files()?;
        }

        println!();
        println!(
            "{} Applied: {}, Skipped: {}, Errors: {}",
            if errors > 0 {
                "⚠".yellow().bold()
            } else {
                "✓".green().bold()
            },
            applied.to_string().green(),
            skipped.to_string().yellow(),
            errors.to_string().red()
        );

        Ok(errors == 0)
    }

    fn detect_diagnostics(
        &self,
        files: &[String],
        severity_filter: &str,
    ) -> Result<Vec<DiagnosticItem>> {
        let lint_diagnostics = self.linter.lint_files(files);

        let filtered: Vec<LintDiagnostic> = lint_diagnostics
            .into_iter()
            .filter(|d| match severity_filter {
                "error" => matches!(d.severity, Severity::Error),
                "warning" => matches!(d.severity, Severity::Error | Severity::Warning),
                _ => true,
            })
            .collect();

        let mut items = Vec::with_capacity(filtered.len());
        for diag in filtered {
            let source = fs::read_to_string(&diag.file).unwrap_or_default();
            let source_lines: Vec<&str> = source.lines().collect();
            items.push(DiagnosticItem::from_lint_diagnostic(diag, &source_lines));
        }

        Ok(items)
    }

    fn process_diagnostics_batch(
        &mut self,
        diagnostics: &[DiagnosticItem],
    ) -> Result<Vec<DiagnosticsDecision>> {
        if diagnostics.is_empty() {
            return Ok(vec![]);
        }

        let mut prompt = format!(
            "I found {} diagnostic issues in code. For EACH issue, analyze if it can be auto-fixed.\n\n",
            diagnostics.len()
        );

        for (i, item) in diagnostics.iter().enumerate() {
            prompt.push_str(&item.to_prompt_entry(i));
            prompt.push_str("\n\n");
        }

        prompt.push_str(&format!(
            r#"Respond with EXACTLY {} lines, one decision per diagnostic in order.
Each line must be EXACTLY one of: FIX:HIGH, FIX:LOW, or SKIP
- FIX:HIGH: Clear auto-fix available, apply it
- FIX:LOW: Possible fix but uncertain, apply with caution
- SKIP: Requires manual intervention"#,
            diagnostics.len()
        ));

        let response = self
            .ai_client
            .send_prompt(&prompt)
            .context("Failed to get AI response")?;

        Ok(self
            .ai_client
            .parse_diagnostics_decisions(&response, diagnostics.len()))
    }

    fn apply_fix(&mut self, diagnostic: &DiagnosticItem, _low_confidence: bool) -> Result<()> {
        let suggestion = diagnostic
            .suggestion
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No suggestion available for this diagnostic"))?;

        let content =
            fs::read_to_string(&diagnostic.file).context("Failed to read file for fix")?;

        let lines: Vec<&str> = content.lines().collect();
        let line_idx = (diagnostic.line as usize).saturating_sub(1);

        if line_idx >= lines.len() {
            return Err(anyhow::anyhow!("Line number out of bounds"));
        }

        let current_line = lines[line_idx];
        let fixed_line = self.apply_suggestion_to_line(current_line, suggestion);

        let mut new_lines: Vec<String> = lines.iter().map(|s| s.to_string()).collect();
        new_lines[line_idx] = fixed_line;

        let new_content = if content.ends_with('\n') {
            format!("{}\n", new_lines.join("\n"))
        } else {
            new_lines.join("\n")
        };

        fs::write(&diagnostic.file, new_content).context("Failed to write fixed file")?;

        self.ai_client
            .modified_files
            .insert(diagnostic.file.clone());

        Ok(())
    }

    fn apply_suggestion_to_line(&self, line: &str, suggestion: &str) -> String {
        let suggestion_lower = suggestion.to_lowercase();

        if suggestion_lower.contains("replace 'var' with 'let'") {
            return line.replacen("var ", "let ", 1);
        }
        if suggestion_lower.contains("replace 'var' with 'const'") {
            return line.replacen("var ", "const ", 1);
        }
        if suggestion_lower.contains("replace 'let' with 'const'") {
            return line.replacen("let ", "const ", 1);
        }
        if suggestion_lower.contains("remove the debugger")
            && (line.trim() == "debugger;" || line.trim() == "debugger")
        {
            return String::new();
        }
        if suggestion_lower.contains("remove console") && line.trim().starts_with("console.") {
            return String::new();
        }

        line.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diagnostics_checker_creation() {
        let checker = DiagnosticsChecker::new(None);
        assert!(!checker.ai_client.modified_files.contains("test"));
    }

    #[test]
    fn test_diagnostic_item_prompt() {
        let item = DiagnosticItem {
            file: "test.js".to_string(),
            line: 5,
            column: 1,
            message: "Unexpected var".to_string(),
            rule: "no-var".to_string(),
            severity: Severity::Warning,
            suggestion: Some("Replace 'var' with 'let'".to_string()),
            context_before: vec!["let x = 1;".to_string()],
            context_line: "var y = 2;".to_string(),
            context_after: vec!["const z = 3;".to_string()],
        };

        let prompt = item.to_prompt_entry(0);
        assert!(prompt.contains("[DIAGNOSTIC 1]"));
        assert!(prompt.contains("test.js"));
        assert!(prompt.contains("Line 5"));
        assert!(prompt.contains("no-var"));
        assert!(prompt.contains("var y = 2;"));
    }

    #[test]
    fn test_apply_suggestion_var_to_let() {
        let checker = DiagnosticsChecker::new(None);
        let result =
            checker.apply_suggestion_to_line("var x = 1;", "Replace 'var' with 'let' or 'const'");
        assert_eq!(result, "let x = 1;");
    }

    #[test]
    fn test_apply_suggestion_let_to_const() {
        let checker = DiagnosticsChecker::new(None);
        let result = checker.apply_suggestion_to_line("let x = 1;", "Replace 'let' with 'const'");
        assert_eq!(result, "const x = 1;");
    }

    #[test]
    fn test_apply_suggestion_remove_debugger() {
        let checker = DiagnosticsChecker::new(None);
        let result = checker.apply_suggestion_to_line("debugger;", "Remove the debugger statement");
        assert_eq!(result, "");
    }
}
