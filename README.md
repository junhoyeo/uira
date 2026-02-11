![Uira](./.github/assets/cover.jpg)

<div align="center">
  <h1>Uira</h1>
  <p>Native Rust AI agent with multi-provider orchestration</p>
</div>

> **Uira** (Māori: "lightning") — A standalone, native AI coding agent built in Rust. Mix Claude, GPT, Gemini—orchestrate by purpose, not by provider. Platform-native sandboxing, session persistence, and a beautiful TUI.

## Features

- **Standalone Native CLI** - Zero-dependency Rust binary, no Node.js required
- **Multi-Provider Support** - Anthropic, OpenAI, and any OpenCode-compatible provider
- **Smart Model Routing** - Route different tasks to different models automatically
- **Platform-Native Sandboxing** - macOS sandbox-exec, Linux Landlock
- **Session Persistence** - JSONL rollout for debugging, replay, and resume
- **Streaming** - Newline-gated streaming with real-time output
- **Ratatui TUI** - Beautiful terminal interface with approval overlays
- **MCP Server** - LSP and AST-grep tools via Model Context Protocol
- **Web & Code Search** - Exa web search, code context, and GitHub code search via hosted MCP endpoints
- **Git Hooks** - Configurable pre/post commit hooks via `uira.yml`
- **Goal Verification** - Score-based verification for persistent work loops
- **AI-Assisted Workflows** - Typos, diagnostics, and comments with AI decision-making

## Quick Start

```bash
# Build from source
git clone https://github.com/junhoyeo/uira
cd uira
cargo build --release

# Run the agent
./target/release/uira-agent

# Or install globally
cargo install --path crates/uira-cli
```

### Environment Setup

```bash
# Set your API key
export ANTHROPIC_API_KEY="sk-ant-..."

# Or use OpenAI
export OPENAI_API_KEY="sk-..."
```

## Usage

```bash
# Interactive TUI mode
uira-agent

# Execute a single task
uira-agent exec "Fix the TypeScript errors in src/"

# Resume a previous session
uira-agent --resume ~/.uira/sessions/abc123.jsonl

# Run with specific model
uira-agent --model claude-sonnet-4-20250514
```

## Authentication

Uira supports OAuth authentication for multiple providers:

```bash
# Login to a provider
uira-agent auth login anthropic
uira-agent auth login openai
uira-agent auth login google

# Check authentication status
uira-agent auth status

# Logout from a provider
uira-agent auth logout anthropic
```

### OAuth Flow

- **Anthropic**: Uses code-copy flow. Opens browser → authorize → copy the code → paste in terminal
- **OpenAI/Google**: Uses device code flow with automatic polling

Credentials are securely stored in `~/.uira/auth.json`.

## TUI Commands & Shortcuts

### Slash Commands

| Command | Description |
|---------|-------------|
| `/help`, `/h`, `/?` | Show available commands |
| `/models` | Open model selector (keyboard-driven) |
| `/model <name>` | Switch to a specific model |
| `/fork [name]` | Create a branch from the current session point |
| `/switch <branch>` | Switch to another session branch |
| `/branches` | List available session branches |
| `/tree` | Show session branch tree |
| `/review` | Review staged changes |
| `/review <file>` | Review changes for a specific file |
| `/review HEAD~1` | Review a specific commit |
| `/theme` | List available TUI themes |
| `/theme <name>` | Switch TUI theme |
| `/share [--public] [--description <text>]` | Share current session as a GitHub Gist |
| `/clear` | Clear chat history |
| `/status`, `/auth` | Show connection status |
| `/exit`, `/quit`, `/q` | Exit the application |

### Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `Enter` | Send message |
| `↑` / `↓` | Scroll through messages |
| `←` / `→` | Move cursor in input |
| `Ctrl+G` | Open external editor for composing input |
| `Ctrl+C` | Quit |
| `Ctrl+L` | Clear screen |
| `Esc` | Quit / Close overlay |

### Model Selector

Press `/models` to open an interactive model selector:
- `↑` / `↓` or `j` / `k` - Navigate models
- `←` / `→` or `h` / `l` - Switch provider groups
- `Enter` - Select model
- `Esc` - Cancel

## Architecture

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              uira-cli                                        │
│                         (CLI Entry Point)                                    │
└─────────────────────────────────────────────────────────────────────────────┘
                                     │
                   ┌─────────────────┼─────────────────┐
                   ▼                 ▼                 ▼
         ┌─────────────────┐ ┌─────────────┐ ┌─────────────────┐
         │   uira-agent    │ │  uira-tui   │ │  uira-sandbox   │
         │  (Agent Loop)   │ │  (Ratatui)  │ │  (Sandboxing)   │
         └─────────────────┘ └─────────────┘ └─────────────────┘
                   │
         ┌─────────┴─────────┐
         ▼                   ▼
┌─────────────────┐  ┌─────────────────┐
│ uira-providers  │  │  uira-protocol  │
│ (Model Clients) │  │ (Types/Events)  │
└─────────────────┘  └─────────────────┘
```

### Core Crates

| Crate | Description |
|-------|-------------|
| **uira-cli** | CLI with session management and multi-provider support |
| **uira-agent** | Core agent loop with state machine, session persistence, and streaming |
| **uira-tui** | Ratatui-based terminal UI with approval overlay, model selector, and thinking display |
| **uira-protocol** | Shared types, events, streaming chunks, and protocol definitions |
| **uira-providers** | Model provider clients (Anthropic, OpenAI) with streaming support |
| **uira-sandbox** | Platform-native sandboxing (macOS sandbox-exec, Linux Landlock) |
| **uira-context** | Context management and conversation history |

### Tool Crates

| Crate | Description |
|-------|-------------|
| **uira-mcp-server** | MCP server with native LSP and AST-grep integration |
| **uira-tools** | LSP client, tool registry, and orchestration utilities |
| **uira-oxc** | OXC-powered JavaScript/TypeScript linting, parsing, transformation |

### Utility Crates

| Crate | Description |
|-------|-------------|
| **uira** | Standalone CLI for git hooks and dev tools |
| **uira-auth** | OAuth authentication for Anthropic, OpenAI, Google |
| **uira-config** | Configuration loading and management |
| **uira-hooks** | Hook implementations |
| **uira-goals** | Score-based goal verification |
| **uira-core** | Shared types and utilities |

## AI Agent Harness System

Uira provides an AI-assisted workflow system that integrates with git hooks to automatically invoke AI agents at commit time. The system uses an **embedded agent** that runs autonomously with full tool access until completion.

### Workflow Diagram

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│                              Developer Workflow                                  │
└─────────────────────────────────────────────────────────────────────────────────┘
                                       │
                     ┌─────────────────┴─────────────────┐
                     ▼                                   ▼
            ┌────────────────┐                  ┌────────────────┐
            │  Manual CLI    │                  │   Git Commit   │
            │  Invocation    │                  │    Trigger     │
            └────────────────┘                  └────────────────┘
                     │                                   │
                     │                                   ▼
                     │                          ┌────────────────┐
                     │                          │  .git/hooks/   │
                     │                          │  pre-commit    │
                     │                          └────────────────┘
                     │                                   │
                     │                                   ▼
                     │                          ┌────────────────┐
                     │                          │  uira run      │
                     │                          │  pre-commit    │
                     │                          └────────────────┘
                     │                                   │
                     └─────────────────┬─────────────────┘
                                       ▼
┌─────────────────────────────────────────────────────────────────────────────────┐
│                            uira CLI (AI Harness)                                 │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                  │
│  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────────┐                  │
│  │  uira typos     │  │ uira diagnostics│  │  uira comments  │                  │
│  │     --ai        │  │      --ai       │  │      --ai       │                  │
│  └────────┬────────┘  └────────┬────────┘  └────────┬────────┘                  │
│           │                    │                    │                           │
│           └────────────────────┼────────────────────┘                           │
│                                ▼                                                 │
│  ┌──────────────────────────────────────────────────────────────────────────┐   │
│  │                         AgentWorkflow                                     │   │
│  │                                                                           │   │
│  │  • Embedded agent session (same harness as uira-agent)                   │   │
│  │  • Full tool access: Read, Edit, Grep, Glob, Write, Bash,                │   │
│  │    WebSearch, CodeSearch, GrepApp, FetchUrl                              │   │
│  │  • Runs autonomously until <DONE/> is output                             │   │
│  │  • Verification via re-detection (no remaining issues)                   │   │
│  │  • Git diff-based modification tracking                                  │   │
│  │                                                                           │   │
│  └──────────────────────────────────────────────────────────────────────────┘   │
│                                                                                  │
└─────────────────────────────────────────────────────────────────────────────────┘
                                 │
                                 ▼
┌─────────────────────────────────────────────────────────────────────────────────┐
│                           Model Providers                                        │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                  │
│    ┌──────────────┐    ┌──────────────┐    ┌──────────────┐                     │
│    │  Anthropic   │    │   OpenAI     │    │   Gemini     │                     │
│    │   Claude     │    │    GPT       │    │              │                     │
│    └──────────────┘    └──────────────┘    └──────────────┘                     │
│                                                                                  │
└─────────────────────────────────────────────────────────────────────────────────┘
                                 │
                                 ▼
┌─────────────────────────────────────────────────────────────────────────────────┐
│                            Agent Workflow Loop                                   │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                  │
│  1. Detect Issues         2. Agent Fixes            3. Verify & Complete        │
│  ┌───────────────┐        ┌───────────────┐        ┌───────────────┐            │
│  │ typos CLI     │───────▶│ Agent uses    │───────▶│ Re-detect     │            │
│  │ lsp_diagnostics│       │ Read/Edit/Bash│        │ issues = 0?   │            │
│  │ comment-checker│       │ to fix issues │        │ → <DONE/>     │            │
│  └───────────────┘        └───────────────┘        └───────────────┘            │
│                                                                                  │
│  4. Stage Changes (--stage)    5. Continue/Fail Hook                            │
│  ┌───────────────┐             ┌───────────────┐                                │
│  │ git add <file>│────────────▶│ Exit 0 (pass) │                                │
│  │               │             │ Exit 1 (fail) │                                │
│  └───────────────┘             └───────────────┘                                │
│                                                                                  │
└─────────────────────────────────────────────────────────────────────────────────┘
```

### AI-Assisted Commands

| Command | Description | AI Decisions |
|---------|-------------|--------------|
| `uira typos --ai` | Check and fix typos | FIX, IGNORE per typo |
| `uira diagnostics --ai` | Fix LSP errors/warnings | FIX:HIGH, FIX:LOW, IGNORE |
| `uira comments --ai` | Review/remove comments | REMOVE, KEEP per comment |

### Git Hook Integration

```bash
# 1. Initialize configuration
uira init

# 2. Install git hooks
uira install

# 3. Commit normally - hooks run automatically
git commit -m "feat: add new feature"
```

When you commit, the pre-commit hook executes:
```
.git/hooks/pre-commit
    └── exec uira run pre-commit
            └── Runs configured commands from uira.yml
                    ├── uira typos --ai --stage
                    ├── uira diagnostics --ai --stage
                    └── uira comments --ai --stage
```

### Hook Configuration Example

```yaml
# uira.yml
pre-commit:
  parallel: false
  commands:
    - name: format
      run: uira format --check
    - name: typos
      run: uira typos --ai --stage
      on_fail: stop
    - name: diagnostics
      run: uira diagnostics --ai --staged --stage --severity error
      on_fail: stop
    - name: comments
      run: uira comments --ai --staged --stage
      on_fail: warn
```

## Configuration

Create `uira.yml` in your project root:

```yaml
# Model configuration
model: claude-sonnet-4-20250514
max_tokens: 128000

# TUI theme
theme: dracula  # default | dark | light | dracula | nord
theme_colors:
  accent: "#ff79c6"   # optional custom override (hex)

# Sandbox settings
sandbox:
  mode: workspace-write  # read-only | workspace-write | danger-full-access

# Agent routing
agents:
  explore:
    model: "gpt-4o-mini"     # Fast, cheap model for exploration
  architect:
    model: "claude-opus-4"    # Powerful model for architecture
  executor:
    model: "claude-sonnet-4"  # Balanced model for execution

# Git hooks
pre-commit:
  commands:
    - name: fmt
      run: cargo fmt
    - name: clippy
      run: cargo clippy -- -D warnings

# Goal verification
goals:
  auto_verify: true
  goals:
    - name: tests
      command: cargo test
      target: 100.0

# AI-Assisted Workflow Settings
# The AgentWorkflow runs an embedded agent with full tool access.
# Only model selection is needed - the agent handles everything autonomously.
typos:
  ai:
    model: anthropic/claude-sonnet-4-20250514

diagnostics:
  ai:
    model: anthropic/claude-sonnet-4-20250514
    severity: error                    # Severity filter: error, warning, all
    confidence_threshold: 0.8          # Minimum confidence to apply fixes
    languages: [js, ts, tsx, jsx]      # Languages to check

comments:
  ai:
    model: anthropic/claude-sonnet-4-20250514
    pragma_format: "@uira-allow"       # Pragma format for preserved comments
    include_docstrings: false          # Whether to review docstrings

# External MCP servers (discovered and exposed as tools)
mcp:
  servers:
    - name: filesystem
      command: npx -y @anthropic/mcp-server-filesystem /path/to/workspace
    - name: github
      command: npx -y @anthropic/mcp-server-github
```

### AI Workflow Configuration

The AI workflow uses an embedded agent that runs autonomously until completion. Configuration is minimal - just specify the model and any task-specific options.

| Section | Option | Default | Description |
|---------|--------|---------|-------------|
| `typos.ai` | `model` | `anthropic/claude-sonnet-4-20250514` | Model for typo fixing |
| `diagnostics.ai` | `model` | `anthropic/claude-sonnet-4-20250514` | Model for diagnostic fixing |
| `diagnostics.ai` | `severity` | `error` | Severity filter: error, warning, all |
| `diagnostics.ai` | `confidence_threshold` | `0.8` | Minimum confidence to apply fixes |
| `diagnostics.ai` | `languages` | `[js, ts, tsx, jsx]` | Languages to check |
| `comments.ai` | `model` | `anthropic/claude-sonnet-4-20250514` | Model for comment review |
| `comments.ai` | `pragma_format` | `@uira-allow` | Pragma for preserving comments |
| `comments.ai` | `include_docstrings` | `false` | Include docstrings in review |

## Multi-Provider Model Routing

Uira routes requests to the appropriate provider based on model ID:

| Model Pattern | Provider |
|---------------|----------|
| `claude-*`, `anthropic/*` | Anthropic API |
| `gpt-*`, `openai/*` | OpenAI API |
| `opencode/*` | OpenCode session API |

```yaml
# Route by task type
agents:
  explore:
    model: "gpt-4o-mini"      # Fast exploration
  architect:
    model: "claude-opus-4"     # Deep reasoning
  executor:
    model: "claude-sonnet-4"   # Balanced execution
```

## Sandboxing

Uira uses platform-native sandboxing to protect your system:

| Platform | Technology | Capabilities |
|----------|------------|--------------|
| macOS | sandbox-exec | File access, network, process restrictions |
| Linux | Landlock | File system access control |
| Windows | Job objects | Process isolation (coming soon) |

```bash
# Run with read-only sandbox (default)
uira-agent --sandbox read-only

# Allow workspace writes
uira-agent --sandbox workspace-write

# Disable sandbox (dangerous!)
uira-agent --sandbox danger-full-access
```

## Session Persistence

Sessions are saved as append-only JSONL files for debugging and replay:

```bash
# Sessions stored in
~/.uira/sessions/<session-id>.jsonl

# Resume a session
uira-agent --resume ~/.uira/sessions/abc123.jsonl

# Replay for debugging
cat ~/.uira/sessions/abc123.jsonl | jq
```

## MCP Server

The `uira-mcp` binary exposes development tools via Model Context Protocol:

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

### Search Tools
| Tool | Endpoint | Description |
|------|----------|-------------|
| `web_search` | `mcp.exa.ai/mcp` | Web search with DuckDuckGo fallback |
| `code_search` | `mcp.exa.ai/mcp` | Code examples and API references |
| `grep_app` | `mcp.grep.app` | GitHub code search across 1M+ repos |
| `fetch_url` | Direct HTTP | Fetch and clean web page content |

## Goal Verification

Define measurable goals that must pass before the agent considers a task complete:

```yaml
goals:
  auto_verify: true
  goals:
    - name: test-coverage
      command: ./scripts/coverage.sh
      target: 80.0
    - name: build-check
      command: cargo build --release && echo 100
      target: 100.0
```

Goals output a number (0-100) to stdout. The agent continues working until all goals pass.

## Development

```bash
# Build all crates
cargo build --workspace --release

# Run tests
cargo test --workspace

# Run the CLI in development
cargo run -p uira-cli

# Run with logging
RUST_LOG=debug cargo run -p uira-cli
```

## License

MIT
