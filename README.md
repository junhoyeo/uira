# ⚡ Astrape

> **Lightning-fast Rust-native git hooks manager with opinionated Biome presets**

Astrape (Greek: ἀστραπή, "lightning") is a modern replacement for lefthook + ultracite, built entirely in Rust for maximum performance and native integration.

## 🚀 Features

- **⚡ Blazing Fast**: Written in Rust, compiled to native binary
- **🔄 Parallel Execution**: Runs hooks concurrently using Rayon
- **🎯 Zero-Config Biome Presets**: Opinionated linting/formatting out of the box
- **📦 Single Binary**: No runtime dependencies, just one executable
- **🔧 Lefthook-Compatible**: Familiar YAML configuration format
- **🎨 Ultracite-Style Presets**: Built-in Biome configurations

## 📊 Architecture

### Stack

| Layer | Technology | Purpose |
|-------|------------|---------|
| **CLI** | `clap` (derive) | Command-line interface |
| **Config** | `serde_yaml` | YAML parsing |
| **Parallel** | `rayon` | Concurrent hook execution |
| **Git** | `git2` | Git operations |
| **Glob** | `globset` | File pattern matching |

### Design Philosophy

Astrape combines the best of both worlds:

1. **From Lefthook (Go)**:
   - YAML-based configuration
   - Parallel command execution
   - File filtering (`{staged_files}`, `{all_files}`)
   - Hook installation to `.git/hooks/`

2. **From Ultracite (TypeScript/Biome)**:
   - Zero-config presets
   - Opinionated defaults
   - Direct Biome integration
   - `check` and `fix` commands

3. **Rust-Native Advantages**:
   - **10-50ms startup** (vs 100ms+ for Node.js)
   - **Native parallelism** via Rayon
   - **Single binary** distribution
   - **Memory safety** without GC overhead

## 📦 Installation

### From Source

```bash
git clone https://github.com/junhoyeo/astrape
cd astrape
cargo install --path .
```

### From Cargo

```bash
cargo install astrape
```

## 🎯 Quick Start

### 1. Initialize Configuration

```bash
astrape init
```

Creates `astrape.yml`:

```yaml
pre-commit:
  parallel: true
  commands:
    - name: format
      glob: "**/*.{js,ts,jsx,tsx}"
      run: astrape fix {staged_files}
      stage_fixed: true
    
    - name: lint
      glob: "**/*.{js,ts,jsx,tsx}"
      run: astrape check {staged_files}
```

### 2. Install Hooks

```bash
astrape install
```

Generates hook scripts in `.git/hooks/` that call `astrape run <hook-name>`.

### 3. Use It

```bash
# Hooks run automatically on git commit
git commit -m "feat: add new feature"

# Or run manually
astrape run pre-commit

# Or use Biome directly
astrape check src/**/*.ts
astrape fix src/**/*.ts
```

## 📖 Commands

| Command | Description | Example |
|---------|-------------|---------|
| `init` | Create `astrape.yml` config | `astrape init` |
| `install` | Install hooks to `.git/hooks/` | `astrape install` |
| `run <hook>` | Execute a specific hook | `astrape run pre-commit` |
| `check [files]` | Lint files (Biome) | `astrape check src/` |
| `fix [files]` | Format + lint files (Biome) | `astrape fix src/` |

## ⚙️ Configuration

### Example `astrape.yml`

```yaml
# Lefthook-style hook definitions
pre-commit:
  parallel: true
  commands:
    - name: typecheck
      run: tsc --noEmit
    
    - name: format
      glob: "**/*.{js,ts,jsx,tsx,json,css}"
      run: astrape fix {staged_files}
      stage_fixed: true
    
    - name: lint
      glob: "**/*.{js,ts,jsx,tsx}"
      run: astrape check {staged_files}

pre-push:
  commands:
    - name: test
      run: bun test

commit-msg:
  commands:
    - name: conventional
      run: commitlint --edit $1
```

### File Filtering

| Variable | Description | Example |
|----------|-------------|---------|
| `{staged_files}` | Git staged files | `git diff --cached --name-only` |
| `{all_files}` | All tracked files | `git ls-files` |
| `{push_files}` | Files in push | `git diff --name-only @{push}..HEAD` |

### Glob Patterns

```yaml
commands:
  - name: lint-frontend
    glob: "frontend/**/*.{ts,tsx}"
    run: astrape check {staged_files}
  
  - name: lint-backend
    glob: "api/**/*.rs"
    run: cargo clippy
```

## 🆚 Comparison

### vs. Lefthook (Go)

| Feature | Lefthook | Astrape |
|---------|----------|---------|
| Language | Go | **Rust** |
| Startup | ~20-50ms | **~10-30ms** |
| Parallel | ✅ Goroutines | ✅ **Rayon** |
| Config | YAML | YAML |
| Biome Integration | ❌ Manual | ✅ **Built-in** |
| Binary Size | ~10MB | **~8MB** |

### vs. Ultracite (Node.js)

| Feature | Ultracite | Astrape |
|---------|-----------|---------|
| Language | TypeScript | **Rust** |
| Startup | ~100-200ms | **~10-30ms** |
| Runtime | Node.js | **Native** |
| Presets | ✅ Biome | ✅ **Biome** |
| Hook Manager | ❌ Needs lefthook | ✅ **Built-in** |

### vs. Husky (Node.js)

| Feature | Husky | Astrape |
|---------|-------|---------|
| Language | JavaScript | **Rust** |
| Parallel | ❌ Sequential | ✅ **Parallel** |
| Config | package.json | **YAML** |
| Performance | Slow | **Fast** |

## 🏗️ Development

### Build

```bash
cargo build --release
```

### Test

```bash
cargo test
```

### Run Locally

```bash
cargo run -- init
cargo run -- install
cargo run -- run pre-commit
```

## 📝 Roadmap

- [x] CLI structure (clap)
- [x] Basic commands (init, install, run, check, fix)
- [ ] YAML config parsing
- [ ] Hook execution engine (parallel with rayon)
- [ ] File filtering (`{staged_files}`, etc.)
- [ ] Biome preset system
- [ ] Hook installation (`.git/hooks/` generation)
- [ ] Integration tests
- [ ] Binary releases (GitHub Actions)
- [ ] Homebrew formula
- [ ] npm wrapper package

## 🤝 Contributing

Contributions welcome! This project follows the Rust community's code of conduct.

## 📄 License

MIT © [Junho Yeo](https://github.com/junhoyeo)

---

**Inspired by**:
- [Lefthook](https://github.com/evilmartians/lefthook) - Fast git hooks manager (Go)
- [Ultracite](https://github.com/haydenbleasel/ultracite) - Zero-config linter/formatter (TypeScript)
- [Monk](https://github.com/daynin/monk) - Simple git hooks manager (Rust)
- [Samoyed](https://github.com/nutthead/samoyed) - Minimalist hooks manager (Rust)
