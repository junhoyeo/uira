![Uira](./.github/assets/cover.jpg)

<div align="center">
  <h1>Uira</h1>
  <p>Lightning-fast multi-agent orchestration with native Rust performance</p>
</div>

> **Uira** (Māori: "lightning") — Native Rust-powered multi-agent orchestration for Claude Code. Sub-millisecond keyword detection and high-performance LSP/AST tools. Route different agents to different models. Mix Claude, GPT, Gemini—orchestrate by purpose, not by provider.

## Features

- **32 Specialized Agents** - Architect, Designer, Executor, Explorer, Librarian, and more with tiered variants (Haiku/Sonnet/Opus)
- **Smart Model Routing** - Automatically select the right model based on task complexity
- **Native Performance** - Sub-millisecond keyword detection via Rust NAPI bindings
- **MCP Server** - LSP and AST-grep tools exposed via Model Context Protocol
- **OXC-Powered Tools** - Fast JavaScript/TypeScript linting, parsing, transformation, and minification
- **Comment Checker** - Tree-sitter powered detection of problematic comments/docstrings
- **Background Task Notifications** - Track and notify on background agent completions
- **Skill System** - Extensible skill templates (ultrawork, analyze, plan, search)
- **Git Hooks** - Configurable pre/post commit hooks via `uira.yml`
- **Goal Verification** - Score-based verification for persistent work loops (ralph mode)

## Quick Start

### As Claude Code Plugin

```bash
# Clone and build
git clone https://github.com/junhoyeo/Uira uira
cd uira
cargo build --release

# Build NAPI bindings (automatically syncs to plugin)
cd crates/uira-napi && bun run build

# Install plugin in Claude Code
# Add packages/uira/claude-plugin to your Claude Code plugins
```

### Usage in Claude Code

Just talk naturally - Uira activates automatically:

```
"ultrawork: fix all TypeScript errors"    → Maximum parallel execution
"analyze why this test fails"             → Deep investigation mode
"search for authentication handling"      → Comprehensive codebase search
"plan the new API design"                 → Strategic planning interview
```

## Agents

| Category | Agents |
|----------|--------|
| **Analysis** | architect, architect-medium, architect-low, analyst, critic |
| **Execution** | executor, executor-high, executor-low |
| **Search** | explore |
| **Design** | designer, designer-high, designer-low |
| **Testing** | qa-tester, qa-tester-high, tdd-guide, tdd-guide-low |
| **Security** | security-reviewer, security-reviewer-low |
| **Build** | build-fixer, build-fixer-low |
| **Research** | librarian, scientist, scientist-high, scientist-low |
| **Other** | writer, vision, planner, code-reviewer, code-reviewer-low |

### Model Tiers

| Tier | Model | Use Case |
|------|-------|----------|
| LOW | Haiku | Quick lookups, simple tasks |
| MEDIUM | Sonnet | Standard implementation |
| HIGH | Opus | Complex reasoning, architecture |

### Custom Model Routing

Some agents can be configured to use non-Anthropic models via `uira.yml`:

```yaml
agents:
  librarian:
    model: "opencode/big-pickle"
  explore:
    model: "opencode/gpt-5-nano"
```

**Important:** Agents with custom model routing must use the `delegate_task` MCP tool instead of the built-in Task tool. The Claude Code plugin automatically blocks Task tool calls for these agents and provides guidance to use `delegate_task`.

## Skills

| Skill | Trigger | Description |
|-------|---------|-------------|
| `/uira:ultrawork` | `ultrawork`, `ulw` | Maximum parallel execution |
| `/uira:analyze` | `analyze`, `debug` | Deep investigation |
| `/uira:search` | `search`, `find` | Comprehensive codebase search |
| `/uira:plan` | `plan` | Strategic planning |
| `/uira:help` | - | Usage guide |

### Keyword Modes

| Keyword | Description |
|---------|-------------|
| `ralph`, `don't stop` | Persistent work loop with goal verification (see [Ralph Mode](#ralph-mode--goal-verification)) |

## Architecture

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              Claude Code                                    │
└─────────────────────────────────────────────────────────────────────────────┘
                                     │
                   ┌────────────────┐
                   ▼                ▼
┌───────────────────┐  ┌───────────────────────┐
│  uira-napi     │  │   uira-mcp-server  │
│  (NAPI Bindings)  │  │     (MCP Server)      │
└───────────────────┘  └───────────────────────┘
          │                          │
          │              ┌───────────┴───────────┐
          │              ▼                       ▼
          │    ┌─────────────────┐    ┌─────────────────┐
          │    │  uira-tools  │    │   uira-oxc   │
          │    │  (LSP Client)   │    │  (JS/TS Tools)  │
          │    └─────────────────┘    └─────────────────┘
          │
          └───────┬─────────────┬─────────────┬
                  ▼             ▼             ▼
    ┌───────────────────┐ ┌───────────┐ ┌─────────────────┐
    │   uira-hooks   │ │  agents   │ │ uira-features│
    │ (Hooks + Goals)   │ │(32 Agents)│ │ (Skills/Router) │
    └───────────────────┘ └───────────┘ └─────────────────┘
              │
              └── Ralph Hook (Stop event → goal verification)

┌─────────────────────────────────────────────────────────────────────────────┐
│                            uira (CLI)                                    │
│                  Git Hooks · Typo Check · Goals · Dev Tools                 │
└─────────────────────────────────────────────────────────────────────────────┘
```

The plugin uses native Rust NAPI bindings for performance-critical operations:

| Crate | Description |
|-------|-------------|
| **uira** | Standalone CLI for git hooks and dev tools |
| **uira-mcp-server** | MCP server with native LSP and AST-grep integration |
| **uira-oxc** | OXC-powered linter, parser, transformer, minifier |
| **uira-tools** | LSP client, tool registry, and orchestration utilities |
| **uira-keywords** | Keyword detection for agent activation |
| **uira-hooks** | Hook implementations (22 hooks) |
| **uira-agents** | Agent definitions and prompt loading |
| **uira-features** | Model routing, skills, state management |
| **uira-goals** | Score-based goal verification for ralph mode |
| **uira-napi** | Node.js bindings exposing Rust to the plugin |
| **uira-comment-checker** | Tree-sitter based comment detection |
| **uira-core** | Shared types and utilities |
| **uira-config** | Configuration loading and management |

### Agent Harness Crates

The native agent harness provides a standalone agent execution environment:

| Crate | Description |
|-------|-------------|
| **uira-protocol** | Shared types, events, streaming chunks, and protocol definitions |
| **uira-providers** | Model provider clients (Anthropic, OpenAI) with streaming support |
| **uira-agent** | Core agent loop with state machine, session persistence, and streaming |
| **uira-sandbox** | Platform-native sandboxing (macOS sandbox-exec, Linux Landlock) |
| **uira-context** | Context management and conversation history |
| **uira-tui** | Ratatui-based terminal UI with approval overlay and syntax highlighting |
| **uira-cli** | Command-line interface with session management and multi-provider support |

## Model Routing Architecture

`delegate_task` provides multi-provider model routing:

- **Anthropic models** (`claude-*`, `anthropic/*`) → Direct via `claude-agent-sdk-rs`
- **External models** (OpenAI, Google, etc.) → OpenCode session API (`POST /session/{id}/message`)
- **OpenCode routing** - Automatically routes to ANY configured provider
- **Streaming support** - Full SSE streaming via OpenCode

### OpenCode Configuration

Configure OpenCode server settings in `uira.yml`:

```yaml
opencode:
  host: "127.0.0.1"      # Server host (default: 127.0.0.1)
  port: 4096             # Server port (default: 4096)
  timeout_secs: 120      # Request timeout (default: 120)
  auto_start: true       # Auto-start server (default: true)
```

**Auto-Start Behavior:**

The OpenCode server automatically starts before the first `delegate_task` call when `auto_start: true`. The MCP server:
1. Checks if OpenCode is running via health check (`GET /health`)
2. If not running, spawns `opencode serve` in the background
3. Waits up to 15 seconds for the server to become ready
4. Proceeds with agent spawning once healthy

**Environment Variable Overrides:**

Environment variables take precedence over config file values:

| Variable | Overrides | Default |
|----------|-----------|---------|
| `OPENCODE_HOST` | `opencode.host` | 127.0.0.1 |
| `OPENCODE_PORT` | `opencode.port` | 4096 |
| `OPENCODE_TIMEOUT_SECS` | `opencode.timeout_secs` | 120 |

**Example:**
```bash
OPENCODE_PORT=8080 uira-mcp  # Use port 8080 instead of 4096
```

### Agent-Based Model Routing

Route different agents to different models based on purpose, not provider:

```yaml
# uira.yml
agents:
  explore:
    model: "opencode/gpt-5-nano"  # Fast, cheap model for exploration
  architect:
    model: "openai/gpt-4.1"       # Powerful model for architecture
  executor:
    model: "openai/gpt-4.1-mini"  # Balanced model for execution
```

## Git Hooks

Uira provides a standalone CLI for git hook management. Configure hooks in `uira.yml`:

```yaml
typos:
  ai:
    model: anthropic/claude-sonnet-4-20250514

pre-commit:
  parallel: false  # fmt must run first before clippy
  commands:
    - name: fmt
      run: |
        staged=$(git diff --cached --name-only --diff-filter=ACM | grep '\.rs$' || true)
        [ -z "$staged" ] && exit 0
        echo "$staged" | xargs cargo fmt --
        echo "$staged" | xargs git add
    - name: clippy
      run: cargo clippy -- -D warnings
    - name: typos
      run: ./target/debug/uira typos --ai --stage

post-commit:
  commands:
    - name: auto-push
      run: git push origin HEAD
```

Install hooks with:
```bash
uira install
```

## Ralph Mode & Goal Verification

Ralph mode is a persistent work loop that keeps Claude working until tasks are truly complete. Combined with goal verification, it ensures objective completion criteria are met before exiting.

### Philosophy

**The Problem**: AI agents in persistent loops tend to declare victory prematurely. They say "fixed" without verifying. They drift from goals. They break things they previously fixed.

**The Solution**: Separate the judge from the worker.

| Role | Responsibility |
|------|----------------|
| **Worker** (AI) | Write code, try fixes, iterate |
| **Judge** (Script) | Measure reality objectively |
| **System** (Uira) | Keep worker working until judge says "done" |

An agent can *think* it's done. A test coverage report doesn't hallucinate. A pixel-diff script doesn't confabulate. **Numbers don't lie.**

The goal system is intentionally simple: `run command → parse stdout → compare to threshold`. This "dumb pipe" approach means infinite flexibility — any measurable property becomes a goal. Write a script, output a number, done.

### How It Works

```
┌─────────────────────────────────────────────────────────────────┐
│                        Ralph Mode Flow                          │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
                    ┌─────────────────┐
                    │  User triggers  │
                    │  "ralph: task"  │
                    └────────┬────────┘
                             │
                             ▼
              ┌──────────────────────────────┐
              │   Claude works on task...    │◄────────────────┐
              └──────────────┬───────────────┘                 │
                             │                                 │
                             ▼                                 │
              ┌──────────────────────────────┐                 │
              │  Claude signals completion:  │                 │
              │  <promise>TASK COMPLETE</promise>              │
              │  or EXIT_SIGNAL: true        │                 │
              └──────────────┬───────────────┘                 │
                             │                                 │
                             ▼                                 │
              ┌──────────────────────────────┐                 │
              │    Run Goal Verification     │                 │
              │  (execute configured cmds)   │                 │
              └──────────────┬───────────────┘                 │
                             │                                 │
                    ┌────────┴────────┐                        │
                    ▼                 ▼                        │
            ┌─────────────┐   ┌─────────────┐                  │
            │ All goals   │   │ Goals fail  │                  │
            │ pass ✓      │   │ or missing  │──────────────────┘
            └──────┬──────┘   └─────────────┘
                   │            (continue loop with feedback)
                   ▼
            ┌─────────────┐
            │  Exit loop  │
            │  Task done! │
            └─────────────┘
```

### Activation

Ralph mode activates when Claude detects keywords in your prompt:

```
"ralph: implement the auth system"
"don't stop until tests pass"
"keep working on this feature"
```

### Goal Configuration

Define verification goals in `uira.yml`:

```yaml
goals:
  auto_verify: true              # Enable automatic verification on completion
  goals:
    - name: test-coverage
      command: ./scripts/coverage.sh
      target: 80.0               # Must output score >= 80
      timeout_secs: 60           # Optional, default 30
      
    - name: build-check
      command: cargo build --release && echo 100
      target: 100.0
      
    - name: lint-score
      command: ./scripts/lint-score.sh
      target: 95.0
      enabled: false             # Temporarily disabled
      
    - name: e2e-tests
      command: npm test
      workspace: packages/app    # Run in subdirectory
      target: 100.0
```

**Goal command requirements:**
- Must output a single number (0-100) to stdout
- Last numeric line is used as the score
- Non-zero exit code = goal failure

### Exit Conditions

Ralph mode exits only when ALL conditions are met:

| Condition | Description |
|-----------|-------------|
| **Completion Intent** | Claude outputs `<promise>TASK COMPLETE</promise>` or `EXIT_SIGNAL: true` |
| **Goals Pass** | All enabled goals meet their targets (hard gate) |
| **Confidence Threshold** | Combined signal confidence ≥ 50% (configurable) |

If goals fail, Claude receives detailed feedback and continues working:

```
[RALPH VERIFICATION FAILED - Iteration 3/10]

Goals not met:
  ✗ test-coverage: 72.5/80.0
  ✓ build-check: 100.0/100.0

Continue working to meet all goals, then signal completion again.
```

### Example Use Cases

| Use Case | Command | Target |
|----------|---------|--------|
| Pixel-perfect UI | `bun run pixel-diff.ts` | 99.9 |
| Test coverage | `jest --coverage --json \| jq '.total.lines.pct'` | 80 |
| Lighthouse perf | `lighthouse --output=json \| jq '.categories.performance.score * 100'` | 90 |
| Bundle size budget | `./scripts/bundle-score.sh` | 100 |
| Type coverage | `type-coverage --json \| jq '.percent'` | 95 |
| Zero console errors | `playwright test --reporter=json \| jq '.suites[].specs[].ok' \| grep -c true` | 100 |
| API response time | `./scripts/latency-check.sh` | 95 |
| Accessibility | `pa11y --reporter=json \| jq '100 - (.issues \| length)'` | 100 |

### CLI Commands

```bash
uira goals list           # List all configured goals
uira goals check          # Run all goals, show results
uira goals check coverage # Run specific goal by name
```

### Safety Features

- **Max iterations**: Stops after 10 iterations (configurable) to prevent infinite loops
- **Circuit breaker**: Detects stagnation (no progress for 3 iterations) and exits
- **Session expiration**: Ralph state expires after 24 hours
- **Fail-open**: Config errors don't block indefinitely — goals are optional

## MCP Server

The `uira-mcp` binary exposes development tools via the Model Context Protocol:

### LSP Tools
| Tool | Description |
|------|-------------|
| `lsp_goto_definition` | Jump to symbol definition |
| `lsp_find_references` | Find all references to a symbol |
| `lsp_symbols` | List symbols in a file or workspace |
| `lsp_diagnostics` | Get errors and warnings |
| `lsp_hover` | Get type info and documentation |
| `lsp_rename` | Rename a symbol across files |

### AST Tools
| Tool | Description |
|------|-------------|
| `ast_search` | Search code patterns with ast-grep |
| `ast_replace` | Search and replace code patterns |

### Agent Tools
| Tool | Description |
|------|-------------|
| `delegate_task` | Delegate task to agent with automatic model routing via OpenCode |
| `background_output` | Get the output from a background task |
| `background_cancel` | Cancel a running background task or all background tasks |

#### delegate_task

Delegates a task to a specialized agent with automatic model routing via OpenCode. Routes requests to the configured model for that agent.

**Parameters:**
- `agent` (required): Agent name (e.g., `librarian`, `explore`, `architect`)
- `prompt` (required): The task for the agent to execute
- `model` (optional): Override model - full model ID (e.g., `anthropic/claude-sonnet-4-20250514`, `openai/gpt-4`)
- `allowedTools` (optional): List of tools to allow
- `maxTurns` (optional): Maximum turns before stopping (default: 10)
- `runInBackground` (optional): If true, runs the agent in the background and returns a task_id immediately. Use `background_output` to get results. Default: false

**Model Routing:**
- `claude-*` or `anthropic/*` → Direct Anthropic API
- All other models → OpenCode session API

**Example:**
```json
{
  "agent": "librarian",
  "prompt": "Find examples of JWT authentication in Express.js"
}
```

Routes to the configured model for that agent (e.g., `librarian` → `opencode/big-pickle` via OpenCode).

**Background Execution Example:**
```json
{
  "agent": "explore",
  "prompt": "Search for authentication patterns",
  "runInBackground": true
}
```

Returns immediately with a task ID:
```json
{
  "taskId": "bg_abc123def",
  "status": "running",
  "message": "Task started in background. Use background_output to get results."
}
```

#### background_output

Get the output from a background task. Returns immediately if complete, otherwise shows current status.

**Parameters:**
- `taskId` (required): The task ID returned from `delegate_task` with `runInBackground=true`
- `block` (optional): If true, blocks until the task completes (max 120s by default). Default: false
- `timeout` (optional): Timeout in seconds when blocking. Default: 120

**Example:**
```json
{
  "taskId": "bg_abc123def"
}
```

**Blocking Example:**
```json
{
  "taskId": "bg_abc123def",
  "block": true,
  "timeout": 60
}
```

#### background_cancel

Cancel a running background task or all background tasks.

**Parameters:**
- `taskId` (optional): The task ID to cancel
- `all` (optional): If true, cancels ALL running background tasks. Default: false

**Example (single task):**
```json
{
  "taskId": "bg_abc123def"
}
```

**Example (all tasks):**
```json
{
  "all": true
}
```

## OXC Tools

The `uira-oxc` crate provides fast JavaScript/TypeScript tooling powered by [OXC](https://oxc.rs):

### Linter
10 built-in rules: `no-console`, `no-debugger`, `no-alert`, `no-eval`, `no-var`, `prefer-const`, `no-unused-vars`, `no-empty-function`, `no-duplicate-keys`, `no-param-reassign`

### Parser
Returns structured AST information including imports, exports, functions, classes, and variables.

### Transformer
Transpile TypeScript and JSX to JavaScript with configurable target ES version.

### Minifier
Minify JavaScript with optional mangling and compression, returning compression stats.

## Hooks

| Event | Handler |
|-------|---------|
| `UserPromptSubmit` | Keyword detection, background notifications |
| `PreToolUse` | README injection, tool validation |
| `PostToolUse` | Comment checker, background task tracking |
| `SessionStart` | State initialization |
| `Stop` | Continuation control |

## Development

```bash
# Build all crates
cargo build --workspace --release

# Build NAPI module (automatically syncs to plugin)
cd crates/uira-napi && bun run build

# Run tests
cargo test --workspace

# Build comment-checker
cargo build --release -p uira-comment-checker
```
