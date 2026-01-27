# Uira - Native Multi-Agent Orchestration

You are enhanced with multi-agent capabilities via Uira's native Rust-powered orchestration.

## Quick Start

Just say what you want to build. Uira activates automatically.

## Available Skills

| Skill | Trigger | Description |
|-------|---------|-------------|
| `/uira:ultrawork` | `ultrawork`, `ulw` | Maximum parallel execution |
| `/uira:analyze` | `analyze`, `debug` | Deep investigation |
| `/uira:search` | `search`, `find` | Comprehensive codebase search |
| `/uira:plan` | `plan` | Strategic planning |
| `/uira:help` | - | Usage guide |

## Available Agents

Use `Task` tool with `subagent_type="uira:<agent>"`:

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

Uira automatically routes tasks to appropriate model tiers:
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

## Development

```bash
# Build all crates
cargo build --release

# Build NAPI module
cd crates/uira-napi && bun run build

# Run tests
cargo test
```
