# Plan v3.0 Updates - Critical Changes

## Date: 2025-01-24

### TL;DR

**BEFORE (v2.0):** Git hooks manager porting oh-my-claudecode  
**AFTER (v3.0):** **Full multi-agent orchestration LIBRARY** (Rust equivalent of oh-my-claudecode)

**CRITICAL CHANGE:** Phase 0 reordered - SDK integration is now **FIRST** (0.1), not last (0.4)

---

## Major Changes

### 1. Architecture Clarification ⭐ **CRITICAL**

**What Changed:**
- Astrape is transforming into a **LIBRARY**, not just a CLI tool
- Will export similar functionality to oh-my-claudecode
- Publishable as: Rust crate + npm package (napi-rs) + optional CLI

**Added Section:** "Astrape as a Library (Not Just a CLI)"
- Shows library export patterns
- Usage examples (Rust, CLI, npm)
- Distribution strategy

### 2. Phase 0 Reordering ⚠️ **MAKE-OR-BREAK**

**What Changed:**
```
OLD ORDER: 0.1 (Comment) → 0.2 (Hooks) → 0.3 (Prompts) → 0.4 (SDK)
NEW ORDER: 0.1 (SDK) → 0.2 (Comment) → 0.3 (Hooks) → 0.4 (Prompts)
```

**Why:** SDK integration is highest risk. Test FIRST before investing 3 weeks in other prototypes.

**If Phase 0.1 fails:** ABORT ENTIRE PORT (Rust cannot integrate with TypeScript SDK)

### 3. Phase 0.1 Detailed Execution Guide

**Added:** 12-day step-by-step guide for SDK integration
- Day-by-day breakdown
- Code examples
- Success criteria checklist
- Failure indicators

### 4. Executive Summary Updates

**Added:**
- Astrape transformation explanation (FROM/TO)
- "oh-my-claudecode but in Rust" positioning
- Deliverables: crate + npm + CLI
- Timeline: 9-12 months (was 6-9)
- Risk: HIGH (was MEDIUM)

### 5. Risk Assessment Rewrite

**New #1 Risk:** SDK integration failure (BLOCKER)
- Probability: HIGH
- Impact: BLOCKER
- Mitigation: Test in Phase 0.1 FIRST

**New Risks Added:**
- napi-rs performance overhead (10-20%)
- TypeScript SDK updates breaking bindings
- Async complexity (Rust ↔ Node.js)

### 6. Success Metrics Updates

**Added:**
- Library distribution metrics
- npm package publication
- API surface compatibility with oh-my-claudecode
- Migration guide requirement

**Performance Metrics Adjusted:**
- Hook execution: < 20ms (was 10ms) - allow napi overhead
- Prompt generation: < 100ms (was 50ms) - allow tera + napi overhead

### 7. Summary Table

**Effort Updated:**
- Total: 1,173 hours (was 1,143)
- Duration: 31 weeks (was 29)
- Timeline: 9-12 months (was 7-9)

**Phase 0 Breakdown:**
- 0.1 (SDK): 80 hours / 2 weeks ← **DO THIS FIRST**
- 0.2 (Comment): 43 hours / 1 week
- 0.3 (Hooks): 32 hours / 4 days
- 0.4 (Prompts): 28 hours / 3-4 days

### 8. Go/No-Go Checkpoints

**Reordered:**
1. **Phase 0.1 Complete** → ABORT if fail (was #4)
2. Phase 0.2 Complete → Reassess tree-sitter
3. Phase 0.3 Complete → Reassess async-trait
4. Phase 0.4 Complete → Reassess tera

### 9. Version & Status

**Updated:**
- Version: 3.0 (was 2.0)
- Subtitle: "FULL PORT (Library Architecture)"
- Status: "START WITH PHASE 0.1 (SDK INTEGRATION)"

### 10. Plan Validation Checklist

**Added at end:**
- 8-item checklist to verify understanding
- Critical reminders (5 items)
- "DO Phase 0.1 FIRST" emphasis

---

## What Didn't Change

✅ Target: oh-my-claudecode (not oh-my-opencode)  
✅ Technology choices: tera, tree-sitter, async-trait, napi-rs  
✅ Phase 1-6 structure (implementation phases)  
✅ Crate architecture diagram  

---

## Critical Reminders for Execution

1. **START WITH PHASE 0.1** - SDK integration before anything else
2. **Library-first design** - Astrape exports like oh-my-claudecode
3. **Accept napi-rs overhead** - 10-20% performance cost is expected
4. **Full port required** - No shortcuts, no hybrid approach
5. **If 0.1 fails, STOP** - Don't continue if SDK integration doesn't work

---

## Files Modified

- `/Users/junhoyeo/astrape/.sisyphus/plans/full-port-plan.md` (v2.0 → v3.0)
  - 751 lines → 971 lines (+220 lines)
  - 9 major sections added/updated

---

**READY FOR EXECUTION - BEGIN PHASE 0.1**
