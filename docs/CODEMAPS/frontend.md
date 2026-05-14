# Frontend: TUI, CLI, WebSocket Server

<!-- Generated: 2026-05-14 | Files scanned: 1601 | Token estimate: ~600 -->

## TUI (rustycode-tui, 110K LOC, 284 files)

Built on ratatui + crossterm. The largest crate — flagged for future extraction.

### Top-Level Modules

```
tui/src/
├── app/         # Application state machine, event loop
│   ├── event_loop.rs   # Main event loop
│   ├── tasks/          # Background task management
│   └── tool_helpers/   # Tool UI integration
├── ui/          # Rendering components
│   ├── message*.rs     # Chat message display (8 files)
│   ├── input*.rs       # User input handling (5 files)
│   ├── status.rs       # Status bar
│   ├── header.rs       # Top header bar
│   ├── footer.rs       # Bottom footer
│   ├── diff_renderer.rs # Diff visualization
│   ├── command_palette.rs # Ctrl+P command palette
│   ├── file_finder.rs  # Quick file navigation
│   ├── model_selector.rs # Model switching
│   ├── session_sidebar.rs # Session list
│   ├── team_panel.rs   # Multi-agent team view
│   ├── worker_panel.rs # Worker status
│   ├── bookmarks.rs    # Message bookmarks
│   ├── skill_palette.rs # Skill browser
│   ├── marketplace_browser.rs # Plugin marketplace
│   ├── toast.rs        # Notification toasts
│   ├── spinner.rs      # Loading indicators
│   ├── progress.rs     # Progress bars
│   ├── animator.rs     # Animation framework
│   ├── clarification.rs # Clarification dialogs
│   ├── wizard.rs       # Setup wizard
│   ├── accessibility.rs # A11y support
│   └── errors.rs       # Error display
└── theme/       # Theme system
```

### UI Support Crates
- `rustycode-ui-core` (2K LOC) — MarkdownRenderer, SyntaxHighlighter (syntect)
- `rustycode-ui-model` (1K LOC) — RunController trait, shared UI types

## CLI (rustycode-cli, 8.5K LOC)

Entry point for both CLI and TUI modes.

```
cli/src/
├── lib.rs          # CLI init, Prompt re-exports
├── commands/       # Subcommand handlers
│   └── ensemble_cmd.rs  # Ensemble/benchmark commands
├── prompt.rs       # Interactive prompts (Select, Input, Confirm, MultiSelect)
└── main.rs         # Binary entry point
```

**Startup flow:** `main.rs` → clap arg parsing → `tui::run()` or CLI subcommand

## WebSocket Server (rustycode-ws-server, 3.4K LOC)

Browser-based remote control of RustyCode sessions.

```
ws-server/src/
├── bridge.rs      # EventBus ↔ WebSocket bridge
├── router.rs      # WsRouter (message routing)
├── session.rs     # WS session management
├── protocol.rs    # ClientMessage / ServerMessage / Envelope
├── approval.rs    # Remote approval for tool calls
└── auth.rs        # WS authentication
```

## HTTP Server (rustycode-server, 185 LOC)

Axum-based approval/interaction server for headless mode.

```
server/src/
├── server.rs      # AppServer
├── router.rs      # Route definitions
├── handler.rs     # Request handlers
└── approval.rs    # Approval endpoints
```

## Server Client (rustycode-server-client, 126 LOC)

Client for connecting to the HTTP/WS server from external tools.
