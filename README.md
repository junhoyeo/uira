# Astrape ⚡

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
  model: claude-sonnet-4-20250514

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

| Key | Description |
|-----|-------------|
| `ai.model` | Default model for AI features (used by `typos --ai`) |

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

## Source Structure

```
src/
├── main.rs           # CLI entry point (clap)
├── config.rs         # YAML config parsing
├── hooks/
│   └── executor.rs   # Parallel hook execution (rayon)
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
