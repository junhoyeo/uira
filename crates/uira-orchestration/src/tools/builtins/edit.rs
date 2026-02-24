use async_trait::async_trait;
use serde::Deserialize;
use similar::TextDiff;
use std::path::Path;
use tokio::fs;
use uira_core::{ApprovalRequirement, JsonSchema, SandboxPreference, ToolOutput};

use crate::tools::{Tool, ToolContext, ToolError};

use super::hashline;

#[derive(Debug, Deserialize)]
struct LegacyEditInput {
    file_path: String,
    old_string: String,
    new_string: String,
    #[serde(default)]
    replace_all: bool,
}

#[derive(Debug, Deserialize)]
struct HashlineEditInput {
    file_path: String,
    #[serde(default)]
    expected_file_hash: Option<String>,
    edits: Vec<HashlineEditOp>,
}

#[derive(Debug, Deserialize)]
struct HashlineEditOp {
    op: String,
    #[serde(default)]
    pos: Option<String>,
    #[serde(default)]
    end: Option<String>,
    #[serde(default)]
    lines: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum EditInput {
    Hashline(HashlineEditInput),
    Legacy(LegacyEditInput),
}

pub struct EditTool;

impl EditTool {
    pub fn new() -> Self {
        Self
    }

    fn split_lines(content: &str) -> (Vec<String>, bool, String) {
        let newline = if content.contains("\r\n") {
            "\r\n".to_string()
        } else {
            "\n".to_string()
        };

        (
            content
                .lines()
                .map(|line| line.strip_suffix('\r').unwrap_or(line).to_string())
                .collect(),
            content.ends_with('\n'),
            newline,
        )
    }

    fn join_lines(lines: &[String], ends_with_newline: bool, newline: &str) -> String {
        if lines.is_empty() {
            return String::new();
        }

        let mut output = lines.join(newline);
        if ends_with_newline {
            output.push_str(newline);
        }
        output
    }

    fn apply_hashline_edits(
        lines: &mut Vec<String>,
        edits: &[HashlineEditOp],
    ) -> Result<(), ToolError> {
        let anchored_edits = edits
            .iter()
            .filter(|edit| edit.pos.is_some() || edit.end.is_some())
            .count();
        if anchored_edits > 1 {
            return Err(ToolError::ExecutionFailed {
                message: "multiple LINE#ID-anchored edits in one request are not supported yet; split into separate Edit calls and re-Read between calls"
                    .to_string(),
            });
        }

        for edit in edits {
            let op = edit.op.to_lowercase();
            let replacement = edit
                .lines
                .iter()
                .map(|line| hashline::parse_line_content(line))
                .collect::<Vec<_>>();

            match op.as_str() {
                "replace" => {
                    let start = edit
                        .pos
                        .as_deref()
                        .and_then(hashline::parse_line_ref)
                        .ok_or(ToolError::ExecutionFailed {
                            message: "replace edit requires a valid `pos` in LINE#ID format"
                                .to_string(),
                        })?;
                    let end = edit
                        .end
                        .as_deref()
                        .and_then(hashline::parse_line_ref)
                        .ok_or(ToolError::ExecutionFailed {
                            message: "replace edit requires a valid `end` in LINE#ID format"
                                .to_string(),
                        })?;

                    hashline::verify_line_ref(lines, start).map_err(|e| {
                        ToolError::ExecutionFailed {
                            message: format!(
                                "{}; run Read again to refresh LINE#ID tags before editing",
                                e
                            ),
                        }
                    })?;
                    hashline::verify_line_ref(lines, end).map_err(|e| {
                        ToolError::ExecutionFailed {
                            message: format!(
                                "{}; run Read again to refresh LINE#ID tags before editing",
                                e
                            ),
                        }
                    })?;

                    if start.line_number > end.line_number {
                        return Err(ToolError::ExecutionFailed {
                            message: format!(
                                "replace range is invalid: start {} is after end {}",
                                start.line_number, end.line_number
                            ),
                        });
                    }

                    let start_idx = start.line_number - 1;
                    let end_idx_exclusive = end.line_number;
                    lines.splice(start_idx..end_idx_exclusive, replacement);
                }
                "append" => {
                    let insert_at = if let Some(pos) = edit.pos.as_deref() {
                        let line_ref =
                            hashline::parse_line_ref(pos).ok_or(ToolError::ExecutionFailed {
                                message: "append edit `pos` must be LINE#ID".to_string(),
                            })?;
                        hashline::verify_line_ref(lines, line_ref).map_err(|e| {
                            ToolError::ExecutionFailed {
                                message: format!(
                                    "{}; run Read again to refresh LINE#ID tags before editing",
                                    e
                                ),
                            }
                        })?;
                        line_ref.line_number
                    } else {
                        lines.len()
                    };

                    lines.splice(insert_at..insert_at, replacement);
                }
                "prepend" => {
                    let insert_at = if let Some(pos) = edit.pos.as_deref() {
                        let line_ref =
                            hashline::parse_line_ref(pos).ok_or(ToolError::ExecutionFailed {
                                message: "prepend edit `pos` must be LINE#ID".to_string(),
                            })?;
                        hashline::verify_line_ref(lines, line_ref).map_err(|e| {
                            ToolError::ExecutionFailed {
                                message: format!(
                                    "{}; run Read again to refresh LINE#ID tags before editing",
                                    e
                                ),
                            }
                        })?;
                        line_ref.line_number - 1
                    } else {
                        0
                    };

                    lines.splice(insert_at..insert_at, replacement);
                }
                _ => {
                    return Err(ToolError::ExecutionFailed {
                        message: format!(
                            "unsupported edit op `{}` (supported: replace, append, prepend)",
                            edit.op
                        ),
                    });
                }
            }
        }

        Ok(())
    }

    fn apply_legacy_edit(content: &str, input: &LegacyEditInput) -> Result<String, ToolError> {
        if !content.contains(&input.old_string) {
            return Err(ToolError::ExecutionFailed {
                message: "The old_string was not found in the file. Make sure it matches exactly."
                    .to_string(),
            });
        }

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

        Ok(if input.replace_all {
            content.replace(&input.old_string, &input.new_string)
        } else {
            content.replacen(&input.old_string, &input.new_string, 1)
        })
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
        "Edit a file using hashline-native edits (`edits`) or legacy exact string replacement (`old_string`/`new_string`). Hashline mode currently supports at most one LINE#ID-anchored edit per call."
    }

    fn schema(&self) -> JsonSchema {
        let edit_op_schema = JsonSchema::object()
            .property(
                "op",
                JsonSchema::string().description("Operation: replace, append, or prepend"),
            )
            .property(
                "pos",
                JsonSchema::string()
                    .description("Start anchor in LINE#ID format (required for replace)"),
            )
            .property(
                "end",
                JsonSchema::string()
                    .description("End anchor in LINE#ID format (required for replace)"),
            )
            .property(
                "lines",
                JsonSchema::array(JsonSchema::string()).description(
                    "New lines to write (accepts raw text or `LINE#ID | text` entries)",
                ),
            )
            .required(&["op"]);

        // Note: oneOf not supported by Anthropic API at top level.
        // Runtime validation in execute() handles the two modes:
        // - Hashline mode: requires edits + expected_file_hash
        // - Legacy mode: requires old_string + new_string
        JsonSchema::object()
            .property(
                "file_path",
                JsonSchema::string().description("The absolute path to the file to edit"),
            )
            .property(
                "expected_file_hash",
                JsonSchema::string().description(
                    "Hash from latest Read output (`file_hash`). Required in hashline mode for stale-safety.",
                ),
            )
            .property(
                "edits",
                JsonSchema::array(edit_op_schema)
                    .description("Hashline-native edit operations applied in order (currently supports at most one LINE#ID-anchored edit per call)"),
            )
            .property(
                "old_string",
                JsonSchema::string().description("Legacy mode: exact string to find"),
            )
            .property(
                "new_string",
                JsonSchema::string().description("Legacy mode: replacement string"),
            )
            .property(
                "replace_all",
                JsonSchema::boolean()
                    .description("Legacy mode: replace all occurrences (default false)"),
            )
            .required(&["file_path"])
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
        false
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

        let file_path = match &input {
            EditInput::Hashline(i) => &i.file_path,
            EditInput::Legacy(i) => &i.file_path,
        };

        let path = Path::new(file_path);
        if !path.exists() {
            return Err(ToolError::ExecutionFailed {
                message: format!("File not found: {}", file_path),
            });
        }

        let content = fs::read_to_string(path)
            .await
            .map_err(|e| ToolError::ExecutionFailed {
                message: format!("Failed to read file: {}", e),
            })?;

        let new_content = match &input {
            EditInput::Legacy(legacy) => Self::apply_legacy_edit(&content, legacy)?,
            EditInput::Hashline(hashline_input) => {
                if hashline_input.expected_file_hash.is_none() {
                    return Err(ToolError::ExecutionFailed {
                        message: "hashline mode requires `expected_file_hash` from the latest Read output"
                            .to_string(),
                    });
                }

                if let Some(expected) = hashline_input.expected_file_hash.as_deref() {
                    let current_hash = hashline::compute_file_hash(&content);
                    if !expected.eq_ignore_ascii_case(&current_hash) {
                        return Err(ToolError::ExecutionFailed {
                            message: format!(
                                "file hash mismatch: expected {}, actual {}. Re-run Read to get fresh hashline context.",
                                expected, current_hash
                            ),
                        });
                    }
                }

                if hashline_input.edits.is_empty() {
                    return Err(ToolError::ExecutionFailed {
                        message: "hashline mode requires a non-empty `edits` array".to_string(),
                    });
                }

                let (mut lines, ends_with_newline, newline) = Self::split_lines(&content);
                Self::apply_hashline_edits(&mut lines, &hashline_input.edits)?;
                Self::join_lines(&lines, ends_with_newline, &newline)
            }
        };

        fs::write(path, &new_content)
            .await
            .map_err(|e| ToolError::ExecutionFailed {
                message: format!("Failed to write file: {}", e),
            })?;

        let diff = TextDiff::from_lines(&content, &new_content);
        let unified = diff
            .unified_diff()
            .header(&format!("a/{}", file_path), &format!("b/{}", file_path))
            .to_string();

        Ok(ToolOutput::text(format!("{}\n{}", file_path, unified)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn read_text(path: &Path) -> String {
        std::fs::read_to_string(path).unwrap()
    }

    #[tokio::test]
    async fn test_edit_legacy_single_replacement() {
        let mut file = NamedTempFile::new().unwrap();
        write!(file, "Hello, World!").unwrap();

        let tool = EditTool::new();
        let ctx = ToolContext::default();
        let result = tool
            .execute(
                json!({
                    "file_path": file.path().to_string_lossy(),
                    "old_string": "World",
                    "new_string": "Rust"
                }),
                &ctx,
            )
            .await
            .unwrap();

        assert_eq!(read_text(file.path()), "Hello, Rust!");
        let output = result.as_text().unwrap();
        assert!(output.starts_with(file.path().to_string_lossy().as_ref()));
        assert!(output.contains("@@"));
    }

    #[tokio::test]
    async fn test_edit_hashline_replace_range() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "fn main() {{").unwrap();
        writeln!(file, "    let x = 1;").unwrap();
        writeln!(file, "}}").unwrap();

        let original = read_text(file.path());
        let file_hash = hashline::compute_file_hash(&original);
        let line2 = hashline::line_tag(2, "    let x = 1;");

        let tool = EditTool::new();
        let ctx = ToolContext::default();
        tool.execute(
            json!({
                "file_path": file.path().to_string_lossy(),
                "expected_file_hash": file_hash,
                "edits": [
                    {
                        "op": "replace",
                        "pos": line2,
                        "end": line2,
                        "lines": ["    let x = 2;"]
                    }
                ]
            }),
            &ctx,
        )
        .await
        .unwrap();

        let updated = read_text(file.path());
        assert!(updated.contains("let x = 2;"));
        assert!(!updated.contains("let x = 1;"));
    }

    #[tokio::test]
    async fn test_edit_hashline_file_hash_mismatch() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "line 1").unwrap();
        writeln!(file, "line 2").unwrap();

        let tool = EditTool::new();
        let ctx = ToolContext::default();
        let result = tool
            .execute(
                json!({
                    "file_path": file.path().to_string_lossy(),
                    "expected_file_hash": "deadbeef",
                    "edits": [
                        {
                            "op": "append",
                            "lines": ["line 3"]
                        }
                    ]
                }),
                &ctx,
            )
            .await;

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("file hash mismatch"));
    }

    #[tokio::test]
    async fn test_edit_hashline_append_and_prepend() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "middle").unwrap();

        let original = read_text(file.path());
        let file_hash = hashline::compute_file_hash(&original);

        let tool = EditTool::new();
        let ctx = ToolContext::default();
        tool.execute(
            json!({
                "file_path": file.path().to_string_lossy(),
                "expected_file_hash": file_hash,
                "edits": [
                    { "op": "prepend", "lines": ["start"] },
                    { "op": "append", "lines": ["end"] }
                ]
            }),
            &ctx,
        )
        .await
        .unwrap();

        let updated = read_text(file.path());
        assert!(updated.contains("start\nmiddle\nend\n"));
    }

    #[tokio::test]
    async fn test_edit_hashline_requires_expected_file_hash() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "line 1").unwrap();

        let tool = EditTool::new();
        let ctx = ToolContext::default();
        let result = tool
            .execute(
                json!({
                    "file_path": file.path().to_string_lossy(),
                    "edits": [
                        {
                            "op": "append",
                            "lines": ["line 2"]
                        }
                    ]
                }),
                &ctx,
            )
            .await;

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("requires `expected_file_hash`"));
    }

    #[tokio::test]
    async fn test_edit_hashline_preserves_crlf() {
        let file = NamedTempFile::new().unwrap();
        std::fs::write(file.path(), "alpha\r\nbeta\r\n").unwrap();

        let original = read_text(file.path());
        let file_hash = hashline::compute_file_hash(&original);
        let beta = hashline::line_tag(2, "beta");

        let tool = EditTool::new();
        let ctx = ToolContext::default();
        tool.execute(
            json!({
                "file_path": file.path().to_string_lossy(),
                "expected_file_hash": file_hash,
                "edits": [
                    {
                        "op": "replace",
                        "pos": beta,
                        "end": beta,
                        "lines": ["gamma"]
                    }
                ]
            }),
            &ctx,
        )
        .await
        .unwrap();

        let bytes = std::fs::read(file.path()).unwrap();
        let text = String::from_utf8(bytes).unwrap();
        assert_eq!(text, "alpha\r\ngamma\r\n");
    }

    #[tokio::test]
    async fn test_edit_hashline_rejects_multiple_anchored_edits() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "line 1").unwrap();
        writeln!(file, "line 2").unwrap();

        let original = read_text(file.path());
        let file_hash = hashline::compute_file_hash(&original);
        let line1 = hashline::line_tag(1, "line 1");
        let line2 = hashline::line_tag(2, "line 2");

        let tool = EditTool::new();
        let ctx = ToolContext::default();
        let result = tool
            .execute(
                json!({
                    "file_path": file.path().to_string_lossy(),
                    "expected_file_hash": file_hash,
                    "edits": [
                        {
                            "op": "replace",
                            "pos": line1,
                            "end": line1,
                            "lines": ["line one"]
                        },
                        {
                            "op": "replace",
                            "pos": line2,
                            "end": line2,
                            "lines": ["line two"]
                        }
                    ]
                }),
                &ctx,
            )
            .await;

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("multiple LINE#ID-anchored edits"));
    }
}
