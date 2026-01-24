# astrape-proxy

HTTP proxy for using non-Anthropic models with Claude Code.

## Overview

`astrape-proxy` is a Rust-based HTTP proxy that enables Claude Code to work with OpenAI, Google Gemini, and other LLM providers by translating between Anthropic's API format and provider-specific formats.

## Features

- **Multi-Provider Support**: OpenAI, Google Gemini, Anthropic
- **OpenCode Authentication**: Reuses OpenCode's auth credentials
- **Model Mapping**: Automatically maps Claude model names to configured targets
- **Streaming Support**: Full SSE (Server-Sent Events) support
- **Type-Safe**: Built with Rust for reliability and performance

## Configuration

### Environment Variables

```bash
# Provider preference (default: openai)
PREFERRED_PROVIDER=openai|google|anthropic

# Model mapping
BIG_MODEL=gpt-4.1              # Maps Claude Sonnet
SMALL_MODEL=gpt-4.1-mini       # Maps Claude Haiku

# Server port (default: 8787)
PORT=8787
```

### OpenCode Authentication

The proxy uses OpenCode's authentication system. Ensure you've logged in:

```bash
opencode auth login openai
opencode auth login google
```

Auth credentials are stored in `~/.local/share/opencode/auth.json` (or platform equivalent).

## Usage

### Start the Proxy

```bash
cargo run --release -p astrape-proxy
```

### Use with Claude Code

```bash
ANTHROPIC_BASE_URL=http://localhost:8787 claude
```

## Model Mapping

The proxy automatically maps Claude model names:

| Claude Model | Default Mapping (OpenAI) | Google Mapping |
|--------------|--------------------------|----------------|
| `claude-3-haiku` | `openai/gpt-4.1-mini` | `gemini/gemini-2.5-flash` |
| `claude-3-sonnet` | `openai/gpt-4.1` | `gemini/gemini-2.5-pro` |

Configure via `PREFERRED_PROVIDER`, `BIG_MODEL`, and `SMALL_MODEL` environment variables.

## Architecture

```
┌─────────────┐         ┌──────────────────┐         ┌─────────────┐
│ Claude Code │ ──────> │ astrape-proxy    │ ──────> │  OpenAI /   │
│             │ Anthropic│                  │ Provider│  Gemini     │
│             │  Format  │  Axum Server     │ Format  │             │
└─────────────┘         └──────────────────┘         └─────────────┘
                              │
                              │ OpenCode Auth
                              v
                        ~/.local/share/opencode/auth.json
```

## Development

### Run Tests

```bash
cargo test -p astrape-proxy
```

### Build

```bash
cargo build --release -p astrape-proxy
```

## License

MIT
