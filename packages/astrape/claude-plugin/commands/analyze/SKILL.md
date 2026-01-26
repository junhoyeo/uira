---
name: analyze
description: Deep analysis and investigation mode for debugging and understanding complex issues
version: 0.1.0
---

# analyze

Deep analysis and investigation mode.

## Activation
- Keywords: `analyze`, `debug`, `investigate`, `why`
- Manual: `/astrape:analyze`

## Behavior

When activated:
1. Gather context via explore agents (parallel)
2. Use architect for complex reasoning
3. Synthesize findings before proceeding

## Context Gathering

- 1-2 explore agents for codebase patterns
- librarian agents for external library docs
- Direct tools: Grep, Glob, LSP

## Complex Analysis

If architectural, multi-system, or debugging after 2+ failures:
- Consult architect agent for strategic guidance
