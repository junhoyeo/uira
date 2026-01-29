# Native Agent Harness Integration Plan

## Goal
Integrate ralph mode, goal verification, LSP/MCP tools, and background task management natively into the Codex-style agent harness.

---

## Phase 1: Wire TUI ↔ Agent Execution Loop (CRITICAL)

### Problem
TUI exists but doesn't actually run the agent. `run_with_agent()` spawns event handler but never starts agent execution.

### Tasks
1. **Connect agent to TUI event loop**
   - In `uira-tui/src/app.rs`, wire `App::run_with_agent()` to:
     - Accept user input from textarea
     - Send to agent via channel
     - Run agent.step() or agent.run() in background task
     - Display streaming results via ThreadEvent

2. **Add input → agent flow**
   - Create channel: `mpsc::channel<UserMessage>`
   - TUI sends user input to channel
   - Agent loop receives and processes

3. **Complete approval overlay**
   - Wire `ApprovalRequest` from agent to TUI
   - Render approval modal with tool details
   - Handle y/n/a keyboard shortcuts
   - Send approval response back to agent

### Files to Modify
- `crates/uira-tui/src/app.rs` - Main integration
- `crates/uira-tui/src/views/approval.rs` - Approval rendering
- `crates/uira-agent/src/agent.rs` - Add approval channel

---

## Phase 2: Native Goal Verification in Agent

### Problem
Goals exist in `uira-goals` but only used by hook system. Agent doesn't know about goals.

### Tasks
1. **Add goal config to AgentConfig**
   ```rust
   // In uira-agent/src/config.rs
   pub struct AgentConfig {
       // ... existing fields
       pub goals: Option<GoalsConfig>,
       pub auto_verify_goals: bool,
   }
   ```

2. **Create GoalVerifier in agent crate**
   ```rust
   // New file: uira-agent/src/goals.rs
   pub struct GoalVerifier {
       runner: uira_goals::GoalRunner,
       config: GoalsConfig,
   }

   impl GoalVerifier {
       pub async fn verify_all(&self) -> VerificationResult;
       pub fn is_all_passed(&self, result: &VerificationResult) -> bool;
   }
   ```

3. **Integrate into agent completion detection**
   - Before agent declares "done", run goal verification
   - If goals fail, continue with feedback
   - Emit `GoalVerificationFailed` event for TUI

4. **Add goal status to ThreadEvent**
   ```rust
   pub enum ThreadEvent {
       // ... existing
       GoalCheckStarted { goals: Vec<String> },
       GoalCheckResult { name: String, score: f64, target: f64, passed: bool },
       GoalVerificationComplete { all_passed: bool, results: Vec<GoalCheckResult> },
   }
   ```

### Files to Create/Modify
- `crates/uira-agent/src/goals.rs` (NEW)
- `crates/uira-agent/src/config.rs` - Add goals config
- `crates/uira-agent/src/agent.rs` - Integrate verification
- `crates/uira-protocol/src/events.rs` - Add goal events

---

## Phase 3: Native Ralph Mode (Persistent Loop)

### Problem
Ralph is implemented as a hook for Claude plugin. Need native implementation in agent.

### Tasks
1. **Add RalphMode to AgentConfig**
   ```rust
   pub struct RalphConfig {
       pub enabled: bool,
       pub max_iterations: u32,
       pub completion_promise: String,
       pub min_confidence: u32,
       pub require_dual_condition: bool,
       pub circuit_breaker: CircuitBreakerConfig,
   }
   ```

2. **Create RalphController in agent**
   ```rust
   // New file: uira-agent/src/ralph.rs
   pub struct RalphController {
       state: RalphState,
       config: RalphConfig,
   }

   impl RalphController {
       pub fn detect_completion_signals(&self, response: &str) -> CompletionSignals;
       pub fn is_exit_allowed(&self, signals: &CompletionSignals, goals_passed: bool) -> bool;
       pub fn increment_iteration(&mut self);
       pub fn get_continuation_prompt(&self) -> String;
   }
   ```

3. **Integrate into agent run loop**
   ```rust
   // In agent.rs run() method
   loop {
       let response = self.get_response().await?;

       if self.ralph.is_some() {
           let signals = self.ralph.detect_completion_signals(&response);
           let goals_passed = self.verify_goals().await?;

           if self.ralph.is_exit_allowed(&signals, goals_passed) {
               break; // Exit ralph loop
           } else {
               // Continue with feedback
               self.ralph.increment_iteration();
               self.add_message(self.ralph.get_continuation_prompt());
           }
       }
   }
   ```

4. **Add circuit breaker**
   - Track consecutive no-progress iterations
   - Detect output decline patterns
   - Auto-exit on stagnation

5. **Persist ralph state to rollout**
   - Add `RolloutItem::RalphState` variant
   - Save state each iteration
   - Resume ralph mode from rollout

### Files to Create/Modify
- `crates/uira-agent/src/ralph.rs` (NEW)
- `crates/uira-agent/src/config.rs` - Add ralph config
- `crates/uira-agent/src/agent.rs` - Integrate ralph loop
- `crates/uira-agent/src/rollout.rs` - Add ralph state

---

## Phase 4: Native Tool Integration (LSP/AST)

### Problem
Tools are in MCP server, not available to agent's tool orchestrator.

### Tasks
1. **Create native tool providers**
   ```rust
   // New file: uira-agent/src/tools/lsp.rs
   pub struct LspToolProvider {
       client: LspClientImpl,
   }

   impl ToolProvider for LspToolProvider {
       fn tools(&self) -> Vec<ToolSpec>;
       async fn execute(&self, name: &str, input: Value) -> ToolResult;
   }
   ```

2. **Register tools with agent**
   ```rust
   // In agent initialization
   let lsp_tools = LspToolProvider::new()?;
   let ast_tools = AstToolProvider::new()?;

   agent.register_tool_provider(lsp_tools);
   agent.register_tool_provider(ast_tools);
   ```

3. **Available tools to integrate**
   - `lsp_goto_definition`
   - `lsp_find_references`
   - `lsp_symbols`
   - `lsp_diagnostics`
   - `lsp_hover`
   - `lsp_rename`
   - `ast_search`
   - `ast_replace`

4. **Tool filtering per agent**
   - Add `allowed_tools` to AgentConfig
   - Filter tool specs before sending to model

### Files to Create/Modify
- `crates/uira-agent/src/tools/mod.rs` (NEW)
- `crates/uira-agent/src/tools/lsp.rs` (NEW)
- `crates/uira-agent/src/tools/ast.rs` (NEW)
- `crates/uira-agent/src/session.rs` - Register providers

---

## Phase 5: Background Task Management

### Problem
BackgroundManager exists but not integrated with agent spawning.

### Tasks
1. **Add background execution to agent**
   ```rust
   impl Agent {
       pub async fn run_in_background(self) -> BackgroundTask {
           let manager = get_background_manager(config);
           manager.launch(LaunchInput {
               agent: self,
               description: self.config.description.clone(),
               // ...
           }).await
       }
   }
   ```

2. **Create sub-agent spawning**
   ```rust
   // In agent.rs
   pub async fn spawn_sub_agent(&self, config: AgentConfig) -> BackgroundTask {
       let sub_agent = Agent::new(config, self.client.clone());
       sub_agent.run_in_background().await
   }
   ```

3. **Track child agents**
   - Parent agent tracks spawned children
   - Wait for children on completion
   - Cancel children on parent cancel

4. **Concurrency limits**
   - Respect BackgroundTaskConfig limits
   - Queue agents when at capacity
   - Report queue status via events

### Files to Modify
- `crates/uira-agent/src/agent.rs` - Add background methods
- `crates/uira-agent/src/control.rs` - Child tracking

---

## Phase 6: CLI Enhancements

### Tasks
1. **Add ralph mode to CLI**
   ```bash
   uira-agent exec --ralph "implement feature X"
   uira-agent exec --ralph --max-iterations 20 "fix all errors"
   ```

2. **Add goal commands**
   ```bash
   uira-agent goals list
   uira-agent goals check
   uira-agent goals check --name test-coverage
   ```

3. **Add streaming output**
   - Print streamed tokens as they arrive
   - Show tool execution progress
   - Display goal verification status

4. **Session resume with execution**
   ```bash
   uira-agent resume <session-id> --continue
   ```

### Files to Modify
- `crates/uira-cli/src/commands.rs` - Add commands
- `crates/uira-cli/src/main.rs` - Wire commands

---

## Implementation Order

```
Phase 1 (CRITICAL) ──► Phase 2 ──► Phase 3
     │                    │           │
     │                    ▼           ▼
     │               Phase 4 ◄──► Phase 5
     │                    │
     ▼                    ▼
Phase 6 (CLI enhancements)
```

### Priority Order
1. **Phase 1** - TUI must work first (blocks everything)
2. **Phase 2** - Goals (enables objective verification)
3. **Phase 3** - Ralph (enables persistent loops with goals)
4. **Phase 4** - Tools (enhanced capabilities)
5. **Phase 5** - Background (parallel execution)
6. **Phase 6** - CLI (user experience)

---

## Estimated Scope

| Phase | New Files | Modified Files | Complexity |
|-------|-----------|----------------|------------|
| Phase 1 | 0 | 3 | Medium |
| Phase 2 | 1 | 4 | Medium |
| Phase 3 | 1 | 4 | High |
| Phase 4 | 3 | 2 | Medium |
| Phase 5 | 0 | 2 | Medium |
| Phase 6 | 0 | 2 | Low |

**Total: ~5 new files, ~17 modified files**

---

## Success Criteria

- [ ] TUI runs agent and displays streaming output
- [ ] Approval overlay works for tool confirmations
- [ ] Goals verify before agent completion
- [ ] Ralph loop continues until goals pass
- [ ] LSP/AST tools available to agent
- [ ] Sub-agents can be spawned in background
- [ ] CLI supports ralph mode and goal commands
