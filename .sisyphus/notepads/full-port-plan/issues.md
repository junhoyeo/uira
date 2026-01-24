## Issues

- `lsp_diagnostics` reported `rust-analyzer (unlinked-file)` hints for newly added hook modules even though `cargo test -p astrape-hooks` compiled them successfully; treated as tooling limitation.
- `preemptive_compaction` warning cooldown logic initially suppressed the very first warning when `last_warning_time == 0`; fixed by skipping cooldown check for the initial state.
- `rules_injector` glob matcher treated `**/` as requiring at least one directory and broke `src/**/*.ts` matching `src/main.ts`; fixed with an optional directory group and careful replacement ordering.

- `rules_injector` tests must reflect the upstream TS glob semantics (naive regex); `src/**/*.ts` does not match `src/main.ts`.

- `lsp_diagnostics` can read stale rust-analyzer state after large file edits; killing `rust-analyzer` processes forced a clean restart and cleared false syntax errors.

- `cargo test -p astrape-hooks` initially failed in `hooks::learner` due to an overly strict detection regex ("fixed the bug by" not matching) and non-normalized skill IDs preventing project-over-user override; fixed by loosening the regex and normalizing IDs during load.

- `learner` detection code consults disk config via `is_learner_enabled()`; unit tests must isolate config path (via `OMC_LEARNER_CONFIG_PATH`) to avoid user-local `~/.claude/omc/learner.json` affecting test outcomes.

- None encountered while scaffolding `astrape-tools`; all new crate files were clean under `lsp_diagnostics` and `cargo test -p astrape-tools`.
