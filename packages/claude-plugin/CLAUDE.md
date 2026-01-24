# Astrape - Native Rust Multi-Agent Orchestration

High-performance Claude Code plugin powered by native Rust bindings.

## Quick Start

Just say what you want:
- "build me a REST API"
- "ultrawork: fix all errors"
- "analyze why this test fails"

## Keywords

| Keyword | Mode | Description |
|---------|------|-------------|
| `ultrawork` / `ulw` | Parallel | Maximum parallel execution |
| `analyze` / `debug` | Analysis | Deep investigation |
| `search` / `find` | Search | Comprehensive codebase search |
| `plan` | Planning | Strategic planning interview |

## Agents (32 total)

### Primary Agents

| Agent | Model | Use For |
|-------|-------|---------|
| `architect` | Opus | Complex problems, architecture |
| `executor` | Sonnet | Implementation tasks |
| `explore` | Haiku | Fast codebase search |
| `designer` | Sonnet | UI/UX work |
| `researcher` | Sonnet | External docs, references |
| `writer` | Haiku | Documentation |

### Tiered Variants

Each agent has `-low` (Haiku), `-medium` (Sonnet), `-high` (Opus) variants for cost optimization.

Example: `architect-low` for quick lookups, `executor-high` for complex refactoring.

## Model Routing

Astrape automatically routes tasks to appropriate model tiers:
- Simple lookups → Haiku (fast, cheap)
- Standard work → Sonnet (balanced)
- Complex reasoning → Opus (most capable)

## Performance

Native Rust NAPI bindings provide:
- Sub-millisecond keyword detection
- Efficient hook execution
- Low memory footprint

## Skills

| Skill | Description |
|-------|-------------|
| `/astrape:ultrawork` | Maximum parallel execution |
| `/astrape:analyze` | Deep analysis mode |
| `/astrape:search` | Comprehensive search |
| `/astrape:plan` | Strategic planning |
| `/astrape:help` | Usage guide |
