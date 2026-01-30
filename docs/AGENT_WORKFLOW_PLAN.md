# AgentWorkflow Implementation Plan

> **Status**: VALIDATED ✅
> **Last Updated**: 2026-01-30
> **Validation**: Oracle-reviewed, codebase-verified

## Overview

Replace HTTP-based `AiDecisionClient` with an embedded agent session that uses the same harness as `uira-agent`. The agent runs autonomously with full tool access until it outputs `<DONE/>` and verification passes.

## Architecture

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│                              Current Architecture                                │
│                                                                                  │
│  uira typos --ai                                                                │
│       │                                                                          │
│       ▼                                                                          │
│  AiDecisionClient ──HTTP──▶ OpenCode Server ──▶ Model Provider                  │
│       │                              │                                           │
│       │◀────── Text Response ────────┘                                          │
│       │                                                                          │
│       ▼                                                                          │
│  parse_decisions() ──▶ Apply fixes locally                                      │
│                                                                                  │
└─────────────────────────────────────────────────────────────────────────────────┘

                                    ▼▼▼

┌─────────────────────────────────────────────────────────────────────────────────┐
│                              New Architecture                                    │
│                                                                                  │
│  uira typos --ai                                                                │
│       │                                                                          │
│       ▼                                                                          │
│  ┌─────────────────────────────────────────────────────────────────────────┐    │
│  │                        AgentWorkflow                                     │    │
│  │                                                                          │    │
│  │  1. Shared Tokio Runtime (OnceLock<Runtime>)                            │    │
│  │  2. ModelClientBuilder → Arc<dyn ModelClient>                           │    │
│  │  3. Agent with:                                                          │    │
│  │     • Built-in tools: Read, Edit, Grep, Glob, Write, Bash               │    │
│  │     • LSP provider: lsp_diagnostics, lsp_goto_definition, etc.          │    │
│  │     • full_auto = true (no approvals in hooks)                          │    │
│  │  4. Completion loop:                                                     │    │
│  │     • Run until <DONE/>                                                 │    │
│  │     • Verify via re-detection (no remaining issues)                     │    │
│  │     • Stage via git diff (before/after comparison)                      │    │
│  │                                                                          │    │
│  │  State: .uira/workflow/{task}-session.json                              │    │
│  │  Rollout: .uira/sessions/{session-id}.jsonl                             │    │
│  │                                                                          │    │
│  └─────────────────────────────────────────────────────────────────────────┘    │
│                                                                                  │
└─────────────────────────────────────────────────────────────────────────────────┘
```

## Validation Summary

| Aspect | Original Assumption | Validated Reality | Status |
|--------|---------------------|-------------------|--------|
| Agent exports | `Agent`, `AgentConfig`, `AgentLoopError` | ✅ All exported from `uira-agent` | ✅ |
| Provider creation | `create_provider()` | ❌ Use `ModelClientBuilder::new().build()` | 🔧 Fixed |
| ExecutionResult | `text`, `tool_results` fields | ❌ Has `output`, `turns`, `usage`, `error` | 🔧 Fixed |
| Async runtime | Async CLI | ❌ Sync CLI, use shared `OnceLock<Runtime>` | 🔧 Fixed |
| Built-in tools | LSP included | ❌ LSP is separate `LspToolProvider` | 🔧 Fixed |
| Git operations | `GitAdd` tool | ❌ Use `Bash` for git commands | 🔧 Fixed |
| SandboxPolicy | In `uira-protocol` | ✅ In `uira-sandbox` with correct variants | ✅ |
| with_rollout/resume | Methods exist | ✅ Both methods available | ✅ |

## File Structure

```
crates/uira/
├── src/
│   ├── main.rs                    # CLI entry point (sync, shared runtime)
│   ├── runtime.rs                 # NEW: Shared Tokio runtime
│   ├── agent_workflow/            # NEW: Agent workflow module
│   │   ├── mod.rs                 # Module exports
│   │   ├── workflow.rs            # AgentWorkflow implementation
│   │   ├── state.rs               # WorkflowState persistence
│   │   ├── completion.rs          # DONE flag detection
│   │   ├── verification.rs        # NEW: Post-completion verification
│   │   ├── git_tracker.rs         # NEW: Git diff-based tracking
│   │   ├── prompts.rs             # System prompts for each task
│   │   └── config.rs              # WorkflowConfig
│   ├── typos/
│   │   └── mod.rs                 # Updated to use AgentWorkflow
│   ├── diagnostics/
│   │   └── mod.rs                 # Updated to use AgentWorkflow
│   ├── comments/
│   │   └── mod.rs                 # Updated to use AgentWorkflow
│   └── ai_decision.rs             # DEPRECATED (remove later)
└── Cargo.toml                     # Add dependencies
```

---

## Phase 1: Core Infrastructure

### 1.1 Add Dependencies

```toml
# crates/uira/Cargo.toml
[dependencies]
uira-agent = { workspace = true }       # NEW
uira-providers = { workspace = true }   # NEW
uira-protocol = { workspace = true }    # NEW
uira-tools = { workspace = true }       # NEW (for LspToolProvider)
uira-sandbox = { workspace = true }     # NEW (for SandboxPolicy)
```

### 1.2 Shared Tokio Runtime

```rust
// crates/uira/src/runtime.rs

use std::sync::OnceLock;
use tokio::runtime::Runtime;

/// Shared Tokio runtime for agent workflows.
/// Uses current-thread runtime to avoid nested runtime issues.
static RUNTIME: OnceLock<Runtime> = OnceLock::new();

/// Get the shared Tokio runtime.
/// Creates it on first access (lazy initialization).
pub fn get_runtime() -> &'static Runtime {
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Failed to create Tokio runtime")
    })
}

/// Run an async function on the shared runtime.
pub fn block_on<F: std::future::Future>(f: F) -> F::Output {
    get_runtime().block_on(f)
}
```

### 1.3 WorkflowTask Enum

```rust
// crates/uira/src/agent_workflow/mod.rs

use serde::{Deserialize, Serialize};

/// The type of workflow being executed
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkflowTask {
    /// Fix typos in code
    Typos,
    /// Fix LSP diagnostics (errors, warnings)
    Diagnostics,
    /// Review and clean up comments
    Comments,
}

impl WorkflowTask {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Typos => "typos",
            Self::Diagnostics => "diagnostics",
            Self::Comments => "comments",
        }
    }
    
    pub fn state_file(&self) -> String {
        format!(".uira/workflow/{}-session.json", self.name())
    }
    
    pub fn rollout_dir(&self) -> String {
        ".uira/workflow/rollouts".to_string()
    }
}
```

### 1.4 WorkflowConfig

```rust
// crates/uira/src/agent_workflow/config.rs

use std::path::PathBuf;
use uira_sandbox::SandboxPolicy;

/// Configuration for agent workflow
#[derive(Debug, Clone)]
pub struct WorkflowConfig {
    /// Model to use (e.g., "claude-sonnet-4-20250514")
    pub model: String,
    
    /// Provider (e.g., "anthropic", "openai")
    pub provider: String,
    
    /// Maximum iterations before giving up
    pub max_iterations: u32,
    
    /// Working directory
    pub working_directory: PathBuf,
    
    /// Sandbox policy (from uira-sandbox)
    pub sandbox_policy: SandboxPolicy,
    
    /// Auto-stage modified files
    pub auto_stage: bool,
    
    /// Only process staged files
    pub staged_only: bool,
    
    /// Files to process (empty = all)
    pub files: Vec<String>,
    
    /// Enable LSP tools (lsp_diagnostics, etc.)
    pub enable_lsp_tools: bool,
    
    /// Task-specific options
    pub task_options: TaskOptions,
}

#[derive(Debug, Clone, Default)]
pub struct TaskOptions {
    /// For diagnostics: severity filter
    pub severity: Option<String>,
    
    /// For diagnostics: language filter  
    pub languages: Vec<String>,
    
    /// For comments: pragma format
    pub pragma_format: Option<String>,
    
    /// For comments: include docstrings
    pub include_docstrings: bool,
}

impl Default for WorkflowConfig {
    fn default() -> Self {
        Self {
            model: "claude-sonnet-4-20250514".to_string(),
            provider: "anthropic".to_string(),
            max_iterations: 10,
            working_directory: std::env::current_dir().unwrap_or_default(),
            sandbox_policy: SandboxPolicy::workspace_write(
                std::env::current_dir().unwrap_or_default()
            ),
            auto_stage: false,
            staged_only: false,
            files: vec![],
            enable_lsp_tools: true,  // Enable by default for diagnostics
            task_options: TaskOptions::default(),
        }
    }
}
```

### 1.5 WorkflowState (Persistence)

```rust
// crates/uira/src/agent_workflow/state.rs

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::Path;

use super::WorkflowTask;

/// Persisted workflow state for resume capability
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowState {
    /// Whether workflow is currently active
    pub active: bool,
    
    /// Task type
    pub task: WorkflowTask,
    
    /// Current iteration count
    pub iteration: u32,
    
    /// Maximum iterations
    pub max_iterations: u32,
    
    /// Agent session ID (for rollout correlation)
    pub session_id: String,
    
    /// When workflow started
    pub started_at: DateTime<Utc>,
    
    /// Last activity timestamp
    pub last_activity_at: DateTime<Utc>,
    
    /// Files changed (tracked via git diff)
    pub files_changed: Vec<String>,
    
    /// Agent rollout path (for resume)
    pub rollout_path: Option<String>,
    
    /// Git state before workflow (for staging)
    pub git_state_before: Option<Vec<String>>,
}

impl WorkflowState {
    pub fn new(task: WorkflowTask, session_id: String, max_iterations: u32) -> Self {
        let now = Utc::now();
        Self {
            active: true,
            task,
            iteration: 0,
            max_iterations,
            session_id,
            started_at: now,
            last_activity_at: now,
            files_changed: vec![],
            rollout_path: None,
            git_state_before: None,
        }
    }
    
    /// Read state from disk
    pub fn read(task: WorkflowTask) -> Option<Self> {
        let path = task.state_file();
        let content = std::fs::read_to_string(&path).ok()?;
        serde_json::from_str(&content).ok()
    }
    
    /// Write state to disk
    pub fn write(&self) -> anyhow::Result<()> {
        let path = self.task.state_file();
        
        // Ensure directory exists
        if let Some(parent) = Path::new(&path).parent() {
            std::fs::create_dir_all(parent)?;
        }
        
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, content)?;
        Ok(())
    }
    
    /// Clear state (on completion or cancellation)
    pub fn clear(task: WorkflowTask) -> anyhow::Result<()> {
        let path = task.state_file();
        if Path::new(&path).exists() {
            std::fs::remove_file(&path)?;
        }
        Ok(())
    }
    
    /// Increment iteration and update timestamp
    pub fn increment(&mut self) {
        self.iteration += 1;
        self.last_activity_at = Utc::now();
    }
}
```

### 1.6 Completion Detection

```rust
// crates/uira/src/agent_workflow/completion.rs

use regex::Regex;

/// Detects <DONE/> completion flag in agent output
pub struct CompletionDetector {
    pattern: Regex,
    summary_pattern: Regex,
}

impl CompletionDetector {
    pub fn new() -> Self {
        // Matches: <DONE/>, <DONE />, <DONE>...</DONE>
        let pattern = Regex::new(r"<DONE\s*(?:/\s*>|>.*?</DONE>)").unwrap();
        let summary_pattern = Regex::new(r"<DONE>(.*?)</DONE>").unwrap();
        Self { pattern, summary_pattern }
    }
    
    /// Check if the text contains a DONE flag
    pub fn is_done(&self, text: &str) -> bool {
        self.pattern.is_match(text)
    }
    
    /// Extract summary from <DONE>summary</DONE> if present
    pub fn extract_summary(&self, text: &str) -> Option<String> {
        self.summary_pattern.captures(text)
            .and_then(|cap| cap.get(1))
            .map(|m| m.as_str().trim().to_string())
            .filter(|s| !s.is_empty())
    }
}

impl Default for CompletionDetector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_done_detection() {
        let detector = CompletionDetector::new();
        
        assert!(detector.is_done("All fixes applied. <DONE/>"));
        assert!(detector.is_done("Complete! <DONE />"));
        assert!(detector.is_done("<DONE>Fixed 3 typos</DONE>"));
        
        assert!(!detector.is_done("Working on fixes..."));
        assert!(!detector.is_done("DONE but not tagged"));
    }
    
    #[test]
    fn test_summary_extraction() {
        let detector = CompletionDetector::new();
        
        assert_eq!(
            detector.extract_summary("<DONE>Fixed 3 typos</DONE>"),
            Some("Fixed 3 typos".to_string())
        );
        assert_eq!(detector.extract_summary("<DONE/>"), None);
        assert_eq!(detector.extract_summary("<DONE></DONE>"), None);
    }
}
```

### 1.7 Git-based Modification Tracking

```rust
// crates/uira/src/agent_workflow/git_tracker.rs

use std::collections::HashSet;
use std::path::Path;
use std::process::Command;

/// Tracks file modifications using git diff
pub struct GitTracker {
    working_dir: std::path::PathBuf,
    baseline: HashSet<String>,
}

impl GitTracker {
    /// Create a new tracker and capture baseline state
    pub fn new(working_dir: impl AsRef<Path>) -> Self {
        let working_dir = working_dir.as_ref().to_path_buf();
        let baseline = Self::get_changed_files(&working_dir);
        Self { working_dir, baseline }
    }
    
    /// Get files that changed since baseline was captured
    pub fn get_modifications(&self) -> Vec<String> {
        let current = Self::get_changed_files(&self.working_dir);
        current.difference(&self.baseline)
            .cloned()
            .collect()
    }
    
    /// Get all currently changed files (staged + unstaged)
    fn get_changed_files(working_dir: &Path) -> HashSet<String> {
        let mut files = HashSet::new();
        
        // Get unstaged changes
        if let Ok(output) = Command::new("git")
            .args(["diff", "--name-only"])
            .current_dir(working_dir)
            .output()
        {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                for line in stdout.lines() {
                    if !line.is_empty() {
                        files.insert(line.to_string());
                    }
                }
            }
        }
        
        // Get staged changes
        if let Ok(output) = Command::new("git")
            .args(["diff", "--cached", "--name-only"])
            .current_dir(working_dir)
            .output()
        {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                for line in stdout.lines() {
                    if !line.is_empty() {
                        files.insert(line.to_string());
                    }
                }
            }
        }
        
        files
    }
    
    /// Stage the specified files
    pub fn stage_files(&self, files: &[String]) -> anyhow::Result<()> {
        if files.is_empty() {
            return Ok(());
        }
        
        Command::new("git")
            .arg("add")
            .arg("--")
            .args(files)
            .current_dir(&self.working_dir)
            .status()?;
        
        Ok(())
    }
}
```

### 1.8 Verification Module

```rust
// crates/uira/src/agent_workflow/verification.rs

use super::WorkflowTask;
use anyhow::Result;

/// Verifies that a workflow actually completed successfully
pub struct WorkflowVerifier;

impl WorkflowVerifier {
    /// Verify no remaining issues for the given task
    pub fn verify(task: WorkflowTask, working_dir: &std::path::Path) -> Result<VerificationResult> {
        match task {
            WorkflowTask::Typos => Self::verify_typos(working_dir),
            WorkflowTask::Diagnostics => Self::verify_diagnostics(working_dir),
            WorkflowTask::Comments => Self::verify_comments(working_dir),
        }
    }
    
    fn verify_typos(working_dir: &std::path::Path) -> Result<VerificationResult> {
        // Run typos CLI to check for remaining issues
        let output = std::process::Command::new("typos")
            .arg("--format=brief")
            .current_dir(working_dir)
            .output()?;
        
        if output.status.success() {
            Ok(VerificationResult::Pass)
        } else {
            let remaining = String::from_utf8_lossy(&output.stdout);
            let count = remaining.lines().count();
            Ok(VerificationResult::Fail {
                remaining_issues: count,
                details: remaining.to_string(),
            })
        }
    }
    
    fn verify_diagnostics(working_dir: &std::path::Path) -> Result<VerificationResult> {
        // For now, trust the agent's verification
        // TODO: Re-run lsp_diagnostics and check for errors
        Ok(VerificationResult::Pass)
    }
    
    fn verify_comments(working_dir: &std::path::Path) -> Result<VerificationResult> {
        // Comments don't have an objective "done" state
        // Trust the agent's judgment
        Ok(VerificationResult::Pass)
    }
}

#[derive(Debug)]
pub enum VerificationResult {
    Pass,
    Fail {
        remaining_issues: usize,
        details: String,
    },
}

impl VerificationResult {
    pub fn is_pass(&self) -> bool {
        matches!(self, Self::Pass)
    }
}
```

---

## Phase 2: AgentWorkflow Implementation

### 2.1 Main Workflow Struct

```rust
// crates/uira/src/agent_workflow/workflow.rs

use std::sync::Arc;
use anyhow::Result;
use uira_agent::{Agent, AgentConfig, AgentLoopError};
use uira_providers::ModelClientBuilder;
use uira_protocol::ExecutionResult;
use uira_tools::{create_builtin_router, LspToolProvider};

use super::{
    CompletionDetector, GitTracker, WorkflowConfig, WorkflowState, WorkflowTask,
    WorkflowVerifier, VerificationResult,
    prompts::build_system_prompt,
};

/// Result of running a workflow
#[derive(Debug)]
pub enum WorkflowResult {
    /// Workflow completed successfully
    Complete {
        iterations: u32,
        files_modified: Vec<String>,
        summary: Option<String>,
    },
    /// Max iterations reached without completion
    MaxIterationsReached {
        iterations: u32,
        files_modified: Vec<String>,
    },
    /// Verification failed after agent said DONE
    VerificationFailed {
        iterations: u32,
        remaining_issues: usize,
        details: String,
    },
    /// Workflow was cancelled
    Cancelled,
    /// Workflow failed with error
    Failed { error: String },
}

/// Agent-based workflow for AI-assisted code quality tasks
pub struct AgentWorkflow {
    task: WorkflowTask,
    config: WorkflowConfig,
    agent: Agent,
    state: WorkflowState,
    completion_detector: CompletionDetector,
    git_tracker: GitTracker,
}

impl AgentWorkflow {
    /// Create a new agent workflow
    pub async fn new(task: WorkflowTask, config: WorkflowConfig) -> Result<Self> {
        // Check for existing state (resume capability)
        let existing_state = WorkflowState::read(task);
        
        // Create model client using builder pattern
        let client = ModelClientBuilder::new()
            .model(&config.model)
            .provider(&config.provider)
            .build()?;
        
        // Create tool router with builtins + optional LSP
        let mut router = create_builtin_router();
        if config.enable_lsp_tools {
            router.register_provider(Arc::new(LspToolProvider::new()));
        }
        
        // Build agent config
        let agent_config = AgentConfig {
            system_prompt: Some(build_system_prompt(task, &config.task_options)),
            working_directory: Some(config.working_directory.clone()),
            sandbox_policy: config.sandbox_policy.clone(),
            require_approval_for_writes: false,  // Full auto for hooks
            require_approval_for_commands: false,
            max_turns: config.max_iterations * 10,  // Generous turn limit
            ..Default::default()
        };
        
        // Create or resume agent
        let agent = if let Some(ref state) = existing_state {
            if let Some(ref rollout_path) = state.rollout_path {
                // Resume from rollout
                Agent::resume_from_rollout(
                    agent_config,
                    client,
                    rollout_path.into(),
                )?
            } else {
                Agent::new(agent_config, client)
            }
        } else {
            Agent::new(agent_config, client)
        };
        
        // Enable rollout for persistence
        let agent = agent.with_rollout()?;
        
        // Initialize git tracker
        let git_tracker = GitTracker::new(&config.working_directory);
        
        // Initialize or restore state
        let state = existing_state.unwrap_or_else(|| {
            WorkflowState::new(
                task,
                agent.session().id.to_string(),
                config.max_iterations,
            )
        });
        
        Ok(Self {
            task,
            config,
            agent,
            state,
            completion_detector: CompletionDetector::new(),
            git_tracker,
        })
    }
    
    /// Run the workflow until completion or max iterations
    pub async fn run(&mut self) -> Result<WorkflowResult> {
        // Build initial prompt
        let initial_prompt = self.build_initial_prompt();
        
        // Save rollout path to state for resume
        if let Some(path) = self.agent.rollout_path() {
            self.state.rollout_path = Some(path.to_string_lossy().to_string());
        }
        self.state.write()?;
        
        // Main workflow loop
        loop {
            // Check iteration limit
            if self.state.iteration >= self.state.max_iterations {
                let files_modified = self.git_tracker.get_modifications();
                let result = WorkflowResult::MaxIterationsReached {
                    iterations: self.state.iteration,
                    files_modified,
                };
                WorkflowState::clear(self.task)?;
                return Ok(result);
            }
            
            // Run one iteration
            let prompt = if self.state.iteration == 0 {
                initial_prompt.clone()
            } else {
                self.build_continuation_prompt()
            };
            
            match self.agent.run(&prompt).await {
                Ok(exec_result) => {
                    // Extract text from result (use `output` field)
                    let response_text = &exec_result.output;
                    
                    // Check for completion
                    if self.completion_detector.is_done(response_text) {
                        // Verify completion is real
                        let verification = WorkflowVerifier::verify(
                            self.task,
                            &self.config.working_directory,
                        )?;
                        
                        match verification {
                            VerificationResult::Pass => {
                                let summary = self.completion_detector.extract_summary(response_text);
                                let files_modified = self.git_tracker.get_modifications();
                                
                                // Stage files if requested
                                if self.config.auto_stage && !files_modified.is_empty() {
                                    self.git_tracker.stage_files(&files_modified)?;
                                }
                                
                                let result = WorkflowResult::Complete {
                                    iterations: self.state.iteration + 1,
                                    files_modified,
                                    summary,
                                };
                                
                                WorkflowState::clear(self.task)?;
                                return Ok(result);
                            }
                            VerificationResult::Fail { remaining_issues, details } => {
                                // Agent said DONE but issues remain
                                // Continue with feedback
                                self.state.increment();
                                self.state.write()?;
                                
                                // If we've tried verification too many times, give up
                                if self.state.iteration >= self.state.max_iterations {
                                    let files_modified = self.git_tracker.get_modifications();
                                    let result = WorkflowResult::VerificationFailed {
                                        iterations: self.state.iteration,
                                        remaining_issues,
                                        details,
                                    };
                                    WorkflowState::clear(self.task)?;
                                    return Ok(result);
                                }
                                
                                // Continue with verification failure feedback
                                // (next iteration will use build_continuation_prompt with failure info)
                            }
                        }
                    } else {
                        // No DONE flag, continue
                        self.state.increment();
                        self.state.write()?;
                    }
                }
                Err(AgentLoopError::Cancelled) => {
                    return Ok(WorkflowResult::Cancelled);
                }
                Err(e) => {
                    WorkflowState::clear(self.task)?;
                    return Ok(WorkflowResult::Failed {
                        error: e.to_string(),
                    });
                }
            }
        }
    }
    
    fn build_initial_prompt(&self) -> String {
        let files_context = if self.config.files.is_empty() {
            if self.config.staged_only {
                "Process only staged files (use `git diff --cached --name-only`)."
            } else {
                "Process all relevant files in the repository."
            }
        } else {
            "Process the specified files."
        };
        
        format!(
            "Begin the {task} workflow.\n\n\
            {files_context}\n\n\
            Files to process: {files}\n\n\
            Remember: Output <DONE/> when all issues are fixed.",
            task = self.task.name(),
            files_context = files_context,
            files = if self.config.files.is_empty() {
                "(auto-detect)".to_string()
            } else {
                self.config.files.join(", ")
            },
        )
    }
    
    fn build_continuation_prompt(&self) -> String {
        format!(
            "Continue the {task} workflow.\n\n\
            Iteration: {iter}/{max}\n\
            Files modified so far: {files}\n\n\
            Continue fixing issues. Output <DONE/> when complete.",
            task = self.task.name(),
            iter = self.state.iteration + 1,
            max = self.state.max_iterations,
            files = self.git_tracker.get_modifications().len(),
        )
    }
}
```

### 2.2 System Prompts

```rust
// crates/uira/src/agent_workflow/prompts.rs

use super::{WorkflowTask, TaskOptions};

/// Build the system prompt for a workflow task
pub fn build_system_prompt(task: WorkflowTask, options: &TaskOptions) -> String {
    let base = format!(r#"You are a code quality agent specialized in {task_name}.

## Your Mission
{mission}

## Available Tools
You have access to these tools:

### File Operations
- **Read**: Read file contents
- **Edit**: Modify files (use oldString/newString for precise edits)
- **Write**: Create or overwrite files
- **Glob**: Find files matching patterns
- **Grep**: Search for patterns in the codebase

### Shell Commands
- **Bash**: Run shell commands (git, tests, build commands)

### Code Analysis (if enabled)
- **lsp_diagnostics**: Get TypeScript/JavaScript errors and warnings
- **lsp_goto_definition**: Jump to symbol definitions
- **lsp_find_references**: Find all references to a symbol

## Workflow
1. First, understand the scope of the task
2. Use Grep/Glob to find relevant files
3. Use Read to examine file contents
4. Use Edit to apply fixes (be precise!)
5. Verify your changes work (run tests/lsp_diagnostics if applicable)
6. When ALL issues are fixed, output: `<DONE/>`

## Completion Protocol
When you have completed ALL fixes and verified they work:
- Output exactly: `<DONE/>`
- Or with summary: `<DONE>Fixed N issues</DONE>`

DO NOT output <DONE/> until you are certain all issues are resolved.
If you encounter an error you cannot fix, explain the issue instead.

## Important Rules
- Make minimal, targeted changes
- Preserve existing code style
- Do not introduce new issues while fixing old ones
- If unsure about a fix, skip it rather than break code
- Use Bash for git operations (e.g., `git add <file>`)
"#,
        task_name = task.name(),
        mission = task_mission(task, options),
    );
    
    base
}

fn task_mission(task: WorkflowTask, options: &TaskOptions) -> String {
    match task {
        WorkflowTask::Typos => r#"
Fix typos in the codebase. This includes:
- Misspelled words in comments
- Typos in string literals  
- Incorrect variable names (if clearly typos)

Use the `typos` CLI via Bash to detect typos, then fix them with Edit.
Example: `typos --format brief` to list typos.
"#.to_string(),

        WorkflowTask::Diagnostics => {
            let severity = options.severity.as_deref().unwrap_or("error");
            format!(r#"
Fix code diagnostics (errors and warnings). Focus on:
- Severity: {severity} (and above)
- Languages: {languages}

Use `lsp_diagnostics` to find issues, then fix them with Edit.
Common fixes:
- Add missing imports
- Fix type errors
- Remove unused variables
- Add missing return statements
"#,
                severity = severity,
                languages = if options.languages.is_empty() { 
                    "all".to_string() 
                } else { 
                    options.languages.join(", ") 
                },
            )
        },

        WorkflowTask::Comments => {
            let pragma = options.pragma_format.as_deref().unwrap_or("@uira-allow");
            format!(r#"
Review and clean up comments in the codebase. Remove:
- Outdated TODO comments that are already done
- Commented-out code blocks
- Obvious/redundant comments that don't add value
- Debug comments left in production code

Keep:
- Comments marked with `{pragma}`
- Meaningful documentation comments
- Complex logic explanations
- License headers
{docstring_note}
"#,
                pragma = pragma,
                docstring_note = if options.include_docstrings {
                    "- Review docstrings too"
                } else {
                    "- Skip docstrings (they are documentation)"
                },
            )
        },
    }
}
```

---

## Phase 3: Integration with Existing Checkers

### 3.1 Update main.rs with Shared Runtime

```rust
// crates/uira/src/main.rs

mod runtime;  // Add runtime module

use runtime::block_on;

fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Typos { ai, stage, files } => {
            if ai {
                // Use shared runtime for async workflow
                block_on(typos_command_ai(stage, &files))
            } else {
                typos_command(&files)
            }
        }
        Commands::Diagnostics { ai, staged, stage, severity, files } => {
            if ai {
                block_on(diagnostics_command_ai(staged, stage, severity.as_deref(), &files))
            } else {
                diagnostics_command(&files)
            }
        }
        Commands::Comments { ai, staged, stage, files } => {
            if ai {
                block_on(comments_command_ai(staged, stage, &files))
            } else {
                comments_command(&files)
            }
        }
        // ... other commands
    };
    
    if let Err(e) = result {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}

async fn typos_command_ai(stage: bool, files: &[String]) -> anyhow::Result<()> {
    use agent_workflow::{AgentWorkflow, WorkflowConfig, WorkflowTask, WorkflowResult};
    
    println!("🔍 Starting AI-assisted typos workflow...\n");
    
    let uira_config = uira_config::load_config(None).ok();
    let typos_settings = uira_config.map(|c| c.typos);
    
    let config = WorkflowConfig {
        model: typos_settings.as_ref()
            .map(|s| s.ai.model.clone())
            .unwrap_or_else(|| "claude-sonnet-4-20250514".to_string()),
        auto_stage: stage,
        files: files.to_vec(),
        enable_lsp_tools: false,  // Typos doesn't need LSP
        ..Default::default()
    };
    
    let mut workflow = AgentWorkflow::new(WorkflowTask::Typos, config).await?;
    
    match workflow.run().await? {
        WorkflowResult::Complete { iterations, files_modified, summary } => {
            println!("\n✅ Typos workflow complete!");
            println!("   Iterations: {}", iterations);
            println!("   Files modified: {}", files_modified.len());
            if let Some(s) = summary {
                println!("   Summary: {}", s);
            }
            Ok(())
        }
        WorkflowResult::MaxIterationsReached { iterations, files_modified } => {
            println!("\n⚠️  Max iterations ({}) reached", iterations);
            println!("   Files modified: {}", files_modified.len());
            std::process::exit(1);
        }
        WorkflowResult::VerificationFailed { remaining_issues, details, .. } => {
            println!("\n❌ Verification failed: {} issues remain", remaining_issues);
            println!("   Details: {}", details);
            std::process::exit(1);
        }
        WorkflowResult::Cancelled => {
            println!("\n⚠️  Workflow cancelled");
            std::process::exit(1);
        }
        WorkflowResult::Failed { error } => {
            eprintln!("\n❌ Workflow failed: {}", error);
            std::process::exit(1);
        }
    }
}

// Similar implementations for diagnostics_command_ai and comments_command_ai
```

---

## Phase 4: Migration & Cleanup

### 4.1 Deprecation Timeline

1. **v0.x.0**: Add AgentWorkflow alongside AiDecisionClient
2. **v0.x.1**: Mark AiDecisionClient as deprecated with warning
3. **v0.x.2**: Remove AiDecisionClient and HTTP-based workflow

### 4.2 Files to Remove (Eventually)

```
crates/uira/src/ai_decision.rs  # Replace entirely
```

### 4.3 Config Migration

Old config (HTTP-based, deprecated):
```yaml
typos:
  ai:
    host: 127.0.0.1
    port: 4096
    disable_tools: true
    disable_mcp: true
```

New config (Agent-based):
```yaml
typos:
  ai:
    model: anthropic/claude-sonnet-4-20250514
    max_iterations: 10
    enable_lsp: false
    
diagnostics:
  ai:
    model: anthropic/claude-sonnet-4-20250514
    max_iterations: 10
    enable_lsp: true  # Uses lsp_diagnostics
    severity: error
    
comments:
  ai:
    model: anthropic/claude-sonnet-4-20250514
    max_iterations: 10
    enable_lsp: false
```

---

## Testing Strategy

### Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_completion_detection() {
        let detector = CompletionDetector::new();
        assert!(detector.is_done("<DONE/>"));
        assert!(detector.is_done("<DONE>Fixed 3 issues</DONE>"));
        assert!(!detector.is_done("Still working..."));
    }
    
    #[test]
    fn test_workflow_state_persistence() {
        let state = WorkflowState::new(
            WorkflowTask::Typos,
            "test-session".to_string(),
            10,
        );
        state.write().unwrap();
        
        let loaded = WorkflowState::read(WorkflowTask::Typos).unwrap();
        assert_eq!(loaded.session_id, "test-session");
        
        WorkflowState::clear(WorkflowTask::Typos).unwrap();
    }
    
    #[test]
    fn test_git_tracker() {
        let temp = tempfile::tempdir().unwrap();
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(temp.path())
            .status()
            .unwrap();
            
        let tracker = GitTracker::new(temp.path());
        assert!(tracker.get_modifications().is_empty());
    }
}
```

### Integration Tests

```rust
#[tokio::test]
async fn test_typos_workflow_e2e() {
    // Create temp directory with a file containing typos
    let temp = tempfile::tempdir().unwrap();
    
    // Initialize git repo
    std::process::Command::new("git")
        .args(["init"])
        .current_dir(temp.path())
        .status()
        .unwrap();
    
    std::fs::write(
        temp.path().join("test.rs"),
        "// This is a tset file with typso\n",
    ).unwrap();
    
    let config = WorkflowConfig {
        working_directory: temp.path().to_path_buf(),
        max_iterations: 3,
        enable_lsp_tools: false,
        ..Default::default()
    };
    
    let mut workflow = AgentWorkflow::new(WorkflowTask::Typos, config).await.unwrap();
    let result = workflow.run().await.unwrap();
    
    assert!(matches!(result, WorkflowResult::Complete { .. }));
}
```

---

## Implementation Checklist

### Week 1: Core Infrastructure
- [ ] Add dependencies to `Cargo.toml`
- [ ] Create `runtime.rs` with shared Tokio runtime
- [ ] Create `agent_workflow/mod.rs` with `WorkflowTask`
- [ ] Create `agent_workflow/config.rs` with `WorkflowConfig`
- [ ] Create `agent_workflow/state.rs` with `WorkflowState`
- [ ] Create `agent_workflow/completion.rs` with `CompletionDetector`
- [ ] Create `agent_workflow/git_tracker.rs` with `GitTracker`
- [ ] Create `agent_workflow/verification.rs` with `WorkflowVerifier`

### Week 2: AgentWorkflow Implementation
- [ ] Create `agent_workflow/workflow.rs` with `AgentWorkflow`
- [ ] Implement `AgentWorkflow::new()` with provider setup
- [ ] Implement `AgentWorkflow::run()` with completion loop
- [ ] Create `agent_workflow/prompts.rs` with system prompts
- [ ] Add LSP provider registration

### Week 3: Integration
- [ ] Update `main.rs` to use shared runtime
- [ ] Update `typos` command for AI workflow
- [ ] Update `diagnostics` command for AI workflow  
- [ ] Update `comments` command for AI workflow
- [ ] Add deprecation warning to `ai_decision.rs`

### Week 4: Testing & Polish
- [ ] Unit tests for all components
- [ ] Integration tests with temp repos
- [ ] Update README documentation
- [ ] Update configuration docs
- [ ] Remove `ai_decision.rs` (or mark deprecated)

---

## Effort Estimate

| Phase | Effort | Description |
|-------|--------|-------------|
| **Prototype** | 1-4 hours | Single workflow (typos) working |
| **Full Migration** | 1-2 days | All three workflows + resume + staging + verification |

---

## Open Questions (Resolved)

| Question | Resolution |
|----------|------------|
| Tool restrictions | Full access by default, configurable via config |
| Timeout | Use agent's max_turns, not per-iteration timeout |
| Parallel execution | No - isolated sessions per workflow |
| Rollback | Future enhancement - not in initial scope |
| Async strategy | Keep sync main, shared `OnceLock<Runtime>` |
| Modification tracking | Git diff before/after, not rollout parsing |
| LSP tools | Register `LspToolProvider` when `enable_lsp: true` |
