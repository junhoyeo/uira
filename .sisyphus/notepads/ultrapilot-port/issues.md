# Issues: Ultrapilot Port to Rust

## Known Limitations

### 1. No Async Hook Implementation
**Issue**: `UltrapilotHook` does not implement the `Hook` trait (unlike `ultrawork`/`ralph`)

**Impact**: Cannot be registered in `HookRegistry` for lifecycle events

**Reason**: TypeScript version is a coordinator module, not a lifecycle hook. It's called programmatically by other code, not triggered by events.

**Future Work**: If ultrapilot needs to intercept stop/start events, add `#[async_trait] impl Hook` with appropriate event handlers.

### 2. Regex Engine Differences
**Issue**: Rust `regex` crate uses RE2 engine (no lookahead/lookbehind support)

**Impact**: Cannot use advanced regex features from JavaScript

**Mitigation**: Current patterns don't require lookahead - simple character classes work. If future TS patterns use lookahead, will need alternative approach (e.g., multiple passes, manual parsing).

### 3. No Global State File
**Issue**: Unlike `ultrawork`/`ralph`, ultrapilot only stores local state in `.omc/state/`

**Impact**: State doesn't persist across different project directories

**Reason**: TypeScript version only uses local state. Ultrapilot is project-specific (workers tied to specific codebase).

**Future Work**: If cross-project ultrapilot coordination is needed, add global state file in `~/.claude/ultrapilot-state.json`.

## Resolved Issues

### ✅ Multiline Regex
**Problem**: TS uses `/pattern/gm` flag for multiline matching
**Solution**: Use `(?m)` inline flag in Rust regex pattern

### ✅ Optional Field Serialization
**Problem**: TS omits `null` fields in JSON, Rust serializes `None` as `null` by default
**Solution**: Use `#[serde(skip_serializing_if = "Option::is_none")]`

### ✅ Enum String Serialization
**Problem**: TS uses string literals `'pending' | 'running'`, Rust enums serialize as `{"Pending": null}` by default
**Solution**: Use `#[serde(rename_all = "lowercase")]` on enum

## Non-Issues (False Alarms)

### ❌ State Directory Creation
**Not an issue**: `ensure_state_dir` creates `.omc/state/` recursively - works correctly

### ❌ HashMap Initialization
**Not an issue**: `HashMap::new()` in `FileOwnership::default()` is correct - no need for capacity hint

### ❌ Test Flakiness
**Not an issue**: All 8 tests pass consistently - no timing/ordering issues
