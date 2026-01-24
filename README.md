# Astrape ⚡

![Astrape](./.github/assets/cover.jpg)

> Lightning-fast Rust-native multi-agent orchestration system with git hooks, native linting, and AI-assisted workflows

Astrape (Greek: "lightning") is a complete Rust port of [oh-my-claudecode](https://github.com/anthropics/oh-my-claudecode), providing multi-agent orchestration, smart model routing, and AI-assisted development tools.

## Features

### Core Orchestration
- **32 Specialized Agents** - Analyst, Architect, Designer, Executor, Explorer, and more
- **Smart Model Routing** - Automatically select Haiku/Sonnet/Opus based on task complexity
- **Hook System** - 22 hooks for Claude Code integration (autopilot, ultrawork, ralph, etc.)
- **MCP Integration** - Stdio client for Model Context Protocol servers
- **Skill System** - Load and execute skill templates from SKILL.md files

### Git Hooks & Linting
- **Native Rust** - No Node.js runtime, single binary
- **oxc-powered linting** - Uses oxc parser for AST-based JS/TS lint rules
- **AI-assisted typos** - Smart spell checking with context-aware AI decisions
- **Parallel execution** - Hooks run concurrently via Rayon

### Node.js Bindings
- **NAPI bindings** - Use from Node.js/TypeScript
- **Hook execution** - Run hooks from JavaScript
- **Agent access** - Query agent definitions and routing

## Install

```bash
cargo install astrape
```

Or from source:

```bash
git clone https://github.com/junhoyeo/Astrape astrape
cd astrape
cargo install --path crates/astrape
```

## Quick Start

### Git Hooks

```bash
astrape init      # Creates astrape.yml
astrape install   # Installs git hooks
git commit        # Hooks run automatically
```

### Agent Orchestration

```bash
astrape agent list              # List all 32 agents
astrape agent info executor     # Show agent details
astrape skill list              # List available skills
astrape session start           # Start orchestration session
```

## Commands

| Command | Description |
|---------|-------------|
| `astrape init` | Create default config file |
| `astrape install` | Install git hooks to `.git/hooks/` |
| `astrape run <hook>` | Run a specific hook manually |
| `astrape lint [files]` | Lint JS/TS files with native oxc |
| `astrape typos [--ai]` | Check for typos (optionally AI-assisted) |
| `astrape hook install` | Install Claude Code AI hooks |
| `astrape hook list` | List installed AI hooks |
| `astrape agent list` | List all available agents |
| `astrape agent info <name>` | Show agent details |
| `astrape session start` | Start new SDK session |
| `astrape session status` | Check session state |
| `astrape skill list` | List available skills |
| `astrape skill show <name>` | Display skill template |

## Architecture

### Crate Structure

```
crates/
├── astrape/                 # CLI binary
├── astrape-agents/          # 32 agent definitions
├── astrape-hooks/           # 22 hook implementations
├── astrape-features/        # State, routing, verification
├── astrape-tools/           # Tool handlers (delegate, background, skill)
├── astrape-mcp/             # MCP stdio client
├── astrape-sdk/             # Session management
├── astrape-config/          # Configuration schema
├── astrape-napi/            # Node.js bindings
├── astrape-comment-checker/ # Tree-sitter comment detection
├── astrape-prompts/         # Prompt builder
├── astrape-core/            # Core types
├── astrape-hook/            # Hook traits
└── astrape-claude/          # Claude Code integration
```

### Agent Tiers

| Tier | Model | Use Case |
|------|-------|----------|
| LOW | Haiku | Quick lookups, simple tasks |
| MEDIUM | Sonnet | Standard implementation work |
| HIGH | Opus | Complex reasoning, architecture |

### Available Agents

| Category | Agents |
|----------|--------|
| Analysis | architect, architect-medium, architect-low, analyst, critic |
| Execution | executor, executor-high, executor-low |
| Search | explore, explore-medium, explore-high |
| Design | designer, designer-high, designer-low |
| Testing | qa-tester, qa-tester-high, tdd-guide, tdd-guide-low |
| Security | security-reviewer, security-reviewer-low |
| Build | build-fixer, build-fixer-low |
| Research | researcher, researcher-low, scientist, scientist-high, scientist-low |
| Documentation | writer |
| Visual | vision |
| Planning | planner |
| Code Review | code-reviewer, code-reviewer-low |

### Hook Events

| Event | Description |
|-------|-------------|
| `UserPromptSubmit` | Before user prompt is processed |
| `Stop` | When agent stops |
| `SessionStart` | Session initialization |
| `PreToolUse` | Before tool execution |
| `PostToolUse` | After tool execution |

## Configuration

### `astrape.yml`

```yaml
ai:
  model: anthropic/claude-sonnet-4-20250514
  temperature: 0.7

hooks:
  pre_commit:
    parallel: true
    commands:
      - name: fmt
        run: cargo fmt --check
      - name: lint
        run: astrape lint {staged_files}
        glob: "**/*.{js,ts,jsx,tsx}"

mcp:
  servers:
    filesystem:
      command: npx
      args: ["-y", "@anthropic/mcp-filesystem", "/path"]
```

### Environment Variables

| Variable | Description |
|----------|-------------|
| `ASTRAPE_CONFIG` | Custom config file path |
| `ASTRAPE_MODEL` | Override default model |
| `ANTHROPIC_API_KEY` | API key for Claude |

## Node.js Usage

```typescript
import {
  listAgents,
  getAgent,
  routeTaskPrompt,
  executeHook,
  getSkill
} from '@astrape/native';

// List all agents
const agents = listAgents();

// Get specific agent
const executor = getAgent('executor');

// Route a task to appropriate model
const routing = routeTaskPrompt('Implement user authentication');
console.log(routing.model, routing.tier, routing.reasoning);

// Execute hook
const result = await executeHook('UserPromptSubmit', JSON.stringify({
  prompt: 'ultrawork: fix all errors'
}));

// Get skill template
const skill = getSkill('autopilot');
```

## Development

```bash
# Build all crates
cargo build --workspace

# Run tests
cargo test --workspace

# Run CLI
cargo run -p astrape -- agent list
```
