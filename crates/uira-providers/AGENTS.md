# uira-providers

Model provider clients (Anthropic, OpenAI, Gemini, Ollama).

## Anthropic Provider (`src/anthropic/`)

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

## OpenAI Provider (`src/openai/`)

OpenAI-specific helpers live under `src/openai/`:
`mod.rs` (client + SSE stream), `error_classify.rs`.

### OAuth (Codex) Support
- Supports both API key and OAuth credentials (Codex CLI flow)
- Token refresh handled automatically with 5-minute buffer before expiry
- Credential lookup priority: credential store → config API key → `OPENAI_API_KEY` env

### Retry Logic
- Reuses `with_retry` from `anthropic/retry.rs` (generic implementation)
- Retries transient errors (429, 5xx, timeouts) — NOT mid-stream errors
- `ProviderConfig::with_max_retries(n)` (default: 3)
- Respects `Retry-After` header from 429 responses

### Error Classification
- OpenAI wraps errors as `{"error": {"message": ..., "type": ..., "code": ...}}`
- `parse_error_body()` tries nested format first, then flat fallback
- Status-code checked first (402, 429, 401/403, 5xx), then code/message patterns
- Recognizes OpenAI-specific codes: `context_length_exceeded`, `rate_limit_exceeded`, `insufficient_quota`

### SSE Streaming
- Buffer uses `drain()` for O(n) event extraction (not O(n²) slice-to-string)
- `[DONE]` sentinel yields `MessageStop` and returns
- Transport errors mid-stream are terminal (no retry)

### Re-exports (`lib.rs`)
- `classify_openai_error`, `OpenAIClient` are `pub`
