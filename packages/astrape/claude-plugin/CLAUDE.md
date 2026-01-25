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

## Agents

### Standard Agents (use Task tool)

| Agent | Model | Use For |
|-------|-------|---------|
| `architect` | Opus | Complex problems, architecture |
| `executor` | Sonnet | Implementation tasks |
| `designer` | Sonnet | UI/UX work |
| `writer` | Haiku | Documentation |

### Custom-Routed Agents (use spawn_agent MCP tool)

These agents use non-Anthropic models and require the `spawn_agent` MCP tool:

| Agent | Model | Use For |
|-------|-------|---------|
| `librarian` | opencode/big-pickle | Multi-repo analysis, external docs |
| `explore` | opencode/gpt-5-nano | Fast codebase pattern matching |

**Usage:**
```
mcp__plugin_astrape_astrape-tools__spawn_agent(
  agent="librarian",
  prompt="Find React hooks documentation"
)
```

**Note:** The built-in Task tool is blocked for these agents. They require
astrape-proxy running to route requests to the correct model provider.

### Tiered Variants

Each agent has `-low` (Haiku), `-medium` (Sonnet), `-high` (Opus) variants for cost optimization.

Example: `architect-low` for quick lookups, `executor-high` for complex refactoring.

## Model Routing

Astrape automatically routes tasks to appropriate model tiers:
- Simple lookups → Haiku (fast, cheap)
- Standard work → Sonnet (balanced)
- Complex reasoning → Opus (most capable)
- Custom models → Via astrape-proxy (librarian, explore)

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
