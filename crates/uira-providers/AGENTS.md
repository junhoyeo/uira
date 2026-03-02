# uira-providers

Model provider clients (Anthropic, OpenAI, Gemini, Ollama, FriendliAI).

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
- Config-based: `providers.anthropic.payload_log.enabled` and `providers.anthropic.payload_log.path` in `uira.yaml`/`uira.jsonc`
- Environment variables override config: `UIRA_ANTHROPIC_PAYLOAD_LOG=true`, `UIRA_ANTHROPIC_PAYLOAD_LOG_FILE=<path>`
- Stages: `"request"`, `"usage"`, `"error"`
- FS errors are logged via `tracing::warn!` (not silently swallowed)
- Default path: `~/.local/share/uira/logs/anthropic-payload.jsonl`

Example config:
```yaml
providers:
  anthropic:
    payload_log:
      enabled: true
      path: ~/.local/share/uira/logs/anthropic-payload.jsonl
```

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

## FriendliAI Provider (`src/friendli.rs`)

Single-file provider client supporting both serverless and dedicated endpoints.

### Credential Resolution
 Priority: `ProviderConfig.api_key` → `FriendliAIConfig.get_token()` (token/token_file) → `FRIENDLI_TOKEN` env
 `FriendliAIConfig` supports inline `token`, `token_file` (reads from disk), and env fallback

### Endpoint Resolution
 Priority: `ProviderConfig.base_url` → `FriendliAIConfig.custom_endpoint` → derived from `ModelType`
 `ModelType::Serverless` → `https://api.friendli.ai/serverless/v1`
 `ModelType::Dedicated` → `https://api.friendli.ai/dedicated/v1`
 Model type resolved via: explicit `base_url` containing `/dedicated/` → `FriendliAIConfig.endpoint_type` → default `Serverless`

### Render API (`/render`)
 FriendliAI-exclusive feature — only provider that overrides `render_request()` from `ModelClient` trait
 Endpoint: `{base_url}/chat/render` (POST)
 Reuses `build_request()` payload but strips `stream` and `tool_choice` fields before sending
 Returns raw rendered prompt text (not included in AI conversation messages)
 TUI displays render output with "render" role using dim + text_muted styling

### Reasoning Mode (`chat_template_kwargs`)
 `ProviderConfig.reasoning_mode` controls reasoning behavior: `"off"` (default), `"on"`, `"interleaved"`, `"preserved"`
 `build_chat_template_kwargs()` computes `HashMap<String, bool>` from model ID + reasoning mode
 Per-model reasoning config via `get_reasoning_config()` — model registry pattern matching reference implementation
 Always-reasoning models (MiniMax): `reasoning_toggle = None` (no toggle key emitted)
 Default config: `enable_thinking: true`, `clear_thinking: false`
 `"off"` mode returns `None` (no kwargs sent), other modes emit appropriate toggle keys
 kwargs are serialized into `FriendliRequest.chat_template_kwargs` field (skipped when `None`)

### SSE Streaming
 Buffer uses `drain()` for O(n) event extraction (consistent with Anthropic/OpenAI)
 `[DONE]` sentinel yields `MessageStop` and returns
 `MessageDelta` with `stop_reason` also triggers `MessageStop` + return
 Transport errors mid-stream are terminal (no retry)
 Max SSE buffer: 10MB (`MAX_SSE_BUFFER`)
 Supports `reasoning_content` field in stream deltas → mapped to `ThinkingDelta`

### Error Handling
 Status 429 → `ProviderError::RateLimited` with `Retry-After` header parsing (default 60s)
 5xx → `ProviderError::Unavailable`
 Other errors → `ProviderError::InvalidResponse` with status + body
 No retry wrapper (unlike Anthropic/OpenAI) — single-attempt requests

### Message Conversion
 User messages with `Blocks` content → multipart `FriendliContentPart` array (text + image_url)
 Non-user `Blocks` → concatenated text string
 Images converted via `image_source_to_data_url()` — failures logged with `tracing::warn!` and skipped
 `ToolCalls` content on user messages → `None` content (no text conversion, unlike Anthropic)
 Tool results embedded as text parts

### Re-exports (`lib.rs`)
 `FriendliClient` is `pub`
