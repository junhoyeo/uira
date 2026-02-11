//! Grep tool for searching file contents

use async_trait::async_trait;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::fs;
use uira_types::{ApprovalRequirement, JsonSchema, SandboxPreference, ToolOutput};
use walkdir::WalkDir;

use crate::{Tool, ToolContext, ToolError};

/// Input for grep tool
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct GrepInput {
    pattern: String,
    #[serde(default)]
    path: Option<String>,
    #[serde(rename = "type", default)]
    file_type: Option<String>,
    #[serde(default)]
    glob: Option<String>,
    #[serde(default)]
    output_mode: Option<String>,
    #[serde(default)]
    context: Option<usize>,
    #[serde(default)]
    head_limit: Option<usize>,
    #[serde(rename = "-i", default)]
    case_insensitive: Option<bool>,
}

/// A single grep match
#[derive(Debug, Serialize)]
struct GrepMatch {
    file: String,
    line_number: usize,
    content: String,
}

/// Grep tool for searching file contents
pub struct GrepTool;

impl GrepTool {
    pub fn new() -> Self {
        Self
    }

    fn should_include_file(
        path: &Path,
        file_type: &Option<String>,
        glob_pattern: &Option<String>,
    ) -> bool {
        let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("");

        // Check file type filter
        if let Some(ref ft) = file_type {
            let type_matches = match ft.as_str() {
                "js" => extension == "js" || extension == "jsx",
                "ts" => extension == "ts" || extension == "tsx",
                "py" => extension == "py",
                "rs" => extension == "rs",
                "go" => extension == "go",
                "java" => extension == "java",
                "c" => extension == "c" || extension == "h",
                "cpp" => {
                    extension == "cpp"
                        || extension == "hpp"
                        || extension == "cc"
                        || extension == "hh"
                }
                "md" => extension == "md",
                "json" => extension == "json",
                "yaml" | "yml" => extension == "yaml" || extension == "yml",
                "toml" => extension == "toml",
                _ => extension == ft,
            };
            if !type_matches {
                return false;
            }
        }

        // Check glob pattern
        if let Some(ref pattern) = glob_pattern {
            if let Ok(glob_matcher) = glob::Pattern::new(pattern) {
                if !glob_matcher.matches(file_name) && !glob_matcher.matches_path(path) {
                    return false;
                }
            }
        }

        true
    }

    fn is_ignored_path(path: &Path) -> bool {
        let path_str = path.to_string_lossy();
        let ignored = [
            "node_modules",
            ".git",
            "target",
            ".next",
            "dist",
            "build",
            "__pycache__",
            ".venv",
            "venv",
        ];
        ignored.iter().any(|i| path_str.contains(i))
    }
}

impl Default for GrepTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for GrepTool {
    fn name(&self) -> &str {
        "Grep"
    }

    fn description(&self) -> &str {
        "Search for a pattern in files. Supports regex patterns and filtering by file type."
    }

    fn schema(&self) -> JsonSchema {
        JsonSchema::object()
            .property(
                "pattern",
                JsonSchema::string().description("The regex pattern to search for"),
            )
            .property(
                "path",
                JsonSchema::string()
                    .description("Directory to search in (defaults to current directory)"),
            )
            .property(
                "type",
                JsonSchema::string().description("File type to search (e.g., 'rs', 'py', 'js')"),
            )
            .property(
                "glob",
                JsonSchema::string().description("Glob pattern to filter files (e.g., '*.ts')"),
            )
            .property(
                "output_mode",
                JsonSchema::string()
                    .description("Output mode: 'content', 'files_with_matches', or 'count'"),
            )
            .property(
                "context",
                JsonSchema::number().description("Number of context lines to show around matches"),
            )
            .property(
                "head_limit",
                JsonSchema::number().description("Maximum number of results to return"),
            )
            .property(
                "-i",
                JsonSchema::boolean().description("Case insensitive search"),
            )
            .required(&["pattern"])
    }

    fn approval_requirement(&self, _input: &serde_json::Value) -> ApprovalRequirement {
        // Grep is read-only and safe
        ApprovalRequirement::Skip {
            bypass_sandbox: false,
        }
    }

    fn sandbox_preference(&self) -> SandboxPreference {
        SandboxPreference::Auto
    }

    fn supports_parallel(&self) -> bool {
        true // Grep operations are safe to parallelize
    }

    async fn execute(
        &self,
        input: serde_json::Value,
        ctx: &ToolContext,
    ) -> Result<ToolOutput, ToolError> {
        let input: GrepInput =
            serde_json::from_value(input).map_err(|e| ToolError::InvalidInput {
                message: e.to_string(),
            })?;

        let case_insensitive = input.case_insensitive.unwrap_or(false);
        let pattern_str = if case_insensitive {
            format!("(?i){}", input.pattern)
        } else {
            input.pattern.clone()
        };

        let regex = Regex::new(&pattern_str).map_err(|e| ToolError::InvalidInput {
            message: format!("Invalid regex pattern: {}", e),
        })?;

        let base_path = input
            .path
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| ctx.cwd.clone());

        let output_mode = input.output_mode.as_deref().unwrap_or("files_with_matches");
        let head_limit = input.head_limit.unwrap_or(100);

        let mut matches: Vec<GrepMatch> = Vec::new();
        let mut files_with_matches: Vec<String> = Vec::new();

        for entry in WalkDir::new(&base_path)
            .into_iter()
            .filter_entry(|e| !Self::is_ignored_path(e.path()))
            .filter_map(|e| e.ok())
        {
            if !entry.file_type().is_file() {
                continue;
            }

            let path = entry.path();
            if !Self::should_include_file(path, &input.file_type, &input.glob) {
                continue;
            }

            // Read file content
            let content = match fs::read_to_string(path).await {
                Ok(c) => c,
                Err(_) => continue, // Skip binary/unreadable files
            };

            let mut file_has_match = false;

            for (line_idx, line) in content.lines().enumerate() {
                if regex.is_match(line) {
                    file_has_match = true;

                    if output_mode == "content" {
                        matches.push(GrepMatch {
                            file: path.display().to_string(),
                            line_number: line_idx + 1,
                            content: line.to_string(),
                        });

                        if matches.len() >= head_limit {
                            break;
                        }
                    }
                }
            }

            if file_has_match && output_mode != "content" {
                files_with_matches.push(path.display().to_string());
                if files_with_matches.len() >= head_limit {
                    break;
                }
            }

            if matches.len() >= head_limit || files_with_matches.len() >= head_limit {
                break;
            }
        }

        let output = match output_mode {
            "content" => {
                if matches.is_empty() {
                    "No matches found".to_string()
                } else {
                    matches
                        .iter()
                        .map(|m| format!("{}:{}:{}", m.file, m.line_number, m.content))
                        .collect::<Vec<_>>()
                        .join("\n")
                }
            }
            "count" => {
                format!(
                    "Found {} matches in {} files",
                    matches.len(),
                    files_with_matches.len()
                )
            }
            _ => {
                // files_with_matches (default)
                if files_with_matches.is_empty() {
                    "No files found".to_string()
                } else {
                    format!(
                        "Found {} files\n{}",
                        files_with_matches.len(),
                        files_with_matches.join("\n")
                    )
                }
            }
        };

        Ok(ToolOutput::text(output))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Write;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_grep_find_pattern() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.rs");
        let mut file = std::fs::File::create(&file_path).unwrap();
        writeln!(file, "fn main() {{").unwrap();
        writeln!(file, "    println!(\"hello\");").unwrap();
        writeln!(file, "}}").unwrap();

        let tool = GrepTool::new();
        let ctx = ToolContext::default();
        let result = tool
            .execute(
                json!({
                    "pattern": "println",
                    "path": dir.path().to_string_lossy(),
                    "output_mode": "content"
                }),
                &ctx,
            )
            .await
            .unwrap();

        let text = result.as_text().unwrap();
        assert!(text.contains("println"));
        assert!(text.contains(":2:"));
    }

    #[tokio::test]
    async fn test_grep_file_type_filter() {
        let dir = tempdir().unwrap();

        let rs_file = dir.path().join("test.rs");
        std::fs::write(&rs_file, "fn main() {}").unwrap();

        let py_file = dir.path().join("test.py");
        std::fs::write(&py_file, "def main(): pass").unwrap();

        let tool = GrepTool::new();
        let ctx = ToolContext::default();
        let result = tool
            .execute(
                json!({
                    "pattern": "main",
                    "path": dir.path().to_string_lossy(),
                    "type": "rs"
                }),
                &ctx,
            )
            .await
            .unwrap();

        let text = result.as_text().unwrap();
        assert!(text.contains("test.rs"));
        assert!(!text.contains("test.py"));
    }
}
