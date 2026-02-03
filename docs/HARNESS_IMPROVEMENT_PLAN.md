# Uira Harness Improvement Plan

> Based on competitive analysis of sst/opencode and openai/codex

## Executive Summary

This plan outlines 65 tasks across 7 phases to bring Uira's agent harness to feature parity with leading AI coding agents. Key improvements include an event bus architecture, session forking, conversation compaction, approval caching, and pattern-based permissions.

---

## Phase Overview

| Phase | Focus | Duration | Key Deliverables |
|-------|-------|----------|------------------|
| **1** | Event Bus Foundation | Week 1 | Replace HookRegistry with EventBus |
| **2** | Session Forking | Week 2 | Fork/branch sessions for experimentation |
| **3** | Conversation Compaction | Weeks 3-4 | Auto-compact with pruning + summarization |
| **4** | Approval Caching | Week 5 | Disk-persisted approval cache |
| **5** | Pattern Permissions | Week 5 | Glob-based permission rules (Allow default) |
| **6** | Sandbox Retry + Config | Week 6 | Smart retry, 7-level config precedence |
| **7** | Polish & Telemetry | Week 7+ | OpenTelemetry, Plan mode, ThreadManager |

---

## New Crates

| Crate | Purpose |
|-------|---------|
| `crates/uira-events` | Event bus replacing HookRegistry |
| `crates/uira-permissions` | Pattern-based permission evaluation |
| `crates/uira-telemetry` | Optional OpenTelemetry integration |

---

## Phase 1: Event Bus Foundation (Week 1)

*Must come first since hooks replacement affects everything*

### Migration Strategy
```
Before: Agent → HookRegistry.execute_hooks(event) → Hook.execute()
After:  Agent → EventBus.publish(event) → Subscriber.handle()
```

### Tasks

| Task ID | Task | Files | Size | Priority |
|---------|------|-------|------|----------|
| 1.1.1 | Create `crates/uira-events` crate with `EventBus` trait | New crate | M | P0 |
| 1.1.2 | Implement `BroadcastBus` using `tokio::sync::broadcast` | `crates/uira-events/src/broadcast.rs` | M | P0 |
| 1.1.3 | Define comprehensive event enums replacing HookEvent | `crates/uira-events/src/events.rs` | L | P0 |
| 1.1.4 | Add wildcard subscription support | `crates/uira-events/src/subscriber.rs` | M | P0 |
| 1.1.5 | Create migration layer: `LegacyHookAdapter` | `crates/uira-events/src/compat.rs` | M | P1 |
| 1.1.6 | Migrate Agent to emit via EventBus | `crates/uira-agent/src/agent.rs` | L | P0 |
| 1.1.7 | Migrate each hook to EventBus subscriber (23 hooks) | `crates/uira-hooks/src/hooks/*.rs` | L | P1 |
| 1.1.8 | Deprecate HookRegistry, mark for removal | `crates/uira-hooks/src/registry.rs` | S | P2 |
| 1.1.9 | Update TUI to subscribe via EventBus | `crates/uira-tui/src/app.rs` | M | P0 |

---

## Phase 2: Session Forking (Week 2)

*High UX impact - enables experimentation without losing context*

### Data Model
```rust
pub struct Session {
    pub id: SessionId,
    pub parent_id: Option<SessionId>,           // NEW
    pub forked_from_message: Option<MessageId>, // NEW
    pub fork_count: u32,                        // NEW: for child tracking
    // ... existing fields
}
```

### Tasks

| Task ID | Task | Files | Size | Tests |
|---------|------|-------|------|-------|
| 2.1.1 | Add `parent_id: Option<SessionId>` to Session | `crates/uira-agent/src/session.rs` | S | Unit |
| 2.1.2 | Add `forked_from_message: Option<MessageId>` | `crates/uira-agent/src/session.rs` | S | Unit |
| 2.1.3 | Add `fork_count: u32` for title generation | `crates/uira-agent/src/session.rs` | S | Unit |
| 2.1.4 | Implement `Session::fork(from_message_id)` method | `crates/uira-agent/src/session.rs` | M | Unit |
| 2.1.5 | Deep copy messages with new IDs up to fork point | `crates/uira-agent/src/session.rs` | M | Unit |
| 2.1.6 | Generate forked title: `"{title} (fork #{n})"` | `crates/uira-agent/src/session.rs` | S | Unit |
| 2.1.7 | Persist fork metadata in rollout JSONL | `crates/uira-agent/src/rollout.rs` | M | Unit |
| 2.1.8 | Add `/fork [message_id]` command to TUI | `crates/uira-tui/src/app.rs` | M | Integration |
| 2.1.9 | Add `SessionForked` event to EventBus | `crates/uira-events/src/events.rs` | S | Unit |
| 2.1.10 | Display fork tree in session list | `crates/uira-cli/src/session.rs` | M | Integration |
| 2.1.11 | Support `--resume-fork <parent_session>` CLI flag | `crates/uira-cli/src/commands.rs` | M | Integration |

---

## Phase 3: Conversation Compaction (Weeks 3-4)

*Critical for long sessions - prevents context overflow*

### Config Schema
```yaml
compaction:
  enabled: true
  threshold: 0.8              # Trigger at 80% context usage
  protected_tokens: 40000     # Never prune recent 40K
  strategy: summarize         # prune | summarize | hybrid
  summarization_model: null   # Use session model if null
```

### Tasks

| Task ID | Task | Files | Size | Tests |
|---------|------|-------|------|-------|
| 3.1.1 | Create `TokenMonitor` with configurable thresholds | `crates/uira-context/src/monitor.rs` (new) | M | Unit |
| 3.1.2 | Implement overflow detection: `is_overflow(usage, model_limit)` | `crates/uira-context/src/monitor.rs` | M | Unit |
| 3.1.3 | Add `protected_tokens: usize` config (default: 40K) | `crates/uira-config/src/schema.rs` | S | Unit |
| 3.1.4 | Implement `PruningStrategy`: remove old tool outputs | `crates/uira-context/src/compact.rs` | M | Unit |
| 3.1.5 | Protect recent N tokens from pruning | `crates/uira-context/src/compact.rs` | M | Unit |
| 3.1.6 | Create compaction agent system prompt | `crates/uira-agents/src/prompts.rs` | M | Unit |
| 3.1.7 | Implement `SummarizationStrategy` using provider | `crates/uira-context/src/compact.rs` | L | Integration |
| 3.1.8 | Add `CompactionConfig` to schema | `crates/uira-config/src/schema.rs` | M | Unit |
| 3.1.9 | Wire auto-compaction into `ContextManager.add_message()` | `crates/uira-context/src/manager.rs` | M | Integration |
| 3.1.10 | Create `CompactionStarted` and `CompactionCompleted` events | `crates/uira-events/src/events.rs` | S | Unit |
| 3.1.11 | Migrate `PreemptiveCompactionHook` to EventBus subscriber | `crates/uira-hooks/src/hooks/preemptive_compaction.rs` | M | Integration |
| 3.1.12 | Add compaction progress indicator to TUI | `crates/uira-tui/src/app.rs` | M | Integration |
| 3.1.13 | Store compaction metadata in session for resume | `crates/uira-agent/src/rollout.rs` | M | Unit |

---

## Phase 4: Approval Caching with Disk Persistence (Week 5)

### Disk Persistence Format
```json
// .uira/approvals/ses_abc123.json
{
  "version": 1,
  "session_id": "ses_abc123",
  "approvals": [
    {
      "key_hash": "a1b2c3...",
      "tool": "edit",
      "pattern": "src/**/*.rs",
      "decision": "ApproveForSession",
      "created_at": "2026-02-03T10:00:00Z",
      "expires_at": null
    }
  ]
}
```

### Tasks

| Task ID | Task | Files | Size | Tests |
|---------|------|-------|------|-------|
| 4.1.1 | Create `ApprovalKey` struct with hash generation | `crates/uira-agent/src/approval.rs` | M | Unit |
| 4.1.2 | Define `CacheDecision` enum variants | `crates/uira-protocol/src/tools.rs` | S | Unit |
| 4.1.3 | Implement in-memory `ApprovalCache` with HashMap | `crates/uira-agent/src/approval.rs` | M | Unit |
| 4.1.4 | Add TTL/expiration support | `crates/uira-agent/src/approval.rs` | M | Unit |
| 4.1.5 | Implement disk persistence to `.uira/approvals/{session_id}.json` | `crates/uira-agent/src/approval.rs` | M | Integration |
| 4.1.6 | Load persisted approvals on session resume | `crates/uira-agent/src/approval.rs` | M | Integration |
| 4.1.7 | Integrate cache lookup in `ToolOrchestrator` | `crates/uira-tools/src/orchestrator.rs` | M | Integration |
| 4.1.8 | Add `ApprovalCached` event | `crates/uira-events/src/events.rs` | S | Unit |
| 4.1.9 | Add approval cache status to TUI statusbar | `crates/uira-tui/src/app.rs` | S | Integration |

---

## Phase 5: Pattern Permissions (Allow Default) (Week 5)

### Permission Evaluation Logic
```rust
pub fn evaluate(permission: &str, pattern: &str, rules: &[Rule]) -> Action {
    // Find last matching rule (later rules override earlier)
    let matched = rules.iter().rev().find(|r| 
        glob_match(&r.permission, permission) && 
        glob_match(&r.pattern, pattern)
    );
    
    // Default: Allow (per user preference)
    matched.map(|r| r.action).unwrap_or(Action::Allow)
}
```

### Tasks

| Task ID | Task | Files | Size | Tests |
|---------|------|-------|------|-------|
| 5.1.1 | Create `crates/uira-permissions` crate | New crate | S | - |
| 5.1.2 | Define `Permission` enum with hierarchical structure | `crates/uira-permissions/src/types.rs` | M | Unit |
| 5.1.3 | Define `Action` enum: `Allow, Deny, Ask` | `crates/uira-permissions/src/types.rs` | S | Unit |
| 5.1.4 | Implement glob pattern matching with `globset` | `crates/uira-permissions/src/pattern.rs` | M | Unit |
| 5.1.5 | Create `PermissionRule` struct | `crates/uira-permissions/src/rule.rs` | M | Unit |
| 5.1.6 | Implement `PermissionEvaluator` with Allow as default | `crates/uira-permissions/src/evaluator.rs` | M | Unit |
| 5.1.7 | Add path expansion (`~/`, `$HOME/`, `$CWD/`) | `crates/uira-permissions/src/pattern.rs` | M | Unit |
| 5.1.8 | Add `permissions` section to config schema | `crates/uira-config/src/schema.rs` | M | Unit |
| 5.1.9 | Integrate evaluator before approval flow | `crates/uira-tools/src/orchestrator.rs` | M | Integration |
| 5.1.10 | Add `PermissionEvaluated` event | `crates/uira-events/src/events.rs` | S | Unit |

---

## Phase 6: Sandbox Retry & Config Enhancement (Week 6)

### Tasks

| Task ID | Task | Files | Size | Tests |
|---------|------|-------|------|-------|
| 6.1.1 | Add `SandboxDenied` variant to ToolError | `crates/uira-tools/src/lib.rs` | S | Unit |
| 6.1.2 | Implement two-attempt retry in orchestrator | `crates/uira-tools/src/orchestrator.rs` | M | Integration |
| 6.1.3 | Use cached approval for retry (no re-prompt) | `crates/uira-tools/src/orchestrator.rs` | M | Integration |
| 6.1.4 | Add `ToolRetried` event | `crates/uira-events/src/events.rs` | S | Unit |
| 6.1.5 | Enhanced config precedence (7 levels) | `crates/uira-config/src/loader.rs` | M | Unit |
| 6.1.6 | Skills trigger-based injection | `crates/uira-hooks/src/hooks/learner.rs` | M | Integration |
| 6.1.7 | Transform hooks (PrePrompt, PostResponse) | `crates/uira-events/src/events.rs` | M | Integration |

---

## Phase 7: Polish & Telemetry (Week 7+)

### Tasks

| Task ID | Task | Files | Size |
|---------|------|-------|------|
| 7.1.1 | Create `crates/uira-telemetry` crate | New crate | M |
| 7.1.2 | Add tracing spans with `tracing` crate | Throughout | M |
| 7.1.3 | Token usage metrics | `crates/uira-telemetry/` | M |
| 7.1.4 | Optional OTLP export | `crates/uira-telemetry/` | M |
| 7.1.5 | Plan mode implementation | `crates/uira-agent/src/plan.rs` | M |
| 7.1.6 | Multi-agent ThreadManager | `crates/uira-agent/src/thread_manager.rs` | L |

---

## Backwards Compatibility

1. **Event Bus Migration**: Includes `LegacyHookAdapter` for gradual transition
2. **Config Schema**: All new fields have defaults, existing configs remain valid
3. **Session Format**: Forking adds optional fields, old sessions load without them
4. **Approval**: Disk persistence is additive, memory-only still works

---

## Summary

| Metric | Count |
|--------|-------|
| Total Tasks | 65 |
| New Crates | 3 |
| Modified Crates | 7 |
| Estimated Duration | 7 weeks |

---

*Generated: 2026-02-03*
*Based on: sst/opencode, openai/codex competitive analysis*
