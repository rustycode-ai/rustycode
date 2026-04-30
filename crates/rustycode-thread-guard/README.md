# rustycode-thread-guard

Thread safety utilities for TUI/UI isolation.

## Purpose

Provides assertions and utilities to ensure terminal UI operations are not run on the terminal/UI thread, which would block the event loop and freeze the UI. Used to catch threading violations early in development.

## Key Types/Functions

- `is_terminal_thread()` — Returns true if current thread is the terminal (UI) thread
- `assert_not_terminal_thread(op: &str)` — Panics if called on terminal thread with operation name

## Public API

```rust
use rustycode_thread_guard::{is_terminal_thread, assert_not_terminal_thread};

// Check thread type
if is_terminal_thread() {
    // In the TUI event loop
}

// Assert safety (panics if violated)
assert_not_terminal_thread("file_read");
assert_not_terminal_thread("tool_execution");
```

## Thread Detection

- Heuristic: thread named "main" is treated as the terminal thread
- Future: can be extended with OS-specific checks for finer detection

## Dependencies

- `std::thread` — Standard library threading

## Architecture Notes

- Part of RustyCode's thread safety layer
- Complements `rustycode-shared-runtime` for safe async work
- Used in tools, file operations, and any blocking I/O
- Zero cost when assertion succeeds (simple string comparison)

## Testing

- Tests for thread naming behavior
- Panic verification on main thread
- Multi-thread scenario testing

## See Also

- `rustycode-shared-runtime` — Safe async execution off terminal thread
- `rustycode-tools` — Enforces thread safety for tool execution
