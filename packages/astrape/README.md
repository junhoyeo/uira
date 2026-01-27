# astrape

Lightning-fast Rust-native multi-agent orchestration for Claude Code.

## Installation

```bash
npm install astrape
# or
pnpm add astrape
# or
yarn add astrape
# or
bun add astrape
```

## Usage

### Native NAPI Bindings

```js
import { detectKeywords, executeHook, listAgents } from 'astrape';

// Detect keywords in a prompt
const result = detectKeywords('ultrawork fix all errors');

// List all available agents
const agents = listAgents();

// Execute a hook
const output = await executeHook('user-prompt-submit', { prompt: 'test' });
```

### Git Hooks (`astrape/hook`)

```js
import { install, run, init } from 'astrape/hook';

// Initialize astrape configuration
await init();

// Install git hooks
await install();

// Run a specific hook
await run('pre-commit');
```

### Claude Plugin (`astrape/claude-plugin`)

```js
import { 
  pluginPath, 
  agentsPath, 
  getPluginConfig 
} from 'astrape/claude-plugin';

// Get the plugin directory path (for Claude Code integration)
console.log(pluginPath);

// Get plugin configuration
const config = getPluginConfig();
```

### CLI

```bash
# Initialize configuration
npx astrape init

# Install git hooks
npx astrape install

# Run a hook manually
npx astrape run pre-commit

# List available agents
npx astrape agent list

# Check goals
npx astrape goals check
```

## Configuration

Create an `astrape.yml` file in your project root:

```yaml
typos:
  ai:
    model: anthropic/claude-sonnet-4-20250514

pre-commit:
  parallel: false
  commands:
    - name: fmt
      run: cargo fmt
    - name: clippy
      run: cargo clippy -- -D warnings

goals:
  goals:
    - name: test-coverage
      command: ./scripts/coverage.sh
      target: 80.0
```

## Subpath Exports

| Export | Description |
|--------|-------------|
| `astrape` | Native NAPI bindings (keyword detection, hooks, agents) |
| `astrape/hook` | Git hooks programmatic API |
| `astrape/claude-plugin` | Claude Code plugin paths and configuration |

## Platform Support

- macOS (arm64, x64)
- Linux (x64, arm64, gnu, musl)
- Windows (x64)

## License

MIT
