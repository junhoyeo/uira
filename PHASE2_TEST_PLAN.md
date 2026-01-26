# Phase 2 Testing Plan: OpenCode Client & OpenAI Provider

## Overview

Phase 2 implements direct provider support for external models (OpenAI, Gemini, OpenCode) without using the deprecated LiteLLM proxy.

**Status**: Implementation complete, ready for testing

## Architecture

```
spawn_agent → route_model(model)
    ├─ Anthropic → anthropic_client (claude-agent-sdk-rs) ✅ Phase 1
    └─ DirectProvider → opencode_client ✅ Phase 2
                            ↓
                    Load OpenCode auth (~/.local/share/opencode/auth.json)
                            ↓
                    Create session (POST localhost:8787/session)
                            ↓
                    Get provider token
                            ↓
                    OpenAIProvider → Direct HTTP to api.openai.com
                            ↓
                    Parse SSE streaming response
                            ↓
                    Return JSON: {"result": "..."}
```

## Files Created (Phase 2)

| File | Purpose | Lines |
|------|---------|-------|
| `crates/astrape-mcp-server/src/auth.rs` | OpenCode auth loading (copied from astrape-proxy) | 109 |
| `crates/astrape-mcp-server/src/opencode_client.rs` | Session creation and provider routing | 42 |
| `crates/astrape-mcp-server/src/providers/mod.rs` | Provider trait definition | 8 |
| `crates/astrape-mcp-server/src/providers/openai.rs` | OpenAI HTTP client with SSE streaming | 80 |

## Files Modified (Phase 2)

| File | Change | Lines |
|------|--------|-------|
| `crates/astrape-mcp-server/src/main.rs` | Added module declarations | +3 |
| `crates/astrape-mcp-server/src/tools.rs` | Implemented DirectProvider branch | ~5 |
| `crates/astrape-mcp-server/Cargo.toml` | Added dependencies | +5 |

## Prerequisites

### 1. OpenCode Authentication

Ensure you have OpenCode credentials configured:

```bash
# Check if auth file exists
ls -la ~/.local/share/opencode/auth.json  # Linux
ls -la ~/Library/Application\ Support/opencode/auth.json  # macOS

# If missing, login to OpenAI
opencode auth login openai
```

**Expected auth.json format**:
```json
{
  "openai": {
    "type": "api",
    "key": "sk-..."
  }
}
```

### 2. Build MCP Server

```bash
cd /Users/junhoyeo/astrape
cargo build --release -p astrape-mcp-server
```

**Expected output**:
```
   Compiling astrape-mcp-server v0.1.0
    Finished `release` profile [optimized] target(s) in X.XXs
```

### 3. Verify Binary

```bash
ls -lh target/release/astrape-mcp
```

**Expected**: Binary exists, ~10-20MB

## Test Cases

### Test 1: Anthropic Model (Regression Test - Phase 1)

**Purpose**: Verify Phase 1 still works after Phase 2 changes

**Command**:
```bash
# Start MCP server
./target/release/astrape-mcp

# In another terminal, send MCP request
echo '{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "tools/call",
  "params": {
    "name": "spawn_agent",
    "arguments": {
      "agent": "test-agent",
      "prompt": "What is 2+2?",
      "model": "claude-3-5-sonnet-20241022"
    }
  }
}' | nc localhost 3000
```

**Expected Result**:
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "content": [
      {
        "type": "text",
        "text": "{\"result\":\"4\"}"
      }
    ]
  }
}
```

**Success Criteria**:
- ✅ Response received within 5 seconds
- ✅ Response format: `{"result": "..."}`
- ✅ Content is correct (4)
- ✅ No errors in MCP server logs

### Test 2: OpenAI Model (Phase 2 - New Functionality)

**Purpose**: Verify OpenAI provider works via OpenCode client

**Prerequisites**:
- OpenCode auth configured with OpenAI credentials
- Valid OpenAI API key in `~/.local/share/opencode/auth.json`

**Command**:
```bash
# Start MCP server
./target/release/astrape-mcp

# In another terminal, send MCP request
echo '{
  "jsonrpc": "2.0",
  "id": 2,
  "method": "tools/call",
  "params": {
    "name": "spawn_agent",
    "arguments": {
      "agent": "test-agent",
      "prompt": "What is 2+2? Answer with just the number.",
      "model": "openai/gpt-4"
    }
  }
}' | nc localhost 3000
```

**Expected Result**:
```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "result": {
    "content": [
      {
        "type": "text",
        "text": "{\"result\":\"4\"}"
      }
    ]
  }
}
```

**Success Criteria**:
- ✅ Response received within 10 seconds
- ✅ Response format: `{"result": "..."}`
- ✅ Content is correct (4)
- ✅ No errors in MCP server logs
- ✅ Logs show "Spawning agent via OpenCode client"

**Expected Logs**:
```
INFO astrape_mcp_server::tools: Spawning agent via OpenCode client agent="test-agent" model="openai/gpt-4"
```

### Test 3: Error Handling - Missing Auth

**Purpose**: Verify graceful error handling when OpenCode auth is missing

**Setup**:
```bash
# Temporarily rename auth file
mv ~/.local/share/opencode/auth.json ~/.local/share/opencode/auth.json.bak
```

**Command**:
```bash
echo '{
  "jsonrpc": "2.0",
  "id": 3,
  "method": "tools/call",
  "params": {
    "name": "spawn_agent",
    "arguments": {
      "agent": "test-agent",
      "prompt": "Hello",
      "model": "openai/gpt-4"
    }
  }
}' | nc localhost 3000
```

**Expected Result**:
```json
{
  "jsonrpc": "2.0",
  "id": 3,
  "error": {
    "code": -32000,
    "message": "Failed to load OpenCode auth: Failed to read ..."
  }
}
```

**Cleanup**:
```bash
# Restore auth file
mv ~/.local/share/opencode/auth.json.bak ~/.local/share/opencode/auth.json
```

**Success Criteria**:
- ✅ Error message is clear and actionable
- ✅ No panic or crash
- ✅ MCP server continues running

### Test 4: Error Handling - Invalid API Key

**Purpose**: Verify graceful error handling when OpenAI API key is invalid

**Setup**:
```bash
# Edit auth.json to use invalid key
# Change "key": "sk-..." to "key": "sk-invalid"
```

**Command**:
```bash
echo '{
  "jsonrpc": "2.0",
  "id": 4,
  "method": "tools/call",
  "params": {
    "name": "spawn_agent",
    "arguments": {
      "agent": "test-agent",
      "prompt": "Hello",
      "model": "openai/gpt-4"
    }
  }
}' | nc localhost 3000
```

**Expected Result**:
```json
{
  "jsonrpc": "2.0",
  "id": 4,
  "error": {
    "code": -32000,
    "message": "OpenAI API returned error 401: ..."
  }
}
```

**Success Criteria**:
- ✅ Error message includes HTTP status code
- ✅ Error message includes OpenAI error details
- ✅ No panic or crash
- ✅ MCP server continues running

### Test 5: Streaming Response

**Purpose**: Verify SSE streaming is parsed correctly

**Command**:
```bash
echo '{
  "jsonrpc": "2.0",
  "id": 5,
  "method": "tools/call",
  "params": {
    "name": "spawn_agent",
    "arguments": {
      "agent": "test-agent",
      "prompt": "Count from 1 to 10",
      "model": "openai/gpt-4"
    }
  }
}' | nc localhost 3000
```

**Expected Result**:
```json
{
  "jsonrpc": "2.0",
  "id": 5,
  "result": {
    "content": [
      {
        "type": "text",
        "text": "{\"result\":\"1, 2, 3, 4, 5, 6, 7, 8, 9, 10\"}"
      }
    ]
  }
}
```

**Success Criteria**:
- ✅ Full response received (not truncated)
- ✅ All numbers present (1-10)
- ✅ Response format: `{"result": "..."}`

## Manual Testing (Alternative)

If MCP server integration is complex, test the OpenCode client directly:

### Direct Test Script

Create `test_opencode_client.rs`:
```rust
use astrape_mcp_server::opencode_client;

#[tokio::main]
async fn main() {
    let result = opencode_client::query(
        "What is 2+2?",
        "openai/gpt-4",
        8787
    ).await;
    
    match result {
        Ok(response) => println!("Success: {}", response),
        Err(e) => eprintln!("Error: {}", e),
    }
}
```

**Run**:
```bash
cargo run --bin test_opencode_client
```

## Performance Benchmarks

### Baseline (Phase 1 - Anthropic via SDK)

| Metric | Value |
|--------|-------|
| Cold start | ~500ms |
| Warm request | ~200ms |
| Streaming latency | ~50ms |

### Target (Phase 2 - OpenAI via Direct HTTP)

| Metric | Target | Rationale |
|--------|--------|-----------|
| Cold start | ~400ms | Eliminate proxy overhead (~100ms) |
| Warm request | ~150ms | Direct HTTP faster than subprocess |
| Streaming latency | ~30ms | Direct SSE parsing |

### Measurement

```bash
# Time a request
time echo '{...}' | nc localhost 3000
```

## Known Limitations (Phase 2)

1. **Only OpenAI supported** - Gemini and OpenCode providers not yet implemented
2. **Hardcoded port** - OpenCode port (8787) is hardcoded, should be configurable
3. **No retry logic** - Network failures are not retried
4. **No timeout** - Requests can hang indefinitely
5. **Session not reused** - Creates new session for each request (inefficient)

## Next Steps (Phase 3)

1. **Add Gemini provider** (`providers/gemini.rs`)
2. **Add OpenCode provider** (`providers/opencode.rs`)
3. **Session pooling** - Reuse sessions across requests
4. **Configuration** - Make OpenCode port configurable
5. **Retry logic** - Add exponential backoff for network failures
6. **Timeout handling** - Add request timeouts
7. **Remove LiteLLM dependency** - Clean up deprecated proxy code

## Success Criteria (Phase 2 Complete)

- [x] Build succeeds with zero errors
- [x] Clippy passes with zero warnings
- [ ] Test 1 passes (Anthropic regression)
- [ ] Test 2 passes (OpenAI new functionality)
- [ ] Test 3 passes (Missing auth error handling)
- [ ] Test 4 passes (Invalid API key error handling)
- [ ] Test 5 passes (Streaming response)
- [ ] Performance meets targets (30-80ms improvement)
- [ ] No breaking changes to Phase 1

## Rollback Plan

If Phase 2 causes issues:

```bash
# Revert to Phase 1
git checkout HEAD~1

# Rebuild
cargo build --release -p astrape-mcp-server

# Verify Phase 1 still works
./target/release/astrape-mcp
```

## Contact

For issues or questions:
- GitHub: https://github.com/junhoyeo/Astrape/issues
- Branch: `refactor/spawn-agent-sdk`
- PR: https://github.com/junhoyeo/Astrape/pull/new/refactor/spawn-agent-sdk
