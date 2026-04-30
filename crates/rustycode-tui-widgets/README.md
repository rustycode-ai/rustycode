# rustycode-tui-widgets

Specialized UI components for RustyCode TUI.

## Purpose

Provides domain-specific UI widgets tailored for RustyCode's feature set. Includes message display with syntax highlighting, tool execution panels, input with history and completion, status bars, and specialized sidebars. All components build on the ratatui framework.

## Key Components

- **Message Display** — Renders conversation history with code syntax highlighting
- **Tool Panels** — Shows tool execution status, results, and error details
- **Input** — Command input with history, completion suggestions, and validation
- **Status Bar** — Session info, model/provider selection, progress indicators
- **Sidebars** — Session browser, file navigation, help, memory, agent status

## Key Types

- Message rendering components
- Tool panel state and display
- Input widget with history
- Status bar configuration
- Sidebar implementations

## Public API

```rust
use rustycode_tui_widgets::message::MessageWidget;

let widget = MessageWidget::new(theme);
widget.render(&messages, area, buf);
```

## Features

- Markdown rendering with code blocks
- Syntax highlighting (Rust, Python, etc.)
- Tool result display with formatting
- Input autocomplete suggestions
- Responsive layout

## Dependencies

- `ratatui` — TUI framework
- `syntect` — Syntax highlighting
- `crossterm` — Terminal manipulation

## Architecture Notes

- All widgets follow ratatui Widget trait
- Stateless rendering (state managed separately)
- Customizable themes and colors
- Efficient incremental updates

## Testing

- Widget layout tests
- Markdown rendering tests
- State management tests
- Theme application tests

## See Also

- `rustycode-ui-core` — Shared UI types and utilities
- `rustycode-tui-core` — Core framework
