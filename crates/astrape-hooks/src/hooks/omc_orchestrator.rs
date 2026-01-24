//! OMC Orchestrator Hook (ported from TypeScript)
//!
//! Enforces orchestrator behavior: prefer delegation over direct implementation.
//! This is a single-file Rust port of:
//! `oh-my-claudecode/src/hooks/omc-orchestrator/*`.
//!
//! Notes:
//! - The upstream TypeScript implementation integrates with "boulder-state".
//!   This Rust crate currently does not include that feature, so continuation
//!   checks are stubbed to "no continuation".

use async_trait::async_trait;
use chrono::Utc;
use lazy_static::lazy_static;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::process::Command;

use crate::hook::{Hook, HookContext, HookResult};
use crate::hooks::notepad::NotepadHook;
use crate::types::{HookEvent, HookInput, HookOutput};

// =============================================================================
// Constants
// =============================================================================

pub const HOOK_NAME: &str = "omc-orchestrator";

/// @deprecated Legacy single prefix
pub const ALLOWED_PATH_PREFIX: &str = ".omc/";

lazy_static! {
    /// Path patterns that orchestrator IS allowed to modify directly
    static ref ALLOWED_PATH_PATTERNS: Vec<Regex> = vec![
        Regex::new(r"^\.omc/").unwrap(),
        Regex::new(r"^\.claude/").unwrap(),
        Regex::new(r"^~?/\.claude/").unwrap(),
        Regex::new(r"/\.claude/").unwrap(),
        Regex::new(r"CLAUDE\.md$").unwrap(),
        Regex::new(r"AGENTS\.md$").unwrap(),
    ];

    static ref WRITE_EDIT_TOOLS: Vec<&'static str> = vec!["Write", "Edit", "write", "edit"];

    static ref WARNED_EXTENSIONS: Vec<&'static str> = vec![
        ".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs",
        ".py", ".pyw",
        ".go",
        ".rs",
        ".java", ".kt", ".scala",
        ".c", ".cpp", ".cc", ".h", ".hpp",
        ".rb",
        ".php",
        ".svelte", ".vue",
        ".graphql", ".gql",
        ".sh", ".bash", ".zsh",
    ];

    static ref REMEMBER_PRIORITY_RE: Regex =
        Regex::new(r"(?si)<remember\s+priority>(.*?)</remember>").unwrap();
    static ref REMEMBER_RE: Regex = Regex::new(r"(?si)<remember>(.*?)</remember>").unwrap();
}

pub const DIRECT_WORK_REMINDER: &str = r#"

---

[SYSTEM REMINDER - DELEGATION REQUIRED]

You just performed direct file modifications outside `.omc/`.

**You are an ORCHESTRATOR, not an IMPLEMENTER.**

As an orchestrator, you should:
- **DELEGATE** implementation work to subagents via the Task tool
- **VERIFY** the work done by subagents
- **COORDINATE** multiple tasks and ensure completion

You should NOT:
- Write code directly (except for `.omc/` files like plans and notepads)
- Make direct file edits outside `.omc/`
- Implement features yourself

**If you need to make changes:**
1. Use the Task tool to delegate to an appropriate subagent
2. Provide clear instructions in the prompt
3. Verify the subagent's work after completion

---
"#;

pub const ORCHESTRATOR_DELEGATION_REQUIRED: &str = r#"

---

[CRITICAL SYSTEM DIRECTIVE - DELEGATION REQUIRED]

**STOP. YOU ARE VIOLATING ORCHESTRATOR PROTOCOL.**

You (coordinator) are attempting to directly modify a file outside `.omc/`.

**Path attempted:** $FILE_PATH

---

**THIS IS FORBIDDEN** (except for VERIFICATION purposes)

As an ORCHESTRATOR, you MUST:
1. **DELEGATE** all implementation work via the Task tool
2. **VERIFY** the work done by subagents (reading files is OK)
3. **COORDINATE** - you orchestrate, you don't implement

**ALLOWED direct file operations:**
- Files inside `.omc/` (plans, notepads, drafts)
- Files inside `~/.claude/` (global config)
- `CLAUDE.md` and `AGENTS.md` files
- Reading files for verification
- Running diagnostics/tests

**FORBIDDEN direct file operations:**
- Writing/editing source code
- Creating new files outside `.omc/`
- Any implementation work

---

**IF THIS IS FOR VERIFICATION:**
Proceed if you are verifying subagent work by making a small fix.
But for any substantial changes, USE the Task tool.

**CORRECT APPROACH:**
```
Task tool with subagent_type="executor"
prompt="[specific single task with clear acceptance criteria]"
```

DELEGATE. DON'T IMPLEMENT.

---
"#;

pub const VERIFICATION_REMINDER: &str = r#"**MANDATORY VERIFICATION - SUBAGENTS LIE**

Subagents FREQUENTLY claim completion when:
- Tests are actually FAILING
- Code has type/lint ERRORS
- Implementation is INCOMPLETE
- Patterns were NOT followed

**YOU MUST VERIFY EVERYTHING YOURSELF:**

1. Run tests yourself - Must PASS (not "agent said it passed")
2. Read the actual code - Must match requirements
3. Check build/typecheck - Must succeed

DO NOT TRUST THE AGENT'S SELF-REPORT.
VERIFY EACH CLAIM WITH YOUR OWN TOOL CALLS."#;

pub const SINGLE_TASK_DIRECTIVE: &str = r#"

[SYSTEM DIRECTIVE - SINGLE TASK ONLY]

**STOP. READ THIS BEFORE PROCEEDING.**

If you were NOT given **exactly ONE atomic task**, you MUST:
1. **IMMEDIATELY REFUSE** this request
2. **DEMAND** the orchestrator provide a single, specific task

**Your response if multiple tasks detected:**
> "I refuse to proceed. You provided multiple tasks. An orchestrator's impatience destroys work quality.
>
> PROVIDE EXACTLY ONE TASK. One file. One change. One verification.
>
> Your rushing will cause: incomplete work, missed edge cases, broken tests, wasted context."

**WARNING TO ORCHESTRATOR:**
- Your hasty batching RUINS deliverables
- Each task needs FULL attention and PROPER verification
- Batch delegation = sloppy work = rework = wasted tokens

**REFUSE multi-task requests. DEMAND single-task clarity.**
"#;

pub const BOULDER_CONTINUATION_PROMPT: &str = r#"[SYSTEM REMINDER - BOULDER CONTINUATION]

You have an active work plan with incomplete tasks. Continue working.

RULES:
- Proceed without asking for permission
- Mark each checkbox [x] in the plan file when done
- Use the notepad at .omc/notepads/{PLAN_NAME}/ to record learnings
- Do not stop until all tasks are complete
- If blocked, document the blocker and move to the next task"#;

// =============================================================================
// Audit
// =============================================================================

const LOG_DIR: &str = ".omc/logs";
const LOG_FILE: &str = "delegation-audit.jsonl";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AuditDecision {
    Allowed,
    Warned,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AuditReason {
    AllowedPath,
    SourceFile,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub timestamp: String,
    pub tool: String,
    #[serde(rename = "filePath")]
    pub file_path: String,
    pub decision: AuditDecision,
    pub reason: AuditReason,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AuditEntryInput {
    pub tool: String,
    pub file_path: String,
    pub decision: AuditDecision,
    pub reason: AuditReason,
    pub session_id: Option<String>,
}

pub fn log_audit_entry(directory: &str, entry: AuditEntryInput) {
    // Audit must never break main functionality.
    let ts = Utc::now().to_rfc3339();
    let full = AuditEntry {
        timestamp: ts,
        tool: entry.tool,
        file_path: entry.file_path,
        decision: entry.decision,
        reason: entry.reason,
        session_id: entry.session_id,
    };

    let log_dir = Path::new(directory).join(LOG_DIR);
    let log_path = log_dir.join(LOG_FILE);
    let _ = fs::create_dir_all(&log_dir);

    let Ok(line) = serde_json::to_string(&full) else {
        return;
    };
    let _ = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .and_then(|mut f| {
            use std::io::Write;
            writeln!(f, "{}", line)
        });
}

pub fn read_audit_log(directory: &str) -> Vec<AuditEntry> {
    let log_path = Path::new(directory).join(LOG_DIR).join(LOG_FILE);
    let Ok(content) = fs::read_to_string(&log_path) else {
        return Vec::new();
    };

    content
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str::<AuditEntry>(l).ok())
        .collect()
}

pub fn get_audit_summary(directory: &str) -> AuditSummary {
    let entries = read_audit_log(directory);
    let mut by_extension: HashMap<String, u64> = HashMap::new();

    for e in &entries {
        if e.decision == AuditDecision::Warned {
            let ext = Path::new(&e.file_path)
                .extension()
                .and_then(|s| s.to_str())
                .map(|s| format!(".{}", s))
                .unwrap_or_else(|| "unknown".to_string());
            *by_extension.entry(ext).or_insert(0) += 1;
        }
    }

    let allowed = entries
        .iter()
        .filter(|e| e.decision == AuditDecision::Allowed)
        .count() as u64;
    let warned = entries
        .iter()
        .filter(|e| e.decision == AuditDecision::Warned)
        .count() as u64;

    AuditSummary {
        total: entries.len() as u64,
        allowed,
        warned,
        by_extension,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditSummary {
    pub total: u64,
    pub allowed: u64,
    pub warned: u64,
    pub by_extension: HashMap<String, u64>,
}

// =============================================================================
// Core helpers
// =============================================================================

pub fn is_allowed_path(file_path: &str) -> bool {
    if file_path.is_empty() {
        return true;
    }
    ALLOWED_PATH_PATTERNS.iter().any(|p| p.is_match(file_path))
}

pub fn is_source_file(file_path: &str) -> bool {
    if file_path.is_empty() {
        return false;
    }
    let ext = Path::new(file_path)
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_lowercase();
    if ext.is_empty() {
        return false;
    }
    WARNED_EXTENSIONS.iter().any(|e| *e == format!(".{}", ext))
}

pub fn is_write_edit_tool(tool_name: &str) -> bool {
    WRITE_EDIT_TOOLS.contains(&tool_name)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitFileStatus {
    Modified,
    Added,
    Deleted,
}

#[derive(Debug, Clone)]
pub struct GitFileStat {
    pub path: String,
    pub added: u32,
    pub removed: u32,
    pub status: GitFileStatus,
}

pub fn get_git_diff_stats(directory: &str) -> Vec<GitFileStat> {
    let diff_output = Command::new("git")
        .args(["diff", "--numstat", "HEAD"])
        .current_dir(directory)
        .output();

    let Ok(diff_output) = diff_output else {
        return Vec::new();
    };
    if !diff_output.status.success() {
        return Vec::new();
    }
    let diff = String::from_utf8_lossy(&diff_output.stdout)
        .trim()
        .to_string();
    if diff.is_empty() {
        return Vec::new();
    }

    let status_output = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(directory)
        .output();
    let status_map = parse_git_status_map(
        status_output
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
            .unwrap_or_default()
            .as_str(),
    );

    let mut stats: Vec<GitFileStat> = Vec::new();
    for line in diff.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() < 3 {
            continue;
        }

        let added = if parts[0] == "-" {
            0
        } else {
            parts[0].parse::<u32>().unwrap_or(0)
        };
        let removed = if parts[1] == "-" {
            0
        } else {
            parts[1].parse::<u32>().unwrap_or(0)
        };
        let path = parts[2].to_string();

        let status = status_map
            .get(&path)
            .cloned()
            .unwrap_or(GitFileStatus::Modified);

        stats.push(GitFileStat {
            path,
            added,
            removed,
            status,
        });
    }

    stats
}

fn parse_git_status_map(status_output: &str) -> HashMap<String, GitFileStatus> {
    let mut map: HashMap<String, GitFileStatus> = HashMap::new();
    for line in status_output.lines() {
        if line.is_empty() {
            continue;
        }
        if line.len() < 4 {
            continue;
        }
        let status = line[..2].trim();
        let file_path = line[3..].to_string();

        let mapped = if status == "A" || status == "??" {
            GitFileStatus::Added
        } else if status == "D" {
            GitFileStatus::Deleted
        } else {
            GitFileStatus::Modified
        };

        map.insert(file_path, mapped);
    }
    map
}

pub fn format_file_changes(stats: &[GitFileStat]) -> String {
    if stats.is_empty() {
        return "[FILE CHANGES SUMMARY]\nNo file changes detected.\n".to_string();
    }

    let modified: Vec<_> = stats
        .iter()
        .filter(|s| s.status == GitFileStatus::Modified)
        .collect();
    let added: Vec<_> = stats
        .iter()
        .filter(|s| s.status == GitFileStatus::Added)
        .collect();
    let deleted: Vec<_> = stats
        .iter()
        .filter(|s| s.status == GitFileStatus::Deleted)
        .collect();

    let mut lines: Vec<String> = vec!["[FILE CHANGES SUMMARY]".to_string()];

    if !modified.is_empty() {
        lines.push("Modified files:".to_string());
        for f in modified {
            lines.push(format!("  {}  (+{}, -{})", f.path, f.added, f.removed));
        }
        lines.push(String::new());
    }

    if !added.is_empty() {
        lines.push("Created files:".to_string());
        for f in added {
            lines.push(format!("  {}  (+{})", f.path, f.added));
        }
        lines.push(String::new());
    }

    if !deleted.is_empty() {
        lines.push("Deleted files:".to_string());
        for f in deleted {
            lines.push(format!("  {}  (-{})", f.path, f.removed));
        }
        lines.push(String::new());
    }

    lines.join("\n")
}

pub fn build_verification_reminder(session_id: Option<&str>) -> String {
    let mut reminder = VERIFICATION_REMINDER.to_string();
    if let Some(sid) = session_id {
        reminder.push_str(
            "\n\n---\n\n**If ANY verification fails, resume the subagent with the fix:**\nTask tool with resume=\"",
        );
        reminder.push_str(sid);
        reminder.push_str("\", prompt=\"fix: [describe the specific failure]\"");
    }
    reminder
}

pub fn build_boulder_continuation(plan_name: &str, remaining: u32, total: u32) -> String {
    let base = BOULDER_CONTINUATION_PROMPT.replace("{PLAN_NAME}", plan_name);
    format!(
        "{}\n\n[Status: {}/{} completed, {} remaining]",
        base,
        total - remaining,
        total,
        remaining
    )
}

fn extract_file_path(tool_input: &serde_json::Value) -> Option<String> {
    let obj = tool_input.as_object()?;
    let keys = ["filePath", "path", "file"]; // TS order
    for k in keys {
        if let Some(v) = obj.get(k).and_then(|v| v.as_str()) {
            return Some(v.to_string());
        }
    }
    None
}

fn process_remember_tags(output: &str, directory: &str) {
    for cap in REMEMBER_PRIORITY_RE.captures_iter(output) {
        let content = cap.get(1).map(|m| m.as_str().trim()).unwrap_or("");
        if !content.is_empty() {
            let _ = NotepadHook::set_priority_context(directory, content, None);
        }
    }

    for cap in REMEMBER_RE.captures_iter(output) {
        let content = cap.get(1).map(|m| m.as_str().trim()).unwrap_or("");
        if !content.is_empty() {
            let _ = NotepadHook::add_working_memory_entry(directory, content);
        }
    }
}

// =============================================================================
// TS-like pre/post tool processors (useful as utilities)
// =============================================================================

#[derive(Debug, Clone)]
pub struct ToolExecuteInput {
    pub tool_name: String,
    pub tool_input: Option<serde_json::Value>,
    pub session_id: Option<String>,
    pub directory: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ToolExecuteOutput {
    pub r#continue: bool,
    pub message: Option<String>,
    pub modified_output: Option<String>,
}

pub fn process_orchestrator_pre_tool(input: &ToolExecuteInput) -> ToolExecuteOutput {
    if !is_write_edit_tool(&input.tool_name) {
        return ToolExecuteOutput {
            r#continue: true,
            message: None,
            modified_output: None,
        };
    }

    let file_path = input.tool_input.as_ref().and_then(extract_file_path);

    if file_path.as_deref().map(is_allowed_path).unwrap_or(true) {
        if let (Some(dir), Some(fp)) = (input.directory.as_deref(), file_path.as_deref()) {
            log_audit_entry(
                dir,
                AuditEntryInput {
                    tool: input.tool_name.clone(),
                    file_path: fp.to_string(),
                    decision: AuditDecision::Allowed,
                    reason: AuditReason::AllowedPath,
                    session_id: input.session_id.clone(),
                },
            );
        }

        return ToolExecuteOutput {
            r#continue: true,
            message: None,
            modified_output: None,
        };
    }

    // Warned
    if let (Some(dir), Some(fp)) = (input.directory.as_deref(), file_path.as_deref()) {
        let reason = if is_source_file(fp) {
            AuditReason::SourceFile
        } else {
            AuditReason::Other
        };
        log_audit_entry(
            dir,
            AuditEntryInput {
                tool: input.tool_name.clone(),
                file_path: fp.to_string(),
                decision: AuditDecision::Warned,
                reason,
                session_id: input.session_id.clone(),
            },
        );
    }

    let warning =
        ORCHESTRATOR_DELEGATION_REQUIRED.replace("$FILE_PATH", file_path.as_deref().unwrap_or(""));

    ToolExecuteOutput {
        r#continue: true,
        message: Some(warning),
        modified_output: None,
    }
}

pub fn process_orchestrator_post_tool(input: &ToolExecuteInput, output: &str) -> ToolExecuteOutput {
    let work_dir = input.directory.clone().unwrap_or_else(|| {
        std::env::current_dir()
            .unwrap()
            .to_string_lossy()
            .to_string()
    });

    // Write/edit tools
    if is_write_edit_tool(&input.tool_name) {
        let file_path = input.tool_input.as_ref().and_then(extract_file_path);

        if file_path
            .as_deref()
            .map(|fp| !is_allowed_path(fp))
            .unwrap_or(false)
        {
            return ToolExecuteOutput {
                r#continue: true,
                message: None,
                modified_output: Some(format!("{}{}", output, DIRECT_WORK_REMINDER)),
            };
        }
    }

    // Task tool completion
    if input.tool_name == "Task" || input.tool_name == "task" {
        if output.contains("Background task launched") || output.contains("Background task resumed")
        {
            return ToolExecuteOutput {
                r#continue: true,
                message: None,
                modified_output: None,
            };
        }

        process_remember_tags(output, &work_dir);

        let file_changes = format_file_changes(&get_git_diff_stats(&work_dir));
        let reminder = build_verification_reminder(input.session_id.as_deref());

        let enhanced_output = format!(
            "## SUBAGENT WORK COMPLETED\n\n{}\n<system-reminder>\n{}\n</system-reminder>",
            file_changes, reminder
        );

        return ToolExecuteOutput {
            r#continue: true,
            message: None,
            modified_output: Some(enhanced_output),
        };
    }

    ToolExecuteOutput {
        r#continue: true,
        message: None,
        modified_output: None,
    }
}

/// Stubbed: this crate currently has no boulder/astrape plan state.
pub fn check_boulder_continuation(_directory: &str) -> (bool, Option<String>) {
    (false, None)
}

// =============================================================================
// Hook implementation
// =============================================================================

pub struct OmcOrchestratorHook;

impl OmcOrchestratorHook {
    pub fn new() -> Self {
        Self
    }
}

impl Default for OmcOrchestratorHook {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Hook for OmcOrchestratorHook {
    fn name(&self) -> &str {
        HOOK_NAME
    }

    fn events(&self) -> &[HookEvent] {
        &[
            HookEvent::PreToolUse,
            HookEvent::PostToolUse,
            HookEvent::SessionIdle,
        ]
    }

    async fn execute(
        &self,
        event: HookEvent,
        input: &HookInput,
        context: &HookContext,
    ) -> HookResult {
        match event {
            HookEvent::PreToolUse => {
                let Some(tool_name) = input.tool_name.clone() else {
                    return Ok(HookOutput::pass());
                };

                let tool_input = input.tool_input.clone();
                let out = process_orchestrator_pre_tool(&ToolExecuteInput {
                    tool_name,
                    tool_input,
                    session_id: input.session_id.clone(),
                    directory: Some(context.directory.clone()),
                });

                if let Some(msg) = out.message {
                    Ok(HookOutput::continue_with_message(msg))
                } else {
                    Ok(HookOutput::pass())
                }
            }
            HookEvent::PostToolUse => {
                let Some(tool_name) = input.tool_name.clone() else {
                    return Ok(HookOutput::pass());
                };

                let tool_input = input.tool_input.clone();
                let tool_output_str = input
                    .tool_output
                    .as_ref()
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                let out = process_orchestrator_post_tool(
                    &ToolExecuteInput {
                        tool_name,
                        tool_input,
                        session_id: input.session_id.clone(),
                        directory: Some(context.directory.clone()),
                    },
                    tool_output_str,
                );

                if let Some(modified) = out.modified_output {
                    Ok(HookOutput::continue_with_message(modified))
                } else {
                    Ok(HookOutput::pass())
                }
            }
            HookEvent::SessionIdle => {
                let (should_continue, message) = check_boulder_continuation(&context.directory);
                if should_continue {
                    Ok(HookOutput::continue_with_message(
                        message.unwrap_or_default(),
                    ))
                } else {
                    Ok(HookOutput::pass())
                }
            }
            _ => Ok(HookOutput::pass()),
        }
    }

    fn priority(&self) -> i32 {
        90
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_is_allowed_path() {
        assert!(is_allowed_path(".omc/foo.txt"));
        assert!(is_allowed_path(".claude/rules/a.md"));
        assert!(is_allowed_path("/Users/me/.claude/rules/a.md"));
        assert!(is_allowed_path("CLAUDE.md"));

        assert!(!is_allowed_path("src/main.rs"));
        assert!(!is_allowed_path("README.md"));
    }

    #[test]
    fn test_is_source_file() {
        assert!(is_source_file("src/main.rs"));
        assert!(is_source_file("app.tsx"));
        assert!(!is_source_file("README.md"));
        assert!(!is_source_file(""));
    }

    #[test]
    fn test_audit_log_roundtrip() {
        let dir = tempdir().unwrap();
        log_audit_entry(
            dir.path().to_str().unwrap(),
            AuditEntryInput {
                tool: "Edit".to_string(),
                file_path: "src/main.rs".to_string(),
                decision: AuditDecision::Warned,
                reason: AuditReason::SourceFile,
                session_id: Some("s".to_string()),
            },
        );

        let entries = read_audit_log(dir.path().to_str().unwrap());
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].tool, "Edit");
        assert_eq!(entries[0].decision, AuditDecision::Warned);
    }

    #[test]
    fn test_process_remember_tags_writes_notepad() {
        let dir = tempdir().unwrap();
        let path = dir.path().to_str().unwrap();
        // Ensure notepad exists
        assert!(NotepadHook::init_notepad(path));

        let output = "<remember priority>Critical</remember>\n<remember>Working</remember>";
        process_remember_tags(output, path);

        let priority = NotepadHook::get_priority_context(path).unwrap();
        assert!(priority.contains("Critical"));

        let working = NotepadHook::get_working_memory(path).unwrap();
        assert!(working.contains("Working"));
    }
}
