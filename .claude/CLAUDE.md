# Astrape - Native Claude Code Plugin

This project uses **Astrape** as the Claude Code plugin, replacing oh-my-claudecode with a native Rust implementation.

## Plugin Architecture

Astrape provides:
- **Native NAPI bindings** for high-performance hook execution
- **Keyword detection** for mode activation (ultrawork, search, analyze, etc.)
- **Agent definitions** with tiered model routing
- **Skill system** for reusable workflows

## Available Agents

Use `Task` tool with `subagent_type="astrape:<agent>"` prefix:

| Agent | Tier | Description |
|-------|------|-------------|
| `architect` | HIGH (Opus) | Architecture & debugging advisor |
| `executor` | MEDIUM (Sonnet) | Focused task executor |
| `explore` | LOW (Haiku) | Fast codebase search |
| `designer` | MEDIUM (Sonnet) | UI/UX design specialist |
| `writer` | LOW (Haiku) | Documentation writer |
| `qa-tester` | HIGH (Opus) | CLI testing specialist |
| `security-reviewer` | HIGH (Opus) | Security vulnerability detection |
| `build-fixer` | MEDIUM (Sonnet) | Build error resolution |

### Tiered Variants

Each agent has tiered variants for cost optimization:
- `-low` suffix: Uses Haiku (fast, cheap)
- `-medium` suffix: Uses Sonnet (balanced)
- `-high` suffix: Uses Opus (most capable)

Example: `architect-low`, `executor-high`, `explore-medium`

## Keyword Detection

The following keywords trigger mode activation:

| Keyword | Mode |
|---------|------|
| `ultrawork`, `ulw` | Maximum parallel execution |
| `search`, `find` | Search mode with explore agent |
| `analyze`, `debug` | Deep analysis mode |
| `plan` | Planning mode |

## Project Structure

```
crates/
├── astrape/          # Main CLI binary
├── astrape-napi/     # Node.js native bindings
├── astrape-hooks/    # Hook implementations
├── astrape-agents/   # Agent definitions
├── astrape-features/ # Skills, model routing
└── astrape-core/     # Shared types
```

## Development

```bash
# Build all crates
cargo build --release

# Build NAPI module
cd crates/astrape-napi && npm run build

# Run tests
cargo test

# Run with pre-commit hooks
git commit  # triggers astrape hooks
```

## Hooks Configuration

Local hooks are in `.claude/hooks/`:
- `astrape-user-prompt.mjs` - Keyword detection
- `astrape-stop.mjs` - Continuation control
- `astrape-pre-tool.mjs` - Tool use hooks
- `astrape-session-start.mjs` - Session initialization

Settings in `.claude/settings.json` point to these hooks.
