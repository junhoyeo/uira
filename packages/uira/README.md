# uira

Lightning-fast Rust-native multi-agent orchestration for Claude Code.

## Installation

```bash
npm install uira
# or
pnpm add uira
# or
yarn add uira
# or
bun add uira
```

## Usage

### Native NAPI Bindings

```js
import { detectKeywords, executeHook, listAgents } from 'uira';

// Detect keywords in a prompt
const result = detectKeywords('ultrawork fix all errors');

// List all available agents
const agents = listAgents();

// Execute a hook
const output = await executeHook('user-prompt-submit', { prompt: 'test' });
```

### Git Hooks (`uira/hook`)

```js
import { install, run, init } from 'uira/hook';

// Initialize uira configuration
await init();

// Install git hooks
await install();

// Run a specific hook
await run('pre-commit');
```

### Claude Plugin (`uira/claude-plugin`)

```js
import { 
  pluginPath, 
  agentsPath, 
  getPluginConfig 
} from 'uira/claude-plugin';

// Get the plugin directory path (for Claude Code integration)
console.log(pluginPath);

// Get plugin configuration
const config = getPluginConfig();
```

### CLI

```bash
# Initialize configuration
npx uira init

# Install git hooks
npx uira install

# Run a hook manually
npx uira run pre-commit

# List available agents
npx uira agent list

# Check goals
npx uira goals check
```

## Configuration

Create an `uira.yml` file in your project root:

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
| `uira` | Native NAPI bindings (keyword detection, hooks, agents) |
| `uira/hook` | Git hooks programmatic API |
| `uira/claude-plugin` | Claude Code plugin paths and configuration |

## Platform Support

- macOS (arm64, x64)
- Linux (x64, arm64, gnu, musl)
- Windows (x64)

## License

MIT
