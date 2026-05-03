# rustycode-tui-agents

Agent lifecycle management and display in TUI.

## Purpose

Manages autonomous agent lifecycle within the TUI context. Handles agent spawning, monitoring, status display, bidirectional communication, and graceful shutdown. Bridges the gap between agents and the terminal interface.

## Key Types

- `AgentManager` — Manages agent instances and lifecycle
- `AgentLifecycle` — Agent state transitions (running, stopped, error, etc.)
- `AgentDisplay` — Renders agent status and progress in TUI
- Agent communication interfaces for bidirectional messaging
- Agent monitoring and health checks

## Public API

```rust
use rustycode_tui_agents::{AgentManager, AgentLifecycle};

let mut manager = AgentManager::new();
manager.spawn_agent(agent_config).await?;

// Monitor agent
while let Some(status) = manager.next_status_update().await {
    println!("Agent: {:?}", status);
}
```

## Architecture Notes

- Manages multiple concurrent agents
- Provides callbacks for lifecycle events
- Communication abstraction for agent I/O
- Thread-safe agent monitoring

## Testing

- Agent lifecycle state transition tests
- Message passing tests
- Cancellation and cleanup tests

## See Also

- `rustycode-tui-core` — Core framework
- `rustycode-agents` — Agent implementations
