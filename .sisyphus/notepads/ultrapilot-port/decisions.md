# Decisions: Ultrapilot Port to Rust

## Architecture Decisions

### 1. Pure State Module (No Hook Trait Implementation)
**Decision**: Implemented `UltrapilotHook` as a state management module without `#[async_trait] impl Hook`

**Rationale**:
- TypeScript version is primarily a coordinator/state manager, not a lifecycle hook
- Functions like `startUltrapilot`, `spawnWorkers`, `trackProgress` are called programmatically
- Unlike `ultrawork`/`ralph` which intercept stop events, ultrapilot manages worker orchestration
- Keeps the port focused on matching TS behavior exactly

### 2. Regex Pattern Simplification
**Decision**: Removed lookahead/lookbehind from list item pattern

**Original TS**: `/^[\s]*(?:\d+\.|[-*+])\s+(.+)$/gm`
**Rust**: `(?m)^[\s]*(?:\d+\.|[-*+])\s+(.+)$`

**Rationale**:
- Rust `regex` crate doesn't support lookahead/lookbehind (uses RE2 engine)
- Original pattern didn't use lookahead anyway - direct port works
- Multiline mode `(?m)` enables `^`/`$` to match line boundaries

### 3. Type Choices
**Decision**: Use `u32` for counts/iterations, `u64` for timeouts, `usize` for array indices

**Rationale**:
- `u32`: Sufficient for worker counts (max 5), iterations (max 3-10)
- `u64`: Matches millisecond precision for timeouts (300000ms = 5min)
- `usize`: Required for Vec indexing (`worker.index`)

### 4. Default Values via Functions
**Decision**: Use `#[serde(default = "function_name")]` instead of const values

**Rationale**:
- Allows complex defaults like `Vec<String>` for shared files
- Matches serde best practices
- Enables easy testing of default config

### 5. State File Location
**Decision**: Store in `.omc/state/ultrapilot-state.json` (not `.omc/ultrapilot-state.json`)

**Rationale**:
- TS version uses `.omc/state/` subdirectory
- Matches existing pattern from TS codebase
- Keeps state files organized in dedicated directory

## Implementation Choices

### HashMap vs BTreeMap
**Decision**: Use `HashMap<String, Vec<String>>` for worker ownership

**Rationale**:
- No need for sorted keys
- Faster lookups for file ownership checks
- Matches TS `Record` semantics

### Error Handling
**Decision**: Return `bool` for write operations, `Option<T>` for reads

**Rationale**:
- Matches existing Rust hook patterns (`ultrawork`, `ralph`)
- Simple success/failure for state writes
- `None` clearly indicates missing state for reads

### Test Coverage
**Decision**: 8 unit tests covering decomposition, serialization, defaults, summary generation

**Rationale**:
- Matches critical TS behavior: list parsing, config defaults, worker state
- Tests regex patterns (numbered/bulleted/sentences)
- Validates serde round-trip
- Ensures integration summary formatting
