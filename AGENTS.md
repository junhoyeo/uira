# AI Agent Guidelines

This document provides context for AI agents working on the Uira codebase.

## Project Overview

**Uira** is a standalone, native AI coding agent built in Rust. It provides multi-provider orchestration (Anthropic, OpenAI, Gemini), platform-native sandboxing, session persistence, and a Ratatui-based terminal UI.

### Crate Structure

| Crate | Purpose |
|-------|---------|
| `uira-cli` | CLI entry point with session management |
| `uira-agent` | Core agent loop, state machine, streaming |
| `uira-tui` | Ratatui-based terminal interface |
| `uira-protocol` | Shared types, events, protocol definitions |
| `uira-providers` | Model provider clients (Anthropic, OpenAI) |
| `uira-sandbox` | Platform-native sandboxing (macOS/Linux) |
| `uira-tools` | LSP client, tool registry, orchestration |
| `uira-mcp-server` | MCP server with LSP and AST-grep tools |
| `uira-auth` | OAuth authentication for providers |
| `uira-context` | Context management and conversation history |

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

## Anthropic Provider (`uira-providers/src/anthropic/`)

All Anthropic-specific helpers live under `src/anthropic/`:
`mod.rs` (client + SSE stream), `error_classify.rs`, `retry.rs`, `turn_validation.rs`, `response_handling.rs`, `payload_log.rs`, `beta_features.rs`.

### Retry Logic
- Retries transient errors (429, 5xx, timeouts) — NOT mid-stream errors
- `ProviderConfig::with_max_retries(n)` (default: 3)
- Exponential backoff with jitter; jitter is clamped to `[0.0, 1.0]` and final delay to `>= 1ms` to prevent negative-to-u64 wrapping
- Respects `Retry-After` header; default rate-limit retry is `DEFAULT_RATE_LIMIT_RETRY_MS` (60s)

### Error Classification
- Status-code first (402, 429, 401/403, 5xx), then message-pattern matching via regex
- `ContextExceeded` attempts to parse token counts from the message; falls back to `used: 0, limit: 0`
- `ProviderError::ContextExceeded { used, limit }` uses plain `u64` (not `Option`) — changing to `Option<u64>` would require a cross-crate migration through `uira-protocol` and `uira-context`

### Error Response Parsing
- Anthropic wraps errors as `{"error": {"type": ..., "message": ...}}`
- `parse_error_body()` tries flat `ErrorResponse` first, then `NestedErrorResponse` fallback

### Turn Validation
- Merges consecutive user messages into a single `Blocks` message
- Preserves `Role::Tool` messages as-is (flushes pending user blocks first)
- User messages with `ToolCalls` content are converted to text blocks with a warning (not dropped)
- `has_thinking_blocks()` exists but is currently unused (`#[allow(dead_code)]`)
- Does NOT enforce strict user→assistant alternation — only merges consecutive user turns

### SSE Streaming
- Buffer uses `drain()` instead of slice-to-string for O(n) event extraction
- Anthropic sends one JSON object per `data:` line; multi-line data accumulation is not needed
- Transport errors mid-stream are terminal (no retry) — returns `ProviderError::StreamError`
- `[DONE]` sentinel yields `MessageStop` and returns

### Extended Thinking
- `ThinkingConfig` is `pub(crate)` with `&'static str` type field — not exposed outside the crate
- `ProviderConfig::with_thinking(budget)` sets `enable_thinking: true` and `thinking_budget: Some(budget)`
- When `enable_thinking: false` (default), `thinking_budget` defaults to `None`
- Temperature is forced to `None` when thinking is enabled (Anthropic requirement)

### Payload Logging
- Enable via `UIRA_ANTHROPIC_PAYLOAD_LOG=true`
- Stages: `"request"`, `"usage"`, `"error"`
- FS errors are logged via `tracing::warn!` (not silently swallowed)
- Default path: `~/.local/share/uira/logs/anthropic-payload.jsonl`
- Override: `UIRA_ANTHROPIC_PAYLOAD_LOG_FILE`

### Re-exports (`lib.rs`)
- `classify_error`, `validate_anthropic_turns`, `AnthropicClient`, `BetaFeatures`, `with_retry`, `PayloadLogEvent`, `PayloadLogger`, `RetryConfig` are intentionally `pub` — used by integration tests and downstream crates (`uira-agent`, `uira-tui`, `uira-cli`)

## LSP Client (`uira-tools/src/lsp/client.rs`)

- `ServerProcess` tracks `opened_documents: HashSet<String>` to avoid sending redundant `textDocument/didOpen` notifications
- `ensure_document_opened()` is called before every LSP operation; checks the set and skips if already opened
- Diagnostics are polled via `publishDiagnostics` notifications with a 2-second timeout

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
