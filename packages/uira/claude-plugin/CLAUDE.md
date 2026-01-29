# Uira - Native Rust Multi-Agent Orchestration

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

**IMPORTANT:** When using `explore` or `librarian` agents, you MUST use `mcp__plugin_uira_t__delegate_task`, NOT the built-in Task tool. The built-in `Explore` agent is different from Uira's `explore` agent.

### Standard Agents (use Task tool)

| Agent | Model | Use For |
|-------|-------|---------|
| `uira:architect` | Opus | Complex problems, architecture |
| `uira:executor` | Sonnet | Implementation tasks |
| `uira:designer` | Sonnet | UI/UX work |
| `uira:writer` | Haiku | Documentation |

### Custom-Routed Agents (use delegate_task MCP tool)

**DO NOT use Task tool for these.** Use `mcp__plugin_uira_t__delegate_task`:

| Agent | Model | Use For |
|-------|-------|---------|
| `librarian` | opencode/big-pickle | Multi-repo analysis, external docs |
| `explore` | opencode/gpt-5-nano | Fast codebase pattern matching |

**Usage:**
```
mcp__plugin_uira_t__delegate_task(
  agent="librarian",
  prompt="Find React hooks documentation"
)
```

**Note:** The built-in Task tool is blocked for these agents. They use
delegate_task with OpenCode session API for model routing.

### Tiered Variants

Each agent has `-low` (Haiku), `-medium` (Sonnet), `-high` (Opus) variants for cost optimization.

Example: `architect-low` for quick lookups, `executor-high` for complex refactoring.

## Model Routing

Uira automatically routes tasks to appropriate model tiers:
- Simple lookups → Haiku (fast, cheap)
- Standard work → Sonnet (balanced)
- Complex reasoning → Opus (most capable)
- Custom models → Via delegate_task with OpenCode (librarian, explore)

## Performance

Native Rust NAPI bindings provide:
- Sub-millisecond keyword detection
- Efficient hook execution
- Low memory footprint

## Skills

| Skill | Description |
|-------|-------------|
| `/uira:ultrawork` | Maximum parallel execution |
| `/uira:analyze` | Deep analysis mode |
| `/uira:search` | Comprehensive search |
| `/uira:plan` | Strategic planning |
| `/uira:help` | Usage guide |
