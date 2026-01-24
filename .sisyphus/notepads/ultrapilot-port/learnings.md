# Learnings: Ultrapilot Port to Rust

## TypeScript → Rust Patterns

### State Management
- TS uses `fs.readFileSync/writeFileSync` → Rust uses `fs::read_to_string/write`
- TS stores in `.omc/state/` → Rust follows same convention
- TS uses `JSON.parse/stringify` → Rust uses `serde_json::from_str/to_string_pretty`

### Regex Differences
- **CRITICAL**: Rust `regex` crate does NOT support lookahead/lookbehind
- TS pattern `/^[\s]*(?:\d+\.|[-*+])\s+(.+)$/gm` → Rust `(?m)^[\s]*(?:\d+\.|[-*+])\s+(.+)$`
- Multiline mode: TS uses `/gm` flag → Rust uses `(?m)` inline flag
- Capture groups work identically in both

### Type Conversions
- TS `Record<string, string[]>` → Rust `HashMap<String, Vec<String>>`
- TS `string | null` → Rust `Option<String>`
- TS `number` → Rust `u32` (for counts/iterations) or `u64` (for timeouts)
- TS enum `'pending' | 'running' | ...` → Rust `#[serde(rename_all = "lowercase")] enum`

### Serde Patterns
- Use `#[serde(skip_serializing_if = "Option::is_none")]` for optional fields
- Use `#[serde(default)]` for fields with default values
- Use `#[serde(default = "function_name")]` for custom defaults
- Enum serialization: `#[serde(rename_all = "lowercase")]` matches TS string literals

### File Ownership
- TS uses `Record<string, string[]>` for workers map
- Rust uses `HashMap<String, Vec<String>>` with `#[derive(Default)]`
- Both use `Vec<String>` for coordinator/conflicts arrays

## Code Organization
- Followed existing hook patterns from `ultrawork.rs`, `ultraqa.rs`, `ralph.rs`
- State file path: `.omc/state/ultrapilot-state.json`
- No async trait needed (unlike ultrawork/ralph hooks) - this is a pure state module
- Exported all public types in `hooks/mod.rs` and `lib.rs`

## Testing Strategy
- Unit tests cover: config defaults, task decomposition (numbered/bulleted/sentences/single), serialization, integration summary
- All 8 tests pass
- Build succeeds with zero warnings
