# Native Agent Harness Integration Plan v3

## Executive Summary

This plan integrates goal-based ralph mode, LSP/MCP tools, and background task management into the native Codex-style agent harness.

**Key Findings (Verified):**
- TUI-Agent wiring exists but Agent's `approval_tx` is **created but never used** in `execute_tool_calls()`
- Ralph hook has 1,550 lines of battle-tested code - **wrap, don't rewrite**
- `ToolProvider` trait **does not exist** - needs creation
- Background task cancel updates status but **doesn't signal running agents**
- Several code samples in v2 had incorrect signatures - **now fixed**

**Critical Corrections from v2:**
- `RalphHook::activate()` returns `bool`, not `Self`
- `build_verification_feedback()` is PRIVATE - must be made `pub`
- `GoalConfig` import is `uira_config::schema::GoalConfig`
- `LspClientImpl` methods take `Value` params, not individual typed params
- `run_without_approval()` does NOT exist - must be added

---

## State Machine

```
                    ┌─────────────────────────────────────────┐
                    │                                         │
                    ▼                                         │
┌──────┐    ┌──────────────┐    ┌──────────────┐    ┌────────┴───────┐
│ Idle │───▶│ WaitingForUser│───▶│   Thinking   │───▶│ ExecutingTool  │
└──────┘    └──────────────┘    └──────────────┘    └────────────────┘
                    ▲                   │                    │
                    │                   │                    ▼
                    │                   │           ┌────────────────────┐
                    │                   │           │ WaitingForApproval │
                    │                   │           └────────────────────┘
                    │                   │                    │
                    │                   │    ┌───────────────┴───────────────┐
                    │                   │    │ approved                denied │
                    │                   │    ▼                               ▼
                    │                   │  (back to                    (skip tool,
                    │                   │   ExecutingTool)              back to Thinking)
                    │                   │
                    │                   ▼
                    │           ┌──────────────────┐
                    │           │ VerifyingGoals   │ (NEW - Phase 2)
                    │           └──────────────────┘
                    │                   │
                    │     ┌─────────────┴─────────────┐
                    │     │ all passed           failed │
                    │     ▼                           ▼
                    │  ┌──────────┐              (continue with
                    │  │ Complete │               feedback)
                    │  └──────────┘
                    │
        ┌───────────┴───────────┐
        │                       │
┌───────┴───────┐       ┌───────┴───────┐
│    Failed     │       │   Cancelled   │
└───────────────┘       └───────────────┘

Ralph Mode Overlay:
┌─────────────────────────────────────────────────────────────┐
│ RalphIterating (wraps Thinking → ExecutingTool → Verifying) │
│                                                             │
│  on completion signal detected:                             │
│    if exit_gate_passed → Complete                           │
│    else → continue with feedback                            │
│                                                             │
│  on circuit_breaker_tripped → Exit with reason              │
│  on max_iterations_reached → Exit with reason               │
└─────────────────────────────────────────────────────────────┘
```

---

## Phase 0: Fix Critical Integration Gaps (HIGHEST PRIORITY)

### Problem Statement (Corrected)

The Agent's `approval_tx` field (agent.rs:35) is **created but never used** in `execute_tool_calls()`. The code path is:

```
Agent.execute_tool_calls() [lines 620-684]
  → session.orchestrator.run() [line 643]
    → ToolOrchestrator checks approval_requirement
    → If NeedsApproval: calls request_approval() [orchestrator.rs:175-202]
      → Sends to orchestrator's OWN approval_tx
      → Blocks on oneshot receiver
      → NOBODY READS from orchestrator's approval channel
      → HANGS FOREVER
```

The TUI's approval handler listens to **Agent's** `approval_tx`, not the orchestrator's.

### Tasks

#### 0.1 Wire Agent's Approval into Tool Execution

**Strategy**: Add approval check in `execute_tool_calls()` BEFORE calling orchestrator, using Agent's connected channel.

```rust
// In uira-agent/src/agent.rs, modify execute_tool_calls() [lines 620-684]

use std::time::Duration;
use tokio::time::timeout;

const APPROVAL_TIMEOUT: Duration = Duration::from_secs(300); // 5 minutes

async fn execute_tool_calls(&mut self, tool_calls: &[ToolCall]) -> Result<Vec<ContentBlock>, AgentLoopError> {
    let ctx = self.session.tool_context();

    for call in tool_calls {
        // Check cancel between each tool
        if self.control.is_cancelled() {
            return Err(AgentLoopError::Cancelled);
        }

        // Skip approval if full_auto mode
        if !ctx.full_auto {
            // Check approval requirement BEFORE calling orchestrator
            if let Some(tool) = self.session.orchestrator.router().get(&call.name) {
                let requirement = tool.approval_requirement(&call.input);

                match requirement {
                    ApprovalRequirement::NeedsApproval { reason } => {
                        if let Some(ref approval_tx) = self.approval_tx {
                            // Emit event for TUI
                            self.emit_event(ThreadEvent::ItemStarted {
                                item: Item::ApprovalRequest {
                                    id: call.id.clone(),
                                    tool_name: call.name.clone(),
                                    input: call.input.clone(),
                                    reason: reason.clone(),
                                }
                            }).await;

                            // Request approval with timeout
                            let decision = timeout(
                                APPROVAL_TIMEOUT,
                                approval_tx.request_approval(ApprovalPending {
                                    tool_name: call.name.clone(),
                                    tool_input: call.input.clone(),
                                    reason,
                                })
                            ).await
                            .map_err(|_| AgentLoopError::ApprovalTimeout {
                                tool: call.name.clone(),
                                timeout_secs: APPROVAL_TIMEOUT.as_secs(),
                            })??;

                            // Emit decision event
                            self.emit_event(ThreadEvent::ItemCompleted {
                                item: Item::ApprovalDecision {
                                    request_id: call.id.clone(),
                                    approved: decision.approved,
                                }
                            }).await;

                            if !decision.approved {
                                // Add denial to results and continue to next tool
                                results.push(ContentBlock::ToolResult {
                                    tool_use_id: call.id.clone(),
                                    content: format!("Tool execution denied: {}",
                                        decision.reason.unwrap_or_default()),
                                    is_error: Some(true),
                                });
                                continue;
                            }
                        }
                    }
                    ApprovalRequirement::Forbidden { reason } => {
                        return Err(AgentLoopError::ToolForbidden {
                            tool: call.name.clone(),
                            reason
                        });
                    }
                    ApprovalRequirement::Skip => {}
                }
            }
        }

        // Execute tool - orchestrator skips approval since we handled it
        // Option 1: Add skip_approval flag to run()
        let result = self.session.orchestrator
            .run_with_options(&call.name, call.input.clone(), &ctx, RunOptions {
                skip_approval: true,  // We already handled approval
                ..Default::default()
            })
            .await;

        // ... rest of handling unchanged
    }
    Ok(results)
}
```

**Add to AgentLoopError** (agent.rs or errors.rs):
```rust
pub enum AgentLoopError {
    // ... existing variants
    ApprovalTimeout { tool: String, timeout_secs: u64 },
    ToolForbidden { tool: String, reason: String },
}
```

**Add to ToolOrchestrator** (orchestrator.rs):
```rust
pub struct RunOptions {
    pub skip_approval: bool,
    pub skip_sandbox: bool,
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            skip_approval: false,
            skip_sandbox: false,
        }
    }
}

impl ToolOrchestrator {
    pub async fn run_with_options(
        &self,
        tool_name: &str,
        input: Value,
        ctx: &ToolContext,
        options: RunOptions,
    ) -> Result<ToolOutput, ToolError> {
        // ... existing setup ...

        // Skip approval check if requested
        if !options.skip_approval {
            // existing approval logic
        }

        // ... rest of execution
    }

    // Keep existing run() for backwards compatibility
    pub async fn run(&self, tool_name: &str, input: Value, ctx: &ToolContext) -> Result<ToolOutput, ToolError> {
        self.run_with_options(tool_name, input, ctx, RunOptions::default()).await
    }
}
```

#### 0.2 Render Streaming Buffer During Generation

```rust
// In uira-tui/src/app.rs, modify render_chat() [around line 196]
fn render_chat(&self, frame: &mut Frame, area: Rect) {
    let mut items: Vec<ListItem> = self.messages.iter()
        .map(|msg| {
            let style = match msg.role.as_str() {
                "user" => Style::default().fg(Color::Green),
                "assistant" => Style::default().fg(Color::Cyan),
                _ => Style::default(),
            };
            ListItem::new(Span::styled(
                format!("{}: {}", msg.role, msg.content),
                style,
            ))
        })
        .collect();

    // NEW: Render streaming buffer as in-progress message
    if let Some(ref buffer) = self.streaming_buffer {
        if !buffer.is_empty() {
            let streaming_item = ListItem::new(Line::from(vec![
                Span::styled("assistant: ", Style::default().fg(Color::Cyan)),
                Span::styled(buffer.as_str(), Style::default().fg(Color::Cyan)),
                Span::styled("▌", Style::default().fg(Color::Yellow).add_modifier(Modifier::SLOW_BLINK)),
            ]));
            items.push(streaming_item);
        }
    }

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title("Chat"));
    frame.render_widget(list, area);
}
```

#### 0.3 Add Streaming Buffer Size Limit

```rust
// In uira-tui/src/app.rs
const MAX_STREAMING_BUFFER_SIZE: usize = 1024 * 1024; // 1MB

// In handle_agent_event(), ContentDelta handler:
ThreadEvent::ContentDelta { delta } => {
    if let Some(ref mut buffer) = self.streaming_buffer {
        if buffer.len() + delta.len() <= MAX_STREAMING_BUFFER_SIZE {
            buffer.push_str(&delta);
        }
        // Silently drop if over limit (or could truncate from front)
    } else {
        self.streaming_buffer = Some(delta);
    }
}
```

**Files to Modify:**
- `crates/uira-agent/src/agent.rs` - Wire approval, add timeout
- `crates/uira-tools/src/orchestrator.rs` - Add `run_with_options()`, `RunOptions`
- `crates/uira-tui/src/app.rs` - Render streaming buffer, add size limit

### Success Criteria
- [ ] Approval modal appears for bash/write tools
- [ ] y/n/a keyboard shortcuts work
- [ ] Tool executes only after explicit approval
- [ ] Denied tools return error result, don't block
- [ ] Streaming content visible during generation
- [ ] No hanging on tool execution
- [ ] 5-minute timeout on approval

### Testing Strategy (Phase 0)

```rust
// Add to crates/uira-agent/tests/integration.rs

#[tokio::test]
async fn test_approval_flow_approved() {
    let (agent, _input_tx, mut approval_rx) = make_agent_with_approval();

    // Spawn agent
    let handle = tokio::spawn(async move {
        agent.run("Run `echo hello`").await
    });

    // Wait for approval request
    let request = approval_rx.recv().await.unwrap();
    assert_eq!(request.tool_name, "bash");

    // Approve
    request.respond(ApprovalDecision { approved: true, reason: None });

    // Agent should complete
    let result = handle.await.unwrap();
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_approval_flow_denied() {
    let (agent, _input_tx, mut approval_rx) = make_agent_with_approval();

    let handle = tokio::spawn(async move {
        agent.run("Run `rm -rf /`").await
    });

    let request = approval_rx.recv().await.unwrap();
    request.respond(ApprovalDecision { approved: false, reason: Some("dangerous".into()) });

    // Agent should complete (tool denied, not agent failed)
    let result = handle.await.unwrap();
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_approval_timeout() {
    let (agent, _input_tx, _approval_rx) = make_agent_with_approval();

    // Set short timeout for test
    let agent = agent.with_approval_timeout(Duration::from_millis(100));

    let result = agent.run("Run `echo hello`").await;

    assert!(matches!(result, Err(AgentLoopError::ApprovalTimeout { .. })));
}

#[tokio::test]
async fn test_full_auto_skips_approval() {
    // Existing test - should still pass
    let config = make_config().full_auto();
    let agent = Agent::new(config, mock_client());

    // Should not block on approval
    let result = agent.run("Run `echo hello`").await;
    assert!(result.is_ok());
}
```

---

## Phase 1: Add Protocol Events

### Tasks

#### 1.1 Add New ThreadEvent Variants

```rust
// In crates/uira-protocol/src/events.rs

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[non_exhaustive]  // Allow adding variants without breaking consumers
pub enum ThreadEvent {
    // ... existing variants ...

    // Goal Verification Events
    GoalVerificationStarted {
        goals: Vec<String>,
        method: String,
    },
    GoalVerificationResult {
        goal: String,
        score: f64,
        target: f64,
        passed: bool,
        duration_ms: u64,
    },
    GoalVerificationCompleted {
        all_passed: bool,
        passed_count: usize,
        total_count: usize,
    },

    // Ralph Mode Events
    RalphIterationStarted {
        iteration: u32,
        max_iterations: u32,
        prompt: String,
    },
    RalphContinuation {
        reason: String,
        confidence: u32,
        details: String,
    },
    RalphCircuitBreak {
        reason: String,
        iteration: u32,
    },

    // Background Task Events
    BackgroundTaskSpawned {
        task_id: String,
        description: String,
        agent: String,
    },
    BackgroundTaskProgress {
        task_id: String,
        status: String,
        message: Option<String>,
    },
    BackgroundTaskCompleted {
        task_id: String,
        success: bool,
        result_preview: Option<String>,
        duration_secs: f64,
    },
}
```

#### 1.2 Update TUI Event Handling

```rust
// In uira-tui/src/app.rs, add to handle_agent_event()

ThreadEvent::GoalVerificationStarted { goals, .. } => {
    self.status = format!("Verifying {} goals...", goals.len());
    self.state = AgentState::VerifyingGoals; // NEW state if added
}
ThreadEvent::GoalVerificationResult { goal, passed, score, target, .. } => {
    let icon = if passed { "✓" } else { "✗" };
    self.add_system_message(format!(
        "{} Goal '{}': {:.1}% (target: {:.1}%)",
        icon, goal, score, target
    ));
}
ThreadEvent::GoalVerificationCompleted { all_passed, passed_count, total_count } => {
    if all_passed {
        self.status = format!("All {}/{} goals passed", passed_count, total_count);
    } else {
        self.status = format!("Goals: {}/{} passed", passed_count, total_count);
    }
}
ThreadEvent::RalphIterationStarted { iteration, max_iterations, .. } => {
    self.status = format!("Ralph iteration {}/{}", iteration, max_iterations);
}
ThreadEvent::RalphContinuation { reason, confidence, .. } => {
    self.add_system_message(format!(
        "Ralph continuing: {} (confidence: {}%)",
        reason, confidence
    ));
}
ThreadEvent::RalphCircuitBreak { reason, iteration } => {
    self.add_system_message(format!(
        "Ralph stopped at iteration {}: {}",
        iteration, reason
    ));
    self.state = AgentState::Complete;
}
ThreadEvent::BackgroundTaskSpawned { task_id, description, .. } => {
    self.add_system_message(format!("Background task started: {} ({})", description, task_id));
}
ThreadEvent::BackgroundTaskCompleted { task_id, success, .. } => {
    let status = if success { "completed" } else { "failed" };
    self.add_system_message(format!("Background task {}: {}", task_id, status));
}
// Handle unknown variants gracefully (due to #[non_exhaustive])
_ => {
    // Log but don't crash
    tracing::debug!("Unknown ThreadEvent variant");
}
```

### Testing Strategy (Phase 1)

```rust
// In crates/uira-protocol/tests/ or existing test file

#[test]
fn test_new_event_serialization() {
    let events = vec![
        ThreadEvent::GoalVerificationStarted {
            goals: vec!["test".into()],
            method: "auto".into(),
        },
        ThreadEvent::RalphIterationStarted {
            iteration: 1,
            max_iterations: 10,
            prompt: "test".into(),
        },
        ThreadEvent::BackgroundTaskSpawned {
            task_id: "bg_123".into(),
            description: "test".into(),
            agent: "executor".into(),
        },
    ];

    for event in events {
        let json = serde_json::to_string(&event).unwrap();
        let parsed: ThreadEvent = serde_json::from_str(&json).unwrap();
        // Verify round-trip
    }
}
```

**Files to Modify:**
- `crates/uira-protocol/src/events.rs` - Add variants with `#[non_exhaustive]`
- `crates/uira-tui/src/app.rs` - Add handlers with fallback

---

## Phase 2: Native Goal Verification

### Tasks

#### 2.1 Add Goals Config to Agent

```rust
// In uira-agent/src/config.rs
use uira_config::schema::GoalConfig;  // CORRECT import path

pub struct AgentGoalsConfig {
    pub goals: Vec<GoalConfig>,
    pub auto_verify: bool,
    pub verify_on_tool_complete: bool,
    pub parallel_check: bool,  // Run goals in parallel
}

impl Default for AgentGoalsConfig {
    fn default() -> Self {
        Self {
            goals: vec![],
            auto_verify: true,
            verify_on_tool_complete: false,
            parallel_check: true,
        }
    }
}
```

#### 2.2 Create Goal Verifier

```rust
// NEW: crates/uira-agent/src/goals.rs

use std::path::Path;
use uira_goals::{GoalRunner, GoalCheckResult, VerificationResult};
use uira_config::schema::GoalConfig;  // CORRECT import
use crate::events::EventSender;
use uira_protocol::ThreadEvent;
use futures::future::join_all;

pub struct GoalVerifier {
    runner: GoalRunner,
    goals: Vec<GoalConfig>,
    event_tx: Option<EventSender>,
    parallel: bool,
}

impl GoalVerifier {
    pub fn new(project_root: impl AsRef<Path>, goals: Vec<GoalConfig>) -> Self {
        Self {
            runner: GoalRunner::new(project_root),
            goals,
            event_tx: None,
            parallel: true,
        }
    }

    pub fn with_events(mut self, tx: EventSender) -> Self {
        self.event_tx = Some(tx);
        self
    }

    pub fn with_parallel(mut self, parallel: bool) -> Self {
        self.parallel = parallel;
        self
    }

    pub async fn verify_all(&self) -> VerificationResult {
        // Emit start event
        if let Some(ref tx) = self.event_tx {
            let _ = tx.send(ThreadEvent::GoalVerificationStarted {
                goals: self.goals.iter().map(|g| g.name.clone()).collect(),
                method: "auto".into(),
            }).await;
        }

        // Run goals (parallel or sequential based on config)
        let results = if self.parallel {
            let futures: Vec<_> = self.goals.iter()
                .map(|g| self.runner.check_goal(g))
                .collect();
            join_all(futures).await
        } else {
            let mut results = Vec::new();
            for goal in &self.goals {
                results.push(self.runner.check_goal(goal).await);
            }
            results
        };

        // Emit per-goal results
        if let Some(ref tx) = self.event_tx {
            for r in &results {
                let _ = tx.send(ThreadEvent::GoalVerificationResult {
                    goal: r.name.clone(),
                    score: r.score,
                    target: r.target,
                    passed: r.passed,
                    duration_ms: r.duration_ms,
                }).await;
            }
        }

        let all_passed = results.iter().all(|r| r.passed);
        let passed_count = results.iter().filter(|r| r.passed).count();

        // Emit completion
        if let Some(ref tx) = self.event_tx {
            let _ = tx.send(ThreadEvent::GoalVerificationCompleted {
                all_passed,
                passed_count,
                total_count: results.len(),
            }).await;
        }

        VerificationResult {
            all_passed,
            results,
            checked_at: chrono::Utc::now(),
            iteration: 0,
        }
    }

    pub fn has_goals(&self) -> bool {
        !self.goals.is_empty()
    }
}
```

**Files to Create/Modify:**
- `crates/uira-agent/src/goals.rs` (NEW)
- `crates/uira-agent/src/config.rs` - Add AgentGoalsConfig
- `crates/uira-agent/src/agent.rs` - Add `goal_verifier` field, `check_completion()`
- `crates/uira-agent/src/lib.rs` - Export goals
- `crates/uira-agent/Cargo.toml` - Add `uira-goals` dependency

---

## Phase 3: Native Ralph Mode (Wrap Existing)

### Pre-requisites

Must make these ralph functions public in `uira-hooks/src/hooks/ralph.rs`:

```rust
// Currently private, must become pub:
pub fn build_verification_feedback(
    signals: &CompletionSignals,
    state: &RalphState,
    goals_result: &Option<VerificationResult>,
) -> String { ... }

pub fn get_continuation_prompt(state: &RalphState) -> String { ... }
```

### Tasks

#### 3.1 Create Ralph Controller

```rust
// NEW: crates/uira-agent/src/ralph.rs

use uira_hooks::hooks::ralph::{
    RalphState, RalphOptions, CompletionSignals,
    detect_completion_signals_with_goals, RalphStatusBlock,
    RalphHook,  // For static methods
};
use uira_hooks::hooks::circuit_breaker::CircuitBreakerConfig;
use uira_hooks::hooks::todo_continuation::TodoContinuationHook;
use uira_goals::VerificationResult;
use crate::events::EventSender;
use uira_protocol::ThreadEvent;

pub struct RalphConfig {
    pub max_iterations: u32,
    pub completion_promise: String,
    pub min_confidence: u32,
    pub require_dual_condition: bool,
    pub circuit_breaker: CircuitBreakerConfig,
}

impl Default for RalphConfig {
    fn default() -> Self {
        Self {
            max_iterations: 10,
            completion_promise: "TASK COMPLETE".into(),
            min_confidence: 50,
            require_dual_condition: true,
            circuit_breaker: CircuitBreakerConfig::default(),
        }
    }
}

pub struct RalphController {
    state: RalphState,
    config: RalphConfig,
    directory: String,
    event_tx: Option<EventSender>,
}

impl RalphController {
    /// Activate ralph mode for a task
    /// NOTE: RalphHook::activate() returns bool, not Self
    pub fn activate(
        prompt: &str,
        session_id: Option<&str>,
        directory: &str,
        config: RalphConfig,
    ) -> Option<Self> {
        let options = RalphOptions {
            max_iterations: config.max_iterations,
            completion_promise: config.completion_promise.clone(),
            min_confidence: config.min_confidence,
            require_dual_condition: config.require_dual_condition,
        };

        // Returns bool indicating success
        let success = RalphHook::activate(
            prompt,
            session_id,
            Some(directory),
            Some(options),
        );

        if !success {
            return None;
        }

        // Read back the state that was written
        let state = RalphHook::read_state(Some(directory))?;

        Some(Self {
            state,
            config,
            directory: directory.to_string(),
            event_tx: None,
        })
    }

    pub fn with_events(mut self, tx: EventSender) -> Self {
        self.event_tx = Some(tx);
        self
    }

    pub async fn check_completion(
        &mut self,
        response_text: &str,
        goals_result: Option<&VerificationResult>,
    ) -> RalphDecision {
        self.emit_iteration_started();

        // Check circuit breaker
        if self.state.circuit_breaker.is_tripped() {
            self.emit_circuit_break("stagnation");
            self.clear();
            return RalphDecision::Exit {
                reason: self.state.circuit_breaker.trip_reason.clone()
                    .unwrap_or_else(|| "Circuit breaker tripped".into()),
            };
        }

        // Check max iterations
        if self.state.iteration >= self.state.max_iterations {
            self.emit_circuit_break("max_iterations");
            self.clear();
            return RalphDecision::Exit {
                reason: format!("Max iterations ({}) reached", self.state.max_iterations),
            };
        }

        // Check todos
        let todo_result = TodoContinuationHook::check_incomplete_todos(
            None, &self.directory, None,
        );

        // Detect completion signals
        let signals = detect_completion_signals_with_goals(
            response_text,
            &self.state.completion_promise,
            Some(&todo_result),
            goals_result,
        );

        // Check exit gate
        let goals_passed = goals_result.map(|r| r.all_passed).unwrap_or(true);
        let exit_allowed = if self.config.require_dual_condition {
            signals.is_exit_allowed()
                && signals.confidence >= self.state.min_confidence
                && goals_passed
        } else {
            signals.confidence >= self.state.min_confidence && goals_passed
        };

        if exit_allowed {
            self.clear();
            RalphDecision::Complete
        } else {
            // Build feedback using the now-public function
            let feedback = RalphHook::build_verification_feedback(
                &signals,
                &self.state,
                &goals_result.cloned(),
            );
            self.emit_continuation(&feedback, signals.confidence);
            self.increment_iteration();
            RalphDecision::Continue { feedback }
        }
    }

    fn increment_iteration(&mut self) {
        self.state.iteration += 1;
        self.state.last_checked_at = chrono::Utc::now();
        RalphHook::write_state(&self.state, Some(&self.directory));
    }

    fn clear(&self) {
        RalphHook::clear_state(Some(&self.directory));
    }

    fn emit_iteration_started(&self) {
        if let Some(ref tx) = self.event_tx {
            let _ = tx.try_send(ThreadEvent::RalphIterationStarted {
                iteration: self.state.iteration,
                max_iterations: self.state.max_iterations,
                prompt: self.state.prompt.clone().unwrap_or_default(),
            });
        }
    }

    fn emit_continuation(&self, details: &str, confidence: u32) {
        if let Some(ref tx) = self.event_tx {
            let _ = tx.try_send(ThreadEvent::RalphContinuation {
                reason: "verification_failed".into(),
                confidence,
                details: details.to_string(),
            });
        }
    }

    fn emit_circuit_break(&self, reason: &str) {
        if let Some(ref tx) = self.event_tx {
            let _ = tx.try_send(ThreadEvent::RalphCircuitBreak {
                reason: reason.to_string(),
                iteration: self.state.iteration,
            });
        }
    }

    pub fn is_active(&self) -> bool { self.state.active }
    pub fn iteration(&self) -> u32 { self.state.iteration }
}

pub enum RalphDecision {
    Continue { feedback: String },
    Complete,
    Exit { reason: String },
}
```

**Files to Create/Modify:**
- `crates/uira-agent/src/ralph.rs` (NEW)
- `crates/uira-hooks/src/hooks/ralph.rs` - Make `build_verification_feedback` pub
- `crates/uira-agent/Cargo.toml` - Add `uira-hooks` dependency

---

## Phase 4: Native Tool Providers (LSP/AST)

### Corrected LSP Provider

Note: `LspClientImpl` methods take `Value` params, not individual typed params.

```rust
// NEW: crates/uira-tools/src/providers/lsp.rs

use crate::{ToolContext, ToolOutput, ToolError};
use crate::lsp::LspClientImpl;
use uira_protocol::ToolSpec;
use async_trait::async_trait;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct LspToolProvider {
    client: Arc<RwLock<Option<LspClientImpl>>>,
    root_path: PathBuf,
}

impl LspToolProvider {
    /// Create with lazy initialization (LSP starts on first use)
    pub fn new(root_path: PathBuf) -> Self {
        Self {
            client: Arc::new(RwLock::new(None)),
            root_path,
        }
    }

    async fn get_client(&self) -> Result<impl std::ops::Deref<Target = LspClientImpl> + '_, ToolError> {
        // Lazy init with retry
        {
            let read = self.client.read().await;
            if read.is_some() {
                return Ok(tokio::sync::RwLockReadGuard::map(read, |opt| opt.as_ref().unwrap()));
            }
        }

        // Initialize
        let mut write = self.client.write().await;
        if write.is_none() {
            let client = LspClientImpl::new(self.root_path.clone())  // Takes PathBuf, not &PathBuf
                .await
                .map_err(|e| ToolError::ExecutionFailed {
                    tool: "lsp".into(),
                    message: format!("LSP init failed: {}", e),
                })?;
            *write = Some(client);
        }
        drop(write);

        let read = self.client.read().await;
        Ok(tokio::sync::RwLockReadGuard::map(read, |opt| opt.as_ref().unwrap()))
    }
}

#[async_trait]
impl super::ToolProvider for LspToolProvider {
    fn specs(&self) -> Vec<ToolSpec> {
        vec![
            ToolSpec {
                name: "lsp_goto_definition".into(),
                description: "Go to symbol definition".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "file_path": { "type": "string" },
                        "line": { "type": "integer" },
                        "character": { "type": "integer" }
                    },
                    "required": ["file_path", "line", "character"]
                }),
            },
            // ... other tools
        ]
    }

    fn handles(&self, name: &str) -> bool {
        name.starts_with("lsp_")
    }

    async fn execute(&self, name: &str, input: Value, _ctx: &ToolContext) -> Result<ToolOutput, ToolError> {
        let client = self.get_client().await?;

        // LspClientImpl methods take Value, so pass input directly
        let result = match name {
            "lsp_goto_definition" => client.goto_definition(input).await,
            "lsp_find_references" => client.find_references(input).await,
            "lsp_symbols" => client.symbols(input).await,
            "lsp_diagnostics" => client.diagnostics(input).await,
            "lsp_hover" => client.hover(input).await,
            "lsp_rename" => client.rename(input).await,
            _ => return Err(ToolError::NotFound { tool: name.into() }),
        };

        result
            .map(|v| ToolOutput::success(serde_json::to_string_pretty(&v).unwrap_or_default()))
            .map_err(|e| ToolError::ExecutionFailed {
                tool: name.into(),
                message: e.to_string(),
            })
    }
}
```

---

## Configuration Loading

All configuration is loaded from `uira.yml` in project root:

```yaml
# uira.yml
goals:
  auto_verify: true
  parallel_check: true
  goals:
    - name: test-coverage
      command: bun run coverage --json | jq '.total'
      target: 80.0
      timeout_secs: 60

ralph:
  max_iterations: 10
  min_confidence: 50
  require_dual_condition: true

agent:
  full_auto: false
  approval_timeout_secs: 300
```

```rust
// Config loading in agent initialization
let config = uira_config::load_config(Some(project_root))?;

let agent_config = AgentConfig {
    goals: config.goals.map(|g| AgentGoalsConfig {
        goals: g.goals,
        auto_verify: g.auto_verify,
        parallel_check: g.parallel_check.unwrap_or(true),
        ..Default::default()
    }),
    full_auto: config.agent.map(|a| a.full_auto).unwrap_or(false),
    ..Default::default()
};
```

---

## Migration & Compatibility

### Breaking Changes

| Change | Migration Path |
|--------|----------------|
| New `ThreadEvent` variants | Use `#[non_exhaustive]`, consumers must handle unknown variants |
| `run_with_options()` added | Existing `run()` preserved for backwards compat |
| Ralph functions made public | No breaking change |

### Feature Flags

```rust
// In Cargo.toml
[features]
default = []
native-approval = []  # Phase 0 approval wiring
native-goals = []     # Phase 2 goal verification
native-ralph = []     # Phase 3 ralph mode

// In code
#[cfg(feature = "native-approval")]
async fn execute_tool_calls_with_approval(...) { ... }

#[cfg(not(feature = "native-approval"))]
async fn execute_tool_calls(...) { /* existing */ }
```

### Deprecations

- `ToolOrchestrator::take_approval_receiver()` - Deprecated, use Agent's approval channel
- Direct calls to `orchestrator.run()` with approval-required tools - Use agent layer

---

## Risk Mitigation (Updated)

| Risk | Mitigation |
|------|------------|
| Phase 0 breaks existing `full_auto` tests | Check `ctx.full_auto` before approval flow |
| Approval timeout too short | Make configurable, default 5 min |
| Ralph adapter misses edge cases | Use existing functions, extensive testing |
| LSP startup latency | Lazy init + connection pooling |
| Background cancel race | Atomic bool + check before each operation |
| Circular dependencies | Verified: `uira-agent` → `uira-hooks` → `uira-goals` OK |
| Event channel overflow | Use `try_send`, drop oldest on overflow |
| Ralph state file concurrent access | Session-scoped files, warn on collision |

---

## Implementation Order (Corrected)

```
Phase 0 (CRITICAL - Approval + Streaming)
├── 0.1 Add RunOptions to orchestrator
├── 0.2 Wire approval in execute_tool_calls
├── 0.3 Add approval timeout
├── 0.4 Render streaming buffer
└── 0.5 Test all existing tests still pass
     │
     ▼
Phase 1 (Protocol Events)
├── 1.1 Add ThreadEvent variants with #[non_exhaustive]
└── 1.2 Add TUI handlers with fallback for unknown
     │
     ▼
Phase 2 (Goals) ←────────────────────┐
├── 2.1 Add goals.rs                 │
├── 2.2 Wire into agent completion   │
└── 2.3 Test goal verification       │
     │                               │
     ▼                               │
Phase 3 (Ralph) ─────────────────────┤
├── 3.1 Make ralph funcs public      │
├── 3.2 Add ralph.rs adapter         │
└── 3.3 Test ralph loop              │
                                     │
Phase 4 (Tools) ─────────────────────┤ (can run parallel)
├── 4.1 Add ToolProvider trait       │
├── 4.2 Add LSP provider (lazy)      │
└── 4.3 Add AST provider             │
                                     │
Phase 5 (Background) ────────────────┘
├── 5.1 Add cancel_signal to tasks
└── 5.2 Wire to agent control
     │
     ▼
Phase 6 (CLI)
├── 6.1 Add --ralph flag
├── 6.2 Add goals commands
└── 6.3 Add tasks commands
```

---

## Appendix: File Change Summary (Updated)

| Phase | New Files | Modified Files |
|-------|-----------|----------------|
| 0 | 0 | 3 (agent.rs, orchestrator.rs, app.rs) |
| 1 | 0 | 2 (events.rs, app.rs) |
| 2 | 1 | 4 (goals.rs NEW, config.rs, agent.rs, lib.rs, Cargo.toml) |
| 3 | 1 | 2 (ralph.rs NEW, hooks/ralph.rs, Cargo.toml) |
| 4 | 4 | 2 (provider.rs, providers/mod.rs, lsp.rs, ast.rs NEW; router.rs, lib.rs) |
| 5 | 0 | 2 (agent.rs, background_agent.rs) |
| 6 | 0 | 2 (commands.rs, main.rs) |
| **Total** | **6** | **17** |
