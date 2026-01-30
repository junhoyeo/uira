//! Edit tool for editing file contents

use async_trait::async_trait;
use serde::Deserialize;
use std::path::Path;
use tokio::fs;
use uira_protocol::{ApprovalRequirement, JsonSchema, SandboxPreference, ToolOutput};

use crate::{Tool, ToolContext, ToolError};

/// Input for edit tool
#[derive(Debug, Deserialize)]
struct EditInput {
    file_path: String,
    old_string: String,
    new_string: String,
    #[serde(default)]
    replace_all: bool,
}

/// Edit tool for string replacement in files
pub struct EditTool;

impl EditTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for EditTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for EditTool {
    fn name(&self) -> &str {
        "Edit"
    }

    fn description(&self) -> &str {
        "Edit a file by replacing an exact string with new content. The old_string must match exactly."
    }

    fn schema(&self) -> JsonSchema {
        JsonSchema::object()
            .property(
                "file_path",
                JsonSchema::string().description("The absolute path to the file to edit"),
            )
            .property(
                "old_string",
                JsonSchema::string().description("The exact string to find and replace"),
            )
            .property(
                "new_string",
                JsonSchema::string().description("The string to replace with"),
            )
            .property(
                "replace_all",
                JsonSchema::boolean()
                    .description("Replace all occurrences (default: false, replaces first only)"),
            )
            .required(&["file_path", "old_string", "new_string"])
    }

    fn approval_requirement(&self, input: &serde_json::Value) -> ApprovalRequirement {
        let path = input
            .get("file_path")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        ApprovalRequirement::NeedsApproval {
            reason: format!("Edit file: {}", path),
        }
    }

    fn sandbox_preference(&self) -> SandboxPreference {
        SandboxPreference::Auto
    }

    fn supports_parallel(&self) -> bool {
        false // Edit operations should be serialized
    }

    async fn execute(
        &self,
        input: serde_json::Value,
        _ctx: &ToolContext,
    ) -> Result<ToolOutput, ToolError> {
        let input: EditInput =
            serde_json::from_value(input).map_err(|e| ToolError::InvalidInput {
                message: e.to_string(),
            })?;

        let path = Path::new(&input.file_path);

        if !path.exists() {
            return Err(ToolError::ExecutionFailed {
                message: format!("File not found: {}", input.file_path),
            });
        }

        let content = fs::read_to_string(path)
            .await
            .map_err(|e| ToolError::ExecutionFailed {
                message: format!("Failed to read file: {}", e),
            })?;

        // Check if old_string exists
        if !content.contains(&input.old_string) {
            return Err(ToolError::ExecutionFailed {
                message: "The old_string was not found in the file. Make sure it matches exactly."
                    .to_string(),
            });
        }

        // Check for uniqueness if not replacing all
        if !input.replace_all {
            let count = content.matches(&input.old_string).count();
            if count > 1 {
                return Err(ToolError::ExecutionFailed {
                    message: format!(
                        "The old_string appears {} times in the file. Either make it more specific or use replace_all: true",
                        count
                    ),
                });
            }
        }

        // Perform replacement
        let new_content = if input.replace_all {
            content.replace(&input.old_string, &input.new_string)
        } else {
            content.replacen(&input.old_string, &input.new_string, 1)
        };

        fs::write(path, &new_content)
            .await
            .map_err(|e| ToolError::ExecutionFailed {
                message: format!("Failed to write file: {}", e),
            })?;

        let replacements = if input.replace_all {
            content.matches(&input.old_string).count()
        } else {
            1
        };

        Ok(ToolOutput::text(format!(
            "Made {} replacement(s) in {}",
            replacements, input.file_path
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn test_edit_single_replacement() {
        let mut file = NamedTempFile::new().unwrap();
        write!(file, "Hello, World!").unwrap();

        let tool = EditTool::new();
        let ctx = ToolContext::default();
        tool.execute(
            json!({
                "file_path": file.path().to_string_lossy(),
                "old_string": "World",
                "new_string": "Rust"
            }),
            &ctx,
        )
        .await
        .unwrap();

        let content = std::fs::read_to_string(file.path()).unwrap();
        assert_eq!(content, "Hello, Rust!");
    }

    #[tokio::test]
    async fn test_edit_replace_all() {
        let mut file = NamedTempFile::new().unwrap();
        write!(file, "foo bar foo baz foo").unwrap();

        let tool = EditTool::new();
        let ctx = ToolContext::default();
        let result = tool
            .execute(
                json!({
                    "file_path": file.path().to_string_lossy(),
                    "old_string": "foo",
                    "new_string": "qux",
                    "replace_all": true
                }),
                &ctx,
            )
            .await
            .unwrap();

        assert!(result.as_text().unwrap().contains("3 replacement"));
        let content = std::fs::read_to_string(file.path()).unwrap();
        assert_eq!(content, "qux bar qux baz qux");
    }

    #[tokio::test]
    async fn test_edit_not_unique_error() {
        let mut file = NamedTempFile::new().unwrap();
        write!(file, "foo foo").unwrap();

        let tool = EditTool::new();
        let ctx = ToolContext::default();
        let result = tool
            .execute(
                json!({
                    "file_path": file.path().to_string_lossy(),
                    "old_string": "foo",
                    "new_string": "bar"
                }),
                &ctx,
            )
            .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("appears 2 times"));
    }
}
