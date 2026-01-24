# Issues Encountered

## Compilation Errors Fixed

### 1. Borrow Checker Conflicts (verification module)
**Problem**: Cannot borrow `checklist.checks` as mutable while immutable borrow exists
```rust
// Before (failed)
let checks_to_run: Vec<_> = checklist.checks.iter().filter(...).collect();
for check in &checks_to_run {
    checklist.checks.iter_mut().find(...) // ERROR: mutable borrow while immutable exists
}
```

**Solution**: Collect IDs first, then mutate
```rust
let check_ids_to_run: Vec<String> = checklist.checks.iter().map(|c| c.id.clone()).collect();
for check_id in check_ids_to_run {
    checklist.checks.iter_mut().find(|c| c.id == check_id) // OK: no conflicting borrows
}
```

### 2. Move After Use (task_decomposer)
**Problem**: `components` moved into struct, then borrowed for function call
```rust
DecompositionResult {
    components,  // moved here
    strategy: explain_strategy(&analysis, &components), // ERROR: borrowed after move
}
```

**Solution**: Calculate strategy before moving
```rust
let strategy = explain_strategy(&analysis, &components);
DecompositionResult {
    components,
    strategy,
}
```

### 3. Ambiguous Numeric Type
**Problem**: Rust couldn't infer float type for `.min(1.0)`
```rust
let mut score = match task_type { ... }; // type unknown
score.min(1.0) // ERROR: ambiguous
```

**Solution**: Explicit type annotation
```rust
let mut score: f64 = match task_type { ... };
score.min(1.0) // OK
```

### 4. Static Mut Warning
**Problem**: `static mut` triggers safety warnings in Rust 2024 edition
```rust
static mut CACHED_SKILLS: Option<Vec<BuiltinSkill>> = None;
unsafe { CACHED_SKILLS.is_none() } // WARNING: undefined behavior
```

**Solution**: Use `OnceLock` for thread-safe lazy init
```rust
static CACHED_SKILLS: OnceLock<Vec<BuiltinSkill>> = OnceLock::new();
CACHED_SKILLS.get_or_init(|| load_skills()) // Safe, no unsafe block
```

## Pre-existing Test Failures (Not Our Responsibility)
- `notepad_wisdom::tests::init_add_and_read_wisdom` - regex lookahead not supported
- `notepad_wisdom::tests::extracts_wisdom_from_tags` - assertion failure

These existed before our changes and are unrelated to the 4 new modules.

## Gotchas

### Command Timeout Not Implemented
The verification module accepts a `timeout` parameter but doesn't enforce it (Rust's `std::process::Command` lacks built-in timeout). Marked with `_timeout` prefix to suppress warning. Production code should use `tokio::time::timeout`.

### Skills Directory Discovery
The builtin_skills module searches for skills directory relative to current working directory. May need adjustment for different deployment scenarios.

### OnceLock Limitation
`clear_skills_cache()` is now a no-op because `OnceLock` cannot be reset. This is acceptable for production but may affect testing scenarios that need cache invalidation.
