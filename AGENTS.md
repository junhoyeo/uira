# AI Agent Guidelines

This document provides context for AI agents working on the Uira codebase.

## Project Overview

**Uira** is a standalone, native AI coding agent built in Rust. It provides multi-provider orchestration (Anthropic, OpenAI, Gemini), platform-native sandboxing, session persistence, and a Ratatui-based terminal UI.

### Crate Structure

All crates live under `crates/` and follow `uira-*` naming. Run `ls crates/` to see the full list.

**Key crates:**
- `uira-cli` — CLI entry point, session management
- `uira-agent` — Core agent loop, state machine, streaming
- `uira-tui` — Ratatui-based terminal interface
- `uira-providers` — Model provider clients (see nested AGENTS.md)
- `uira-tools` — LSP client, tool registry (see nested AGENTS.md)
- `uira-protocol` — Shared types, events, protocol definitions
- `uira-hooks` — Hook system for extensibility
- `uira-mcp-server` — MCP server exposing LSP and AST-grep tools

## Issue Labels

### Priority

| Label | Description |
|-------|-------------|
| `priority/high` | Critical - competitive disadvantage if not addressed |
| `priority/medium` | Normal priority - competitive parity |
| `priority/low` | Nice to have |

### Area

| Label | Description |
|-------|-------------|
| `area/tui` | Terminal UI (Ratatui) |
| `area/mcp` | MCP (Model Context Protocol) |
| `area/session` | Session management |
| `area/provider` | Model providers |
| `area/tools` | Tool integrations |

### Size (Effort Estimate)

| Label | Description |
|-------|-------------|
| `size/S` | Small - 1-2 days |
| `size/M` | Medium - 3-5 days |
| `size/L` | Large - 1-2 weeks |
| `size/XL` | Extra large - weeks/months |

### Type

| Label | Description |
|-------|-------------|
| `bug` | Something isn't working |
| `enhancement` | New feature or request |
| `documentation` | Documentation improvements |
| `good first issue` | Good for newcomers |
| `help wanted` | Extra attention needed |

## Contributing

### Issue Guidelines

1. Check existing issues before creating a new one
2. Use clear, descriptive titles
3. Include reproduction steps for bugs
4. Add appropriate labels (priority, area, size)

### Pull Request Guidelines

1. Reference related issues in the PR description
2. Keep PRs focused on a single change
3. Update documentation if behavior changes
4. Ensure all tests pass: `cargo test --workspace`
5. Run clippy: `cargo clippy -- -D warnings`

## Coding Conventions

## Runtime Defaults

- Permissions should be allow-by-default when no explicit rule matches.
- TUI input footer should show the active model so users always see current routing.
- TUI todo sidebar should be visible by default when todo items exist.
- TUI chat/tool output should append chronologically and keep newest entries at the bottom.

### Rust Style

- Follow standard Rust conventions (rustfmt)
- Use `?` for error propagation
- Prefer `thiserror` for custom errors
- Document public APIs with doc comments
- Keep functions focused and small

### Error Handling

```rust
// Use thiserror for custom errors
#[derive(Debug, thiserror::Error)]
pub enum MyError {
    #[error("operation failed: {0}")]
    OperationFailed(String),
}

// Propagate errors with ?
pub fn do_something() -> Result<(), MyError> {
    let result = fallible_operation()?;
    Ok(())
}
```

### Async Patterns

- Use `tokio` for async runtime
- Prefer channels for cross-task communication
- Use `Arc<Mutex<T>>` sparingly - prefer message passing

### Testing

- Place unit tests in the same file as the code
- Place integration tests in `tests/` directory
- Use descriptive test names: `test_should_parse_valid_input`

## Commit Message Convention

```
<type>: <description>

[optional body]
```

### Types

| Type | Description |
|------|-------------|
| `feat` | New feature |
| `fix` | Bug fix |
| `refactor` | Code refactoring (no behavior change) |
| `docs` | Documentation only |
| `test` | Adding or updating tests |
| `chore` | Maintenance tasks |
| `perf` | Performance improvements |

### Examples

```
feat: add session branching with /fork command
fix: handle empty response from provider
refactor: extract streaming logic to separate module
docs: update README with new CLI options
```

## Development Commands

```bash
# Build all crates
cargo build --workspace

# Run tests
cargo test --workspace

# Run with logging
RUST_LOG=debug cargo run -p uira-cli

# Check formatting
cargo fmt --all -- --check

# Run linter
cargo clippy --workspace -- -D warnings

# Run tmux-based TUI smoke checks
scripts/tui_smoke_tmux.sh
```
