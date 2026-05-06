# rustycode-ui-core

Shared UI components and types for web and terminal interfaces.

## Purpose

Provides cross-platform UI abstractions and components usable by both terminal (TUI) and web frontends. Includes message types, session management, markdown rendering, and syntax highlighting.

## Key Types

- `FrontendMessage` — Message with kind (User/Assistant/System/Tool/Error)
- `FrontendMessageKind` — Enumerated message types
- `FrontendSession` — Session state with messages and input
- `SubmittedInput` — Parsed user input (ChatMessage/SlashCommand/BangCommand/Empty)
- `RunController` — Trait for managing request/response lifecycle
- `SessionRunController` — Default implementation

## Public API

```rust
use rustycode_ui_core::{FrontendSession, FrontendMessageKind, SubmittedInput};

let mut session = FrontendSession::default();
session.add_message("Hello", FrontendMessageKind::User);

let input = SubmittedInput::parse("/help");
match input {
    SubmittedInput::SlashCommand(cmd) => println!("Command: {}", cmd),
    _ => {}
}

// Request lifecycle
session.start_assistant_request();
session.append_assistant_chunk("response...");
session.finish_assistant_message("complete response".to_string());
```

## Features

- Type-safe message representation
- Session state management
- Request/response lifecycle tracking
- Input command parsing
- Serde serialization for IPC/persistence

## Key Components

### Message Types

- **User** — User-submitted messages
- **Assistant** — LLM responses
- **System** — System prompts or information
- **Tool** — Tool execution results
- **Error** — Error messages and failures

### Input Parsing

- `/command` → SlashCommand
- `!command` → BangCommand (shell-like)
- `regular text` → ChatMessage
- `` (empty) → Empty

## Dependencies

- `serde`/`serde_json` — Serialization
- `handlebars` — Template rendering
- `syntect` — Syntax highlighting
- `ratatui` — TUI rendering

## Architecture Notes

- Agnostic to frontend implementation (TUI or Web)
- Serializable for IPC between processes
- Markdown support with customizable rendering

## Testing

- Message serialization tests
- Session state tests
- Input parsing tests
- Request lifecycle tests

## See Also

- `rustycode-tui-widgets` — TUI widget implementations
- `rustycode-prompt` — Prompt templating
