# rustycode-tui

Interactive terminal user interface for RustyCode, built with ratatui.

## Purpose

Provides the full interactive TUI experience: conversation with LLMs, tool execution approval, workspace browsing, session management, slash commands, agent orchestration display, and first-run configuration. The `rustycode` CLI launches this library by default when no task or subcommand is provided.

This crate now exposes the TUI as a library that the `rustycode` CLI launches by default. It sits at the top of the dependency graph and consumes nearly every other crate in the workspace.

## Current Architecture

The crate is organized into 14 top-level modules spanning 245 source files and ~92K lines of code:

```
src/
  lib.rs               Public API, module declarations, run() entry point
  logging.rs           File-based log writer with rotation
  handlers.rs          Key handler type aliases and dispatch documentation
  unicode.rs           Unicode width and segmentation utilities
  utils.rs             Shared helper functions
  async_io.rs          Async I/O bridging for terminal events

  app/                 Application state and event loop
  ui/                  Ratatui rendering components (plugin-based)
  services/            Background service integrations
  agents/              Multi-agent lifecycle management
  memory/              Context compaction and memory injection
  workspace/           Workspace scanning and context loading
  skills/              Skill loading, composition, and lifecycle
  slash_commands/      Slash command handlers (/compact, /model, /help, etc.)
  plugin/              Experimental plugin system
  marketplace/         Skill and tool marketplace client
  help/                Help system with topics and keyboard shortcuts
  tool_approval/       Risk-based tool approval UI
  observability/       Metrics dashboard and progress display
  theme/               Theme definitions and color parsing
```

### Entry Point

```rust
pub fn run(cwd: PathBuf, reconfigure: bool, resume: bool) -> Result<()>
```

Called from `main.rs` after CLI parsing. Creates a `TUI` instance, initializes background services, and enters the responsive event loop.

## Key Types & Public API

### Application Core

- `TUI` — Main application struct (defined in `app::event_loop`). Wires together all UI components, service manager, agent manager, and state. Owns the responsive event loop with 60 FPS / 16ms frame budget and 50ms input latency guarantee.
- `ServiceManager` — Background service integration layer (`app::service_integration`). Manages channels for LLM streaming, tool execution, and workspace loading.
- `StateManager` — Application state persistence (`app::state_manager`).

### Interaction Modes

- `AiMode` — User-facing autonomy level: `Ask` (default), `Plan`, `Act`, `Yolo`. Controls whether tools require approval.
- `AgentMode` — Task specialization: `Code`, `Architect`, `Debug`, `Review`, `Test`, `Refactor`, `Docs`.
- `InputMode` — Input focus state managed by `InputHandler`.

### Services

- `ConversationService` — LLM prompt construction, conversation management, memory integration.
- `SessionRecoveryManager` — Crash detection via lock files, state serialization, and session restore.
- `CheckpointManager` — Save/restore conversation snapshots.
- `McpMode` — MCP server management UI with tool discovery and execution.
- `SessionMode` — Session history browser with compaction controls.

### UI Components

All in the `ui/` module, following a plugin-based architecture where each component is self-contained:

- `MessageRenderer` — Hierarchical message display with markdown, syntax highlighting, diffs, and thinking blocks
- `InputHandler` — Multi-line input with history, paste, image support, and vi-like keybindings
- `CommandPalette` — Fuzzy-search command palette
- `ModelSelector` — Model picker with provider grouping
- `SessionSidebar` — Session list with metadata and switching
- `ToastManager` — Non-blocking notification toasts
- `TeamPanel` / `WorkerPanel` — Agent timeline and sub-agent status display
- `FileFinder` / `FileSelector` — Fuzzy file navigation
- `SkillPalette` — Skill browser and parameter input

## Features

### Feature Flags

| Feature | Default | Description |
|---------|---------|-------------|
| `default` | enabled | Standard TUI build |
| `vector-memory` | disabled | Semantic memory via `rustycode-vector-memory` |
| `live-api-tests` | disabled | Live API tests (requires API key) |
| `ollama-tests` | disabled | Ollama integration tests |

### Major Capabilities

1. **LLM Conversation** — Streaming token display, tool use parsing, multi-turn context with auto-compaction
2. **Tool Execution** — Risk-based approval UI, safe auto-approve, session-scoped permissions
3. **Slash Commands** — 16 built-in: `/compact`, `/model`, `/help`, `/cost`, `/save`, `/review`, `/memory`, `/skill`, `/skillify`, `/mcp`, `/marketplace`, `/theme`, `/hook`, `/stats`, `/load`, `/copilot`
4. **Session Management** — Create, switch, archive, export sessions. Crash recovery via lock file detection
5. **Checkpoint System** — Save/restore conversation snapshots with branching
6. **Multi-Agent Orchestration** — Spawn, monitor, cancel, retry background agents
7. **Workspace Intelligence** — Project file detection, directory structure, git status, incremental scanning
8. **Memory System** — Auto-extraction, relevance scoring, injection, optional vector-based semantic memory
9. **Skill System** — YAML-based skill definitions, composition, lifecycle management
10. **Marketplace** — Browse, search, install community skills, tools, and MCP servers
11. **Deep Thinking** — Heuristic complexity detection with planning prompts
12. **Theme System** — Built-in themes, custom color support, live preview switching
13. **Observability** — Metrics dashboard, token usage tracking, cost estimation

## Known Limitations & God Object Status

This is the **largest crate in the workspace** and is explicitly recognized as a god object requiring decomposition.

### Scale

| Metric | Value |
|--------|-------|
| Source files | 245 |
| Lines of code | ~92,000 |
| Internal crate dependencies | 22 |
| External crate dependencies | 20+ |
| Top-level modules | 14 |

### Structural Issues

1. **Excessive dependency count** (22 internal crates). The TUI directly depends on nearly every other crate.
2. **`app/event_loop.rs` is ~1960 lines** and contains the `TUI` struct that owns all state.
3. **`services/conversation_service.rs` is ~1800 lines** combining prompt construction, conversation management, and memory integration.
4. **`services/session_mode.rs` (~1600 lines)** and **`services/mcp_mode.rs` (~1000 lines)** embed rendering code within service modules.
5. **Services overlap with other crates.** `ConversationService` duplicates logic from `rustycode-core`. Memory modules duplicate `rustycode-memory`.
6. **Plugin system is experimental** — no dynamic loading, no permission enforcement.

### Contributing Guidelines

- Keep changes localized to modules. Do not add new dependencies.
- Prefer extracting to new crates or delegating to existing ones.
- Do not expand `event_loop.rs`. New features should be components, not inline code.

## Intended Future Architecture

```
rustycode-tui (thin shell)
  |
  +-- rustycode-tui-renderer     -- Ratatui layout, widget rendering
  +-- rustycode-tui-input        -- Input handling, history, paste
  +-- rustycode-tui-services     -- ServiceManager, streaming, tool bridge
  +-- rustycode-conversation     -- ConversationService (extracted)
  +-- rustycode-tui-workspace    -- WorkspaceContext, scanner
  +-- rustycode-tui-agents       -- AgentManager, task tracking
  +-- rustycode-tui-skills       -- Skill palette, lifecycle, composition
  +-- rustycode-marketplace      -- Marketplace client and registry
```

### Migration Priorities

1. Extract `conversation_service.rs` — largest service, heavy overlap with `rustycode-core`
2. Extract rendering from service modules — `session_mode.rs` and `mcp_mode.rs` contain UI code
3. Consolidate memory modules — delegate to `rustycode-memory` and `rustycode-vector-memory`
4. Split `app/event_loop.rs` — state to `StateManager`, rendering to components

## Dependencies

### Internal (22 crates)

`rustycode-protocol`, `rustycode-runtime`, `rustycode-core`, `rustycode-shared-runtime`, `rustycode-tools`, `rustycode-llm`, `rustycode-memory`, `rustycode-ui-core`, `rustycode-config`, `rustycode-prompt`, `rustycode-providers`, `rustycode-session`, `rustycode-skill`, `rustycode-auth`, `rustycode-mcp`, `rustycode-orchestration`, `rustycode-storage`, `rustycode-observability`, `rustycode-guard`, `rustycode-vector-memory` (optional)

### External (key)

- `ratatui` + `crossterm` — Terminal rendering framework
- `pulldown-cmark` — Markdown parsing
- `similar` — Diff generation
- `tokio` — Async runtime
- `arboard` — Clipboard integration
- `serde` + `serde_json` + `serde_yaml` + `toml` — Serialization
- `tracing` — Structured logging
- `image` + `base64` — Image handling for vision models
- `parking_lot` — High-performance synchronization
- `unicode-segmentation` + `unicode-width` — Terminal-aware text handling

## Architecture Notes

### Responsive Event Loop

The event loop processes one item per frame from each service channel, guaranteeing input latency under 50ms and rendering at up to 60 FPS:

```
Frame:
  1. Poll ServiceManager (one stream chunk, one tool result, one workspace update)
  2. Check frame budget (16ms)
  3. Render if budget allows
  4. Handle input with remaining budget
```

Backpressure is handled by bounded channels. Services drop events when the TUI cannot keep up.

### Terminal Safety

A `TerminalCleanupGuard` and custom panic hook ensure the terminal is restored on both normal exit and panic. Raw mode is disabled, alternate screen is left, cursor is shown.

### Mode-Aware Tool Approval

Tool execution follows a risk classification pipeline:
- Risk level (`Safe`, `Medium`, `High`, `Dangerous`) determines approval requirements
- Session-scoped approvals persist across requests
- Safe tools can be auto-approved when `AiMode::Yolo` is active

## Testing

```bash
# Unit tests
cargo test -p rustycode-tui

# With live API (requires ANTHROPIC_API_KEY)
cargo test -p rustycode-tui --features live-api-tests

# With Ollama (requires running Ollama instance)
cargo test -p rustycode-tui --features ollama-tests

# Run the TUI through the CLI
cargo run -p rustycode-cli

# With options
cargo run -p rustycode-cli -- tui --resume
cargo run -p rustycode-cli -- tui --reconfigure
cargo run -p rustycode-cli -- tui --model claude-opus-4-7
```

Tests use mock terminal backends (`ratatui::backend::TestBackend`) for rendering assertions. Service tests use mock LLM providers.

## See Also

- `rustycode-ui-core` — Shared rendering utilities
- `rustycode-core` — Session runtime and headless execution
- `rustycode-llm` — LLM provider abstraction
- `rustycode-tools` — Tool execution framework
- `rustycode-memory` / `rustycode-vector-memory` — Memory management
- `rustycode-orchestration` — Structured reasoning and orchestration engine
- `rustycode-cli` — Alternative CLI interface
- `/docs/architecture/ARCHITECTURE-REVIEW-2026-04-20.md` — Architecture review
