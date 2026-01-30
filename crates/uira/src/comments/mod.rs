use anyhow::{Context, Result};
use colored::Colorize;
use std::fs;

use crate::ai_decision::{AiDecisionClient, AiWorkflowConfig, CommentDecision, IntoAiPrompt};
use uira_comment_checker::{CommentDetector, CommentInfo, CommentType, FilterChain};
use uira_config::CommentsSettings;

pub struct CommentItem {
    pub file: String,
    pub line: usize,
    pub text: String,
    pub comment_type: CommentType,
    pub context_before: Vec<String>,
    pub context_line: String,
    pub context_after: Vec<String>,
}

impl CommentItem {
    fn from_comment_info(info: CommentInfo, source_lines: &[&str]) -> Self {
        let line_idx = info.line_number.saturating_sub(1);
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
            file: info.file_path,
            line: info.line_number,
            text: info.text,
            comment_type: info.comment_type,
            context_before,
            context_line,
            context_after,
        }
    }
}

impl IntoAiPrompt for CommentItem {
    fn to_prompt_entry(&self, index: usize) -> String {
        let type_str = match self.comment_type {
            CommentType::Line => "Line",
            CommentType::Block => "Block",
            CommentType::Docstring => "Docstring",
        };

        let context_before = self.context_before.join("\n");
        let context_after = self.context_after.join("\n");

        format!(
            r#"[COMMENT {}]
File: {}
Line {}: {}
Type: {}
Context:
```
{}
> {}
{}
```"#,
            index + 1,
            self.file,
            self.line,
            self.text.trim(),
            type_str,
            context_before,
            self.context_line,
            context_after
        )
    }
}

pub struct CommentsChecker {
    ai_client: AiDecisionClient,
    detector: CommentDetector,
    filter_chain: FilterChain,
    #[allow(dead_code)]
    config: CommentsSettings,
    pragma_format: String,
    include_docstrings: bool,
}

impl CommentsChecker {
    pub fn new(config: Option<CommentsSettings>) -> Self {
        let config = config.unwrap_or_default();
        let (provider, model) = config.ai.parse_model();
        let pragma_format = config.ai.pragma_format.clone();
        let include_docstrings = config.ai.include_docstrings;

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
            detector: CommentDetector::new(),
            filter_chain: FilterChain::new(),
            config,
            pragma_format,
            include_docstrings,
        }
    }

    pub fn with_auto_stage(mut self, auto_stage: bool) -> Self {
        self.ai_client.auto_stage = auto_stage;
        self
    }

    pub fn run(&mut self, files: &[String]) -> Result<bool> {
        let comments = self.detect_comments(files)?;

        if comments.is_empty() {
            println!("{} No actionable comments found", "✓".green().bold());
            return Ok(true);
        }

        println!(
            "{} Found {} comment(s) to analyze",
            "!".yellow().bold(),
            comments.len()
        );

        self.ai_client.ensure_server()?;

        let decisions = self.process_comments_batch(&comments)?;

        let mut removed = 0;
        let mut kept = 0;
        let mut pragma_added = 0;
        let mut skipped = 0;
        let mut errors = 0;

        println!();
        for (comment, decision) in comments.iter().zip(decisions.iter()) {
            let type_str = match comment.comment_type {
                CommentType::Line => "line".blue(),
                CommentType::Block => "block".cyan(),
                CommentType::Docstring => "doc".magenta(),
            };

            let status = match decision {
                CommentDecision::Remove => "REMOVE".red(),
                CommentDecision::Keep => "KEEP".green(),
                CommentDecision::AllowPragma => "PRAGMA".yellow(),
                CommentDecision::Skip => "SKIP".dimmed(),
            };

            let text_preview: String = comment.text.chars().take(50).collect();
            let text_preview = if comment.text.len() > 50 {
                format!("{}...", text_preview)
            } else {
                text_preview
            };

            println!(
                "  {} [{}] {}:{} - {} [{}]",
                "→".cyan(),
                type_str,
                comment.file.dimmed(),
                comment.line,
                text_preview.trim(),
                status
            );

            match decision {
                CommentDecision::Remove => match self.remove_comment(comment) {
                    Ok(()) => {
                        removed += 1;
                        println!("      {} Removed", "✓".green());
                    }
                    Err(e) => {
                        eprintln!("      {} {}", "✗".red(), e);
                        errors += 1;
                    }
                },
                CommentDecision::Keep => {
                    kept += 1;
                }
                CommentDecision::AllowPragma => match self.add_pragma(comment) {
                    Ok(()) => {
                        pragma_added += 1;
                        println!("      {} Pragma added", "✓".green());
                    }
                    Err(e) => {
                        eprintln!("      {} {}", "✗".red(), e);
                        errors += 1;
                    }
                },
                CommentDecision::Skip => {
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
            "{} Removed: {}, Kept: {}, Pragma: {}, Skipped: {}, Errors: {}",
            if errors > 0 {
                "⚠".yellow().bold()
            } else {
                "✓".green().bold()
            },
            removed.to_string().red(),
            kept.to_string().green(),
            pragma_added.to_string().yellow(),
            skipped.to_string().dimmed(),
            errors.to_string().red()
        );

        Ok(errors == 0)
    }

    fn detect_comments(&self, files: &[String]) -> Result<Vec<CommentItem>> {
        let mut items = Vec::new();

        for file in files {
            let content = match fs::read_to_string(file) {
                Ok(c) => c,
                Err(_) => continue,
            };

            let source_lines: Vec<&str> = content.lines().collect();
            let raw_comments = self
                .detector
                .detect(&content, file, self.include_docstrings);

            for comment in raw_comments {
                if self.filter_chain.should_skip(&comment) {
                    continue;
                }
                items.push(CommentItem::from_comment_info(comment, &source_lines));
            }
        }

        Ok(items)
    }

    fn process_comments_batch(&mut self, comments: &[CommentItem]) -> Result<Vec<CommentDecision>> {
        if comments.is_empty() {
            return Ok(vec![]);
        }

        let mut prompt = format!(
            "I found {} comments/docstrings in code. For EACH, analyze if it should be removed.\n\n",
            comments.len()
        );

        for (i, item) in comments.iter().enumerate() {
            prompt.push_str(&item.to_prompt_entry(i));
            prompt.push_str("\n\n");
        }

        prompt.push_str(&format!(
            r#"Respond with EXACTLY {} lines, one decision per comment in order.
Each line must be EXACTLY one of: REMOVE, KEEP, PRAGMA, or SKIP
- REMOVE: Unnecessary comment, delete it
- KEEP: Necessary (complex algorithm, API docs, security)
- PRAGMA: Add {} pragma to suppress warnings
- SKIP: Uncertain, leave unchanged"#,
            comments.len(),
            self.pragma_format
        ));

        let response = self
            .ai_client
            .send_prompt(&prompt)
            .context("Failed to get AI response")?;

        Ok(self
            .ai_client
            .parse_comment_decisions(&response, comments.len()))
    }

    fn remove_comment(&mut self, comment: &CommentItem) -> Result<()> {
        let content = fs::read_to_string(&comment.file).context("Failed to read file")?;
        let lines: Vec<&str> = content.lines().collect();
        let line_idx = comment.line.saturating_sub(1);

        if line_idx >= lines.len() {
            return Err(anyhow::anyhow!("Line number out of bounds"));
        }

        let mut new_lines: Vec<String> = Vec::new();
        for (i, line) in lines.iter().enumerate() {
            if i == line_idx {
                let trimmed = line.trim();
                if trimmed == comment.text.trim()
                    || trimmed.starts_with("//")
                    || trimmed.starts_with("#")
                    || trimmed.starts_with("/*")
                {
                    continue;
                }
            }
            new_lines.push(line.to_string());
        }

        let new_content = if content.ends_with('\n') {
            format!("{}\n", new_lines.join("\n"))
        } else {
            new_lines.join("\n")
        };

        fs::write(&comment.file, new_content).context("Failed to write file")?;
        self.ai_client.modified_files.insert(comment.file.clone());

        Ok(())
    }

    fn add_pragma(&mut self, comment: &CommentItem) -> Result<()> {
        let content = fs::read_to_string(&comment.file).context("Failed to read file")?;
        let lines: Vec<&str> = content.lines().collect();
        let line_idx = comment.line.saturating_sub(1);

        if line_idx >= lines.len() {
            return Err(anyhow::anyhow!("Line number out of bounds"));
        }

        let pragma_line = format!("// {}: allowed by AI", self.pragma_format);

        let current_line = lines[line_idx];
        let indent: String = current_line
            .chars()
            .take_while(|c| c.is_whitespace())
            .collect();
        let indented_pragma = format!("{}{}", indent, pragma_line);

        let mut new_lines: Vec<String> = lines[..line_idx].iter().map(|s| s.to_string()).collect();
        new_lines.push(indented_pragma);
        new_lines.extend(lines[line_idx..].iter().map(|s| s.to_string()));

        let new_content = if content.ends_with('\n') {
            format!("{}\n", new_lines.join("\n"))
        } else {
            new_lines.join("\n")
        };

        fs::write(&comment.file, new_content).context("Failed to write file")?;
        self.ai_client.modified_files.insert(comment.file.clone());

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_comments_checker_creation() {
        let checker = CommentsChecker::new(None);
        assert!(!checker.ai_client.modified_files.contains("test"));
        assert_eq!(checker.pragma_format, "@uira-allow");
    }

    #[test]
    fn test_comment_item_prompt() {
        let item = CommentItem {
            file: "test.js".to_string(),
            line: 5,
            text: "// TODO: fix this".to_string(),
            comment_type: CommentType::Line,
            context_before: vec!["let x = 1;".to_string()],
            context_line: "// TODO: fix this".to_string(),
            context_after: vec!["const z = 3;".to_string()],
        };

        let prompt = item.to_prompt_entry(0);
        assert!(prompt.contains("[COMMENT 1]"));
        assert!(prompt.contains("test.js"));
        assert!(prompt.contains("Line 5"));
        assert!(prompt.contains("TODO: fix this"));
        assert!(prompt.contains("Type: Line"));
    }

    #[test]
    fn test_config_pragma_format() {
        let mut settings = CommentsSettings::default();
        settings.ai.pragma_format = "@allow-comment".to_string();
        let checker = CommentsChecker::new(Some(settings));
        assert_eq!(checker.pragma_format, "@allow-comment");
    }

    #[test]
    fn test_config_include_docstrings() {
        let mut settings = CommentsSettings::default();
        settings.ai.include_docstrings = true;
        let checker = CommentsChecker::new(Some(settings));
        assert!(checker.include_docstrings);
    }
}
