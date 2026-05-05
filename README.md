# RustyCode

Rust-native AI-powered autonomous development framework.

47 crates · 1,282 source files · ~588K LOC · 10,000+ tests

## Start Here

| Goal | Doc |
| --- | --- |
| Full project overview | [Comprehensive Documentation](docs/RUSTYCODE.md) |
| Documentation hub | [docs/README.md](docs/README.md) |
| Contributing | [CONTRIBUTING.md](docs/contributing/CONTRIBUTING.md) |
| Working on the codebase | [CLAUDE.md](CLAUDE.md) |
| Agent-specific guidance | [AGENTS.md](AGENTS.md) |

## Quick Start

```bash
# Build
cargo build --release

# Launch interactive TUI
cargo run -- tui

# Run a task directly
cargo run -- "fix the authentication bug in src/auth.rs"

# Autonomous agent mode
cargo run -- agent new "implement user registration endpoint"

# List available LLM providers
cargo run -- provider list

# List installed skills
cargo run -- skills list
```

## Install

### Unix (Linux/macOS)

Download from [GitHub Releases](https://github.com/rustycode-ai/rustycode/releases):

```bash
# macOS arm64
curl -sSL https://github.com/rustycode-ai/rustycode/releases/latest/download/rustycode-macos-arm64.tar.gz | tar xz
chmod +x rustycode-macos-arm64 && mv rustycode-macos-arm64 /usr/local/bin/rustycode
```

### Windows

Download `rustycode-windows-x86_64.zip` from [GitHub Releases](https://github.com/rustycode-ai/rustycode/releases).

## Build Requirements

- Linux: `protobuf-compiler`, `libssl-dev`, `pkg-config`
- macOS: `protobuf` via Homebrew

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
