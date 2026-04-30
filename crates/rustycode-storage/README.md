# rustycode-storage

Session persistence and data storage for RustyCode.

## Purpose

Manages persistent storage of sessions, conversations, checkpoints, and metadata using SQLite. Provides CRUD operations for all session-related data with automatic schema management.

## Key Types

- `SessionStore` — Main storage interface
- `StorageConfig` — Configuration (database path, connection pooling)
- `Session` — Session record with metadata
- `Message` — Message in conversation
- `Checkpoint` — Session snapshot
- `ToolCall` — Tool invocation record
- `StorageError`, `StorageResult` — Error handling

## Public API

```rust
use rustycode_storage::{SessionStore, StorageConfig};

// Create store
let config = StorageConfig::default()
    .with_database_path("/data/rustycode.db");

let store = SessionStore::new(config).await?;

// Create session
let session = store.create_session(
    "sess_123",
    "claude-opus-4-7",
    SessionMode::TUI
).await?;

// Add message
store.add_message(
    "sess_123",
    "user",
    "Hello"
).await?;

// Save checkpoint
let checkpoint = store.create_checkpoint(
    "sess_123",
    &session_state
).await?;

// Query sessions
let sessions = store.list_sessions().await?;
for session in sessions {
    println!("{}: {}", session.id, session.status);
}

// Recover from checkpoint
let recovered = store.load_from_checkpoint(&checkpoint).await?;
```

## Tables

- `sessions` — Session metadata (id, model, mode, status, created_at)
- `messages` — Conversation history (session_id, role, content, timestamp)
- `checkpoints` — Session snapshots (session_id, state_json, timestamp)
- `tool_calls` — Tool execution history (session_id, tool, args, result)
- `api_calls` — LLM API calls (session_id, model, tokens_in, tokens_out, cost)

## Features

- **ACID Transactions** — Reliable state persistence
- **Connection Pooling** — Efficient database access
- **Migration** — Automatic schema updates
- **Backup** — Session export/import
- **Query** — Filter sessions by status, model, date range

## Dependencies

- `sqlx` — SQLite driver with compile-time query checking
- `tokio` — Async database operations
- `rustycode-protocol` — Core types
- `anyhow` — Error handling

## Architecture Notes

Storage is primarily read-after-write (checkpoint → load → continue). Queries are optimized for common patterns (recent sessions, sessions by status).

Checkpoints store full session state as JSON for simple recovery. Tool call history enables analysis and replay.

Database is local SQLite by default. Can be configured for remote databases (PostgreSQL, etc.) with appropriate drivers.

## Testing

Tests use in-memory SQLite for fast test execution. Migrations are tested for correctness. ACID properties are verified with concurrent operations.

## See Also

- `rustycode-core` — Session consumer
- `rustycode-protocol` — Types stored
- `rustycode-session` — Session lifecycle (uses storage)
