# rustycode-session

Session lifecycle management and initialization for RustyCode.

## Purpose

Manages session creation, loading, resumption, and cleanup. Coordinates between storage, configuration, and core to provide complete session lifecycle from creation through completion and archival.

## Key Types

- `SessionManager` — Central session lifecycle manager
- `SessionBuilder` — Builder for creating sessions
- `SessionMode` — Execution mode (TUI, CLI, Headless, ACP)
- `SessionMetadata` — Session properties (created, modified, model, status)
- `SessionRecovery` — Recovery strategies for crashed sessions

## Public API

```rust
use rustycode_session::{SessionManager, SessionBuilder};

// Create manager
let manager = SessionManager::new()?;

// Create new session
let session = SessionBuilder::new()
    .with_model("claude-opus-4-7")
    .with_mode(SessionMode::TUI)
    .with_max_turns(50)
    .build(&manager)
    .await?;

println!("Created session: {}", session.id);

// Load existing session
if let Some(session) = manager.load_session("sess_123").await? {
    println!("Loaded: {} (model: {})", session.id, session.model);
}

// List all sessions
let all = manager.list_sessions().await?;
for s in all {
    println!("{}: {} - {}", s.id, s.status, s.created_at);
}

// Complete session
manager.complete_session("sess_123", "success").await?;

// Archive old sessions
manager.archive_before(Duration::days(30)).await?;
```

## Session Lifecycle

1. **Create** — Initialize with model, mode, config
2. **Init** — Load from storage or create in core
3. **Run** — Interactive or headless execution
4. **Checkpoint** — Save recovery points
5. **Complete** — Mark finished (success/failure)
6. **Archive** — Move old sessions to archive

## Features

- **Multi-mode** — TUI, CLI, Headless, ACP modes
- **Recovery** — Resume from checkpoints
- **Metadata** — Track model, start time, last activity
- **Cleanup** — Automatic archive of old sessions
- **Validation** — Ensure valid config before creating

## Dependencies

- `rustycode-storage` — Persistence layer
- `rustycode-config` — Session configuration
- `rustycode-core` — Core session implementation
- `rustycode-protocol` — Types
- `tokio` — Async operations
- `anyhow` — Error handling

## Architecture Notes

SessionManager coordinates between storage layer and core. It validates configuration, creates or loads sessions, and manages lifecycle transitions.

Each session has metadata (ID, model, mode, status, timestamps). Status progresses: Created → Running → Paused/Suspended → Completed/Failed.

Recovery handles crashes: if session wasn't properly closed, manager detects and offers resume from last checkpoint.

## Testing

Tests verify creation, loading, status transitions, recovery, and cleanup. Mock storage allows testing without disk I/O.

## See Also

- `rustycode-core` — Core session implementation
- `rustycode-storage` — Data persistence
- `rustycode-config` — Configuration
- `rustycode-protocol` — Session types
