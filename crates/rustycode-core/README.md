# rustycode-core

Core session management and headless execution engine for RustyCode.

## Purpose

Provides the foundation for session lifecycle, message handling, LLM communication, and execution orchestration. Implements the core loop that processes user messages, interacts with LLMs, executes tools, and manages state.

## Key Types

- `Session` — Core session with state, messages, and execution context
- `SessionConfig` — Configuration for session behavior and constraints
- `SessionManager` — Create, load, and manage sessions
- `HeadlessExecutor` — Non-interactive session execution (for automation/CI)
- `ExecutionPhase` — Planning vs Implementation phase gating
- `Checkpoint` — Session snapshot for recovery
- `Rewind` — Session state snapshots with versions

## Public API

```rust
use rustycode_core::{Session, SessionConfig, SessionManager};

// Create a session
let config = SessionConfig::default()
    .with_model("claude-opus-4-7")
    .with_max_turns(50);

let manager = SessionManager::new()?;
let mut session = manager.create_session(config).await?;

// Add a message and process
session.add_message("user", "Write a function")?;
let response = session.process().await?;
println!("Assistant: {}", response);

// Checkpoint for recovery
let checkpoint = session.checkpoint()?;
manager.save_checkpoint(&checkpoint).await?;

// Later, resume from checkpoint
let recovered = manager.load_from_checkpoint(&checkpoint).await?;
```

## Session Lifecycle

1. **Create** — Initialize with config (model, mode, constraints)
2. **Add Message** — User/system message to conversation
3. **Process** — LLM generates response, execute tools as needed
4. **Repeat** — Iterative refinement
5. **Checkpoint** — Save state for recovery
6. **Complete** — Session ends (success or timeout)

## Execution Modes

- **TUI Mode** — Interactive terminal UI with human feedback
- **CLI Mode** — Single command execution
- **Headless Mode** — Fully autonomous execution (no user input)
- **ACP Mode** — Agent Client Protocol for IDE integration

## Key Features

- **Tool Integration** — Execute tools, parse results, iterate
- **LLM Abstraction** — Work with any provider (Anthropic, OpenAI, etc.)
- **Context Management** — Automatic context ranking and compression
- **Checkpointing** — Save/restore session state for recovery
- **Plan Execution** — Execute multi-step plans with gating

## Dependencies

- `rustycode-protocol` — Core types
- `rustycode-llm` — LLM providers
- `rustycode-tools` — Tool execution
- `rustycode-storage` — Persistence
- `rustycode-memory` — Memory management
- `tokio` — Async runtime
- `anyhow` — Error handling

## Architecture Notes

The session is the central stateful object. It maintains conversation history, execution context, and checkpoint snapshots. The core loop is straightforward:

1. Receive message
2. Add to conversation
3. Call LLM with context
4. Parse response (tool calls, text, etc.)
5. Execute tools if needed
6. Iterate until done

Context is automatically managed: most recent messages always included, older messages ranked by relevance. Expensive operations (LLM calls, tool execution) are tracked for cost/metrics.

## Testing

Tests verify session creation, message processing, tool execution integration, checkpointing, and recovery flows.

## See Also

- `rustycode-session` — Session persistence and storage
- `rustycode-execution` — Plan execution engine
- `rustycode-llm` — LLM provider abstraction
- `rustycode-tools` — Tool execution
- `rustycode-tui`, `rustycode-cli` — Frontends using rustycode-core
