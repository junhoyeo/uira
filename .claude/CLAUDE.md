# Astrape - Native Multi-Agent Orchestration

You are enhanced with multi-agent capabilities via Astrape's native Rust-powered orchestration.

## Quick Start

Just say what you want to build. Astrape activates automatically.

## Available Skills

| Skill | Trigger | Description |
|-------|---------|-------------|
| `/astrape:ultrawork` | `ultrawork`, `ulw` | Maximum parallel execution |
| `/astrape:analyze` | `analyze`, `debug` | Deep investigation |
| `/astrape:search` | `search`, `find` | Comprehensive codebase search |
| `/astrape:plan` | `plan` | Strategic planning |
| `/astrape:help` | - | Usage guide |

## Available Agents

Use `Task` tool with `subagent_type="astrape:<agent>"`:

| Agent | Model | Use For |
|-------|-------|---------|
| `architect` | Opus | Complex problems, architecture |
| `executor` | Sonnet | Implementation tasks |
| `explore` | Haiku | Fast codebase search |
| `designer` | Sonnet | UI/UX work |
| `researcher` | Sonnet | External docs, references |
| `writer` | Haiku | Documentation |
| `qa-tester` | Opus | CLI testing |
| `security-reviewer` | Opus | Security analysis |
| `build-fixer` | Sonnet | Build error resolution |

### Tiered Variants

Each agent has tiered variants: `-low` (Haiku), `-medium` (Sonnet), `-high` (Opus)

## Model Routing

Astrape automatically routes tasks to appropriate model tiers:
- Simple lookups → Haiku (fast, cheap)
- Standard work → Sonnet (balanced)
- Complex reasoning → Opus (most capable)

## Keyword Detection

| Keyword | Mode |
|---------|------|
| `ultrawork`, `ulw` | Maximum parallel execution |
| `search`, `find` | Search mode |
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

plugin/               # Claude Code plugin package
├── .claude-plugin/   # Plugin manifest
├── agents/           # 32 agent definitions
├── skills/           # Skill definitions
├── hooks/            # Bun-powered hooks
└── native/           # NAPI bindings
```

## Development

```bash
# Build all crates
cargo build --release

# Build NAPI module
cd crates/astrape-napi && bun run build

# Copy native module to plugin
cp crates/astrape-napi/astrape.darwin-arm64.node plugin/native/

# Run tests
cargo test
```
