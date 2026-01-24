![Astrape](./.github/assets/cover.jpg)

<div align="center">
  <h1>Astrape</h1>
  <p>Native Rust-powered multi-agent orchestration for Claude Code</p>
</div>

Astrape (Greek: "lightning") provides high-performance multi-agent orchestration, smart model routing, and AI-assisted development tools as a Claude Code plugin.

## Features

- **32 Specialized Agents** - Architect, Designer, Executor, Explorer, Researcher, and more with tiered variants (Haiku/Sonnet/Opus)
- **Smart Model Routing** - Automatically select the right model based on task complexity
- **Native Performance** - Sub-millisecond keyword detection via Rust NAPI bindings
- **MCP Server** - LSP and AST-grep tools exposed via Model Context Protocol
- **OXC-Powered Tools** - Fast JavaScript/TypeScript linting, parsing, transformation, and minification
- **Comment Checker** - Tree-sitter powered detection of problematic comments/docstrings
- **Background Task Notifications** - Track and notify on background agent completions
- **Skill System** - Extensible skill templates (ultrawork, analyze, plan, search)
- **Git Hooks** - Configurable pre/post commit hooks via `astrape.yml`
- **Goal Verification** - Score-based verification for persistent work loops (ralph mode)

## Git Hooks

Astrape provides a standalone CLI for git hook management. Configure hooks in `astrape.yml`:

```yaml
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
      run: ./target/debug/astrape typos --ai --stage

post-commit:
  commands:
    - name: auto-push
      run: git push origin HEAD
```

Install hooks with:
```bash
astrape install
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
| **System** (Astrape) | Keep worker working until judge says "done" |

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

Define verification goals in `astrape.yml`:

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
astrape goals list           # List all configured goals
astrape goals check          # Run all goals, show results
astrape goals check coverage # Run specific goal by name
```

### Safety Features

- **Max iterations**: Stops after 10 iterations (configurable) to prevent infinite loops
- **Circuit breaker**: Detects stagnation (no progress for 3 iterations) and exits
- **Session expiration**: Ralph state expires after 24 hours
- **Fail-open**: Config errors don't block indefinitely — goals are optional

## Quick Start

### As Claude Code Plugin

```bash
# Clone and build
git clone https://github.com/junhoyeo/Astrape astrape
cd astrape
cargo build --release

# Build NAPI bindings
cd crates/astrape-napi && bun run build

# Copy native module to plugin
cp *.node ../../packages/claude-plugin/native/

# Install plugin in Claude Code
# Add packages/claude-plugin to your Claude Code plugins
```

### Usage in Claude Code

Just talk naturally - Astrape activates automatically:

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
| **Research** | researcher, researcher-low, scientist, scientist-high, scientist-low |
| **Other** | writer, vision, planner, code-reviewer, code-reviewer-low |

### Model Tiers

| Tier | Model | Use Case |
|------|-------|----------|
| LOW | Haiku | Quick lookups, simple tasks |
| MEDIUM | Sonnet | Standard implementation |
| HIGH | Opus | Complex reasoning, architecture |

## Skills

| Skill | Trigger | Description |
|-------|---------|-------------|
| `/astrape:ultrawork` | `ultrawork`, `ulw` | Maximum parallel execution |
| `/astrape:ralph` | `ralph`, `don't stop` | Persistent work loop with goal verification |
| `/astrape:analyze` | `analyze`, `debug` | Deep investigation |
| `/astrape:search` | `search`, `find` | Comprehensive codebase search |
| `/astrape:plan` | `plan` | Strategic planning |
| `/astrape:help` | - | Usage guide |

## MCP Server

The `astrape-mcp` binary exposes development tools via the Model Context Protocol:

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

## OXC Tools

The `astrape-oxc` crate provides fast JavaScript/TypeScript tooling powered by [OXC](https://oxc.rs):

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

# Build NAPI module
cd crates/astrape-napi && bun run build

# Run tests
cargo test --workspace

# Build comment-checker
cargo build --release -p astrape-comment-checker
```

## Architecture

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              Claude Code                                    │
└─────────────────────────────────────────────────────────────────────────────┘
                                     │
          ┌──────────────────────────┼──────────────────────────┐
          ▼                          ▼                          ▼
┌───────────────────┐  ┌───────────────────────┐  ┌───────────────────────────┐
│  astrape-napi     │  │   astrape-mcp-server  │  │  astrape-comment-checker  │
│  (NAPI Bindings)  │  │     (MCP Server)      │  │    (Comment Detection)    │
└───────────────────┘  └───────────────────────┘  └───────────────────────────┘
          │                          │
          │              ┌───────────┴───────────┐
          │              ▼                       ▼
          │    ┌─────────────────┐    ┌─────────────────┐
          │    │  astrape-tools  │    │   astrape-oxc   │
          │    │  (LSP Client)   │    │  (JS/TS Tools)  │
          │    └─────────────────┘    └─────────────────┘
          │
          ├───────────────┬───────────────┬───────────────┬───────────────┐
          ▼               ▼               ▼               ▼               ▼
┌───────────────┐ ┌───────────────┐ ┌───────────────┐ ┌───────────────┐ ┌───────────────┐
│ astrape-hooks │ │astrape-agents │ │astrape-features│ │ astrape-hook │ │ astrape-goals │
│  (22 Hooks)   │ │  (32 Agents)  │ │(Skills/Router)│ │(Keyword Match)│ │  (Goal Verify)│
└───────────────┘ └───────────────┘ └───────────────┘ └───────────────┘ └───────────────┘
        │                                                                       │
        └───────────────────────── Ralph Hook ──────────────────────────────────┘
                                (Stop event verification)

┌─────────────────────────────────────────────────────────────────────────────┐
│                            astrape (CLI)                                    │
│                  Git Hooks · Typo Check · Goals · Dev Tools                 │
└─────────────────────────────────────────────────────────────────────────────┘
```

The plugin uses native Rust NAPI bindings for performance-critical operations:

| Crate | Description |
|-------|-------------|
| **astrape** | Standalone CLI for git hooks and dev tools |
| **astrape-mcp-server** | MCP server with native LSP and AST-grep integration |
| **astrape-oxc** | OXC-powered linter, parser, transformer, minifier |
| **astrape-tools** | LSP client, tool registry, and orchestration utilities |
| **astrape-hook** | Keyword detection and pattern matching |
| **astrape-hooks** | Hook implementations (22 hooks) |
| **astrape-agents** | Agent definitions and prompt loading |
| **astrape-features** | Model routing, skills, state management |
| **astrape-goals** | Score-based goal verification for ralph mode |
| **astrape-napi** | Node.js bindings exposing Rust to the plugin |
| **astrape-comment-checker** | Tree-sitter based comment detection |
| **astrape-core** | Shared types and utilities |
| **astrape-config** | Configuration loading and management |
