# End-to-End Testing Guide for astrape-proxy

## Prerequisites

1. **OpenCode installed and authenticated**:
   ```bash
   opencode auth login opencode
   opencode auth login openai
   ```

2. **LiteLLM proxy running** (if using external LiteLLM):
   ```bash
   # Default: http://localhost:4000
   litellm --config litellm_config.yaml
   ```

3. **astrape.yml configured**:
   ```yaml
   agents:
     explore:
       model: "opencode/gpt-5-nano"
     architect:
       model: "openai/gpt-4.1"
   ```

## Test 1: Start the Proxy

```bash
cd /Users/junhoyeo/astrape
cargo run --release -p astrape-proxy
```

Expected output:
```
astrape-proxy listening addr=0.0.0.0:8787
```

## Test 2: Health Check

```bash
curl http://localhost:8787/health
```

Expected: `OK`

## Test 3: Agent-Based Model Routing

Test that the proxy correctly routes based on agent metadata:

```bash
curl -X POST http://localhost:8787/v1/messages \
  -H "Content-Type: application/json" \
  -d '{
    "model": "claude-3-sonnet",
    "max_tokens": 100,
    "messages": [{"role": "user", "content": "Hello"}],
    "metadata": {"agent": "explore"}
  }'
```

**Expected behavior**:
- Proxy extracts `agent: "explore"` from metadata
- Looks up `agents.explore.model` in astrape.yml
- Uses `"opencode/gpt-5-nano"` instead of `"claude-3-sonnet"`
- Forwards to LiteLLM with the configured model

## Test 4: Fallback to Original Model

Test with unknown agent:

```bash
curl -X POST http://localhost:8787/v1/messages \
  -H "Content-Type: application/json" \
  -d '{
    "model": "claude-3-sonnet",
    "max_tokens": 100,
    "messages": [{"role": "user", "content": "Hello"}],
    "metadata": {"agent": "unknown"}
  }'
```

**Expected behavior**:
- Agent "unknown" not in astrape.yml
- Falls back to original model: `"claude-3-sonnet"`

## Test 5: Streaming Response

```bash
curl -X POST http://localhost:8787/v1/messages \
  -H "Content-Type: application/json" \
  -d '{
    "model": "claude-3-sonnet",
    "max_tokens": 100,
    "messages": [{"role": "user", "content": "Count to 5"}],
    "stream": true,
    "metadata": {"agent": "explore"}
  }'
```

**Expected**:
- SSE stream with Anthropic-format events
- `event: message_start`
- `event: content_block_delta`
- `event: message_delta`
- `event: message_stop`

## Test 6: Use with Claude Code

```bash
ANTHROPIC_BASE_URL=http://localhost:8787 claude
```

Then in Claude Code, trigger an agent that uses the explore agent. The proxy should:
1. Receive request with `metadata.agent = "explore"`
2. Route to `opencode/gpt-5-nano`
3. Return response in Anthropic format

## Test 7: Token Counting

```bash
curl -X POST http://localhost:8787/v1/messages/count_tokens \
  -H "Content-Type: application/json" \
  -d '{
    "model": "claude-3-sonnet",
    "messages": [{"role": "user", "content": "Hello world"}]
  }'
```

**Expected**:
```json
{"input_tokens": 5}
```

## Test 8: Authentication Error Handling

Stop OpenCode auth and test:

```bash
mv ~/.local/share/opencode/auth.json ~/.local/share/opencode/auth.json.bak
curl -X POST http://localhost:8787/v1/messages \
  -H "Content-Type: application/json" \
  -d '{
    "model": "claude-3-sonnet",
    "max_tokens": 100,
    "messages": [{"role": "user", "content": "Hello"}],
    "metadata": {"agent": "explore"}
  }'
```

**Expected**:
- HTTP 401 Unauthorized
- Error message: "Run 'opencode auth login opencode' to refresh"

Restore auth:
```bash
mv ~/.local/share/opencode/auth.json.bak ~/.local/share/opencode/auth.json
```

## Verification Checklist

- [ ] Proxy starts successfully on port 8787
- [ ] Health endpoint returns OK
- [ ] Agent-based routing works (explore → gpt-5-nano)
- [ ] Fallback to original model works
- [ ] Streaming responses work
- [ ] Token counting works
- [ ] Authentication errors are handled gracefully
- [ ] Integration with Claude Code works

## Debugging

### Enable verbose logging:
```bash
RUST_LOG=debug cargo run --release -p astrape-proxy
```

### Check what model is being used:
Look for log lines like:
```
DEBUG astrape_proxy::server: agent_name="explore" resolved_model="opencode/gpt-5-nano"
```

### Verify OpenCode auth:
```bash
cat ~/.local/share/opencode/auth.json | jq
```

Should show providers with valid tokens and expiry times.

## Success Criteria

✅ All 8 tests pass
✅ Agent-based routing confirmed working
✅ No model-name-based mapping (forbidden pattern)
✅ OpenCode authentication integration working
✅ Streaming and non-streaming both work
✅ Error handling is graceful
