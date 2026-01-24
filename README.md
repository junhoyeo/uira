# Astrape ⚡

![ASTRAPE](./.github/assets/cover.jpg)

> Lightning-fast Rust-native git hooks manager with native oxc linting and AI-assisted typos checking

Astrape (Greek: "lightning") combines git hooks management with native JavaScript/TypeScript linting powered by [oxc](https://oxc.rs) and AI-assisted spell checking via [opencode](https://opencode.ai).

## Features

- **Native Rust** - No Node.js runtime, single binary
- **oxc-powered linting** - Uses oxc parser for AST-based JS/TS lint rules
- **AI-assisted typos** - Smart spell checking with context-aware AI decisions
- **Parallel execution** - Hooks run concurrently via Rayon
- **Lefthook-style config** - Familiar YAML configuration
- **Variable expansion** - `{staged_files}`, `{all_files}` in commands

## Install

```bash
cargo install astrape
```

Or from source:

```bash
git clone https://github.com/junhoyeo/Astrape
cd Astrape
cargo install --path .
```

## Quick Start

```bash
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
| `astrape typos --ai` | AI-assisted typos fixing with opencode |

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
      run: astrape typos

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
| `ai.host` | `127.0.0.1` | Opencode server host |
| `ai.port` | `4096` | Opencode server port |
| `ai.disable_tools` | `true` | Disable built-in tools (bash, edit, etc.) |
| `ai.disable_mcp` | `true` | Disable MCP servers |

Tools and MCPs are disabled by default for typos checking (only text analysis needed).

### Variable Expansion

| Variable | Expands To |
|----------|------------|
| `{staged_files}` | Git staged files matching glob |
| `{all_files}` | All files matching glob |

## Built-in Lint Rules

| Rule | Severity | Description |
|------|----------|-------------|
| `no-console` | warning | Disallow console.* calls |
| `no-debugger` | error | Disallow debugger statements |

## AI-Assisted Typos

The `--ai` flag enables intelligent typos handling:

```bash
astrape typos --ai
```

For each typo found, AI analyzes the context and decides:
- **APPLY** - Fix the typo automatically
- **IGNORE** - Add to `_typos.toml` ignore list (false positive)
- **SKIP** - Leave unchanged (needs human review)

Requires [opencode](https://opencode.ai) to be installed.

### AI Hooks

Extend the AI typos workflow with custom hooks at each stage:

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

Hooks support glob-style matchers and can stop execution with `on_fail: stop`.

## Source Structure

```
src/
├── main.rs           # CLI entry point (clap)
├── config.rs         # YAML config parsing
├── hooks/
│   ├── mod.rs        # AI hooks (pre-check, pre-ai, etc.)
│   └── executor.rs   # Git hooks (pre-commit, etc.)
├── linter/
│   └── mod.rs        # oxc-based JS/TS linting
└── typos/
    └── mod.rs        # AI-assisted spell checking
```

## Architecture

| Component | Technology |
|-----------|------------|
| Parser | oxc |
| Parallel | Rayon |
| Git | git2 |
| CLI | clap |
| Typos | typos-cli + opencode |

## License

MIT
