//! Glob tool for file pattern matching

use async_trait::async_trait;
use serde::Deserialize;
use std::path::Path;
use uira_types::{ApprovalRequirement, JsonSchema, SandboxPreference, ToolOutput};

use crate::{Tool, ToolContext, ToolError};

/// Input for glob tool
#[derive(Debug, Deserialize)]
struct GlobInput {
    pattern: String,
    #[serde(default)]
    path: Option<String>,
}

/// Glob tool for file pattern matching
pub struct GlobTool;

impl GlobTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for GlobTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for GlobTool {
    fn name(&self) -> &str {
        "Glob"
    }

    fn description(&self) -> &str {
        "Find files matching a glob pattern. Supports patterns like '**/*.rs' or 'src/**/*.ts'."
    }

    fn schema(&self) -> JsonSchema {
        JsonSchema::object()
            .property(
                "pattern",
                JsonSchema::string().description("The glob pattern to match files against"),
            )
            .property(
                "path",
                JsonSchema::string()
                    .description("The directory to search in (defaults to current directory)"),
            )
            .required(&["pattern"])
    }

    fn approval_requirement(&self, _input: &serde_json::Value) -> ApprovalRequirement {
        // Globbing is read-only and safe
        ApprovalRequirement::Skip {
            bypass_sandbox: false,
        }
    }

    fn sandbox_preference(&self) -> SandboxPreference {
        SandboxPreference::Auto
    }

    fn supports_parallel(&self) -> bool {
        true // Glob operations are safe to parallelize
    }

    async fn execute(
        &self,
        input: serde_json::Value,
        ctx: &ToolContext,
    ) -> Result<ToolOutput, ToolError> {
        let input: GlobInput =
            serde_json::from_value(input).map_err(|e| ToolError::InvalidInput {
                message: e.to_string(),
            })?;

        let base_path = input
            .path
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| ctx.cwd.clone());

        // Build full pattern
        let full_pattern = if input.pattern.starts_with('/') {
            input.pattern.clone()
        } else {
            format!("{}/{}", base_path.display(), input.pattern)
        };

        let entries = glob::glob(&full_pattern).map_err(|e| ToolError::ExecutionFailed {
            message: format!("Invalid glob pattern: {}", e),
        })?;

        let mut files: Vec<String> = entries
            .filter_map(|entry| entry.ok())
            .filter(|path| path.is_file())
            .map(|path| path.display().to_string())
            .collect();

        // Sort by modification time (newest first) if possible
        files.sort_by(|a, b| {
            let a_time = Path::new(a).metadata().and_then(|m| m.modified()).ok();
            let b_time = Path::new(b).metadata().and_then(|m| m.modified()).ok();
            b_time.cmp(&a_time)
        });

        if files.is_empty() {
            Ok(ToolOutput::text("No files found matching pattern"))
        } else {
            Ok(ToolOutput::text(files.join("\n")))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::fs::File;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_glob_find_files() {
        let dir = tempdir().unwrap();
        File::create(dir.path().join("test1.rs")).unwrap();
        File::create(dir.path().join("test2.rs")).unwrap();
        File::create(dir.path().join("test.txt")).unwrap();

        let tool = GlobTool::new();
        let ctx = ToolContext::default();
        let result = tool
            .execute(
                json!({
                    "pattern": "*.rs",
                    "path": dir.path().to_string_lossy()
                }),
                &ctx,
            )
            .await
            .unwrap();

        let text = result.as_text().unwrap();
        assert!(text.contains("test1.rs"));
        assert!(text.contains("test2.rs"));
        assert!(!text.contains("test.txt"));
    }

    #[tokio::test]
    async fn test_glob_no_matches() {
        let dir = tempdir().unwrap();

        let tool = GlobTool::new();
        let ctx = ToolContext::default();
        let result = tool
            .execute(
                json!({
                    "pattern": "*.nonexistent",
                    "path": dir.path().to_string_lossy()
                }),
                &ctx,
            )
            .await
            .unwrap();

        assert!(result.as_text().unwrap().contains("No files found"));
    }
}
