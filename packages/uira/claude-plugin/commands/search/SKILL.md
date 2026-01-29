---
name: search
description: Comprehensive codebase search mode with parallel exploration
version: 0.1.0
---

# search

Comprehensive codebase search mode.

## Activation
- Keywords: `search`, `find`, `locate`, `where is`
- Manual: `/uira:search`

## Behavior

Maximize search effort with parallel agents:
- Direct tools: Grep, Glob (preferred for speed)
- **delegate_task** agents (via MCP tool):
  - `explore`: Codebase patterns, file structures
  - `librarian`: Remote repos, official docs

Use `mcp__plugin_uira_t__delegate_task(agent="explore", prompt="...")` for agent delegation.

NEVER stop at first result - be exhaustive.
