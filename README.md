# RustyCode

[![Latest Release](https://img.shields.io/github/v/release/rustycode-ai/rustycode?label=latest)](https://github.com/rustycode-ai/rustycode/releases/latest)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Platforms](https://img.shields.io/badge/platforms-macOS%20%7C%20Linux%20%7C%20Windows-green.svg)](https://github.com/rustycode-ai/rustycode/releases/latest)

AI-powered autonomous development framework. Built in Rust for speed, reliability, and full control over your codebase.

## Features

- **Autonomous Mode** — Plan, execute, and verify tasks with structured reasoning, quality gates, and checkpoint recovery
- **Terminal UI** — Beautiful ratatui-based TUI with streaming, cost tracking, multi-session, and keyboard shortcuts
- **Web UI** — React-based interface with session management, tool approvals, and command palette
- **15+ LLM Providers** — Anthropic Claude, OpenAI GPT, Google Gemini, Amazon Bedrock, Azure, Ollama, Cohere, Mistral, GitHub Copilot, Together AI, Perplexity, Hugging Face, LiteRT, and more
- **Computer Use** — Desktop control via MCP: screenshots, clicks, typing, scrolling, drag. Multi-platform with surface-aware routing
- **Multi-Protocol** — MCP client for tool discovery, ACP server for IDE integration, A2A for agent-to-agent communication
- **20+ Built-in Tools** — File read/write/edit, bash execution, grep, glob, LSP integration, web fetch, git, notebook editing
- **Skill System & Marketplace** — YAML frontmatter-based skill discovery with marketplace for browsing, installing, and updating skills, tools, and MCP servers
- **Workflow Engine** — Static DAG and dynamic Rhai-scripted workflows with parallel agents, adversarial verification, and blind judge scoring
- **Wiki & Learning** — Built-in wiki with FTS5 search, knowledge management, and citation tracking
- **Security First** — Permission system, path validation, command sandboxing, secret sanitization

## Install

### One-Line Install (Recommended)

```bash
# macOS / Linux
curl -fsSL https://rustycode-ai.github.io/install.sh | sh

# Windows (PowerShell)
irm https://rustycode-ai.github.io/install.ps1 | iex
```

### Binary Download

Download from [GitHub Releases](https://github.com/rustycode-ai/rustycode/releases/latest):

| Platform | Asset |
|----------|-------|
| macOS (Apple Silicon) | `rustycode-macos-arm64.tar.gz` |
| Linux (x64) | `rustycode-linux-x64.tar.gz` |
| Linux (arm64) | `rustycode-linux-arm64.tar.gz` |
| Windows (x64) | `rustycode-windows-x64.zip` |

### Build from Source

```bash
git clone https://github.com/rustycode-ai/rustycode.git
cd rustycode
cargo build --release
```

## Quick Start

```bash
# Set your API key
export ANTHROPIC_API_KEY="sk-ant-..."

# Start the TUI
rustycode tui

# Or run headless
rustycode run "Fix the bug in src/main.rs"

# Or start the web interface
rustycode web
```

## Architecture

RustyCode is a modular workspace of 71 crates:

- **CLI & TUI** — Interactive terminal and web interfaces
- **Orchestration** — Autonomous execution with reasoning loops, quality gates, AST pipeline, and milestone sequencing
- **LLM Providers** — Unified provider trait supporting 15+ providers
- **Tools & Skills** — 20+ built-in tools, extensible framework, YAML skills with marketplace
- **MCP Servers** — Browser automation, Docker management, web fetch, and computer use (desktop control)
- **Protocols** — MCP client, ACP server, A2A agent communication
- **Storage & Session** — SQLite persistence, event bus, session handoff
- **Security** — Permission system, sandboxing (macOS Seatbelt, Linux landlock)
- **Team & Bench** — Multi-agent coordination and rtk-bench for native/Docker benchmark evaluation

## Configuration

RustyCode uses TOML config files with layered overrides:

```toml
# ~/.rustycode/config.toml
[provider]
default = "anthropic"

[provider.anthropic]
model = "claude-sonnet-4-6"
max_tokens = 8192
```

See the [full documentation](https://rustycode-ai.github.io/configuration.html) for all options.

## Documentation

- [Getting Started](https://rustycode-ai.github.io/getting-started.html)
- [Manual](https://rustycode-ai.github.io/manual.html)
- [Configuration](https://rustycode-ai.github.io/configuration.html)
- [Skills, MCP & Tools](https://rustycode-ai.github.io/skills-mcp-tools.html)
- [Tips & Tricks](https://rustycode-ai.github.io/tips-and-tricks.html)

## License

MIT