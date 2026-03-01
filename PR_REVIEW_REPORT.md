# PR Review Report

Scope: Reviewed open PRs `#74`, `#75`, `#76`, `#77`, and `#78` by checking out each PR branch and inspecting every changed file.

## PR #74 - feat: add shell completion scripts

Changed files reviewed:
- `Cargo.lock`
- `crates/uira-cli/Cargo.toml`
- `crates/uira-cli/src/commands.rs`
- `crates/uira-cli/src/main.rs`
- `crates/uira/src/agent_workflow/detectors/typos.rs`

Findings:
- No functional issues found in changed lines.

Notes:
- `cargo test -p uira-cli --no-run` completed successfully on this branch during review.

## PR #75 - feat: add external editor support (Ctrl+G)

Changed files reviewed:
- `crates/uira-tui/Cargo.toml`
- `crates/uira-tui/src/app.rs`

### Issue 75-1
- File: `crates/uira-tui/src/app.rs:894`
- Problem: `run_editor_command` uses `sh -c` with `eval "$UIRA_EDITOR" "$1"`. `eval` concatenates arguments and re-parses them, which breaks file paths containing spaces and can alter argument semantics unexpectedly.
- Why it matters: editor invocation can fail on valid temp paths in environments where temp directories include spaces.
- Suggested fix: avoid `eval`; parse editor command into executable + args (e.g., with shell-words parsing) and invoke via `Command::new(program).args(parsed_args).arg(temp_path)`.

## PR #76 - feat: add session share to GitHub Gist

Changed files reviewed:
- `Cargo.lock`
- `README.md`
- `crates/uira-tui/Cargo.toml`
- `crates/uira-tui/src/app.rs`
- `crates/uira-tui/src/events.rs`
- `crates/uira/src/agent_workflow/detectors/typos.rs`

### Issue 76-1
- File: `crates/uira-tui/src/app.rs:153`
- Problem: `format_gh_error` treats any stderr containing `"not found"` as "gh is not installed".
- Why it matters: unrelated gh failures can include "not found" (e.g., 404-style API errors), causing misleading diagnostics and incorrect remediation guidance.
- Suggested fix: narrow detection to command-launch failures (`io::ErrorKind::NotFound`) and explicit CLI-not-installed patterns; do not classify generic `"not found"` substrings as missing binary.

Notes:
- `cargo test -p uira-tui --no-run` completed successfully on this branch during review.

## PR #77 - feat: add keyboard shortcuts for TODO sidebar

Changed files reviewed:
- `crates/uira-tui/src/app.rs`

### Issue 77-1
- File: `crates/uira-tui/src/app.rs:738`
- Problem: new TODO shortcuts (`t`, `d`) are implemented inside the `Ctrl`-modifier branch (`if key.modifiers.contains(KeyModifiers::CONTROL)`), so plain `t`/`d` do not trigger sidebar actions.
- Why it matters: feature behavior does not match the PR intent/description and users cannot use advertised shortcuts.
- Suggested fix: handle `KeyCode::Char('t'/'d')` in the non-control input path (or in a dedicated global non-control shortcut block) before text insertion.

## PR #78 - feat: add collapsible tool output in TUI

Changed files reviewed:
- `crates/uira-tui/src/app.rs`
- `crates/uira-tui/src/widgets/chat.rs`

### Issue 78-1
- File: `crates/uira-tui/src/app.rs:767`
- Problem: collapse/expand-all shortcuts (`o` / `Shift+O`) are implemented under the `Ctrl`-modifier branch, requiring `Ctrl+o` / `Ctrl+Shift+o` instead of plain `o` / `Shift+O`.
- Why it matters: behavior diverges from documented/intended shortcut UX.
- Suggested fix: move `o` / `Shift+O` handling to non-control key handling (before normal character insertion), similar to other global non-control shortcuts.

Notes:
- `cargo test -p uira-tui --no-run` completed successfully on this branch during review.
