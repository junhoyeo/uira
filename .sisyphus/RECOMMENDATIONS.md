# Claude Agent SDK Integration - Actionable Recommendations

## Overview

This document provides concrete, actionable recommendations for integrating the Claude Agent SDK with Astrape, based on the comprehensive analysis of runtime assumptions, session creation, and integration risks.

---

## 1. Immediate Actions (Next Sprint)

### 1.1 Document Node.js Requirement

**Action:** Create installation guide for users

**Files to Create:**
- `docs/INSTALLATION.md` - Node.js and Claude Code CLI setup
- `docs/TROUBLESHOOTING.md` - Common issues and solutions
- `docs/REQUIREMENTS.md` - System requirements

**Content Template:**

```markdown
# Installation Requirements

## Node.js
- **Minimum:** Node.js 18.x
- **Recommended:** Node.js 20.x or later
- **Install:** https://nodejs.org/

## Claude Code CLI
```bash
npm install -g @anthropic-ai/claude-code
```

## Verify Installation
```bash
node --version          # Should be 18+
claude-code --version   # Should be installed
```
```

**Effort:** 2-4 hours

### 1.2 Add Error Handling for Missing Dependencies

**Action:** Detect and report missing Node.js or Claude Code CLI

**Files to Modify:**
- `crates/astrape-sdk/src/bridge.rs` - Add pre-flight checks

**Code Example:**

```rust
impl SdkBridge {
    pub fn new() -> SdkResult<Self> {
        // Check Node.js is available
        Self::check_node_available()?;
        
        // Check Claude Code CLI is installed
        Self::check_claude_code_cli()?;
        
        // Spawn bridge process
        Self::with_bridge_path(None)
    }

    fn check_node_available() -> SdkResult<()> {
        Command::new("node")
            .arg("--version")
            .output()
            .map_err(|_| SdkError::Bridge(
                "Node.js not found. Install from https://nodejs.org/".to_string()
            ))?;
        Ok(())
    }

    fn check_claude_code_cli() -> SdkResult<()> {
        Command::new("claude-code")
            .arg("--version")
            .output()
            .map_err(|_| SdkError::Bridge(
                "Claude Code CLI not found. Install with: npm install -g @anthropic-ai/claude-code".to_string()
            ))?;
        Ok(())
    }
}
```

**Effort:** 4-6 hours

### 1.3 Add Comprehensive Logging

**Action:** Log all bridge communication for debugging

**Files to Modify:**
- `crates/astrape-sdk/src/bridge.rs` - Add logging to request/response

**Code Example:**

```rust
impl SdkBridge {
    fn send_request(&mut self, request: &BridgeRequest) -> SdkResult<()> {
        let json = serde_json::to_string(request)?;
        
        // Log request
        if std::env::var("ASTRAPE_DEBUG").is_ok() {
            eprintln!("[BRIDGE] Request: {}", json);
        }
        
        writeln!(self.stdin, "{}", json)?;
        Ok(())
    }

    fn read_response(&mut self) -> SdkResult<BridgeResponse> {
        let mut line = String::new();
        self.stdout.read_line(&mut line)?;
        
        let response: BridgeResponse = serde_json::from_str(&line)?;
        
        // Log response
        if std::env::var("ASTRAPE_DEBUG").is_ok() {
            eprintln!("[BRIDGE] Response: {}", line);
        }
        
        Ok(response)
    }
}
```

**Usage:**
```bash
ASTRAPE_DEBUG=1 astrape query "Your prompt"
```

**Effort:** 2-3 hours

---

## 2. Short-term Improvements (1-2 Months)

### 2.1 Implement Persistent Bridge Process

**Action:** Keep Node.js process alive between queries (reduces startup from 500ms to <10ms)

**Architecture:**

```
Rust (astrape-sdk)
  ↓ (spawn once)
Node.js (bridge/src/index.js)
  ↓ (keep alive)
  ├─ Query 1 (reuse)
  ├─ Query 2 (reuse)
  └─ Query 3 (reuse)
```

**Implementation:**

```rust
pub struct PersistentBridge {
    process: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    request_id: AtomicU64,
}

impl PersistentBridge {
    pub fn new() -> SdkResult<Self> {
        // Spawn bridge process once
        let process = Command::new("node")
            .arg("bridge/dist/index.js")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()?;
        
        // Keep alive for entire session
        Ok(Self { process, stdin, stdout, request_id: AtomicU64::new(1) })
    }

    pub async fn query(&mut self, params: QueryParams) -> SdkResult<mpsc::Receiver<SdkResult<StreamMessage>>> {
        // Reuse same process for multiple queries
        // No startup overhead
    }
}
```

**Benefits:**
- ✅ Reduces startup from 500ms to <10ms
- ✅ Reduces memory overhead (1 process instead of N)
- ✅ Improves interactive experience

**Risks:**
- ⚠️ Process lifecycle management more complex
- ⚠️ Must handle process crashes gracefully
- ⚠️ Requires connection pooling for concurrent queries

**Effort:** 20-30 hours

### 2.2 Add Process Pooling

**Action:** Maintain pool of bridge processes for concurrent queries

**Architecture:**

```
Rust (astrape-sdk)
  ↓ (distribute)
Bridge Pool (4 processes)
  ├─ Process 1 (Query A)
  ├─ Process 2 (Query B)
  ├─ Process 3 (Query C)
  └─ Process 4 (idle)
```

**Implementation:**

```rust
pub struct BridgePool {
    processes: Vec<PersistentBridge>,
    queue: mpsc::UnboundedSender<QueryTask>,
}

impl BridgePool {
    pub fn new(size: usize) -> SdkResult<Self> {
        let processes = (0..size)
            .map(|_| PersistentBridge::new())
            .collect::<SdkResult<Vec<_>>>()?;
        
        Ok(Self { processes, queue })
    }

    pub async fn query(&self, params: QueryParams) -> SdkResult<mpsc::Receiver<SdkResult<StreamMessage>>> {
        // Find available process
        // Send query
        // Return response channel
    }
}
```

**Benefits:**
- ✅ Enables concurrent queries
- ✅ Better resource utilization
- ✅ Improved throughput

**Risks:**
- ⚠️ More complex lifecycle management
- ⚠️ Requires load balancing logic
- ⚠️ Potential for process starvation

**Effort:** 30-40 hours

### 2.3 Add Retry Logic

**Action:** Automatically retry transient failures

**Implementation:**

```rust
pub async fn query_with_retry(
    bridge: &mut SdkBridge,
    params: QueryParams,
    max_retries: u32,
) -> SdkResult<mpsc::Receiver<SdkResult<StreamMessage>>> {
    for attempt in 0..max_retries {
        match bridge.query(params.clone()) {
            Ok(rx) => return Ok(rx),
            Err(e) if is_transient(&e) && attempt < max_retries - 1 => {
                // Exponential backoff: 100ms, 200ms, 400ms, ...
                let delay = Duration::from_millis(100 * 2_u64.pow(attempt));
                tokio::time::sleep(delay).await;
                continue;
            }
            Err(e) => return Err(e),
        }
    }
    
    Err(SdkError::Bridge("Max retries exceeded".to_string()))
}

fn is_transient(error: &SdkError) -> bool {
    matches!(error, 
        SdkError::Bridge(msg) if msg.contains("timeout") || msg.contains("ECONNREFUSED")
    )
}
```

**Benefits:**
- ✅ Handles transient failures gracefully
- ✅ Improves reliability
- ✅ Better user experience

**Effort:** 4-6 hours

---

## 3. Medium-term Enhancements (2-4 Months)

### 3.1 Implement Session Persistence

**Action:** Save and restore session state to disk

**Files to Create:**
- `crates/astrape-sdk/src/persistence.rs` - Session serialization

**Implementation:**

```rust
impl AstrapeSession {
    pub fn save(&self, path: &str) -> SdkResult<()> {
        let json = serde_json::to_string_pretty(&self)?;
        std::fs::write(path, json)?;
        Ok(())
    }

    pub fn load(path: &str) -> SdkResult<Self> {
        let json = std::fs::read_to_string(path)?;
        let session = serde_json::from_str(&json)?;
        Ok(session)
    }
}
```

**Benefits:**
- ✅ Resume interrupted sessions
- ✅ Audit trail of session history
- ✅ Better debugging

**Effort:** 8-12 hours

### 3.2 Add Metrics and Telemetry

**Action:** Track performance metrics (startup time, memory, latency)

**Implementation:**

```rust
pub struct BridgeMetrics {
    startup_time_ms: u64,
    memory_usage_mb: u64,
    query_latency_ms: u64,
    error_count: u64,
}

impl SdkBridge {
    pub fn query_with_metrics(&mut self, params: QueryParams) -> SdkResult<(mpsc::Receiver<SdkResult<StreamMessage>>, BridgeMetrics)> {
        let start = Instant::now();
        let rx = self.query(params)?;
        let latency = start.elapsed().as_millis() as u64;
        
        let metrics = BridgeMetrics {
            startup_time_ms: latency,
            memory_usage_mb: self.get_memory_usage()?,
            query_latency_ms: latency,
            error_count: 0,
        };
        
        Ok((rx, metrics))
    }
}
```

**Benefits:**
- ✅ Identify performance bottlenecks
- ✅ Monitor system health
- ✅ Better debugging

**Effort:** 12-16 hours

### 3.3 Add Configuration Validation

**Action:** Validate session options before sending to SDK

**Implementation:**

```rust
impl AstrapeSession {
    pub fn validate(&self) -> SdkResult<()> {
        // Validate system prompt
        if self.query_options.system_prompt.is_empty() {
            return Err(SdkError::Validation("System prompt cannot be empty".to_string()));
        }
        
        // Validate agents
        for (name, agent) in &self.query_options.agents {
            if agent.description.is_empty() {
                return Err(SdkError::Validation(format!("Agent '{}' missing description", name)));
            }
        }
        
        // Validate tools
        for tool in &self.query_options.allowed_tools {
            if !is_valid_tool(tool) {
                return Err(SdkError::Validation(format!("Unknown tool: {}", tool)));
            }
        }
        
        Ok(())
    }
}
```

**Benefits:**
- ✅ Catch errors early
- ✅ Better error messages
- ✅ Prevent invalid configurations

**Effort:** 6-8 hours

---

## 4. Long-term Strategy (4+ Months)

### 4.1 Monitor SDK Updates

**Action:** Track official SDK releases and breaking changes

**Process:**
1. Subscribe to GitHub releases: https://github.com/anthropics/claude-agent-sdk-typescript
2. Review changelog for breaking changes
3. Test against new versions in CI/CD
4. Update documentation as needed

**Effort:** 2-4 hours per release

### 4.2 Consider Alternative Approaches (If Needed)

**Only if current approach becomes untenable:**

1. **Persistent Bridge Process** (Recommended)
   - Reduces startup overhead
   - Maintains full SDK compatibility
   - Moderate complexity

2. **Process Pooling** (If concurrent queries critical)
   - Enables parallel execution
   - Better resource utilization
   - Higher complexity

3. **HTTP API Wrapper** (If SDK adds HTTP support)
   - Would eliminate Node.js dependency
   - Requires official SDK changes
   - Unlikely in near term

4. **Native Rust Implementation** (Last resort)
   - Eliminates all dependencies
   - Massive engineering effort
   - High maintenance burden
   - Not recommended

---

## 5. Decision Matrix

### 5.1 Should We Use napi-rs?

| Factor | Assessment | Recommendation |
|--------|-----------|-----------------|
| **Eliminates Node.js?** | ❌ No | Don't use napi-rs |
| **Improves startup?** | ❌ No | Don't use napi-rs |
| **Reduces memory?** | ❌ No | Don't use napi-rs |
| **Adds complexity?** | ✅ Yes | Don't use napi-rs |
| **Harder to maintain?** | ✅ Yes | Don't use napi-rs |

**Verdict:** ❌ **Do NOT use napi-rs**

### 5.2 Should We Replace with HTTP?

| Factor | Assessment | Recommendation |
|--------|-----------|-----------------|
| **SDK supports HTTP?** | ❌ No | Not possible now |
| **Would eliminate Node.js?** | ✅ Yes | Would be nice |
| **Requires SDK changes?** | ✅ Yes | Unlikely to happen |
| **Worth waiting for?** | ❌ No | Pursue alternatives |

**Verdict:** ❌ **Don't wait for HTTP API**

### 5.3 Should We Implement Native Rust?

| Factor | Assessment | Recommendation |
|--------|-----------|-----------------|
| **Eliminates dependencies?** | ✅ Yes | Would be nice |
| **Engineering effort?** | 🔴 Massive | Not worth it |
| **Maintenance burden?** | 🔴 Very high | Not sustainable |
| **Risk of divergence?** | 🔴 High | Likely |

**Verdict:** ❌ **Don't implement native Rust**

### 5.4 Should We Use Persistent Bridge?

| Factor | Assessment | Recommendation |
|--------|-----------|-----------------|
| **Improves startup?** | ✅ Yes (500ms → <10ms) | Worth it |
| **Maintains compatibility?** | ✅ Yes | Good |
| **Adds complexity?** | 🟡 Moderate | Manageable |
| **Improves UX?** | ✅ Yes | Important |

**Verdict:** ✅ **YES - Implement persistent bridge**

---

## 6. Implementation Roadmap

### Phase 1: Foundation (Weeks 1-2)
- [ ] Document Node.js requirement
- [ ] Add error handling for missing dependencies
- [ ] Add comprehensive logging
- **Effort:** 8-13 hours

### Phase 2: Performance (Weeks 3-6)
- [ ] Implement persistent bridge process
- [ ] Add retry logic
- [ ] Add metrics and telemetry
- **Effort:** 36-52 hours

### Phase 3: Reliability (Weeks 7-10)
- [ ] Implement session persistence
- [ ] Add configuration validation
- [ ] Improve error messages
- **Effort:** 20-28 hours

### Phase 4: Monitoring (Weeks 11+)
- [ ] Monitor SDK updates
- [ ] Track performance metrics
- [ ] Gather user feedback
- **Effort:** Ongoing

---

## 7. Success Criteria

### Phase 1 Success
- ✅ Users understand Node.js requirement
- ✅ Clear error messages for missing dependencies
- ✅ Debug logs available for troubleshooting

### Phase 2 Success
- ✅ Startup latency < 100ms (from 500ms)
- ✅ Memory usage < 100MB per session
- ✅ Automatic retry for transient failures

### Phase 3 Success
- ✅ Sessions can be saved and restored
- ✅ Invalid configurations caught early
- ✅ Better error messages

### Phase 4 Success
- ✅ No breaking changes from SDK updates
- ✅ Performance metrics tracked
- ✅ User satisfaction improved

---

## 8. Risk Mitigation

### Risk: Node.js Dependency
**Mitigation:**
- Document clearly in README
- Provide installation instructions
- Add pre-flight checks
- Suggest alternatives if needed

### Risk: Startup Latency
**Mitigation:**
- Implement persistent bridge process
- Add process pooling for concurrent queries
- Monitor and optimize performance

### Risk: SDK Breaking Changes
**Mitigation:**
- Pin SDK version strictly
- Test against new versions in CI/CD
- Monitor GitHub releases
- Maintain compatibility matrix

### Risk: Process Crashes
**Mitigation:**
- Implement robust error handling
- Add automatic restart logic
- Log all errors for debugging
- Monitor process health

---

## 9. Conclusion

**Recommended Path Forward:**

1. **Immediate:** Document requirements, add error handling, add logging
2. **Short-term:** Implement persistent bridge process
3. **Medium-term:** Add session persistence, metrics, validation
4. **Long-term:** Monitor SDK updates, gather feedback

**Key Principle:** Keep the current bridge approach; optimize it incrementally.

**Why:**
- ✅ Full SDK compatibility
- ✅ Minimal custom code
- ✅ Maintainable long-term
- ✅ Clear upgrade path

---

**Document Version:** 1.0  
**Last Updated:** 2025-01-24  
**Status:** Ready for Implementation
