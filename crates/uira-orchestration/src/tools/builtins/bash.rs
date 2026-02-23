//! Bash tool for executing shell commands

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::process::Stdio;
use std::time::Duration;
use tokio::time::timeout;
use uira_core::{ApprovalRequirement, JsonSchema, SandboxPreference, ToolOutput};
use uira_security::{SandboxManager, SandboxPolicy, SandboxType};

use crate::tools::{Tool, ToolContext, ToolError};

const MAX_OUTPUT_BYTES: usize = 5 * 1024 * 1024;

fn truncate_output(s: &str) -> String {
    if s.len() <= MAX_OUTPUT_BYTES {
        return s.to_string();
    }
    let half = MAX_OUTPUT_BYTES / 2;
    let prefix_end = s.ceil_char_boundary(half);
    let suffix_start = s.floor_char_boundary(s.len() - half);
    let omitted = s.len() - prefix_end - (s.len() - suffix_start);
    format!(
        "{}\n\n[...truncated {} bytes...]\n\n{}",
        &s[..prefix_end],
        omitted,
        &s[suffix_start..]
    )
}

/// Input for bash tool
#[derive(Debug, Deserialize)]
struct BashInput {
    command: String,
    #[serde(default)]
    timeout_ms: Option<u64>,
    #[serde(default)]
    working_directory: Option<String>,
}

/// Output for bash tool
#[derive(Debug, Serialize, Deserialize)]
struct BashOutput {
    stdout: String,
    stderr: String,
    exit_code: i32,
}

/// Bash tool for executing shell commands
pub struct BashTool;

impl BashTool {
    pub fn new() -> Self {
        Self
    }

    fn is_dangerous_command(cmd: &str) -> bool {
        let dangerous_patterns = [
            "rm -rf /",
            "rm -rf /*",
            "dd if=",
            "mkfs",
            "> /dev/sd",
            "chmod -R 777 /",
            ":(){ :|:& };:",
        ];

        let lower = cmd.to_lowercase();
        dangerous_patterns.iter().any(|p| lower.contains(p))
    }

    fn is_safe_command(cmd: &str) -> bool {
        if cmd.contains('\n')
            || ['|', '&', ';', '>', '<', '$', '`', '(', ')']
                .iter()
                .any(|c| cmd.contains(*c))
        {
            return false;
        }

        let parts: Vec<&str> = cmd.split_whitespace().collect();
        let base_cmd = parts.first().copied().unwrap_or("");

        let safe_commands = [
            "ls", "pwd", "whoami", "echo", "cat", "head", "tail", "wc", "date", "uname", "which",
            "type", "file", "stat", "df", "du", "free", "uptime", "hostname",
        ];

        if safe_commands.contains(&base_cmd) {
            return true;
        }

        // git read-only commands
        if base_cmd == "git" {
            let git_cmd = parts.get(1).copied().unwrap_or("");
            let safe_git = [
                "status", "log", "diff", "branch", "remote", "show", "ls-files",
            ];
            if safe_git.contains(&git_cmd) {
                return true;
            }
        }

        // cargo read-only commands
        if base_cmd == "cargo" {
            let cargo_cmd = parts.get(1).copied().unwrap_or("");
            let safe_cargo = ["check", "clippy", "fmt", "test", "build", "doc"];
            if safe_cargo.contains(&cargo_cmd) {
                return true;
            }
        }

        false
    }
}

impl Default for BashTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for BashTool {
    fn name(&self) -> &str {
        "Bash"
    }

    fn description(&self) -> &str {
        "Execute a bash command. Use for running shell commands, scripts, and system utilities."
    }

    fn schema(&self) -> JsonSchema {
        JsonSchema::object()
            .property(
                "command",
                JsonSchema::string().description("The bash command to execute"),
            )
            .property(
                "timeout_ms",
                JsonSchema::number().description("Timeout in milliseconds (default: 120000)"),
            )
            .property(
                "working_directory",
                JsonSchema::string().description("Working directory for the command"),
            )
            .required(&["command"])
    }

    fn approval_requirement(&self, input: &serde_json::Value) -> ApprovalRequirement {
        let cmd = input.get("command").and_then(|v| v.as_str()).unwrap_or("");

        if Self::is_dangerous_command(cmd) {
            return ApprovalRequirement::Forbidden {
                reason: "This command could cause irreversible damage".to_string(),
            };
        }

        if Self::is_safe_command(cmd) {
            return ApprovalRequirement::Skip {
                bypass_sandbox: false,
            };
        }

        ApprovalRequirement::NeedsApproval {
            reason: format!("Execute command: {}", cmd),
        }
    }

    fn sandbox_preference(&self) -> SandboxPreference {
        SandboxPreference::Auto
    }

    fn supports_parallel(&self) -> bool {
        false // Bash commands can have side effects
    }

    async fn execute(
        &self,
        input: serde_json::Value,
        ctx: &ToolContext,
    ) -> Result<ToolOutput, ToolError> {
        let input: BashInput =
            serde_json::from_value(input).map_err(|e| ToolError::InvalidInput {
                message: e.to_string(),
            })?;

        let command = input.command.clone();
        let timeout_duration = Duration::from_millis(input.timeout_ms.unwrap_or(120_000));
        let working_dir = input
            .working_directory
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| ctx.cwd.clone());
        let bash_output = match ctx.sandbox_type {
            SandboxType::Native => {
                self.execute_sandboxed(
                    &command,
                    &working_dir,
                    timeout_duration,
                    &ctx.sandbox_policy,
                )
                .await
            }
            SandboxType::None | SandboxType::Container => {
                self.execute_direct(&command, &working_dir, timeout_duration)
                    .await
            }
        }?;

        Ok(ToolOutput::text(Self::format_output(&command, &bash_output)))
    }
}
impl BashTool {
    /// Format BashOutput into a human-readable string for TUI rendering.
    /// Format: command on first line, then stdout, then stderr/exit_code if relevant.
    fn format_output(command: &str, output: &BashOutput) -> String {
        let mut result = String::new();
        result.push_str(command);
        if !output.stdout.is_empty() {
            result.push('\n');
            result.push_str(&output.stdout);
        }
        if !output.stderr.is_empty() {
            result.push_str("\nstderr:\n");
            result.push_str(&output.stderr);
        }
        if output.exit_code != 0 {
            result.push_str(&format!("\nexit code: {}", output.exit_code));
        }
        result
    }

    async fn execute_direct(
        &self,
        command: &str,
        working_dir: &std::path::Path,
        timeout_duration: Duration,
    ) -> Result<BashOutput, ToolError> {
        let mut cmd = tokio::process::Command::new("bash");
        cmd.arg("-c")
            .arg(command)
            .current_dir(working_dir)
            .kill_on_drop(true)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let child = cmd.spawn().map_err(|e| ToolError::ExecutionFailed {
            message: format!("Failed to start command: {}", e),
        })?;
        let result = timeout(timeout_duration, child.wait_with_output()).await;

        match result {
            Ok(Ok(output)) => {
                let stdout = truncate_output(&String::from_utf8_lossy(&output.stdout));
                let stderr = truncate_output(&String::from_utf8_lossy(&output.stderr));
                let exit_code = output.status.code().unwrap_or(-1);
                Ok(BashOutput {
                    stdout,
                    stderr,
                    exit_code,
                })
            }
            Ok(Err(e)) => Err(ToolError::ExecutionFailed {
                message: format!("Failed to execute command: {}", e),
            }),
            Err(_) => Err(ToolError::ExecutionFailed {
                message: format!("Command timed out after {}ms", timeout_duration.as_millis()),
            }),
        }
    }

    async fn execute_sandboxed(
        &self,
        command: &str,
        working_dir: &std::path::Path,
        timeout_duration: Duration,
        sandbox_policy: &SandboxPolicy,
    ) -> Result<BashOutput, ToolError> {
        let sandbox_manager = SandboxManager::new(sandbox_policy.clone());

        let mut cmd = std::process::Command::new("bash");
        cmd.arg("-c")
            .arg(command)
            .current_dir(working_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        if let Err(e) = sandbox_manager.wrap_command(&mut cmd, SandboxType::Native) {
            return Err(ToolError::ExecutionFailed {
                message: format!("Failed to apply sandbox wrapper: {}", e),
            });
        }

        let result = tokio::time::timeout(
            timeout_duration,
            tokio::task::spawn_blocking(move || cmd.output()),
        )
        .await;

        match result {
            Ok(Ok(Ok(output))) => {
                let stdout = truncate_output(&String::from_utf8_lossy(&output.stdout));
                let stderr = truncate_output(&String::from_utf8_lossy(&output.stderr));
                let exit_code = output.status.code().unwrap_or(-1);
                Ok(BashOutput {
                    stdout,
                    stderr,
                    exit_code,
                })
            }
            Ok(Ok(Err(e))) => Err(ToolError::ExecutionFailed {
                message: format!("Failed to execute sandboxed command: {}", e),
            }),
            Ok(Err(e)) => Err(ToolError::ExecutionFailed {
                message: format!("Sandbox task panicked: {}", e),
            }),
            Err(_) => Err(ToolError::ExecutionFailed {
                message: format!(
                    "Sandboxed command timed out after {}ms",
                    timeout_duration.as_millis()
                ),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn test_bash_echo() {
        let tool = BashTool::new();
        let ctx = ToolContext::default();
        let result = tool
            .execute(json!({"command": "echo hello"}), &ctx)
            .await
            .unwrap();
        let text = result.as_text().unwrap();
        // format_output puts command on first line, stdout follows
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines[0], "echo hello");
        assert_eq!(lines[1], "hello");
    }

    #[test]
    fn test_dangerous_command_detection() {
        assert!(BashTool::is_dangerous_command("rm -rf /"));
        assert!(BashTool::is_dangerous_command("sudo rm -rf /*"));
        assert!(!BashTool::is_dangerous_command("rm file.txt"));
    }

    #[test]
    fn test_safe_command_detection() {
        assert!(BashTool::is_safe_command("ls -la"));
        assert!(BashTool::is_safe_command("git status"));
        assert!(BashTool::is_safe_command("cargo check"));
        assert!(!BashTool::is_safe_command("npm install"));
    }
}
