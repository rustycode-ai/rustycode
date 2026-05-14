# RustyCode

Rust-native AI-powered autonomous development framework.

48 crates · 1,102 source files · ~525K LOC · 10,000+ tests

## Start Here

| Goal | Doc |
| --- | --- |
| Full project overview | [Comprehensive Documentation](docs/RUSTYCODE.md) |
| Documentation hub | [docs/README.md](docs/README.md) |
| Contributing | [CONTRIBUTING.md](docs/contributing/CONTRIBUTING.md) |
| Working on the codebase | [CLAUDE.md](CLAUDE.md) |
| Agent-specific guidance | [AGENTS.md](AGENTS.md) |

## Install

### macOS / Linux

```bash
curl -fsSL https://rustycode-ai.github.io/install.sh | sh
```

### Windows (PowerShell)

```powershell
irm https://rustycode-ai.github.io/install.ps1 | iex
```

### Nightly channel

```bash
curl -fsSL https://rustycode-ai.github.io/install.sh | sh -s -- --nightly
```

Downloads are available at [GitHub Releases](https://github.com/rustycode-ai/rustycode/releases).

## Quick Start

```bash
# Launch interactive TUI
rustycode tui

# Run a task directly
rustycode "fix the authentication bug in src/auth.rs"

# Autonomous agent mode
rustycode agent new "implement user registration endpoint"

# List available LLM providers
rustycode provider list

# List installed skills
rustycode skills list

# Self-update to latest release
rustycode update

# Check for updates without installing
rustycode update --check
```

## Build from Source

```bash
cargo build --release
```

### Build Requirements

- Linux: `protobuf-compiler`, `libssl-dev`, `pkg-config`
- macOS: `protobuf` via Homebrew

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

Set your API key via environment variable: `export ANTHROPIC_API_KEY=sk-...`

Run `rustycode provider list` to see all configured providers.

## Features

- **Multi-provider LLM**: Anthropic, OpenAI, Gemini, Bedrock, Azure, Ollama, LiteRT, Mistral, Cohere, HuggingFace, OpenRouter, Copilot
- **20+ built-in tools**: file I/O, bash, grep, glob, LSP, web fetch, notebook editing, git operations
- **Skill system**: YAML frontmatter skills with path-based activation and brace expansion
- **Autonomous orchestration**: structured reasoning, quality gates, AST pipeline
- **Benchmark runner**: Terminal Bench 2.0 compatible (native + Docker)
- **Interactive TUI**: ratatui-based terminal UI with streaming, mouse support, syntax highlighting

## Documentation

| Path | Description |
| --- | --- |
| [docs/RUSTYCODE.md](docs/RUSTYCODE.md) | Comprehensive project documentation |
| [docs/guides/](docs/guides/) | Tutorials, quickstart, troubleshooting |
| [docs/architecture/](docs/architecture/) | System architecture and reviews |
| [docs/reference/](docs/reference/) | API reference, specs, permissions |
| [docs/adr/](docs/adr/) | Architecture Decision Records |
| `crates/*/README.md` | Per-crate documentation |

Two files stay at the repository root because the code reads them directly:

- [CLAUDE.md](CLAUDE.md) — project-wide development instructions
- [TEAM_LEARNINGS.md](TEAM_LEARNINGS.md) — persisted team learnings

## License

MIT
