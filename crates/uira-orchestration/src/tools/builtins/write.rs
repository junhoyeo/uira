//! Write tool for writing file contents

use async_trait::async_trait;
use serde::Deserialize;
use similar::TextDiff;
use std::path::Path;
use tokio::fs;
use uira_core::{ApprovalRequirement, JsonSchema, SandboxPreference, ToolOutput};

use crate::tools::{Tool, ToolContext, ToolError};

/// Input for write tool
#[derive(Debug, Deserialize)]
struct WriteInput {
    file_path: String,
    content: String,
}

/// Write tool for creating/overwriting files
pub struct WriteTool;

impl WriteTool {
    pub fn new() -> Self {
        Self
    }

    fn is_sensitive_file(path: &str) -> bool {
        let sensitive_patterns = [
            ".env",
            ".pem",
            ".key",
            "credentials",
            "secrets",
            "password",
            ".ssh/",
            "id_rsa",
            "id_ed25519",
        ];

        let lower = path.to_lowercase();
        sensitive_patterns.iter().any(|p| lower.contains(p))
    }

    fn is_system_path(path: &str) -> bool {
        let system_patterns = [
            "/etc/", "/usr/", "/bin/", "/sbin/", "/var/", "/boot/", "/sys/", "/proc/",
        ];

        system_patterns.iter().any(|p| path.starts_with(p))
    }
}

impl Default for WriteTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for WriteTool {
    fn name(&self) -> &str {
        "Write"
    }

    fn description(&self) -> &str {
        "Write content to a file. Creates the file if it doesn't exist, or overwrites if it does."
    }

    fn schema(&self) -> JsonSchema {
        JsonSchema::object()
            .property(
                "file_path",
                JsonSchema::string().description("The absolute path to the file to write"),
            )
            .property(
                "content",
                JsonSchema::string().description("The content to write to the file"),
            )
            .required(&["file_path", "content"])
    }

    fn approval_requirement(&self, input: &serde_json::Value) -> ApprovalRequirement {
        let path = input
            .get("file_path")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if Self::is_system_path(path) {
            return ApprovalRequirement::Forbidden {
                reason: "Cannot write to system directories".to_string(),
            };
        }

        if Self::is_sensitive_file(path) {
            return ApprovalRequirement::NeedsApproval {
                reason: format!("Writing to potentially sensitive file: {}", path),
            };
        }

        ApprovalRequirement::NeedsApproval {
            reason: format!("Write file: {}", path),
        }
    }

    fn sandbox_preference(&self) -> SandboxPreference {
        SandboxPreference::Auto
    }

    fn supports_parallel(&self) -> bool {
        false // Write operations should be serialized
    }

    async fn execute(
        &self,
        input: serde_json::Value,
        _ctx: &ToolContext,
    ) -> Result<ToolOutput, ToolError> {
        let input: WriteInput =
            serde_json::from_value(input).map_err(|e| ToolError::InvalidInput {
                message: e.to_string(),
            })?;

        let path = Path::new(&input.file_path);

        // Create parent directories if they don't exist
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent)
                    .await
                    .map_err(|e| ToolError::ExecutionFailed {
                        message: format!("Failed to create directory: {}", e),
                    })?;
            }
        }

        let existed = path.exists();
        let old_content = if existed {
            fs::read_to_string(path)
                .await
                .map_err(|e| ToolError::ExecutionFailed {
                    message: format!("Failed to read existing file: {}", e),
                })?
        } else {
            String::new()
        };

        fs::write(path, &input.content)
            .await
            .map_err(|e| ToolError::ExecutionFailed {
                message: format!("Failed to write file: {}", e),
            })?;

        let diff = TextDiff::from_lines(&old_content, &input.content);
        let unified = diff
            .unified_diff()
            .header(&format!("a/{}", input.file_path), &format!("b/{}", input.file_path))
            .to_string();

        Ok(ToolOutput::text(format!("{}\n{}", input.file_path, unified)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_write_new_file() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");

        let tool = WriteTool::new();
        let ctx = ToolContext::default();
        let result = tool
            .execute(
                json!({
                    "file_path": file_path.to_string_lossy(),
                    "content": "Hello, world!"
                }),
                &ctx,
            )
            .await
            .unwrap();

        let output = result.as_text().unwrap();
        assert!(output.starts_with(file_path.to_string_lossy().as_ref()));
        assert!(output.contains("@@"));
        assert!(output.contains("+Hello, world!"));
        assert_eq!(
            std::fs::read_to_string(&file_path).unwrap(),
            "Hello, world!"
        );
    }

    #[tokio::test]
    async fn test_write_creates_directories() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("nested/dir/test.txt");

        let tool = WriteTool::new();
        let ctx = ToolContext::default();
        tool.execute(
            json!({
                "file_path": file_path.to_string_lossy(),
                "content": "nested content"
            }),
            &ctx,
        )
        .await
        .unwrap();

        assert!(file_path.exists());
    }

    #[tokio::test]
    async fn test_write_overwrite_file_returns_diff() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        std::fs::write(&file_path, "before\n").unwrap();

        let tool = WriteTool::new();
        let ctx = ToolContext::default();
        let result = tool
            .execute(
                json!({
                    "file_path": file_path.to_string_lossy(),
                    "content": "after\n"
                }),
                &ctx,
            )
            .await
            .unwrap();

        let output = result.as_text().unwrap();
        assert!(output.starts_with(file_path.to_string_lossy().as_ref()));
        assert!(output.contains("@@"));
        assert!(output.contains("-before"));
        assert!(output.contains("+after"));
        assert_eq!(std::fs::read_to_string(&file_path).unwrap(), "after\n");
    }

    #[test]
    fn test_sensitive_file_detection() {
        assert!(WriteTool::is_sensitive_file(".env"));
        assert!(WriteTool::is_sensitive_file("/path/to/.env.local"));
        assert!(WriteTool::is_sensitive_file("secrets.json"));
        assert!(!WriteTool::is_sensitive_file("main.rs"));
    }
}
