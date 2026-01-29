---
name: analyze
description: Deep analysis and investigation mode for debugging and understanding complex issues
version: 0.1.0
---

# analyze

Deep analysis and investigation mode.

## Activation
- Keywords: `analyze`, `debug`, `investigate`, `why`
- Manual: `/uira:analyze`

## Behavior

When activated:
1. Gather context via explore/librarian agents (parallel, via delegate_task)
2. Use architect for complex reasoning
3. Synthesize findings before proceeding

## Context Gathering

- Direct tools: Grep, Glob, LSP (preferred for speed)
- **delegate_task** agents (via MCP tool):
  - `explore`: Codebase patterns (1-2 parallel)
  - `librarian`: External library docs

Use `mcp__plugin_uira_uira-tools__delegate_task(agent="explore", prompt="...")` for agent delegation.

## Complex Analysis

If architectural, multi-system, or debugging after 2+ failures:
- Consult architect agent for strategic guidance
