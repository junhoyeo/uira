# Astrape

> Lightning-fast Rust-native git hooks manager with native oxc integration

Astrape (Greek: "lightning") combines git hooks management with native JavaScript/TypeScript linting powered by [oxc](https://oxc.rs).

## Features

- **Native Rust** - No Node.js runtime, single binary
- **oxc-powered linting** - Uses oxc parser for AST-based lint rules  
- **Parallel execution** - Hooks run concurrently via Rayon
- **Lefthook-style config** - Familiar YAML configuration

## Install

```bash
cargo install astrape
```

Or from source:

```bash
git clone https://github.com/junhoyeo/astrape
cd astrape
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
| `astrape init` | Create config file |
| `astrape install` | Install git hooks to `.git/hooks/` |
| `astrape run <hook>` | Run a specific hook |
| `astrape lint [files]` | Lint JS/TS files with native oxc |

## Configuration

`astrape.yml`:

```yaml
pre-commit:
  parallel: true
  commands:
    - name: lint
      run: astrape lint {staged_files}
      glob: "**/*.{js,ts,jsx,tsx}"
```

## Built-in Lint Rules

| Rule | Severity | Description |
|------|----------|-------------|
| `no-console` | warning | Disallow console.* calls |
| `no-debugger` | error | Disallow debugger statements |

## Architecture

| Component | Technology |
|-----------|------------|
| Parser | oxc |
| Parallel | Rayon |
| Git | git2 |
| CLI | clap |

## License

MIT
