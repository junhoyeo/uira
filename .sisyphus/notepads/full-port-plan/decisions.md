## Decisions

- Keep TypeScript quirks/bugs when explicitly observable (non-interactive banned command reporting).
- Avoid new dependencies; implement path normalization internally instead of pulling in a path-cleaning crate.
- Add narrow env override for storage directory used only by tests to prevent cross-test interference.
- For background-task-related hooks, allow a test-only tasks-dir override via `ASTRAPE_BACKGROUND_TASKS_DIR` to avoid writing to real `~/.claude/.omc/background-tasks`.
- Scope auto slash command expansion to `/astrape:*` to avoid interfering with Claude Code built-in `/` commands.

- Keep `rules_injector` glob matching behavior identical to the TS implementation (simple string->regex), even when it differs from typical globstar semantics.
- Avoid adding a hashing dependency for `rules_injector`; implement minimal SHA-256 internally for deterministic 16-char content hashes.

- Prefer regex constructions compatible with Rust `regex` (no lookaround, no non-capturing groups) even when porting TS patterns that use them.

- Keep `learner` detection regex patterns identical to the TS source (even when some phrases like "fixed the bug by" don't match).
- Implement `learner` skill caching as an in-process map keyed by `project_root` with an explicit `clear_loader_cache()` invalidation (called after writing a new skill) to avoid filesystem watching.

- Keep tool handler wiring minimal: `ToolDefinition` stores an `Arc<dyn ToolHandler>` returning a boxed `Future`, avoiding `async-trait` until full execution is implemented.
- Prefer the OpenCode-style `filePath` key in JSON schemas for file-based tools; upstream hooks already support `filePath`/`path`/`file` key variants when extracting paths.
