---
name: ultrawork
description: Activate maximum parallel execution mode with intelligent agent orchestration
version: 0.1.0
---

# ultrawork

Maximum parallel execution mode with intelligent agent orchestration.

## Activation
- Keywords: `ultrawork`, `ulw`
- Manual: `/uira:ultrawork`

## Behavior

When activated:
1. Spawn multiple agents in parallel for independent tasks
2. Use background tasks for exploration and research
3. Track all work via TODO list
4. Verify ALL requirements before completion

## Rules

- NO Scope Reduction - deliver FULL implementation
- NO Partial Completion - finish 100%
- NO Premature Stopping - ALL TODOs must be complete
- PARALLEL execution for independent tasks
- DELEGATE to specialized agents

## Agent Utilization

### Standard Agents (Task tool)
- **architect**: Complex decisions, debugging
- **executor**: Implementation tasks

### Custom-Routed Agents (delegate_task MCP tool)
- **explore**: Codebase patterns, file structures
- **librarian**: External docs, references

Use `mcp__plugin_uira_t__delegate_task(agent="explore", prompt="...")` for these.
