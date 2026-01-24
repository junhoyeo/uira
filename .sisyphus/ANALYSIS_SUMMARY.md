# Claude Agent SDK Analysis - Executive Summary

## Quick Facts

| Aspect | Finding |
|--------|---------|
| **SDK Type** | Node.js TypeScript library (ESM-only) |
| **Min Node.js** | 18+ (Astrape uses 20+) |
| **Session API** | No persistent sessions; stateless `query()` calls |
| **Protocol** | Async generator (for await...of) |
| **CLI Dependency** | Requires `npm install -g @anthropic-ai/claude-code` |
| **Auth** | ANTHROPIC_API_KEY environment variable |
| **Current Astrape** | TypeScript bridge subprocess (JSON-RPC over stdio) |

---

## Runtime Assumptions

### What the SDK Requires

1. **Node.js 18+** - No way around this; SDK is JavaScript
2. **Claude Code CLI** - Must be installed globally; SDK spawns it as subprocess
3. **ANTHROPIC_API_KEY** - API authentication (required)
4. **ESM Support** - SDK is ESM-only; CommonJS not supported
5. **Outbound HTTPS** - Access to `api.anthropic.com`

### What the SDK Does NOT Require

- ❌ CLAUDE_CODE_CLI environment variable (auto-detected)
- ❌ Specific file system paths (uses home directory)
- ❌ Pre-existing session state (stateless)
- ❌ HTTP server (uses stdio with CLI)

---

## Session Creation

### SDK Pattern (No Sessions)

```typescript
// Each call is independent; no session object
for await (const message of query({
  prompt: "...",
  options: { systemPrompt: "...", agents: {...}, ... }
})) {
  // Handle message
}
```

### Astrape's Abstraction

```rust
// Astrape creates a "session-like" wrapper
let session = AstrapeSession::new(options);
// session.query_options contains pre-configured options
// session.state tracks agents, tasks, context files
```

**Key Difference:** SDK has no session concept; Astrape adds one for convenience.

---

## Protocol Analysis

### Current: stdio (JSON-RPC)

```
Rust → spawn Node.js → import SDK → spawn Claude Code CLI
```

**Pros:**
- ✅ Full SDK compatibility
- ✅ Simple JSON-RPC protocol
- ✅ Process isolation

**Cons:**
- ❌ 500ms-1s startup per query
- ❌ 50-100MB memory per process
- ❌ Requires Node.js installation

### Alternative: Direct HTTP

**Feasibility:** ❌ **Impossible**
- SDK doesn't expose HTTP API
- SDK is a library, not a server
- Would require forking SDK

### Alternative: Direct stdio

**Feasibility:** ❌ **Impossible**
- SDK is a library, not a CLI
- No CLI entrypoint exists

### Alternative: Native Rust

**Feasibility:** ⚠️ **Possible but impractical**
- Would require reimplementing ~10k+ lines of TypeScript
- Maintenance burden unsustainable
- Risk of divergence from official SDK

---

## Integration Risks for Rust/napi-rs

### Critical Risks

1. **Node.js Dependency** (Unavoidable)
   - Both bridge and napi-rs require Node.js
   - Defeats "native Rust CLI" goal
   - Adds ~100MB to system requirements

2. **Claude Code CLI Dependency** (Unavoidable)
   - SDK requires it; no fallback
   - Users must install: `npm install -g @anthropic-ai/claude-code`
   - If missing, SDK fails at runtime

3. **Process Lifecycle** (Bridge-specific)
   - Must spawn, monitor, clean up Node.js process
   - Crashes in Node.js crash the bridge
   - Requires robust error handling

### High Risks

4. **Startup Latency** (500ms-1s per query)
   - Unacceptable for interactive CLI
   - Mitigation: persistent bridge process (complex)

5. **Memory Overhead** (50-100MB per process)
   - Significant for resource-constrained systems
   - Mitigation: process pooling

6. **Platform Compatibility** (napi-rs only)
   - Must pre-compile for: macOS (Intel/ARM), Linux (x86/ARM), Windows
   - Build complexity increases significantly

### Medium Risks

7. **Error Propagation** - Errors cross process boundary
8. **Version Mismatch** - SDK updates may break compatibility
9. **Serialization Overhead** - JSON serialization for each message

---

## Comparison: Bridge vs napi-rs vs Native Rust

| Aspect | Bridge | napi-rs | Native Rust |
|--------|--------|---------|-------------|
| **Node.js Required** | ✅ Yes | ✅ Yes | ❌ No |
| **SDK Compatibility** | ✅ Full | ✅ Full | ❌ None |
| **Startup Time** | 🟠 500ms-1s | 🟠 500ms-1s | ✅ <100ms |
| **Memory** | 🟠 50-100MB | 🟠 50-100MB | ✅ <10MB |
| **Maintenance** | 🟡 Medium | 🔴 High | 🔴 Very High |
| **Debugging** | ✅ Easy | 🟠 Hard | ✅ Easy |
| **Distribution** | ✅ Simple | 🟠 Complex | ✅ Simple |

---

## Recommendations

### Keep Current Bridge Approach If:
- ✅ You accept Node.js as a dependency
- ✅ You want full SDK compatibility
- ✅ You prioritize maintainability
- ✅ 500ms startup is acceptable

### Improvements to Current Approach:
1. **Persistent Bridge Process** - Keep alive between queries (reduces startup to <10ms)
2. **Process Pooling** - Reuse processes for concurrent queries
3. **Error Handling** - Implement retry logic, timeouts, logging
4. **Documentation** - Clearly document Node.js requirement

### Do NOT Pursue napi-rs Unless:
- You have dedicated team for maintenance
- You need sub-100ms startup (not achievable with napi-rs)
- You need <10MB memory footprint (not achievable with napi-rs)

### Do NOT Pursue Native Rust Unless:
- You have dedicated team for long-term maintenance
- You need features not in official SDK
- You're willing to accept maintenance burden

---

## Conclusion

**The current bridge subprocess approach is the most pragmatic solution.**

**Why:**
- Full SDK compatibility without reimplementation
- Minimal custom code to maintain
- Can upgrade SDK independently
- JSON-RPC protocol is transparent and debuggable

**The main trade-off is Node.js dependency**, which is unavoidable with the current SDK architecture.

**Next Steps:**
1. Implement persistent bridge process (if startup latency is critical)
2. Add comprehensive error handling and logging
3. Document Node.js requirement clearly
4. Monitor SDK updates for breaking changes

---

## Key Evidence

- **SDK Package:** `@anthropic-ai/claude-agent-sdk` v0.2.19 (current)
- **Node.js Requirement:** 18+ (official docs), 20+ (Astrape)
- **Bridge Location:** `/Users/junhoyeo/astrape/bridge/`
- **SDK Wrapper:** `/Users/junhoyeo/astrape/crates/astrape-sdk/`
- **Protocol:** JSON-RPC over stdio (see bridge/src/index.ts)

---

**Analysis Date:** 2025-01-24  
**Status:** Complete  
**Full Report:** `.sisyphus/analysis-claude-agent-sdk.md`
