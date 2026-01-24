# Astrape ⚡

![ASTRAPE](./.github/assets/cover.jpg)

> Lightning-fast Rust-native git hooks manager with AI harness for Claude Code

Astrape (Greek: "lightning") is a comprehensive developer toolchain that combines git hooks management, native JavaScript/TypeScript linting, AI-assisted typos checking, and an extensible AI harness system for Claude Code.

## Features

- **Native Rust CLI** - Single binary, no Node.js runtime for core features
- **AI Harness System** - Extensible hooks for Claude Code integration
- **oxc-powered linting** - AST-based JS/TS lint rules via oxc parser
- **AI-assisted typos** - Smart spell checking with context-aware decisions
- **Parallel execution** - Hooks run concurrently via Rayon
- **Lefthook-style config** - Familiar YAML configuration
- **Variable expansion** - `{staged_files}`, `{all_files}` in commands

## Quick Start

```bash
# Install from source
git clone https://github.com/junhoyeo/Astrape
cd Astrape
cargo install --path .

# Initialize configuration
astrape init      # Creates astrape.yml
astrape install   # Installs git hooks
git commit        # Hooks run automatically
```

## Commands

| Command | Description |
|---------|-------------|
| `astrape init` | Create default config file |
| `astrape install` | Install git hooks to `.git/hooks/` |
| `astrape run <hook>` | Run a specific hook manually |
| `astrape lint [files]` | Lint JS/TS files with native oxc |
| `astrape typos` | Check for typos (fast, pre-commit) |
| `astrape typos --ai` | AI-assisted typos with grouped fixes |
| `astrape typos --ai --stage` | Auto-stage modified files after fixing |
| `astrape hook install` | Install AI harness hooks for Claude Code |
| `astrape hook list` | List installed AI harness hooks |

## Configuration

`astrape.yml`:

```yaml
ai:
  model: anthropic/claude-sonnet-4-20250514

pre-commit:
  parallel: true
  commands:
    - name: fmt
      run: cargo fmt --check
    - name: lint
      run: astrape lint {staged_files}
      glob: "**/*.{js,ts,jsx,tsx}"
    - name: typos
      run: astrape typos --ai --stage

post-commit:
  parallel: false
  commands:
    - name: auto-push
      run: git push origin HEAD
```

### AI Config

| Key | Default | Description |
|-----|---------|-------------|
| `ai.model` | `anthropic/claude-sonnet-4-20250514` | Model in `provider/model` format |
| `ai.host` | `127.0.0.1` | OpenCode server host |
| `ai.port` | `4096` | OpenCode server port |
| `ai.disable_tools` | `true` | Disable built-in tools (bash, edit, etc.) |
| `ai.disable_mcp` | `true` | Disable MCP servers |

### Variable Expansion

| Variable | Expands To |
|----------|------------|
| `{staged_files}` | Git staged files matching glob |
| `{all_files}` | All files matching glob |

## AI-Assisted Typos

The `--ai` flag enables intelligent batch typos handling:

```bash
astrape typos --ai --stage
```

**Features:**
- **Batch processing** - Groups typos by keyword, sends unique typos in single AI request
- **Smart decisions** - AI analyzes context and decides for each unique typo:
  - **APPLY** - Fix automatically
  - **IGNORE** - Add to `_typos.toml` ignore list
  - **SKIP** - Leave unchanged (uncertain)
- **Auto-staging** - `--stage` flag auto git-adds modified files

**Example output:**
```
! Found 6 typo(s) (1 unique)
  → Analyzing 1 unique typo(s) with AI...
  ✓ AI analysis complete

  → 'teh' → 'the' (6 occurrences) [IGNORE]
      ./src/hooks/mod.rs:298
      ./src/hooks/mod.rs:301
      ... (all locations grouped)
    ✓ Added 'teh' to _typos.toml
  → Staging 1 modified file(s)...
    ✓ _typos.toml
```

Requires [OpenCode](https://opencode.ai) to be installed.

### AI Hooks

Extend the AI typos workflow with custom hooks:

```yaml
ai_hooks:
  pre-check:
    - run: echo "Starting typos check"
  
  post-check:
    - run: echo "Found $TYPO_COUNT typos"
  
  pre-ai:
    - matcher: "*.rs"
      run: echo "Checking $TYPO in $FILE"
  
  post-ai:
    - run: echo "Decision: $DECISION for $TYPO"
  
  pre-fix:
    - run: echo "Applying: $TYPO → $CORRECTION"
  
  post-fix:
    - run: git add $FILE
```

| Hook | Event | Available Variables |
|------|-------|---------------------|
| `pre-check` | Before typos scan | - |
| `post-check` | After typos found | `TYPO_COUNT` |
| `pre-ai` | Before each AI decision | `FILE`, `LINE`, `TYPO`, `CORRECTIONS` |
| `post-ai` | After each AI decision | `FILE`, `TYPO`, `DECISION` |
| `pre-fix` | Before applying fix | `FILE`, `LINE`, `TYPO`, `CORRECTION` |
| `post-fix` | After applying fix | `FILE`, `TYPO`, `CORRECTION` |

## AI Harness System

Astrape includes a comprehensive AI harness system for integrating with Claude Code. The harness provides extensible hooks that enhance AI interactions with features like persistent memory, smart prompting, and quality assurance.

### Available Hooks

| Hook | Purpose | Status |
|------|---------|--------|
| **keyword_detector** | Detect keywords in prompts and inject context | ✅ Implemented |
| **notepad** | Persistent working memory across sessions | ✅ Implemented |
| **persistent_mode** | Maintain conversation state persistence | ✅ Implemented |
| **ralph** | Self-referential execution loop until completion | ✅ Implemented |
| **think_mode** | Enhanced reasoning with thinking blocks | ✅ Implemented |
| **todo_continuation** | Enforce task completion from todo lists | ✅ Implemented |
| **ultraqa** | Quality assurance validation loop | ✅ Implemented |
| **ultrawork** | Parallel work orchestration | ✅ Implemented |

### Installing AI Harness Hooks

```bash
# Install for Claude Code
astrape hook install

# List installed hooks
astrape hook list
```

Hooks are installed to Claude Code's configuration directory and automatically activate when you use Claude Code.

## Architecture

### Crate Structure

```
crates/
├── astrape/                    # Main CLI binary
├── astrape-core/               # Core types and utilities
├── astrape-hook/               # Hook execution engine
├── astrape-hooks/              # AI harness hooks collection
├── astrape-prompts/            # Prompt building system
├── astrape-claude/             # Claude Code integration
├── astrape-comment-checker/    # Code comment analyzer
├── astrape-sdk/                # Claude Agent SDK bridge (TypeScript)
└── astrape-napi/               # N-API bindings for TypeScript interop
```

### Technology Stack

| Component | Technology |
|-----------|------------|
| Parser | oxc |
| Parallel Execution | Rayon |
| Git Operations | git2 |
| CLI | clap |
| Typos Detection | typos-cli |
| AI Integration | OpenCode + Claude Agent SDK |
| TypeScript Bridge | napi-rs |

### Built-in Lint Rules

| Rule | Severity | Description |
|------|----------|-------------|
| `no-console` | warning | Disallow console.* calls |
| `no-debugger` | error | Disallow debugger statements |

## Development

### Prerequisites

- Rust 1.70+
- Node.js 20+ (for AI harness features)
- OpenCode CLI (for AI-assisted typos)

### Building

```bash
cargo build --release
```

### Testing

```bash
cargo test
```

## Documentation

Comprehensive analysis and planning documents are available in `.sisyphus/`:

- **[ANALYSIS_SUMMARY.md](.sisyphus/ANALYSIS_SUMMARY.md)** - Quick reference for SDK integration
- **[TECHNICAL_DETAILS.md](.sisyphus/TECHNICAL_DETAILS.md)** - Deep dive into architecture
- **[RECOMMENDATIONS.md](.sisyphus/RECOMMENDATIONS.md)** - Implementation roadmap
- **[analysis-claude-agent-sdk.md](.sisyphus/analysis-claude-agent-sdk.md)** - Complete analysis

## License

MIT
