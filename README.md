![Astrape](./.github/assets/cover.jpg)

<div align="center">
  <h1>Astrape</h1>
  <p>Native Rust-powered multi-agent orchestration for Claude Code</p>
</div>

Astrape (Greek: "lightning") provides high-performance multi-agent orchestration, smart model routing, and AI-assisted development tools as a Claude Code plugin.

## Features

- **32 Specialized Agents** - Architect, Designer, Executor, Explorer, Researcher, and more with tiered variants (Haiku/Sonnet/Opus)
- **Smart Model Routing** - Automatically select the right model based on task complexity
- **Native Performance** - Sub-millisecond keyword detection via Rust NAPI bindings
- **Comment Checker** - Tree-sitter powered detection of problematic comments/docstrings
- **Background Task Notifications** - Track and notify on background agent completions
- **Skill System** - Extensible skill templates (ultrawork, analyze, plan, search)
- **Git Hooks** - Configurable pre/post commit hooks via `astrape.yml`

## Git Hooks

Astrape provides a standalone CLI for git hook management. Configure hooks in `astrape.yml`:

```yaml
ai:
  model: anthropic/claude-sonnet-4-20250514

pre-commit:
  parallel: false  # fmt must run first before clippy
  commands:
    - name: fmt
      run: |
        staged=$(git diff --cached --name-only --diff-filter=ACM | grep '\.rs$' || true)
        [ -z "$staged" ] && exit 0
        echo "$staged" | xargs cargo fmt --
        echo "$staged" | xargs git add
    - name: clippy
      run: cargo clippy -- -D warnings
    - name: typos
      run: ./target/debug/astrape typos --ai --stage

post-commit:
  commands:
    - name: auto-push
      run: git push origin HEAD
```

Install hooks with:
```bash
astrape install
```

## Quick Start

### As Claude Code Plugin

```bash
# Clone and build
git clone https://github.com/junhoyeo/Astrape astrape
cd astrape
cargo build --release

# Build NAPI bindings
cd crates/astrape-napi && bun run build

# Copy native module to plugin
cp *.node ../../packages/claude-plugin/native/

# Install plugin in Claude Code
# Add packages/claude-plugin to your Claude Code plugins
```

### Usage in Claude Code

Just talk naturally - Astrape activates automatically:

```
"ultrawork: fix all TypeScript errors"    → Maximum parallel execution
"analyze why this test fails"             → Deep investigation mode
"search for authentication handling"      → Comprehensive codebase search
"plan the new API design"                 → Strategic planning interview
```

## Agents

| Category | Agents |
|----------|--------|
| **Analysis** | architect, architect-medium, architect-low, analyst, critic |
| **Execution** | executor, executor-high, executor-low |
| **Search** | explore, explore-medium, explore-high |
| **Design** | designer, designer-high, designer-low |
| **Testing** | qa-tester, qa-tester-high, tdd-guide, tdd-guide-low |
| **Security** | security-reviewer, security-reviewer-low |
| **Build** | build-fixer, build-fixer-low |
| **Research** | researcher, researcher-low, scientist, scientist-high, scientist-low |
| **Other** | writer, vision, planner, code-reviewer, code-reviewer-low |

### Model Tiers

| Tier | Model | Use Case |
|------|-------|----------|
| LOW | Haiku | Quick lookups, simple tasks |
| MEDIUM | Sonnet | Standard implementation |
| HIGH | Opus | Complex reasoning, architecture |

## Skills

| Skill | Trigger | Description |
|-------|---------|-------------|
| `/astrape:ultrawork` | `ultrawork`, `ulw` | Maximum parallel execution |
| `/astrape:analyze` | `analyze`, `debug` | Deep investigation |
| `/astrape:search` | `search`, `find` | Comprehensive codebase search |
| `/astrape:plan` | `plan` | Strategic planning |
| `/astrape:help` | - | Usage guide |

## Hooks

| Event | Handler |
|-------|---------|
| `UserPromptSubmit` | Keyword detection, background notifications |
| `PreToolUse` | README injection, tool validation |
| `PostToolUse` | Comment checker, background task tracking |
| `SessionStart` | State initialization |
| `Stop` | Continuation control |

## Development

```bash
# Build all crates
cargo build --workspace --release

# Build NAPI module
cd crates/astrape-napi && bun run build

# Run tests
cargo test --workspace

# Build comment-checker
cargo build --release -p astrape-comment-checker
```

## Architecture

The plugin uses native Rust NAPI bindings for performance-critical operations:

- **astrape** - Standalone CLI for git hooks and dev tools
- **astrape-hook** - Keyword detection and pattern matching
- **astrape-hooks** - Hook implementations (22 hooks)
- **astrape-agents** - Agent definitions and prompt loading
- **astrape-features** - Model routing, skills, state management
- **astrape-napi** - Node.js bindings exposing Rust to the plugin
- **astrape-comment-checker** - Tree-sitter based comment detection
