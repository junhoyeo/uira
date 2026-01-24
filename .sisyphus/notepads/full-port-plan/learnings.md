## Learnings

- `directory_readme_injector` TS storage path is under `~/.omc/directory-readme/<session>.json`; in Rust we mirrored this and added a test-only env override (`OMC_README_INJECTOR_STORAGE_DIR`) to avoid mutating process-wide `HOME` during parallel `cargo test`.
- `non_interactive_env` TS implementation has an indexing bug: banned regexes are built from a filtered list (removing items containing `(`) but the reported banned command is indexed from the unfiltered list; port preserves this behavior.
- `preemptive_compaction` token estimation is `ceil(len/4)`; context limit toggles to 1M when `ANTHROPIC_1M_CONTEXT` or `VERTEX_ANTHROPIC_1M_CONTEXT` is `true`.
- `auto_slash_command` port mirrors TS structure (detector + executor + constants + types) in a single module; regex is anchored and avoids lookahead/lookbehind.
- Prefer testing command discovery via project `.claude/commands` under a temp working directory (vs `~/.claude`) to avoid process-wide `HOME` mutation races during parallel `cargo test`.
- `agent_usage_reminder`/`background_notification` tests that mutate env need a shared lock (`Mutex`) to avoid cross-test interference in parallel `cargo test`.
- `rules_injector` glob conversion must handle `**/` as "zero or more" directories and must not perform `?`-glob replacement after inserting regex fragments containing `?` quantifiers.

- `rules_injector` TS glob-to-regex conversion is intentionally naive: `src/**/*.ts` does NOT match `src/main.ts` (it becomes `^src/.*/[^/]*\.ts$`). The Rust port preserves this behavior.
- `rules_injector` content dedupe uses SHA-256 hex truncated to 16 chars; implemented inline (no new `sha2` dependency).
- `omc_orchestrator` TS integration with boulder/plan state is not present in this crate; Rust port exposes the same helpers but stubs `check_boulder_continuation` to "no-op".

- `recovery` hook ports TS unified recovery (context limit + edit error + session repair) into `crates/astrape-hooks/src/hooks/recovery.rs`; context limit retry state is session-scoped with a 5-minute TTL.
- `thinking_block_validator` and `empty_message_sanitizer` hooks are implemented as `HookEvent::MessagesTransform` transforms that read message payloads from `HookInput.tool_input` (or `extra["messages"]`) and return updates via `HookOutput.modified_input`.
- Rust `regex` does not support non-capturing groups; globstar patterns must use capturing groups (e.g. `(.*/)?`) rather than `(?:.*/)?`.

- `autopilot` hook port uses a simple persisted state machine in `.omc/autopilot-state.json` with phase transitions driven by completion signals in the assistant output (`PLANNING_COMPLETE`, `EXECUTION_COMPLETE`, `AUTOPILOT_COMPLETE`).
- `learner` detection patterns should allow a subject token between "the/this" and "by/with/using" (e.g. "fixed the bug by ..."); keep patterns lookaround-free.
- `learner` loader needs to normalize skill IDs (trim whitespace/CR and stray quotes) before deduping project vs user skills.

- `learner` port uses test-only env overrides for home-based paths (`OMC_LEARNER_CONFIG_PATH`, `OMC_LEARNER_USER_SKILLS_DIR`) to avoid writing to real `~/.claude` during `cargo test`.
- `learner` YAML frontmatter parser is a minimal subset: string/int scalars and string arrays (inline `[...]` with quote-aware splitting, or multi-line `- item`).
- `learner` promotion needs only `learnings` from `ralph` progress, so the Rust port includes a minimal `progress.txt` reader that checks both `progress.txt` and `.omc/progress.txt`.

- Tool outputs in oh-my-claudecode tools consistently use `{ content: [{ type: "text", text: "..." }] }`; in Rust this maps cleanly to a `ToolOutput { content: Vec<ToolContent> }` with `#[serde(tag = "type")]` for `ToolContent::Text`.
- Tool input schemas in oh-my-claudecode are Zod raw shapes; for Rust tool foundations, representing schema as `serde_json::Value` keeps the crate lightweight and defers full validation/execution until later phases.
