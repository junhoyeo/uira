//! Read tool for reading file contents

use async_trait::async_trait;
use serde::Deserialize;
use std::path::Path;
use tokio::fs;
use uira_protocol::{ApprovalRequirement, JsonSchema, SandboxPreference, ToolOutput};

use crate::{Tool, ToolContext, ToolError};

/// Input for read tool
#[derive(Debug, Deserialize)]
struct ReadInput {
    file_path: String,
    #[serde(default)]
    offset: Option<usize>,
    #[serde(default)]
    limit: Option<usize>,
}

/// Read tool for reading file contents
pub struct ReadTool;

impl ReadTool {
    pub fn new() -> Self {
        Self
    }

    fn format_output(content: &str, offset: usize) -> String {
        content
            .lines()
            .enumerate()
            .map(|(i, line)| {
                let line_num = offset + i + 1;
                let truncated = if line.len() > 2000 {
                    format!("{}...", &line[..2000])
                } else {
                    line.to_string()
                };
                format!("{:>6}\t{}", line_num, truncated)
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

impl Default for ReadTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for ReadTool {
    fn name(&self) -> &str {
        "Read"
    }

    fn description(&self) -> &str {
        "Read the contents of a file. Returns the file content with line numbers."
    }

    fn schema(&self) -> JsonSchema {
        JsonSchema::object()
            .property(
                "file_path",
                JsonSchema::string().description("The absolute path to the file to read"),
            )
            .property(
                "offset",
                JsonSchema::number().description("Line number to start reading from (0-indexed)"),
            )
            .property(
                "limit",
                JsonSchema::number().description("Maximum number of lines to read"),
            )
            .required(&["file_path"])
    }

    fn approval_requirement(&self, _input: &serde_json::Value) -> ApprovalRequirement {
        // Reading files is generally safe
        ApprovalRequirement::Skip {
            bypass_sandbox: false,
        }
    }

    fn sandbox_preference(&self) -> SandboxPreference {
        SandboxPreference::Auto
    }

    fn supports_parallel(&self) -> bool {
        true // Read operations are safe to parallelize
    }

    async fn execute(
        &self,
        input: serde_json::Value,
        _ctx: &ToolContext,
    ) -> Result<ToolOutput, ToolError> {
        let input: ReadInput =
            serde_json::from_value(input).map_err(|e| ToolError::InvalidInput {
                message: e.to_string(),
            })?;

        let path = Path::new(&input.file_path);

        if !path.exists() {
            return Err(ToolError::ExecutionFailed {
                message: format!("File not found: {}", input.file_path),
            });
        }

        if !path.is_file() {
            return Err(ToolError::ExecutionFailed {
                message: format!("Path is not a file: {}", input.file_path),
            });
        }

        let content = fs::read_to_string(path).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::InvalidData {
                ToolError::ExecutionFailed {
                    message: format!(
                        "File appears to be binary or contains invalid UTF-8: {}",
                        input.file_path
                    ),
                }
            } else {
                ToolError::ExecutionFailed {
                    message: format!("Failed to read file: {}", e),
                }
            }
        })?;

        let lines: Vec<&str> = content.lines().collect();
        let offset = input.offset.unwrap_or(0);
        let limit = input.limit.unwrap_or(2000);

        let selected_lines: Vec<&str> = lines.iter().skip(offset).take(limit).copied().collect();

        let output = Self::format_output(&selected_lines.join("\n"), offset);

        Ok(ToolOutput::text(output))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn test_read_file() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "line 1").unwrap();
        writeln!(file, "line 2").unwrap();
        writeln!(file, "line 3").unwrap();

        let tool = ReadTool::new();
        let ctx = ToolContext::default();
        let result = tool
            .execute(json!({"file_path": file.path().to_string_lossy()}), &ctx)
            .await
            .unwrap();

        let text = result.as_text().unwrap();
        assert!(text.contains("line 1"));
        assert!(text.contains("line 2"));
        assert!(text.contains("line 3"));
    }

    #[tokio::test]
    async fn test_read_with_offset_and_limit() {
        let mut file = NamedTempFile::new().unwrap();
        for i in 1..=10 {
            writeln!(file, "line {}", i).unwrap();
        }

        let tool = ReadTool::new();
        let ctx = ToolContext::default();
        let result = tool
            .execute(
                json!({"file_path": file.path().to_string_lossy(), "offset": 2, "limit": 3}),
                &ctx,
            )
            .await
            .unwrap();

        let text = result.as_text().unwrap();
        assert!(text.contains("line 3"));
        assert!(text.contains("line 4"));
        assert!(text.contains("line 5"));
        assert!(!text.contains("line 1"));
        assert!(!text.contains("line 6"));
    }
}
