---
name: plan
description: Strategic planning mode with interview workflow for complex tasks
version: 0.1.0
---

# plan

Strategic planning mode with interview workflow.

## Activation
- Keywords: `plan`, `plan this`
- Manual: `/uira:plan`

## Behavior

1. Gather context via explore agents (delegate_task MCP tool)
2. Consult architect for guidance (Task tool)
3. Interview user for requirements
4. Create comprehensive implementation plan
5. Get user approval before proceeding

**Note:** Use `mcp__plugin_uira_t__delegate_task(agent="explore", prompt="...")` for explore agents.

## Planning Approach

- Understand the full scope
- Identify dependencies and risks
- Break into actionable tasks
- Consider architectural implications
