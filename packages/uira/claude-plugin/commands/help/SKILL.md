---
name: help
description: Show Uira plugin usage guide and available commands
version: 0.1.0
---

# help

Show Uira plugin usage guide.

## Activation
- Manual: `/uira:help`

## Available Commands

| Command | Description |
|---------|-------------|
| `/uira:ultrawork` | Maximum parallel execution |
| `/uira:analyze` | Deep analysis mode |
| `/uira:search` | Comprehensive search |
| `/uira:plan` | Strategic planning |

## Keywords

Say these naturally in your prompt:
- `ultrawork` / `ulw` - Parallel execution
- `analyze` / `debug` - Deep analysis
- `search` / `find` - Comprehensive search
- `plan` - Strategic planning

## Agents

### Standard Agents (Task tool)

27 agents across 3 tiers (Haiku/Sonnet/Opus):
- **Analysis**: architect, analyst, critic, planner
- **Execution**: executor
- **Frontend**: designer, vision
- **Quality**: qa-tester, code-reviewer, security-reviewer
- **Specialists**: scientist, writer, tdd-guide, build-fixer

Use `-low`, `-medium`, `-high` suffixes for tier control (e.g., `executor-low`, `architect-high`).

### Custom-Routed Agents (delegate_task MCP tool)

- **explore**: Fast codebase search (opencode/gpt-5-nano)
- **librarian**: External docs, multi-repo (opencode/big-pickle)

```
mcp__plugin_uira_t__delegate_task(agent="explore", prompt="...")
```
