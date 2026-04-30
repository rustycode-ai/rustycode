# rustycode-connector

Terminal connector abstraction for multiple terminal multiplexers.

## Purpose

Provides unified interface for managing terminal multiplexer connections (tmux, iTerm2, etc.). Enables launching and controlling isolated terminal windows for different development contexts.

## Key Types

- `TerminalConnector` — Main abstraction for terminal operations
- `ConnectorType` — Multiplexer type (Tmux, iTerm2)
- `Session` — Terminal session/window
- `WindowId` — Identifier for window
- `Command` — Command to execute in terminal

## Supported Multiplexers

- **Tmux** — Session/window management
- **iTerm2** — Window and tab management
- **Auto-detect** — Detect running multiplexer automatically

## MCP Plan

For the live agent-control design, see `TMUX_MCP_SPEC.md`. That document defines the tmux MCP tool surface, capture behavior, lifecycle rules, and implementation phases for turning tmux into a stateful control plane for the model.

## Public API

```rust
use rustycode_connector::{TerminalConnector, ConnectorType};

// Auto-detect available multiplexer
let connector = TerminalConnector::detect()?;

// Or create specific connector
let connector = TerminalConnector::new(ConnectorType::Tmux)?;

// Create new session/window
let session = connector.create_session("dev-session")?;

// Send command to window
connector.send_command(&session, "cd /path && cargo build")?;

// List windows
for window in connector.list_windows()? {
    println!("Window {}: {}", window.id, window.name);
}

// Close session
connector.close_session("dev-session")?;
```

## Features

- **Multiplexer Agnostic** — Works with different multiplexers
- **Session Management** — Create, list, close sessions
- **Command Execution** — Send commands to windows
- **Window Tracking** — Know where commands are running
- **Error Recovery** — Handle multiplexer crashes gracefully
- **Auto-detection** — Find running multiplexer automatically

## Tmux Implementation

- Uses `tmux` CLI for operations
- Sessions as tmux sessions
- Windows as tmux windows
- Commands via `send-keys`

## iTerm2 Implementation

- Uses AppleScript for control
- Sessions as window groups
- Windows as individual windows
- Commands via keyboard simulation

## Dependencies

- Multiplexer CLIs (tmux, iTerm2) must be installed
- `anyhow` — Error handling
- `regex` — Parsing multiplexer output
- `std::process` — Subprocess management

## Architecture Notes

Each multiplexer has adapter implementing `TerminalConnector` trait. Auto-detection checks environment variables and running processes.

Operations are synchronous for simplicity (future: async version).

Error handling recovers from multiplexer crashes and disconnections.

## Testing

Tests use mock multiplexers to verify operations without real tmux/iTerm2.

## See Also

- `rustycode-cli` — Uses for isolated command execution
- `rustycode-tools` — May spawn terminals for long-running tools
