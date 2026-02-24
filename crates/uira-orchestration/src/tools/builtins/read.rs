//! Read tool for reading file contents

use async_trait::async_trait;
use serde::Deserialize;
use std::path::Path;
use tokio::fs;
use uira_core::{ApprovalRequirement, JsonSchema, SandboxPreference, ToolOutput};

use crate::tools::{Tool, ToolContext, ToolError};

use super::hashline;

const MAX_READ_FILE_BYTES: u64 = 10 * 1024 * 1024;
const MAX_LINE_DISPLAY_CHARS: usize = 2000;

/// Input for read tool
#[derive(Debug, Deserialize)]
struct ReadInput {
    file_path: String,
    #[serde(default)]
    offset: Option<usize>,
    #[serde(default)]
    limit: Option<usize>,
    #[serde(default)]
    around_line: Option<usize>,
    #[serde(default)]
    before: Option<usize>,
    #[serde(default, rename = "after")]
    after_lines: Option<usize>,
}

/// Read tool for reading file contents
pub struct ReadTool;

impl ReadTool {
    pub fn new() -> Self {
        Self
    }

    fn format_output(lines: &[&str], offset: usize) -> String {
        lines
            .iter()
            .enumerate()
            .map(|(i, line)| {
                let line_num = offset + i + 1;
                let tag = hashline::line_tag(line_num, line);
                let truncated = if line.chars().count() > MAX_LINE_DISPLAY_CHARS {
                    format!(
                        "{}...",
                        line.chars()
                            .take(MAX_LINE_DISPLAY_CHARS)
                            .collect::<String>()
                    )
                } else {
                    (*line).to_string()
                };
                format!("  {} | {}", tag, truncated)
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
        "Read file contents with LINE#ID hashline tags and file hash metadata."
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
            .property(
                "around_line",
                JsonSchema::number().description("Center line number (1-based) for reading. When set, reads lines centered around this line. Overrides offset/limit."),
            )
            .property(
                "before",
                JsonSchema::number().description("Number of lines to include before around_line (default: 5)"),
            )
            .property(
                "after",
                JsonSchema::number().description("Number of lines to include after around_line (default: 10)"),
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

        let metadata = fs::metadata(path)
            .await
            .map_err(|e| ToolError::ExecutionFailed {
                message: format!("Failed to read file metadata: {}", e),
            })?;
        if metadata.len() > MAX_READ_FILE_BYTES {
            return Err(ToolError::ExecutionFailed {
                message: format!(
                    "File too large to read safely ({} bytes > {} bytes): {}",
                    metadata.len(),
                    MAX_READ_FILE_BYTES,
                    input.file_path
                ),
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
        let (offset, limit) = if let Some(center) = input.around_line {
            let before_count = input.before.unwrap_or(5);
            let after_count = input.after_lines.unwrap_or(10);
            let center_0based = center.saturating_sub(1); // Convert 1-based to 0-based
            let start = center_0based.saturating_sub(before_count);
            let count = before_count + 1 + after_count;
            (start, count)
        } else {
            (input.offset.unwrap_or(0), input.limit.unwrap_or(2000))
        };
        let selected_lines: Vec<&str> = lines.iter().skip(offset).take(limit).copied().collect();
        let formatted = Self::format_output(&selected_lines, offset);
        let file_hash = hashline::compute_file_hash(&content);
        let start_line = if selected_lines.is_empty() {
            0
        } else {
            offset + 1
        };
        let end_line = offset + selected_lines.len();

        // Prepend file path so TUI render_read can extract it
        let output = format!(
            "{}\nfile_hash: {}\nrange: L{}-L{}\n{}",
            input.file_path, file_hash, start_line, end_line, formatted
        );
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
        assert!(text.contains("file_hash:"));
        assert!(text.contains("range: L1-L3"));
        assert!(text.contains("1#"));
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
        assert!(text.contains("range: L3-L5"));
        assert!(text.contains("line 3"));
        assert!(text.contains("line 4"));
        assert!(text.contains("line 5"));
        assert!(!text.contains("line 1"));
        assert!(!text.contains("line 6"));
    }

    #[tokio::test]
    async fn test_read_truncates_long_lines() {
        let mut file = NamedTempFile::new().unwrap();
        let long_line = "a".repeat(2500);
        writeln!(file, "{}", long_line).unwrap();

        let tool = ReadTool::new();
        let ctx = ToolContext::default();
        let result = tool
            .execute(json!({"file_path": file.path().to_string_lossy()}), &ctx)
            .await
            .unwrap();

        let text = result.as_text().unwrap();
        assert!(text.contains("..."));
        assert!(!text.contains(&long_line));
        let expected_prefix = "a".repeat(2000);
        assert!(text.contains(&expected_prefix));
    }

    #[tokio::test]
    async fn test_read_around_line() {
        let mut file = NamedTempFile::new().unwrap();
        for i in 1..=10 {
            writeln!(file, "line {}", i).unwrap();
        }

        let tool = ReadTool::new();
        let ctx = ToolContext::default();
        let result = tool
            .execute(
                json!({
                    "file_path": file.path().to_string_lossy(),
                    "around_line": 5,
                    "before": 2,
                    "after": 3
                }),
                &ctx,
            )
            .await
            .unwrap();

        let text = result.as_text().unwrap();
        // Center at line 5, before=2, after=3 -> lines 3-8
        assert!(text.contains("range: L3-L8"));
        assert!(text.contains("line 3"));
        assert!(text.contains("line 5"));
        assert!(text.contains("line 8"));
        assert!(!text.contains("line 1"));
        assert!(!text.contains("line 9"));
    }

    #[tokio::test]
    async fn test_read_around_line_defaults() {
        let mut file = NamedTempFile::new().unwrap();
        for i in 1..=100 {
            writeln!(file, "line {}", i).unwrap();
        }

        let tool = ReadTool::new();
        let ctx = ToolContext::default();
        let result = tool
            .execute(
                json!({
                    "file_path": file.path().to_string_lossy(),
                    "around_line": 50
                }),
                &ctx,
            )
            .await
            .unwrap();

        let text = result.as_text().unwrap();
        // Center at line 50, before=5 (default), after=10 (default) -> lines 45-60
        assert!(text.contains("range: L45-L60"));
        assert!(text.contains("line 45"));
        assert!(text.contains("line 50"));
        assert!(text.contains("line 60"));
        assert!(!text.contains("line 44"));
        assert!(!text.contains("line 61"));
    }

    #[tokio::test]
    async fn test_read_around_line_near_start() {
        let mut file = NamedTempFile::new().unwrap();
        for i in 1..=10 {
            writeln!(file, "line {}", i).unwrap();
        }

        let tool = ReadTool::new();
        let ctx = ToolContext::default();
        let result = tool
            .execute(
                json!({
                    "file_path": file.path().to_string_lossy(),
                    "around_line": 2,
                    "before": 5,
                    "after": 3
                }),
                &ctx,
            )
            .await
            .unwrap();

        let text = result.as_text().unwrap();
        // Center at line 2, before=5 would go to -3, saturates to 0
        // count = 5+1+3 = 9, so we get lines 1 through min(9, 10) = lines 1-9
        assert!(text.contains("range: L1-L9"));
        assert!(text.contains("line 1"));
        assert!(text.contains("line 2"));
        assert!(text.contains("line 9"));
        assert!(!text.contains("line 10"));
    }

    #[tokio::test]
    async fn test_read_around_line_near_end() {
        let mut file = NamedTempFile::new().unwrap();
        for i in 1..=100 {
            writeln!(file, "line {}", i).unwrap();
        }

        let tool = ReadTool::new();
        let ctx = ToolContext::default();
        let result = tool
            .execute(
                json!({
                    "file_path": file.path().to_string_lossy(),
                    "around_line": 98,
                    "before": 5,
                    "after": 10
                }),
                &ctx,
            )
            .await
            .unwrap();

        let text = result.as_text().unwrap();
        // Center at line 98, before=5, after=10 -> lines 93-108, but file ends at 100
        // So we get lines 93-100
        assert!(text.contains("line 93"));
        assert!(text.contains("line 98"));
        assert!(text.contains("line 100"));
        assert!(!text.contains("line 92"));
    }

    #[tokio::test]
    async fn test_read_around_line_overrides_offset() {
        let mut file = NamedTempFile::new().unwrap();
        for i in 1..=10 {
            writeln!(file, "line {}", i).unwrap();
        }

        let tool = ReadTool::new();
        let ctx = ToolContext::default();
        let result = tool
            .execute(
                json!({
                    "file_path": file.path().to_string_lossy(),
                    "around_line": 5,
                    "before": 1,
                    "after": 1,
                    "offset": 0,
                    "limit": 10
                }),
                &ctx,
            )
            .await
            .unwrap();

        let text = result.as_text().unwrap();
        // around_line should override offset/limit -> lines 4-6
        assert!(text.contains("range: L4-L6"));
        assert!(text.contains("line 4"));
        assert!(text.contains("line 5"));
        assert!(text.contains("line 6"));
        assert!(!text.contains("line 1"));
        assert!(!text.contains("line 7"));
    }
}
