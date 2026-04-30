# rustycode-tui-core

Core TUI framework for RustyCode terminal interface.

## Purpose

Provides the foundational framework for the RustyCode terminal user interface. Handles terminal management, event loop with frame budgeting, reactive event processing, and efficient screen updates. Core infrastructure that all TUI components build upon.

## Key Types

- `EventLoop` — Main event loop with frame budgeting (60 FPS default)
- `TerminalManager` — Terminal lifecycle and control
- `TerminalCleanupGuard` — RAII cleanup on drop
- `TuiBackend` — Terminal backend abstraction (Crossterm)
- `EventLoopConfig` — Configurable event loop parameters
- Placeholder types for dependency injection: `InputState`, `SearchState`, `Task`, `WorkspaceTasks`, `ThemeColors`, etc.

## Public API

```rust
use rustycode_tui_core::{EventLoop, EventLoopConfig, TerminalManager};

let config = EventLoopConfig::default().with_fps(60);
let mut event_loop = EventLoop::new(config)?;

let mut terminal_manager = TerminalManager::setup()?;
let _guard = terminal_manager.cleanup_on_drop();

event_loop.run(&mut terminal_manager)?;
```

## Architecture Notes

- Frame budgeting prevents excessive CPU usage (e.g., 16ms per frame at 60 FPS)
- Input latency cap ensures responsiveness
- Terminal backend abstraction allows swapping implementations
- Placeholder pattern for dependency injection of components

## Key Constants

- `FRAME_BUDGET_60FPS` — ~16ms per frame
- `MAX_INPUT_LATENCY` — Maximum input processing latency

## Testing

- Event loop timing tests
- Terminal lifecycle tests
- Input event processing tests

## See Also

- `rustycode-tui-widgets` — UI components built on core
- `rustycode-tui-agents` — Agent management
- `rustycode-tui-memory` — Memory integration
