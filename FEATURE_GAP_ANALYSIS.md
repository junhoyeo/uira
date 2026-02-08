# Uira Feature Gap Analysis

A comprehensive comparison of Uira against three competitor coding agent CLIs: OpenCode, Codex CLI (OpenAI), and Pi.

## Executive Summary

Uira has strong foundational features (native Rust, multi-provider routing, sandboxing, MCP server), but lacks several user-facing features and extensibility patterns that competitors have implemented. The highest-impact gaps are in **extensibility/plugin systems**, **cloud/remote execution**, and **third-party integrations**.

---

## Feature Comparison Matrix

### Core Architecture

| Feature | Uira | OpenCode | Codex CLI | Pi |
|---------|------|----------|-----------|-----|
| **Language** | Rust | TypeScript/Bun | Rust | TypeScript |
| **Package Distribution** | Cargo/npm (NAPI) | npm | npm/Homebrew | npm |
| **Desktop App** | No | Yes (Tauri) | Yes (cask) | No |
| **IDE Extension** | No | VS Code (planned) | VS Code/Cursor/Windsurf | No |
| **Client/Server Architecture** | No | Yes (remote drive) | Yes (app-server) | Yes (RPC mode) |
| **Web Interface** | No | Yes (console app) | Yes (Codex Cloud) | No (web-ui package) |

### Provider Support

| Feature | Uira | OpenCode | Codex CLI | Pi |
|---------|------|----------|-----------|-----|
| **Anthropic** | Yes | Yes | No (ChatGPT only) | Yes |
| **OpenAI** | Yes | Yes | Yes (primary) | Yes |
| **Google/Gemini** | Planned | Yes | No | Yes |
| **Azure OpenAI** | No | Yes | No | Yes |
| **Amazon Bedrock** | No | Yes | No | Yes |
| **Local Models** | No | Yes | No | Yes |
| **OpenRouter** | No | Yes | No | Yes |
| **OAuth Subscription Login** | Yes (partial) | Yes | Yes (ChatGPT) | Yes (full) |
| **Model Switching (runtime)** | /model command | Tab key agents | /model command | /model + Ctrl+L |
| **Per-Task Model Routing** | Yes (config) | No | No | No |

### UI/TUI Features

| Feature | Uira | OpenCode | Codex CLI | Pi |
|---------|------|----------|-----------|-----|
| **Full-screen TUI** | Yes (Ratatui) | Yes (OpenTUI) | Yes (Rust TUI) | Yes (custom pi-tui) |
| **Themes** | No | Yes (gruvbox, etc) | No | Yes (hot-reload) |
| **Custom Keybindings** | No | Partial | No | Yes (full) |
| **Thinking Display** | Yes | Yes | Yes | Yes (levels) |
| **Model Selector UI** | Yes | Yes | Yes | Yes |
| **Image/Screenshot Input** | No | No | Yes (paste/CLI) | Yes (paste/drag) |
| **File Reference (@syntax)** | No | No | Yes (@fuzzy) | Yes (@fuzzy) |
| **Message Queue (steering)** | No | No | Yes (Enter/Alt+Enter) | Yes (Enter/Alt+Enter) |
| **Approval Overlays** | Yes | Yes | Yes (modes) | No (via extensions) |
| **Session Tree/Branching UI** | No | No | Yes (Esc+Esc) | Yes (/tree) |
| **Collapse Tool Output** | No | Yes | Yes | Yes (Ctrl+O) |
| **External Editor (Ctrl+G)** | No | No | Yes | Yes |

### Session & Context Management

| Feature | Uira | OpenCode | Codex CLI | Pi |
|---------|------|----------|-----------|-----|
| **Session Persistence** | Yes (JSONL) | Yes | Yes | Yes (JSONL tree) |
| **Session Resume** | Yes (--resume) | Yes | Yes (picker/--last) | Yes (-c/-r) |
| **Session Branching** | No | No | Yes (Esc+Esc fork) | Yes (/tree, /fork) |
| **Context Compaction** | No | Yes (summarization) | Yes (auto) | Yes (auto/manual) |
| **Session Export (HTML)** | No | No | Yes | Yes (/export) |
| **Session Share (gist)** | No | No | No | Yes (/share) |
| **Multi-Directory Roots** | No | No | Yes (--add-dir) | No |

### Tool Integrations

| Feature | Uira | OpenCode | Codex CLI | Pi |
|---------|------|----------|-----------|-----|
| **MCP Server (expose tools)** | Yes | Yes | Yes | No (via extension) |
| **MCP Client (consume tools)** | No | Yes (full) | Yes (full) | No (via extension) |
| **MCP OAuth** | No | Yes | Yes | No |
| **MCP Streamable HTTP** | No | Yes | Yes | No |
| **LSP Integration** | Yes (native) | Yes | No (via MCP) | No |
| **AST-grep** | Yes | No | No | No |
| **Web Search** | No | No | Yes (cached/live) | No |
| **Browser Automation** | No | Via MCP (Playwright) | Via MCP | No |
| **Git Integration** | Hooks only | Full | Full | No (use tmux) |
| **GitHub CLI (gh)** | No | Yes | Yes | No |
| **Code Review Mode** | No | No | Yes (/review) | No (via extension) |

### Extensibility

| Feature | Uira | OpenCode | Codex CLI | Pi |
|---------|------|----------|-----------|-----|
| **Plugin/Extension System** | No | No | Skills system | Yes (TypeScript) |
| **Custom Tools** | No | MCP only | MCP + Skills | Yes (full) |
| **Custom Commands** | Slash commands | Slash commands | Slash commands | Yes (extensions) |
| **Prompt Templates** | No | No | Yes | Yes (Markdown) |
| **Skills/Capabilities** | No | No | Yes (curated) | Yes (SKILL.md) |
| **Theme System** | No | Yes | No | Yes (hot-reload) |
| **Package Distribution** | No | No | No | Yes (npm/git) |
| **Custom UI Components** | No | No | No | Yes (widgets) |

### Security & Sandboxing

| Feature | Uira | OpenCode | Codex CLI | Pi |
|---------|------|----------|-----------|-----|
| **macOS Sandbox** | Yes (sandbox-exec) | No | Yes (Seatbelt) | No |
| **Linux Sandbox** | Yes (Landlock) | No | Yes (Landlock+seccomp) | No |
| **Windows Sandbox** | Planned | No | WSL/experimental | No |
| **Approval Modes** | Yes | Yes | Yes (Auto/Read-only/Full) | No (via extension) |
| **Per-Project Trust** | No | No | Yes (config.toml) | No |
| **Network Access Control** | Yes | No | Yes (configurable) | No |
| **Enterprise/Admin Config** | No | No | Yes (MDM, requirements.toml) | No |
| **Telemetry (OTel)** | Yes (uira-telemetry) | No | Yes (opt-in) | No |

### Automation & CI/CD

| Feature | Uira | OpenCode | Codex CLI | Pi |
|---------|------|----------|-----------|-----|
| **Non-interactive Mode** | Yes (exec) | Yes | Yes (exec) | Yes (-p/--print) |
| **JSON Output Mode** | No | No | Yes (--json) | Yes (--mode json) |
| **RPC Mode** | No | No | No | Yes (--mode rpc) |
| **SDK/Programmatic Use** | Yes (uira-sdk) | Yes (opencode-sdk) | Yes (codex-sdk) | Yes (full) |
| **GitHub Action** | No | No | Yes | No |
| **Git Hooks Integration** | Yes (native) | No | No | No (use extension) |
| **Cloud Execution** | No | No | Yes (Codex Cloud) | No |

### AI-Assisted Workflows

| Feature | Uira | OpenCode | Codex CLI | Pi |
|---------|------|----------|-----------|-----|
| **Typo Detection/Fix** | Yes (--ai) | No | No | No |
| **Diagnostics Fix** | Yes (--ai) | No | No | No |
| **Comment Review** | Yes (--ai) | No | No | No |
| **Goal Verification** | Yes (score-based) | No | No | No |
| **Built-in Agents** | No | Yes (build/plan) | No | No (via extension) |

---

## Feature Gap Analysis by Priority

### HIGH Priority (Major Competitive Disadvantage)

| Missing Feature | Found In | Impact | Difficulty |
|-----------------|----------|--------|------------|
| **MCP Client Support** | OpenCode, Codex | Cannot consume external MCP tools (Playwright, Figma, etc.) | Medium |
| **Extension/Plugin System** | Pi, Codex Skills | No user extensibility without forking | High |
| **Session Branching** | Codex, Pi | Users can't explore alternatives or recover from mistakes | Medium |
| **Image/Screenshot Input** | Codex, Pi | Can't use visual context in prompts | Medium |
| **Context Compaction** | OpenCode, Codex, Pi | Long sessions exhaust context window | Medium |
| **Theme System** | OpenCode, Pi | No visual customization | Low |
| **Desktop App** | OpenCode, Codex | Less accessible to non-terminal users | High |

### MEDIUM Priority (Competitive Parity)

| Missing Feature | Found In | Impact | Difficulty |
|-----------------|----------|--------|------------|
| **Web Search** | Codex | Can't fetch live documentation | Low |
| **Code Review Mode** | Codex | No dedicated review workflow | Low |
| **File Reference (@syntax)** | Codex, Pi | Less convenient file inclusion | Low |
| **External Editor (Ctrl+G)** | Codex, Pi | Can't use preferred editor for long prompts | Low |
| **Session Export (HTML)** | Codex, Pi | No shareable session output | Low |
| **Message Queue** | Codex, Pi | Can't queue follow-up messages | Low |
| **Per-Project Trust Config** | Codex | No project-scoped permissions | Low |
| **Collapse Tool Output** | OpenCode, Codex, Pi | Cluttered TUI during long operations | Low |
| **More Provider Support** | OpenCode, Pi | Missing Azure, Bedrock, local models | Medium |
| **Custom Keybindings** | Pi | Users stuck with default keybindings | Low |

### LOW Priority (Nice to Have)

| Missing Feature | Found In | Impact | Difficulty |
|-----------------|----------|--------|------------|
| **Cloud Execution** | Codex | Requires infrastructure investment | Very High |
| **IDE Extension** | OpenCode, Codex | Major development effort | Very High |
| **GitHub Action** | Codex | CI/CD automation | Medium |
| **RPC Mode** | Pi | Process integration for non-Node apps | Medium |
| **Session Share (gist)** | Pi | Convenient sharing | Low |
| **Shell Completions** | Codex | Minor DX improvement | Low |
| **Enterprise MDM** | Codex | Only needed for enterprise | High |

---

## Unique Features Uira Has That Others Don't

| Feature | Description | Advantage |
|---------|-------------|-----------|
| **Native Rust Core** | Zero-dependency binary, no Node.js required | Performance, deployment simplicity |
| **Per-Task Model Routing** | Route different agent roles to different models | Cost optimization, flexibility |
| **AI-Assisted Git Hooks** | Autonomous typo/diagnostic/comment fixing | Unique workflow automation |
| **Platform-Native Sandboxing** | macOS sandbox-exec, Linux Landlock | Security without containers |
| **Goal Verification System** | Score-based completion verification | Persistent work loops |
| **OXC Integration** | Native JS/TS linting/parsing | Fast tooling |
| **NAPI Bridge** | Cross-language Rust<->Node.js | Ecosystem integration |

---

## Recommended Implementation Roadmap

### Phase 1: Core Gaps (1-2 months)
1. **MCP Client Support** - Enable consuming external MCP servers
2. **Context Compaction** - Implement session summarization
3. **Image Input** - Add screenshot/image support to prompts
4. **Session Branching** - Implement /tree and /fork commands

### Phase 2: User Experience (2-3 months)
1. **Theme System** - Add configurable TUI themes
2. **File Reference (@syntax)** - Fuzzy file search in editor
3. **Custom Keybindings** - User-configurable shortcuts
4. **Session Export** - HTML export and gist sharing

### Phase 3: Extensibility (3-4 months)
1. **Extension System** - TypeScript/Rust plugin architecture
2. **Custom Tools** - Allow user-defined tools
3. **Prompt Templates** - Reusable prompt system
4. **Package Distribution** - Install extensions from npm/git

### Phase 4: Enterprise & Scale (4-6 months)
1. **Desktop App** - Tauri-based GUI wrapper
2. **Cloud Execution** - Remote sandbox environments
3. **IDE Extension** - VS Code integration
4. **Enterprise Config** - MDM and admin controls

---

## Appendix: Data Sources

| Project | Source | Version |
|---------|--------|---------|
| Uira | /Users/junhoyeo/uira | 0.1.0 |
| OpenCode | /Users/junhoyeo/opencode | 1.1.45 |
| Codex CLI | GitHub + installed | 0.98.0 |
| Pi | GitHub badlogic/pi-mono | @mariozechner/pi-coding-agent |

---

*Generated: 2026-02-09*
