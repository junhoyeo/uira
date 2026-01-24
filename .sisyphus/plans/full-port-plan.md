# FULL PORT IMPLEMENTATION PLAN
## oh-my-claudecode (TypeScript) + go-claude-code-comment-checker (Go) → Astrape (Rust)

**Version:** 4.0 - FULL PORT (Accurate Scope + Hybrid Agent Approach)
**Created:** 2025-01-24
**Updated:** 2025-01-24 18:00 (All 24 hooks + 10 features + hybrid agents)
**Status:** READY FOR EXECUTION - START WITH PHASE 0.1 (SDK INTEGRATION)

**v4.0 Critical Updates:**
- ✅ ALL 24 hook modules included (95 files) - was 9 in v3.0
- ✅ Features layer added (10 modules, 44 files) - completely missing in v3.0
- ✅ Hybrid agent approach (keep .md prompts, port infrastructure only)
- ✅ Naming: boulder → astrape, sisyphus → astrape
- ✅ Timeline: 12-15 months (was 9-12) - 1,607 hours (was 1,173)

---

## Executive Summary

This plan details the complete port of:
1. **oh-my-claudecode** (~53K LOC TypeScript) - Multi-agent orchestration library for Claude Code with 32 agents, 8 hooks, 40 skills
2. **go-claude-code-comment-checker** (~1.5K LOC Go) - Tree-sitter based comment detection

**Astrape Transformation:**
- **FROM:** Git hooks manager with oxc linting
- **TO:** Full multi-agent orchestration library (Rust equivalent of oh-my-claudecode)
- **Architecture:** Publishable Rust crate + optional CLI, with napi-rs bindings for Claude Code integration
- **Positioning:** "oh-my-claudecode but in Rust" - same features, native performance

**Actual oh-my-claudecode Scope (v4.0 Plan):**
- **24 hook modules** (95 TypeScript files) - NOT 8!
- **10 feature modules** (44 TypeScript files) - Completely missed in v3.0
- **35 agent prompt .md files** - Keep as markdown (hybrid approach)
- **Tools, MCP, installer** - Full port

**Why oh-my-claudecode (not oh-my-opencode):**
- Targets official Anthropic Claude Code CLI (better long-term support)
- Library architecture (simpler than plugin system)
- 22% smaller codebase (53K vs 68K LOC)
- Cleaner SDK: `@anthropic-ai/claude-agent-sdk` vs `@opencode-ai/plugin`

**Timeline:** 12-15 months (was 9-12 in v3.0) - UNLIMITED TIME AVAILABLE
**Risk Level:** HIGH (SDK integration, massive scope)
**Estimated Total LOC:** ~60-80K Rust (was 50-70K)
**Deliverables:** 
- `astrape` crate (library for Claude Code)
- `astrape-cli` binary (optional)
- npm package with native Rust bindings
- 35 agent .md prompts (embedded at compile time)

---

## Why oh-my-claudecode (Not oh-my-opencode)

| Factor | oh-my-opencode | oh-my-claudecode | Winner |
|--------|----------------|------------------|--------|
| **Target Platform** | OpenCode (community fork) | Claude Code (Anthropic official) | ✅ claudecode |
| **SDK** | `@opencode-ai/plugin` | `@anthropic-ai/claude-agent-sdk` | ✅ claudecode |
| **Architecture** | Plugin with IPC | Library with direct SDK | ✅ claudecode |
| **Size** | 68K LOC, 403 files | 53K LOC, 253 files | ✅ claudecode |
| **Hooks** | 31 hooks | 8 hooks | ✅ claudecode |
| **Agents** | 10 agents | 32 agents (12 base + tiers) | ✅ claudecode |
| **Complexity** | Plugin system, hook bridge | Library exports | ✅ claudecode |
| **Long-term Support** | Community | Anthropic official | ✅ claudecode |

**Decision:** Port oh-my-claudecode. It's 22% smaller, targets official CLI, and has simpler architecture.

---

## Target Crate Architecture

```
astrape/
├── Cargo.toml                          # Workspace root
├── agents/                             # [NEW] 35 .md prompt files (NOT ported - kept as markdown)
│   ├── architect.md
│   ├── explore.md
│   ├── executor.md
│   └── ... (35 total)
│
├── crates/
│   ├── astrape-core/                   # [EXISTS] Core types, events
│   ├── astrape-hook/                   # [EXISTS] Hook execution + keyword detection
│   ├── astrape-claude/                 # [EXISTS] Claude Code shell integration
│   ├── astrape-napi/                   # [EXISTS] Node.js bindings
│   │
│   ├── astrape-comment-checker/        # [NEW] Port of go-claude-code-comment-checker
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── detector.rs             # Comment detection with tree-sitter
│   │   │   ├── languages.rs            # Language registry (30+ languages)
│   │   │   ├── filters/
│   │   │   │   ├── mod.rs
│   │   │   │   ├── bdd.rs              # BDD keyword filter
│   │   │   │   ├── directive.rs        # Linter directive filter
│   │   │   │   └── shebang.rs          # Shebang filter
│   │   │   ├── models.rs               # CommentInfo, CommentType
│   │   │   └── output.rs               # XML/message formatting
│   │   └── Cargo.toml
│   │
│   ├── astrape-hooks/                  # [NEW] ALL 24 hook modules (95 files total!)
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── registry.rs             # Hook registration system
│   │   │   ├── executor.rs             # Hook execution engine
│   │   │   ├── hooks/
│   │   │   │   ├── mod.rs
│   │   │   │   ├── keyword_detector.rs # 1 file
│   │   │   │   ├── todo_continuation.rs # 1 file
│   │   │   │   ├── ralph/              # 5 files (loop, PRD, progress, verifier)
│   │   │   │   ├── comment_checker/    # 4 files
│   │   │   │   ├── think_mode/         # 4 files
│   │   │   │   ├── auto_slash_command/ # 5 files
│   │   │   │   ├── rules_injector/     # 7 files
│   │   │   │   ├── autopilot/          # 13 files!
│   │   │   │   ├── ultrapilot/         # 3 files
│   │   │   │   ├── ultrawork.rs        # 1 file
│   │   │   │   ├── ultraqa.rs          # 1 file
│   │   │   │   ├── omc_orchestrator/   # 3 files
│   │   │   │   ├── learner/            # 12 files!
│   │   │   │   ├── agent_usage_reminder/ # 4 files
│   │   │   │   ├── notepad.rs          # 1 file
│   │   │   │   ├── thinking_block_validator/ # 3 files
│   │   │   │   ├── recovery/           # 7 files
│   │   │   │   ├── empty_message_sanitizer/ # 3 files
│   │   │   │   ├── directory_readme_injector/ # 4 files
│   │   │   │   ├── non_interactive_env/ # 4 files
│   │   │   │   ├── persistent_mode.rs  # 1 file
│   │   │   │   ├── preemptive_compaction/ # 3 files
│   │   │   │   ├── background_notification/ # 2 files
│   │   │   │   └── plugin_patterns.rs  # 1 file
│   │   │   └── state.rs                # Session state management
│   │   └── Cargo.toml
│   │
│   ├── astrape-agents/                 # [NEW] Agent infrastructure (NO individual agent ports)
│   │   ├── src/
│   │   │   ├── lib.rs                  # Main exports
│   │   │   ├── registry.rs             # Agent registration
│   │   │   ├── config.rs               # AgentConfig types (ModelType, AgentCategory, etc.)
│   │   │   ├── prompt_loader.rs        # Load .md prompts with include_str!
│   │   │   ├── tier_builder.rs         # Build -low/-high variants dynamically
│   │   │   ├── tool_restrictions.rs    # Per-agent tool allowlists
│   │   │   └── definitions.rs          # getAgentDefinitions() equivalent
│   │   └── Cargo.toml
│   │   
│   │   # NOTE: Individual agents are JUST .md files in /agents/ directory
│   │   # No Rust code per agent - they're loaded at compile time with include_str!
│   │
│   ├── astrape-tools/                  # [NEW] All 16 tools
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── lsp/                    # LSP client wrapper
│   │   │   ├── ast_grep/               # AST-grep operations
│   │   │   ├── delegate_task/          # Task delegation
│   │   │   ├── background_task/        # Background task tools
│   │   │   ├── session_manager/        # Session management
│   │   │   ├── skill/                  # Skill loading
│   │   │   └── ...
│   │   └── Cargo.toml
│   │
│   ├── astrape-features/               # [NEW] Features layer (10 modules, 44 files)
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── astrape_state/          # Was boulder-state
│   │   │   │   ├── storage.rs          # Plan state persistence
│   │   │   │   ├── types.rs            # AstrapeState, PlanProgress
│   │   │   │   └── constants.rs        # Paths, filenames
│   │   │   ├── notepad_wisdom/         # Learning extraction
│   │   │   ├── model_routing/          # Complexity-based model selection (v2.0)
│   │   │   ├── state_manager/          # Session lifecycle
│   │   │   ├── background_agent/       # Background task orchestration
│   │   │   ├── task_decomposer/        # Task breakdown
│   │   │   ├── delegation_categories/  # Agent routing rules
│   │   │   ├── builtin_skills/         # Skill definitions
│   │   │   ├── context_injector/       # Context file auto-loading
│   │   │   └── verification/           # Quality gates
│   │   └── Cargo.toml
│   │
│   ├── astrape-mcp/                    # [NEW] MCP integrations
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── client.rs               # MCP client
│   │   │   ├── websearch.rs            # Exa integration
│   │   │   ├── context7.rs             # Context7 docs
│   │   │   └── grep_app.rs             # GitHub code search
│   │   └── Cargo.toml
│   │
│   ├── astrape-sdk/                    # [NEW] Claude Agent SDK bindings
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── session.rs              # Session management types
│   │   │   ├── query.rs                # Query options builder
│   │   │   ├── agent.rs                # Agent definition types
│   │   │   └── mcp.rs                  # MCP server config types
│   │   └── Cargo.toml
│   │
│   ├── astrape-config/                 # [NEW] Configuration
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── schema.rs               # Config schema
│   │   │   └── loader.rs               # Config loading
│   │   └── Cargo.toml
│   │
│   └── astrape/                        # [EXISTS] Main CLI (extend)
│       └── src/
│           └── commands/
│               ├── harness.rs          # AI harness commands
│               └── agent.rs            # Agent commands
```

---

## Phase 0: Prototypes & Validation (Weeks 1-4)

**Goal:** Prove the approach works before committing 9+ months

### Phase 0.1: SDK Integration Prototype (CRITICAL - DO THIS FIRST)
**Parallelizable:** NO (HIGHEST RISK - validate before everything else)
**Note:** This phase determines if the entire port is feasible

⚠️ **CRITICAL: This is now Phase 0.1 (was 0.4). If this fails, ABORT ENTIRE PORT.**

- [ ] **0.1.1** Create `astrape-sdk` crate with Rust types
  - Effort: 8 hours
  - Success: Rust types for AgentConfig, QueryOptions, MCP config
  - Types mirror @anthropic-ai/claude-agent-sdk interfaces

- [ ] **0.1.2** Create minimal napi-rs bindings
  - Effort: 24 hours  
  - Dependencies: 0.1.1
  - Success: Can call TypeScript SDK from Rust via napi
  - Test: Import @anthropic-ai/claude-agent-sdk in Rust

- [ ] **0.1.3** Implement createSession equivalent in Rust
  - Effort: 16 hours
  - Dependencies: 0.1.2
  - Success: Rust can create Claude session with agents, MCP, system prompt
  - Test: `astrape_sdk::create_session(agents, mcp_config)` works

- [ ] **0.1.4** Implement agent invocation from Rust
  - Effort: 16 hours
  - Dependencies: 0.1.3
  - Success: Rust triggers agent execution, receives streaming result
  - Test: Invoke "explore" agent, get response

- [ ] **0.1.5** Full integration test: Rust orchestrates TypeScript SDK
  - Effort: 16 hours
  - Dependencies: 0.1.4
  - Success: Complete workflow works (session → agent → result)
  - Test: Run orchestration loop entirely from Rust

**Phase 0.1 Total:** 80 hours (~2 weeks)
**GO/NO-GO:** ⚠️ If Rust cannot integrate with SDK → ABORT ENTIRE PORT ⚠️

#### Phase 0.1 Detailed Execution Guide

**Goal:** Prove Rust can orchestrate TypeScript SDK before investing in anything else.

**Step-by-step:**

1. **Setup (Day 1):**
   - Create `crates/astrape-sdk/` directory
   - Add to workspace in root `Cargo.toml`
   - Add dependencies: `napi = "2.0"`, `napi-derive = "2.0"`, `tokio = "1.0"`
   - Add build dependency: `napi-build = "2.0"`

2. **Type Definitions (Days 1-2):**
   ```rust
   // crates/astrape-sdk/src/types.rs
   #[napi(object)]
   pub struct AgentConfig {
     pub name: String,
     pub model: String,
     pub system_prompt: String,
   }
   
   #[napi(object)]
   pub struct McpServerConfig {
     pub name: String,
     pub command: String,
     pub args: Vec<String>,
   }
   ```

3. **napi-rs Bridge (Days 3-5):**
   ```rust
   // crates/astrape-sdk/src/bridge.rs
   use napi::bindgen_prelude::*;
   
   #[napi]
   pub struct ClaudeAgentBridge {
     create_session_fn: ThreadsafeFunction<SessionConfig, JsObject>,
   }
   
   #[napi]
   impl ClaudeAgentBridge {
     #[napi(constructor)]
     pub fn new(env: Env) -> Result<Self> {
       // Get JS SDK functions
     }
   }
   ```

4. **Session Creation (Days 6-8):**
   - Implement `create_session()` wrapper
   - Test: Create session from Rust with agents and MCP config
   - Verify session object returned

5. **Agent Invocation (Days 9-10):**
   - Implement `invoke_agent()` wrapper
   - Test: Call "explore" agent with prompt
   - Test: Receive streaming response

6. **Integration Test (Days 11-12):**
   ```rust
   #[tokio::test]
   async fn test_full_orchestration_from_rust() {
     let bridge = ClaudeAgentBridge::new();
     let session = bridge.create_session(agents, mcp).await?;
     let result = bridge.invoke_agent("explore", "test").await?;
     assert!(result.contains("expected"));
   }
   ```

**Success Criteria:**
- [ ] Rust can import @anthropic-ai/claude-agent-sdk
- [ ] Rust can create Claude sessions
- [ ] Rust can invoke agents and receive responses
- [ ] No panics or crashes
- [ ] TypeScript SDK runs correctly when called from Rust

**Failure Indicators:**
- Cannot link to TypeScript SDK
- napi-rs bindings fail to compile
- Async operations hang or crash
- Type conversions fail across FFI

**If ANY failure occurs:** STOP and reassess entire approach.

### Phase 0.2: Comment Checker Full Implementation
**Parallelizable:** YES (with 0.1 after SDK validation passes)
**Note:** Full production implementation - integrates as hook in Phase 2

- [ ] **0.2.1** Create `astrape-comment-checker` crate scaffold
  - Effort: 2 hours
  - Success: `cargo build` passes

- [ ] **0.2.2** Port `CommentInfo` and `CommentType` models
  - Effort: 1 hour
  - Success: Types match Go implementation

- [ ] **0.2.3** Implement tree-sitter language registry
  - Effort: 8 hours
  - Dependencies: 0.2.1
  - Success: 30+ languages supported via `tree-sitter` crate

- [ ] **0.2.4** Implement `CommentDetector` with tree-sitter queries
  - Effort: 16 hours
  - Dependencies: 0.2.3
  - Success: Detects comments in Python, JS, Go, Rust

- [ ] **0.2.5** Port BDD, Directive, Shebang filters
  - Effort: 4 hours
  - Dependencies: 0.2.2
  - Success: All filter tests pass

- [ ] **0.2.6** Port CLI with clap, JSON stdin/stdout
  - Effort: 4 hours
  - Dependencies: 0.2.4, 0.2.5
  - Success: `echo '{"tool_name":"Write",...}' | astrape-comment-checker` works

- [ ] **0.2.7** Integration tests matching Go behavior
  - Effort: 8 hours
  - Dependencies: 0.2.6
  - Success: 100% test parity with Go implementation

**Phase 0.2 Total:** 43 hours (~1 week)
**GO/NO-GO:** If comment detection doesn't match Go output 100% → STOP

### Phase 0.3: Hook System Foundation
**Parallelizable:** YES (with 0.1, 0.2)
**Note:** Foundational traits and registry - full implementation in Phase 2

- [ ] **0.3.1** Design Hook trait with async support
  - Effort: 4 hours
  - Success: Trait compiles with async-trait

- [ ] **0.3.2** Implement HookRegistry for registration
  - Effort: 8 hours
  - Dependencies: 0.3.1
  - Success: Can register and lookup hooks

- [ ] **0.3.3** Port one sample hook (keyword-detector)
  - Effort: 12 hours
  - Dependencies: 0.3.2
  - Success: Matches TypeScript behavior for magic keywords

- [ ] **0.3.4** Test hook execution flow
  - Effort: 8 hours
  - Dependencies: 0.3.3
  - Success: Hooks execute in correct order

**Phase 0.3 Total:** 32 hours (~4 days)
**GO/NO-GO:** If hook lifecycle doesn't match → STOP

### Phase 0.4: Agent Prompt System Foundation
**Parallelizable:** YES (with 0.1, 0.2, 0.3)
**Note:** Uses `tera` template engine for dynamic prompt generation

- [ ] **0.4.1** Add tera dependency and test basic templating
  - Effort: 2 hours
  - Success: Can render simple Tera templates

- [ ] **0.4.2** Implement PromptSection enum
  - Effort: 2 hours
  - Success: All 7 sections represented (TASK, EXPECTED OUTCOME, etc.)

- [ ] **0.4.3** Implement PromptBuilder with builder pattern
  - Effort: 8 hours
  - Dependencies: 0.4.2
  - Success: Can build multi-section prompts

- [ ] **0.4.4** Port `buildToolSelectionTable` function
  - Effort: 4 hours
  - Dependencies: 0.4.3
  - Success: Output matches TypeScript

- [ ] **0.4.5** Port `buildDelegationTable` function
  - Effort: 4 hours
  - Dependencies: 0.4.3
  - Success: Output matches TypeScript

- [ ] **0.4.6** Full prompt generation test
  - Effort: 8 hours
  - Dependencies: 0.4.4, 0.4.5
  - Success: Generated prompt matches TypeScript output

**Phase 0.4 Total:** 28 hours (~3-4 days)
**GO/NO-GO:** If prompt output doesn't match → STOP

### Phase 0 Decision Point

| Prototype | Status | Decision |
|-----------|--------|----------|
| **SDK Integration** (0.1) | [ ] Pass [ ] Fail | ⚠️ ABORT if fail |
| Comment Checker (0.2) | [ ] Pass [ ] Fail | Reassess approach |
| Hook System (0.3) | [ ] Pass [ ] Fail | Reassess approach |
| Prompt Builder (0.4) | [ ] Pass [ ] Fail | Reassess approach |

**CRITICAL: 0.1 (SDK Integration) MUST PASS. If it fails, entire port is not feasible.**

**ALL FOUR MUST PASS TO CONTINUE to Phase 1-6.**

---

## Phase 1: Comment Checker Full Port (Week 4)

**Dependencies:** Phase 0.1 complete

**STATUS (2025-01-24):**
- ✅ Phase 1 PARTIALLY COMPLETE
- **Languages implemented:** 15 (target was 30+)
  - Core: python, javascript, typescript, tsx, go, java, c, cpp, rust, ruby, bash
  - Added: csharp, html, xml, zsh
- **Blocked languages (8):** Swift, Kotlin, YAML, TOML, Vue, Svelte, SQL, Lua
  - Reason: tree-sitter grammar crates incompatible with tree-sitter 0.24
  - Resolution: Wait for updated grammar crates or downgrade tree-sitter
- **Tests passing:** 37/37 (26 unit + 11 integration)

- [ ] **1.1** Complete all 30+ language grammars
  - Effort: 16 hours
  - Status: PARTIAL - 15 languages implemented, 8 blocked by tree-sitter version

- [ ] **1.2** Port query templates for each language
  - Effort: 8 hours
  - Success: QueryTemplates map complete

- [ ] **1.3** Port docstring detection
  - Effort: 8 hours
  - Success: Docstrings detected for Python, JS

- [ ] **1.4** Port XML output formatter
  - Effort: 4 hours
  - Success: Output format matches Go

- [ ] **1.5** Add to astrape-napi bindings
  - Effort: 8 hours
  - Success: `checkComments()` callable from Node.js

- [ ] **1.6** Create hook-compatible interface
  - Effort: 8 hours
  - Success: Library exposes PostToolUse hook-compatible API
  - Returns: `HookOutput` with warnings/blocks for problematic comments

- [ ] **1.7** Full integration tests
  - Effort: 8 hours
  - Success: All Go tests ported and passing

- [ ] **1.8** Performance benchmarks
  - Effort: 4 hours
  - Success: Rust is faster than Go

**Phase 1 Total:** 64 hours (~1.5 weeks)

**Critical:** This phase produces a standalone library that Phase 2 will integrate as a hook

---

## Phase 2: Hook Modules - ALL 24 (Weeks 5-11)

**Dependencies:** Phase 0.3 complete, Phase 1 complete

**Reality Check:** oh-my-claudecode has **24 hook modules** (95 TypeScript files), not 8.

### Tier 1: Critical Hooks (Must Have)

- [ ] **2.1** `keyword-detector` (1 file)
  - Effort: 8 hours
  - Success: Detects ultrawork, ultrapilot, ralph, ralplan, swarm, pipeline, eco, plan, autopilot

- [ ] **2.2** `todo-continuation` (1 file)
  - Effort: 8 hours
  - Success: Forces continuation on incomplete todos

- [ ] **2.3** `ralph` (5 files: loop, PRD, progress, verifier)
  - Effort: 40 hours
  - Success: Self-referential loop, PRD integration, memory persistence, architect verification

- [ ] **2.4** `comment-checker` (4 files)
  - Effort: 24 hours
  - Dependencies: Phase 1 (astrape-comment-checker)
  - Success: PostToolUse hook detects problematic comments

- [ ] **2.5** `think-mode` (4 files: detector, switcher, types, index)
  - Effort: 16 hours
  - Success: Enhanced thinking mode activation

- [ ] **2.6** `auto-slash-command` (5 files: detector, executor, constants, types, index)
  - Effort: 24 hours
  - Success: Detects and expands /astrape:* commands

- [ ] **2.7** `rules-injector` (7 files)
  - Effort: 32 hours
  - Success: Conditional rule file injection from project/user config

### Tier 2: Advanced Execution Hooks

- [ ] **2.8** `autopilot` (13 files!)
  - Effort: 80 hours
  - Success: Fully autonomous execution workflow

- [ ] **2.9** `ultrapilot` (3 files)
  - Effort: 16 hours
  - Success: Parallel autopilot (3-5x faster execution)

- [ ] **2.10** `ultrawork` (1 file)
  - Effort: 8 hours
  - Success: Maximum parallel agent execution

- [ ] **2.11** `ultraqa` (1 file)
  - Effort: 8 hours
  - Success: Quality assurance automation

- [ ] **2.12** `omc-orchestrator` (3 files)
  - Effort: 16 hours
  - Success: Orchestrator behavior enforcement

### Tier 3: Intelligence & Learning Hooks

- [ ] **2.13** `learner` (12 files!)
  - Effort: 72 hours
  - Success: Extract reusable insights from sessions

- [ ] **2.14** `agent-usage-reminder` (4 files)
  - Effort: 16 hours
  - Success: Reminds to use appropriate agents

- [ ] **2.15** `notepad` (1 file)
  - Effort: 8 hours
  - Success: Session note-taking integration

### Tier 4: Validation & Quality Hooks

- [ ] **2.16** `thinking-block-validator` (3 files)
  - Effort: 12 hours
  - Success: Validates thinking block structure

- [ ] **2.17** `recovery` (7 files)
  - Effort: 32 hours
  - Success: Edit error recovery and retry logic

- [ ] **2.18** `empty-message-sanitizer` (3 files)
  - Effort: 12 hours
  - Success: Prevents empty message submission

### Tier 5: Context & Environment Hooks

- [ ] **2.19** `directory-readme-injector` (4 files)
  - Effort: 16 hours
  - Success: Auto-injects README context

- [ ] **2.20** `non-interactive-env` (4 files)
  - Effort: 16 hours
  - Success: Handles non-interactive environment detection

- [ ] **2.21** `persistent-mode` (1 file)
  - Effort: 8 hours
  - Success: Persistent session state management

### Tier 6: Optimization Hooks

- [ ] **2.22** `preemptive-compaction` (3 files)
  - Effort: 12 hours
  - Success: Proactive context window management

- [ ] **2.23** `background-notification` (2 files)
  - Effort: 8 hours
  - Success: Background task completion notifications

- [ ] **2.24** `plugin-patterns` (1 file)
  - Effort: 8 hours
  - Success: Plugin pattern detection and handling

**Phase 2 Total:** ~488 hours (~12 weeks / 3 months)

**Critical Reality:** This is 2.8x more than original estimate (176 hours)

---

## Phase 2.5: Features Layer - ALL 10 Modules (Weeks 12-16)

**Dependencies:** Phase 2 complete

**NEW PHASE:** Features layer provides core orchestration infrastructure (44 TypeScript files).

**Naming Convention:** `boulder-*` → `astrape-*`, `sisyphus` → `astrape`

**STATUS (2025-01-24):**
- 1/10 modules complete: `state-manager` ✅
- Crate created: `crates/astrape-features/` (added to workspace)
- Tests: 9/9 passing

### Critical Features

- [ ] **2.5.1** `astrape-state` (was boulder-state) - 4 files
  - Effort: 32 hours
  - Success: Plan state management, progress tracking
  - Types: AstrapeState, PlanProgress, PlanSummary
  - Functions: readAstrapeState, writeAstrapeState, getPlanProgress

- [ ] **2.5.2** `notepad-wisdom` - 3 files
  - Effort: 24 hours
  - Success: Session memory persistence, learning extraction
  - Functions: extractWisdom, getRecentLearnings, formatWisdomForContext

- [ ] **2.5.3** `model-routing` - 5 files (NEW in oh-my-claudecode v2.0!)
  - Effort: 40 hours
  - Success: Complexity-based model selection (Haiku/Sonnet/Opus)
  - Components: signals.rs, scorer.rs, rules.rs, router.rs

- [x] **2.5.4** `state-manager` - 2 files ✅ COMPLETE (2025-01-24)
  - Effort: 16 hours
  - Success: Session state tracking and lifecycle management
  - **Implemented:** `crates/astrape-features/src/state_manager/mod.rs`
  - **Features:** StateManager, SessionState enum, Session struct, thread-safe RwLock
  - **Tests:** 9/9 passing

- [ ] **2.5.5** `background-agent` - 4 files
  - Effort: 32 hours
  - Success: Background task orchestration, concurrency management
  - Already partially planned in Phase 5, merge here

### Supporting Features

- [ ] **2.5.6** `task-decomposer` - 2 files
  - Effort: 16 hours
  - Success: Task breakdown logic for parallel execution

- [ ] **2.5.7** `delegation-categories` - 5 files
  - Effort: 24 hours
  - Success: Agent delegation rules and routing

- [ ] **2.5.8** `builtin-skills` - 3 files
  - Effort: 24 hours
  - Success: Skill definitions (orchestrator, ultrawork, git-master, etc.)

- [ ] **2.5.9** `context-injector` - 5 files
  - Effort: 32 hours
  - Success: Context file injection (AGENTS.md, CLAUDE.md auto-loading)

- [ ] **2.5.10** `verification` - 4 files
  - Effort: 24 hours
  - Success: Architect verification system for quality gates

### Already Planned Elsewhere

- ✅ `magic-keywords` - Covered in Phase 2 (keyword-detector hook)
- ✅ `continuation-enforcement` - Covered in Phase 2 (todo-continuation hook)
- ✅ `auto-update` - Lower priority, Phase 6
- ✅ `delegation-enforcer` - Covered in Phase 2 (omc-orchestrator hook)

**Phase 2.5 Total:** ~264 hours (~6.5 weeks)

**Note:** This phase was COMPLETELY MISSING from v3.0 plan

## Phase 3: Agent System (Weeks 17-21)

**Dependencies:** Phase 0.3 complete, Phase 2 complete

### Agent Infrastructure

- [ ] **3.1** `AgentConfig` and `AgentPromptMetadata` types
  - Effort: 8 hours
  - Success: Types match TypeScript

- [ ] **3.2** Agent registry and factory pattern
  - Effort: 16 hours
  - Success: Can register and create agents

- [ ] **3.3** Tool restrictions system
  - Effort: 8 hours
  - Success: Per-agent tool restrictions work

- [ ] **3.4** `dynamic-agent-prompt-builder` full port
  - Effort: 40 hours
  - Success: Dynamic prompts match TS output

### Agent Strategy: Hybrid Approach (Smart!)

**Key Insight:** Most agents are just **prompt .md files** with NO logic.

**What to Port:**
- ✅ Agent **infrastructure** (registry, factory, config types) → Rust
- ✅ Agent prompts (35 .md files) → **Keep as markdown** (embed at compile time)
- ❌ **DON'T port** individual agent logic (there isn't any - they're just prompts!)

**Structure:**
```
agents/
├── architect.md          # Keep as markdown
├── explore.md            # Keep as markdown
├── executor.md           # Keep as markdown
└── ... (35 total)        # All markdown prompts

crates/astrape-agents/src/
├── registry.rs           # Port: Agent registration
├── config.rs             # Port: AgentConfig types
├── prompt_loader.rs      # Port: Load .md files
├── tier_builder.rs       # Port: Build -low/-high variants
└── lib.rs                # Port: getAgentDefinitions() equivalent
```

### Agent Infrastructure (Port to Rust)

- [ ] **3.5** Agent type system
  - Effort: 16 hours
  - Success: AgentConfig, ModelType, AgentCategory types

- [ ] **3.6** Agent registry and factory
  - Effort: 24 hours
  - Success: Register and create agents dynamically

- [ ] **3.7** Prompt loader (from embedded .md)
  - Effort: 24 hours
  - Success: Load 35 agent prompts with include_str!

- [ ] **3.8** Tier builder (low/medium/high variants)
  - Effort: 32 hours
  - Success: Generate -low (Haiku), -medium (Sonnet), -high (Opus) variants

- [ ] **3.9** Tool restrictions system
  - Effort: 16 hours
  - Success: Per-agent tool allowlists

- [ ] **3.10** `getAgentDefinitions()` equivalent
  - Effort: 24 hours
  - Success: Returns Record<string, AgentConfig> for SDK

### Agent Prompts (35 .md files - NO PORTING, JUST COPY)

**Base Agents (12):**
- architect.md, researcher.md, explore.md, executor.md
- designer.md, writer.md, vision.md, critic.md
- analyst.md, planner.md, qa-tester.md, scientist.md

**Tiered Variants (23):**
- architect-{low,medium}.md
- executor-{low,high}.md
- researcher-low.md
- explore-medium.md
- scientist-low.md
- qa-tester-high.md
- code-reviewer{,-low}.md
- security-reviewer{,-low}.md
- build-fixer{,-low}.md
- tdd-guide-low.md

**Effort:** Just copy 35 .md files (2 hours)

**Phase 3 Total:** ~138 hours (~3.5 weeks) - Dramatically reduced!

**Savings:** 190 hours by keeping prompts as markdown instead of porting

---

## Phase 4: Tools (Weeks 13-16)

**Dependencies:** Phase 3 complete

### Core Tools

- [ ] **4.1** `delegate-task` tool
  - Effort: 80 hours
  - Success: Full category-based delegation

- [ ] **4.2** Background task tools (launch, output, cancel)
  - Effort: 24 hours
  - Success: Background task management

- [ ] **4.3** LSP tools (goto, references, rename, diagnostics)
  - Effort: 40 hours
  - Success: Full LSP integration

- [ ] **4.4** AST-grep tools (search, replace)
  - Effort: 24 hours
  - Success: AST-aware code search

- [ ] **4.5** Session manager tools
  - Effort: 32 hours
  - Success: Session list, read, search, info

- [ ] **4.6** Skill tools
  - Effort: 16 hours
  - Success: Skill loading and execution

- [ ] **4.7** Interactive bash (tmux)
  - Effort: 16 hours
  - Success: Tmux integration

- [ ] **4.8** Remaining tools
  - Effort: 40 hours
  - Success: All tools ported

**Phase 4 Total:** ~272 hours (~7 weeks)

---

## Phase 5: Background Manager & MCP (Weeks 17-20)

**Dependencies:** Phase 4 complete

### Background Agent Manager

- [ ] **5.1** Task lifecycle state machine
  - Effort: 24 hours
  - Success: Pending → Running → Complete/Failed

- [ ] **5.2** Concurrency management
  - Effort: 16 hours
  - Success: Per-provider limits enforced

- [ ] **5.3** Toast notifications integration
  - Effort: 8 hours
  - Success: Task status toasts

- [ ] **5.4** Session tracking
  - Effort: 16 hours
  - Success: Parent-child session relationships

### MCP Integrations

- [ ] **5.5** MCP client implementation
  - Effort: 40 hours
  - Success: Can connect to MCP servers

- [ ] **5.6** Websearch (Exa) integration
  - Effort: 16 hours
  - Success: Web search works

- [ ] **5.7** Context7 docs integration
  - Effort: 16 hours
  - Success: Documentation lookup works

- [ ] **5.8** Grep.app GitHub search
  - Effort: 16 hours
  - Success: GitHub code search works

**Phase 5 Total:** ~152 hours (~4 weeks)

---

## Phase 6: Integration & Polish (Weeks 21-24)

**Dependencies:** All previous phases

### OpenCode Plugin Bridge

- [ ] **6.1** IPC server implementation
  - Effort: 24 hours
  - Success: HTTP server for OpenCode communication

- [ ] **6.2** Event mapping layer
  - Effort: 16 hours
  - Success: OpenCode events → Astrape events

- [ ] **6.3** TypeScript wrapper (minimal)
  - Effort: 8 hours
  - Success: Thin TS layer for OpenCode plugin

### CLI & Configuration

- [ ] **6.4** Full CLI commands
  - Effort: 24 hours
  - Success: All harness commands work

- [ ] **6.5** Configuration system
  - Effort: 16 hours
  - Success: YAML/JSON config loading

- [ ] **6.6** Claude Code installation
  - Effort: 8 hours
  - Success: `astrape install --claude` works

### Testing & Documentation

- [ ] **6.7** Integration test suite
  - Effort: 40 hours
  - Success: Full E2E tests

- [ ] **6.8** Performance benchmarks
  - Effort: 16 hours
  - Success: Benchmarks documented

- [ ] **6.9** Documentation
  - Effort: 24 hours
  - Success: README, API docs complete

- [ ] **6.10** Migration guide
  - Effort: 8 hours
  - Success: Guide from oh-my-opencode

**Phase 6 Total:** ~184 hours (~5 weeks)

---

## Summary

| Phase | Description | Effort | Duration |
|-------|-------------|--------|----------|
| **0** | **Prototypes & Validation (4 phases)** | **183 hours** | **4-5 weeks** |
| 0.1 | SDK Integration (CRITICAL - FIRST) | 80 hours | 2 weeks |
| 0.2 | Comment Checker Implementation | 43 hours | 1 week |
| 0.3 | Hook System Foundation | 32 hours | 4 days |
| 0.4 | Prompt Builder Foundation | 28 hours | 3-4 days |
| **1** | **Comment Checker Completion** | **64 hours** | **1.5 weeks** |
| **2** | **ALL Hook Modules (24 modules, 95 files)** | **488 hours** | **12 weeks** |
| **2.5** | **NEW: Features Layer (10 modules, 44 files)** | **264 hours** | **6.5 weeks** |
| **3** | **Agent System (infrastructure + 35 .md prompts)** | **138 hours** | **3.5 weeks** |
| **4** | **Tools & Features** | **200 hours** | **5 weeks** |
| **5** | **Background & MCP** | **120 hours** | **3 weeks** |
| **6** | **Integration & Polish** | **150 hours** | **4 weeks** |
| **TOTAL** | | **1,607 hours** | **40 weeks** |

**Realistic Timeline with Buffer:** 12-15 months (UNLIMITED TIME AVAILABLE)

**Critical Changes from v3.0:**
- Phase 2: 176 hours → **488 hours** (+312 hours) - All 24 hook modules
- Phase 2.5: **NEW** - 264 hours - Features layer (completely missing before)
- Phase 3: 280 hours → **138 hours** (-142 hours) - Agent prompts stay as .md
- **Total: 1,173 hours → 1,607 hours (+434 hours, +37%)**

**Phase 0.1 is MAKE-OR-BREAK:** If SDK integration fails, stop immediately.

---

## Risk Assessment

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| **SDK integration failure** | **HIGH** | **BLOCKER** | **Phase 0.1 prototype FIRST - ABORT if fails** |
| Prompt generation mismatch | High | Critical | Use `tera` template engine, test in Phase 0.4 |
| napi-rs performance overhead | Medium | High | Benchmark early, accept 10-20% cost |
| Tree-sitter grammar incompatibility | Medium | High | Use tree-sitter-rust crate, test in Phase 0.2 |
| Hook execution order guarantees | Medium | High | Use tokio with strict ordering |
| Async complexity (Rust ↔ Node.js) | High | Medium | Use tokio + napi ThreadsafeFunction |
| Agent prompt loading from .md files | Low | Medium | Embed prompts with include_str! |
| MCP protocol changes | Low | Medium | Abstract MCP client |
| TypeScript SDK updates breaking napi bindings | High | High | Pin SDK version, monitor releases |
| Performance regression vs TypeScript | Low | Low | Benchmark, but expect slight overhead |

**CRITICAL RISK #1:** SDK integration (Phase 0.1) determines project viability. Test FIRST before ANY other work.

**CRITICAL RISK #2:** napi-rs complexity. Wrapper approach adds FFI overhead but is only feasible option.

---

## Success Metrics

1. **Functional Parity**
   - [ ] All 8 hook modules execute correctly (each module has 3-5 files ported)
   - [ ] All 32 agents (12 base + tiers) generate correct prompts
   - [ ] All 40 skills load and inject properly
   - [ ] Comment checker matches Go output 100%
   - [ ] Magic keywords detected: ultrawork, ultrapilot, ralph, ralplan, swarm, pipeline, eco, plan, autopilot
   - [ ] Background task manager handles concurrency correctly
   - [ ] MCP integrations work (Context7, Exa, GitHub, Filesystem)

2. **Performance**
   - [ ] Hook execution < 20ms average (allowing for napi-rs overhead)
   - [ ] Prompt generation < 100ms (tera + napi overhead)
   - [ ] Comment checking faster than Go (pure Rust, no FFI)
   - [ ] SDK calls < 50ms overhead vs pure TypeScript

3. **Reliability**
   - [ ] 0 panics in production
   - [ ] All integration tests passing
   - [ ] Memory safe (no leaks across Rust/Node boundary)
   - [ ] Graceful error handling across FFI

4. **Usability & Distribution**
   - [ ] Compatible with Claude Code CLI (official Anthropic)
   - [ ] Publishable as Rust crate: `cargo add astrape`
   - [ ] Publishable as npm package: `npm install astrape` (with napi-rs native bindings)
   - [ ] Library exports match oh-my-claudecode API surface
   - [ ] Clear migration guide from oh-my-claudecode (TypeScript → Rust)
   - [ ] CLI tool: `astrape install` for Claude Code setup

---

## Go/No-Go Decision Points

| Checkpoint | Criteria | Action if Fail |
|------------|----------|----------------|
| **Phase 0.1 Complete** | **SDK integration works from Rust** | **ABORT ENTIRE PORT IMMEDIATELY** |
| Phase 0.2 Complete | Comment checker 100% parity | Re-evaluate tree-sitter approach |
| Phase 0.3 Complete | Hook system lifecycle works | Re-evaluate async-trait pattern |
| Phase 0.4 Complete | Prompt generation matches TS | Re-evaluate tera vs alternatives |
| Phase 0 Complete | All 4 prototypes pass | Proceed to full implementation |
| Phase 2 Complete | Core hooks work in production | Evaluate scope reduction if needed |
| Phase 3 Complete | Agents generate correct prompts | Assess template engine performance |
| Phase 6 Start | Integration test suite passes | Final go/no-go |

**CRITICAL:** Phase 0.1 (SDK integration) is the HIGHEST-RISK checkpoint. Test FIRST.

**If Phase 0.1 fails:** Rust cannot call @anthropic-ai/claude-agent-sdk via napi-rs → entire port is not feasible → STOP.

---

## Astrape as a Library (Not Just a CLI)

**Critical Understanding:** Astrape is transforming from a git hooks manager into a full multi-agent orchestration library.

### Library Architecture

```rust
// Main library export (astrape crate)
pub use astrape_agents::{AgentRegistry, AgentDefinition, invoke_agent};
pub use astrape_hooks::{HookRegistry, HookExecutor};
pub use astrape_sdk::{create_session, SessionConfig};
pub use astrape_tools::{lsp_tools, ast_tools, delegate_task};
pub use astrape_background::{BackgroundManager, spawn_task};
pub use astrape_mcp::{McpServerConfig, default_mcp_servers};

// Similar to oh-my-claudecode's exports:
// export { loadConfig, getAgentDefinitions, omcSystemPrompt }
// export { lspTools, astTools, allCustomTools }
```

### Usage Patterns

**As a Library (Primary):**
```rust
use astrape::{AgentRegistry, create_session, HookExecutor};

let agents = AgentRegistry::default();
let session = create_session(agents.get_definitions()).await?;
let result = invoke_agent("explore", "find auth logic").await?;
```

**As a CLI (Optional):**
```bash
astrape install              # Install to Claude Code
astrape agent invoke explore "find auth"
astrape hook run keyword-detector
```

**As an npm Package (via napi-rs):**
```typescript
import { createSession, invokeAgent } from 'astrape';

const session = await createSession(agentDefinitions);
const result = await invokeAgent('explore', 'find auth logic');
```

### Distribution

1. **Rust Crate:** `cargo add astrape`
2. **npm Package:** `npm install astrape` (includes native bindings)
3. **CLI Binary:** `cargo install astrape-cli`

## Key Architectural Decisions

### 1. Library-First Design (NEW)
**Decision:** Astrape is a library that exports orchestration functionality
**Rationale:**
- oh-my-claudecode is a library, not a standalone app
- Rust crate can be consumed by other Rust projects
- napi-rs enables npm distribution with native bindings
- CLI is a thin wrapper around library functions

### 2. Template Engine for Prompts
**Decision:** Use `tera` crate for dynamic prompt generation
**Rationale:** 
- oh-my-claudecode loads prompts from `/agents/*.md` files
- Rust string formatting is verbose compared to TypeScript template literals
- `tera` provides Jinja2-like templating with good performance
- Alternative considered: `handlebars-rust` (rejected: heavier, less ergonomic)

### 3. SDK Integration Strategy (CRITICAL)
**Decision:** napi-rs bindings to call TypeScript SDK from Rust
**Rationale:**
- @anthropic-ai/claude-agent-sdk has no Rust equivalent
- Creating Rust SDK from scratch = 6+ months additional work
- napi-rs proven for Rust ↔ Node.js integration (see research findings)
- Accept 10-20% performance overhead for feasibility
- **Wrapper pattern** recommended (not full rewrite)

### 4. Agent Prompt Storage (HYBRID APPROACH)
**Decision:** Keep 35 .md prompts as markdown, embed with `include_str!` at compile time
**Rationale:**
- Agent prompts are JUST TEXT - no logic to port
- oh-my-claudecode structure: `agents/*.md` files with YAML frontmatter
- Rust approach: Copy .md files, embed with `include_str!("../../agents/architect.md")`
- Only port agent **infrastructure** (registry, config, tier builder)
- **Huge savings:** 190 hours (don't port individual agents)

**What to Port:**
- ✅ Agent registry and factory → Rust
- ✅ AgentConfig types → Rust
- ✅ Prompt loader → Rust (uses include_str!)
- ✅ Tier builder (-low/-high variants) → Rust
- ❌ Individual agents → **Keep as .md** (no Rust code needed!)

**Agent .md file structure:**
```markdown
---
name: architect
model: opus
tools: [Read, Glob, Grep, WebSearch]
---

You are an architecture and debugging advisor...
```

**Rust loads it:**
```rust
const ARCHITECT_PROMPT: &str = include_str!("../../agents/architect.md");
```

### 5. Hook System
**Decision:** Async trait-based hooks with registry pattern
**Rationale:**
- Matches oh-my-claudecode's hook architecture
- `async-trait` crate handles async in traits
- Registry pattern allows dynamic hook registration

---

## Next Steps

1. ✅ **Plan updated to target oh-my-claudecode (v3.0)**
2. ✅ **Architecture clarified: Astrape = Rust library (oh-my-claudecode equivalent)**
3. ✅ **Phase 0 reordered: SDK integration FIRST (0.1, not 0.4)**
4. **START Phase 0.1** - SDK Integration Prototype (CRITICAL, HIGHEST RISK)
5. **If 0.1 passes:** Parallel Phase 0.2, 0.3, 0.4 (comment checker, hooks, prompts)
6. **Go/No-Go decision after Phase 0.1** (make-or-break)

**EXECUTION ORDER:**
1. Phase 0.1 (SDK) - 2 weeks - **DO THIS FIRST**
2. If Pass → Phase 0.2/0.3/0.4 in parallel - 2 weeks
3. If All Pass → Phase 1-6 (full implementation) - 7-10 months

**READY TO BEGIN EXECUTION - START WITH PHASE 0.1 (SDK INTEGRATION)**

---

## Plan Validation Checklist

Before starting execution, verify:

- [x] **Target confirmed:** oh-my-claudecode (not oh-my-opencode)
- [x] **Architecture understood:** Astrape = Rust library (exports like oh-my-claudecode)
- [x] **Phase 0 ordering correct:** SDK integration is 0.1 (FIRST), not 0.4 (last)
- [x] **Risk prioritized:** SDK integration tested before any other work
- [x] **Abort condition clear:** If Phase 0.1 fails, stop entire port
- [x] **Distribution plan:** Rust crate + npm package + CLI binary + 35 .md prompts
- [x] **Scope accurate:** 24 hook modules (not 9), 10 feature modules (not 0)
- [x] **Timeline realistic:** 12-15 months (was 9-12) with unlimited time
- [x] **Effort estimate:** 1,607 hours (was 1,173) - 37% increase from v3.0
- [x] **Hybrid approach:** Agent prompts stay as .md, only port infrastructure
- [x] **Naming convention:** boulder → astrape, sisyphus → astrape

## Critical Reminders

1. **DO Phase 0.1 FIRST** - SDK integration is make-or-break
2. **Library, not just CLI** - Design exports like oh-my-claudecode
3. **napi-rs is complex** - Accept 10-20% performance overhead
4. **Full scope understood** - 24 hooks + 10 features + 35 agent prompts
5. **Hybrid agent approach** - Keep .md prompts, port infrastructure only
6. **Naming convention** - boulder → astrape, sisyphus → astrape
7. **Match TypeScript API** - createSisyphusSession → createAstrapeSession

---

## v4.0 Changes from v3.0

**Major Updates:**
1. ✅ Phase 2: 176 hours → **488 hours** - All 24 hook modules (was 9)
2. ✅ Phase 2.5: **NEW** - 264 hours - Features layer (completely missing)
3. ✅ Phase 3: 280 hours → **138 hours** - Hybrid agent approach (keep .md)
4. ✅ Naming: boulder-state → astrape-state, sisyphus → astrape
5. ✅ Total: 1,173 hours → **1,607 hours** (+37% more accurate)
6. ✅ Timeline: 9-12 months → **12-15 months** (realistic)

**What This Means:**
- v3.0 underestimated hooks by 2.8x (missed 16 modules)
- v3.0 completely missed features layer (44 files)
- v4.0 now MATCHES actual oh-my-claudecode structure
- Smart hybrid approach saves 190 hours on agents

---

**END OF PLAN - VERSION 4.0**
