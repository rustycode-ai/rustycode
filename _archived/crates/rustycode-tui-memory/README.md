# rustycode-tui-memory

Memory management and injection for TUI.

## Purpose

Integrates the memory system with the terminal interface. Provides automatic memory persistence, context injection from relevant memories, memory-related slash commands, and relevance-based memory filtering for session context.

## Key Types

- `AutoMemoryManager` — Automatic save/load of session memory
- `MemoryInjector` — Injects relevant memories into prompts
- `ThreadSafeMemory` — Thread-safe memory operations
- Memory relevance ranking and filtering
- Memory slash command handlers

## Public API

```rust
use rustycode_tui_memory::{AutoMemoryManager, MemoryInjector};

let mut auto_mem = AutoMemoryManager::new();
auto_mem.save_session().await?;

let injector = MemoryInjector::new();
let relevant = injector.find_relevant(&query, 5).await?;
```

## Features

- Auto-save after each message
- Context-aware memory ranking
- Slash command support (/remember, /forget, /memories)
- Thread-safe memory access

## Architecture Notes

- Sits between TUI and memory backend
- Handles relevance scoring
- Manages memory lifecycle within sessions

## Testing

- Memory persistence tests
- Relevance ranking tests
- Injection accuracy tests

## See Also

- `rustycode-memory` — Core memory system
- `rustycode-tui-core` — TUI framework
