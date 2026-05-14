# RustyCode

AI-powered autonomous development framework built in Rust.

## Features

- **Multi-Provider LLM** — Anthropic, OpenAI, Google, Ollama, and more through a unified interface
- **Autonomous Mode** — Structured reasoning, task planning, and multi-step execution strategies
- **Terminal UI** — Full ratatui-based TUI with session management, themes, and skill plugins
- **Tool Framework** — File editing, bash execution, web fetching, LSP integration, and MCP support
- **Security First** — Permission system, path validation, secret sanitization, and pre-commit hooks

## Install

**macOS / Linux:**

```bash
curl -fsSL https://rustycode-ai.github.io/install.sh | sh
```

**Windows (PowerShell):**

```powershell
irm https://rustycode-ai.github.io/install.ps1 | iex
```

**Build from source:**

```bash
git clone https://github.com/rustycode-ai/rustycode.git
cd rustycode
cargo build --release
```

## Quick Start

```bash
# Launch interactive TUI
rustycode tui

# Run a one-shot task
rustycode "fix the authentication bug in src/auth.rs"

# Autonomous agent mode
rustycode agent new "implement user registration endpoint"

# Self-update to latest release
rustycode update
```

## Configuration

Create `~/.rustycode/config.json` to set your default provider and model:

```json
{
  "provider": "anthropic",
  "model": "claude-sonnet-4-20250514",
  "max_tokens": 16384,
  "providers": {
    "anthropic": {
      "api_key": "your-api-key-here",
      "models": ["claude-sonnet-4-20250514", "claude-opus-4-20250514"]
    },
    "openai": {
      "api_key": "your-api-key-here",
      "models": ["gpt-4o"]
    },
    "ollama": {
      "base_url": "http://localhost:11434",
      "models": ["codellama"]
    }
  }
}
```

Set your API key via environment variable:

```bash
export ANTHROPIC_API_KEY=sk-...
```

Run `rustycode provider list` to see all configured providers.

## Commands

| Command | Description |
|---------|-------------|
| `rustycode "task"` | Run a single task with the default agent |
| `rustycode tui` | Launch interactive terminal UI |
| `rustycode agent new "task"` | Start an autonomous agent session |
| `rustycode agent list` | List agent sessions |
| `rustycode provider list` | Show configured LLM providers |
| `rustycode skills list` | List installed skill plugins |
| `rustycode update` | Self-update to latest stable release |
| `rustycode update --nightly` | Update to latest nightly build |
| `rustycode bench` | Run benchmarks (Terminal Bench 2.0 compatible) |

## LLM Providers

Unified interface across 12+ providers: Anthropic, OpenAI, Google Gemini, AWS Bedrock, Azure, Ollama, LiteRT, Mistral, Cohere, HuggingFace, OpenRouter, GitHub Copilot.

## Documentation

- [Install Script](https://rustycode-ai.github.io/install.sh)
- [GitHub Releases](https://github.com/rustycode-ai/rustycode/releases)
- [Landing Page](https://rustycode-ai.github.io/)

## License

MIT
