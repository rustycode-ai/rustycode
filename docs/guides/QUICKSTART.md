# RustyCode Quickstart

Get up and running with RustyCode in 5 minutes.

## Installation (2 min)

### Build from Source

```bash
git clone https://github.com/luengnat/rustycode.git
cd rustycode
cargo build --release
```

The binary is at `target/release/rustycode-cli`.

Verify:

```bash
./target/release/rustycode-cli --version    # CLI mode
./target/release/rustycode-cli tui          # TUI mode (interactive)
```

### Pre-built Binaries

Download from [GitHub Releases](https://github.com/luengnat/rustycode/releases) for your platform:

- `rustycode-macos-arm64.tar.gz`
- `rustycode-macos-amd64.tar.gz`
- `rustycode-linux-arm64.tar.gz`
- `rustycode-linux-amd64.tar.gz`
- `rustycode-windows-amd64.zip`

## Configuration

Set your API key:

```bash
export ANTHROPIC_API_KEY="sk-ant-..."
# or
export OPENAI_API_KEY="sk-..."
```

RustyCode reads configuration from `~/.rustycode/config.json`.

## First Run (3 min)

### TUI Mode (Interactive)

```bash
./target/release/rustycode-tui
```

Type your message and press Enter to send. Press `Ctrl+C` or `Ctrl+Q` to quit.

### CLI Mode

```bash
./target/release/rustycode-cli --auto "Add a hello world function"
```

This will:
1. Create a new agent session
2. Analyze your codebase
3. Generate and execute a plan

## What You Get

- **Multi-provider LLM** — Anthropic, OpenAI, OpenRouter, Gemini, Ollama, and more
- **Interactive TUI** — Terminal UI with streaming responses
- **Tool execution** — Read/write files, run commands, git operations
- **Session history** — Track all interactions and changes
- **Autonomous Mode autonomous mode** — Hands-off development with crash recovery

## Orchestra Commands (Project Management)

```bash
# Initialize a new Orchestra project
rustycode-cli orchestra init

# Show project progress
rustycode-cli orchestra progress

# Start autonomous execution
rustycode-cli orchestra auto
```

## Next Steps

- [Tutorial](TUTORIAL.md) for a guided walkthrough
- [Developer Guide](developer-guide.md) for architecture and contributing
- [Troubleshooting](troubleshooting.md) for common issues
- Run `rustycode-cli --help` for all commands
