# Architectural Decisions

## 1. Module Organization
**Decision**: Use `mod.rs` + `types.rs` pattern for all modules

**Rationale**:
- Matches existing astrape-features conventions (astrape_state, model_routing)
- Separates type definitions from implementation logic
- Provides clean public API via re-exports in mod.rs

**Alternatives Considered**:
- Single file per module: Rejected due to large file sizes (verification ~500 lines)
- Multiple implementation files: Deferred until modules grow larger

## 2. Async Runtime Choice
**Decision**: Use tokio with full features for verification module

**Rationale**:
- Industry standard for async Rust
- Already used elsewhere in astrape workspace
- Provides comprehensive async utilities (spawn, join_all)

**Alternatives Considered**:
- async-std: Less ecosystem support
- Blocking only: Would lose parallel verification capability

## 3. Static Cache Implementation
**Decision**: Use `OnceLock` instead of `static mut` for builtin_skills cache

**Rationale**:
- Thread-safe without unsafe blocks
- Aligns with Rust 2024 edition best practices
- Prevents undefined behavior from mutable static references

**Trade-offs**:
- Cannot clear cache once initialized (acceptable for production)
- Slightly more complex API than simple static mut

## 4. Error Handling Strategy
**Decision**: Use `Result<T, String>` for command execution errors

**Rationale**:
- Simple error messages sufficient for verification failures
- Avoids creating custom error types for straightforward cases
- Easy to convert to structured errors later if needed

**Future Consideration**:
- Could introduce `VerificationError` enum for richer error context

## 5. Type Mapping Choices

### ComplexityTier Duplication
**Decision**: Duplicate `ComplexityTier` enum in delegation_categories instead of importing from model_routing

**Rationale**:
- Avoids circular dependency between modules
- Each module owns its type definitions
- Small enum, duplication cost is low

**Alternative**: Could extract to shared types module if more duplication emerges

### Thinking Budget as Enum
**Decision**: Use enum `ThinkingBudget` instead of raw token counts

**Rationale**:
- Type-safe representation of budget levels
- Easy to map to token counts via function
- Matches TypeScript's string literal union pattern

## 6. Test Strategy
**Decision**: Include unit tests in each module's mod.rs file

**Rationale**:
- Tests live close to implementation
- Easy to run module-specific tests
- Follows Rust convention (#[cfg(test)] mod tests)

**Coverage Goals**:
- Core functionality: 100% (all public functions tested)
- Edge cases: Covered where critical (e.g., empty inputs, boundary values)
- Integration: Deferred to higher-level crate tests

## 7. Dependency Management
**Decision**: Add tokio and futures to astrape-features Cargo.toml

**Rationale**:
- Required for async verification
- Workspace-level versions ensure consistency
- Full tokio features for flexibility (can optimize later)

**Future Optimization**:
- Could reduce tokio features to only what's needed (rt-multi-thread, macros)
