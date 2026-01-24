# Learnings: TypeScript to Rust Module Port

## Successful Patterns

### Module Structure
- Used `mod.rs` + `types.rs` pattern consistently across all 4 modules
- Followed existing astrape-features conventions (astrape_state, model_routing)
- Exported types and functions at module root for clean API

### Type Conversions
- TypeScript union types → Rust enums with `#[derive(Serialize, Deserialize)]`
- TypeScript interfaces → Rust structs with serde support
- Optional fields → `Option<T>` with `#[serde(skip_serializing_if = "Option::is_none")]`
- String literals → `&'static str` or `String` depending on ownership needs

### Naming Conventions
- snake_case for modules, functions, variables (Rust convention)
- CamelCase for types, enums (Rust convention)
- Preserved semantic meaning from TypeScript (e.g., `boulder-` → `astrape-`)

### Async Handling
- Used tokio for async runtime in verification module
- Added `futures` crate for `join_all` pattern
- Converted Node.js `exec` → Rust `std::process::Command`

### Static Data
- Replaced `static mut` with `OnceLock` for thread-safe lazy initialization
- Used `HashMap` for configuration lookups instead of object literals

## Key Differences from TypeScript

### Ownership & Borrowing
- Had to clone `components` before moving into `DecompositionResult`
- Used `check_ids_to_run` to avoid borrow checker conflicts in verification
- Explicit lifetime management vs. garbage collection

### Error Handling
- No exceptions - used `Result<T, E>` pattern
- `Option<T>` for nullable values instead of `null | undefined`
- Pattern matching instead of try/catch

### Type System
- Explicit type annotations required (e.g., `f64` for floating point)
- No implicit type coercion
- Trait bounds for generic constraints

## Dependencies Added
- `tokio` with full features for async runtime
- `futures` for async utilities
- `regex` for pattern matching (already in workspace)
- `serde_json` for metadata HashMap values

## Test Coverage
All 4 modules include unit tests:
- task_decomposer: 5 tests (task type detection, complexity, decomposition)
- delegation_categories: 5 tests (category resolution, detection, prompt enhancement)
- builtin_skills: 3 tests (frontmatter parsing, skill listing)
- verification: 3 tests (protocol creation, checklist, summary generation)

Total: 16 new tests, all passing
