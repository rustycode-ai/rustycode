# rustycode-web

WebAssembly frontend for RustyCode with brutalist UI design, rendered in the browser via ratzilla and ratatui.

## Purpose

Provides a browser-based interface to RustyCode by compiling to WASM and rendering a ratatui TUI layout to the DOM. This crate enables users to interact with the RustyCode session, manage skills, and execute tools through a web page -- without installing anything locally.

Because WASM runs in a sandbox, this crate cannot call the LLM or execute tools directly. All tool execution and LLM calls are proxied through an external `rustycode-tool-server` over HTTP. Persistence uses IndexedDB instead of the filesystem.

**Note:** This crate is excluded from the main Cargo workspace and has its own build target. It is built with `trunk` for WASM output, not with the standard `cargo build` workspace pipeline.

## Current Architecture

```
src/
  lib.rs            -- WASM-exported helpers (init, version, theme colors, formatters)
  main.rs           -- Application entry point, ratatui DOM renderer, key handling, tool-server HTTP
  skills.rs         -- Skill data model and in-memory skill manager
  slash_commands.rs -- Slash command parser and handler (/help, /skills, /memory, etc.)
```

The UI is a two-panel layout (60/40 split): the left panel shows the conversation, and the right panel shows contextual information (skills browser, stats, marketplace). A header bar displays the app title, and a footer bar shows debug status (last key, quit state, pending requests).

### Module Summary

- **lib.rs** -- Wasm-bindgen exports: `init()`, `version()`, `name()`, `brutalist_mode()`, `theme_colors()`, `format_message()`, `format_tool()`, `greeting()`, `streaming_frame()`, `log()`. These are callable from JavaScript and handle theme colors (Nord palette), message formatting with role prefixes, and streaming animation frames.
- **main.rs** -- The WASM binary entry point. Sets up the ratzilla DOM backend, constructs a ratatui `Terminal`, binds keyboard events, and runs the render loop. User input is routed through `FrontendSession::submit_input()` (from `rustycode-ui-core`), which distinguishes chat messages, slash commands, and bang commands. Chat messages are sent as `ToolCall` JSON to the tool-server at `http://127.0.0.1:3000/call`.
- **skills.rs** -- Defines `WebSkill`, `WebSkillStatus`, `SkillCategory`, and `WebSkillManager`. Maintains a static list of built-in skills (code-review, write-tests, explain-code, git-commit, refactor, deploy) with activate/deactivate/run operations. Skill execution is deferred to the tool-server.
- **slash_commands.rs** -- Parses and dispatches slash commands: `/help`, `/stats`, `/skills`, `/skill`, `/memory`, `/marketplace`, `/theme`, `/compact`, `/save`, `/load`, `/mcp`. Each handler returns a `CommandResult` with an optional right-panel update.

## Key Types & Public API

### WASM Exports (lib.rs)

```rust
// Initialization
pub fn init() -> Result<(), JsValue>;
pub fn version() -> String;           // "v0.1.0"
pub fn name() -> String;              // "RustyCode"
pub fn brutalist_mode() -> bool;      // always true

// Display helpers
pub fn theme_colors() -> String;      // JSON with Nord palette colors
pub fn format_message(role: &str, content: &str, timestamp: &str) -> String;
pub fn format_tool(name: &str, status: &str) -> String;
pub fn greeting() -> String;
pub fn streaming_frame(frame: usize) -> char;  // 4-cycle animation: ◐◑◒◓
pub fn log(message: &str);            // console.log bridge
```

### Skills Module (skills.rs)

```rust
pub struct WebSkill {
    pub name: String,
    pub description: String,
    pub category: SkillCategory,
    pub status: WebSkillStatus,
    pub auto_enabled: bool,
    pub triggers: Vec<String>,
    pub run_count: usize,
}

pub struct WebSkillManager {
    pub skills: Vec<WebSkill>,
    pub selected_index: usize,
}

impl WebSkillManager {
    pub fn new() -> Self;
    pub fn list_skills(&self) -> String;
    pub fn activate_skill(&mut self, name: &str) -> Result<String, String>;
    pub fn deactivate_skill(&mut self, name: &str) -> Result<String, String>;
    pub fn run_skill(&mut self, name: &str) -> Result<String, String>;
    pub fn get_skills_for_panel(&self) -> String;
}
```

### Slash Commands (slash_commands.rs)

```rust
pub struct CommandResult {
    pub success: bool,
    pub message: String,
    pub panel_update: Option<PanelUpdate>,
    pub refresh_skills: bool,
}

pub fn execute_command(input: &str, skill_manager: &mut WebSkillManager) -> CommandResult;
```

### Application State (main.rs)

```rust
struct WebAppState {
    session: FrontendSession,         // from rustycode-ui-core
    selected_panel: usize,            // 0 = conversation, 1 = task/info
    last_key: String,                 // debug display
    quit_requested: bool,
    theme: MessageTheme,              // from rustycode-ui-core
    right_panel_content: String,      // contextual panel text
    skill_manager: WebSkillManager,
}
```

## Features

- **Brutalist UI Design** -- Nord color palette with box-drawing characters for a distinctive terminal aesthetic in the browser
- **Two-Panel Layout** -- 60/40 split between conversation view and contextual info (skills, stats, marketplace)
- **Shared Session Logic** -- Uses `FrontendSession`, `RunController`, and `SubmittedInput` from `rustycode-ui-core` for input parsing and message management, keeping behavior consistent with the TUI
- **Skill Management** -- Built-in skill browser with activate/deactivate/run operations
- **Slash Commands** -- 11 slash commands covering help, skills, memory, marketplace, theme, compact, save/load, and MCP
- **Tool Server Proxy** -- All tool execution is proxied to an external HTTP server (`rustycode-tool-server`) to work around WASM sandbox constraints
- **WASM Exports** -- JavaScript-callable functions for initialization, theme data, message formatting, and streaming animations

## Dependencies

### External (WASM-specific)

- `wasm-bindgen` -- Rust/JS interop bindings
- `wasm-bindgen-futures` -- Future spawning in WASM
- `js-sys` -- JavaScript standard library bindings
- `web-sys` -- Browser API bindings (console, Window, Location)
- `gloo-net` -- HTTP client for WASM (fetch-based)
- `console_error_panic_hook` -- Panic handler that logs to browser console
- `console_log` -- Log backend for browser console
- `ratzilla` -- Ratatui DOM renderer for WASM
- `getrandom` (with `wasm_js` feature) -- Random number generation via `crypto.getRandomValues`

### External (General)

- `serde` / `serde_json` -- Serialization
- `log` -- Logging facade
- `itertools` -- Iterator utilities

### Cross-Crate

- `rustycode-ui-core` -- Shared session types (`FrontendSession`, `MarkdownRenderer`, `MessageTheme`, `RunController`, `SubmittedInput`)
- `rustycode-protocol` -- Shared types (`ToolCall`, `ToolResult`)

## Architecture Notes

**WASM Sandbox Constraints:** The browser WASM environment has no filesystem access, no subprocess execution, and no raw TCP sockets. This crate works around these limitations by:

1. Delegating tool execution to `rustycode-tool-server` via HTTP POST to `/call`
2. Using `gloo-net` (built on `fetch()`) for all HTTP requests
3. Using `getrandom` with the `wasm_js` feature for cryptographic randomness
4. Planning IndexedDB for persistence (not yet implemented)

**Build System:** This crate uses `trunk` as its WASM build tool, configured via `Trunk.toml`. The `index.html` file is the entry point. This crate is excluded from the main workspace (`Cargo.toml` exclude list) because it targets `wasm32-unknown-unknown` rather than the native host target.

**Shared UI Code:** The crate depends on `rustycode-ui-core` for session management and input parsing, ensuring consistent behavior between the TUI and web frontends. The `FrontendSession` type handles message storage and input parsing, while `SessionRunController` manages request lifecycle state.

**Theme:** The Nord color palette is hardcoded. Theme switching via `/theme` is accepted but requires a reload to apply (not yet dynamically wired).

## Testing

```bash
# Run unit tests (native target, not WASM)
cargo test

# Run tests for a specific module
cargo test -- test_format_message
cargo test -- test_streaming_frame
cargo test -- test_theme_colors
```

The crate has 15+ unit tests covering all WASM export functions: version, name, brutalist mode, message formatting (all roles, multiline, unknown roles), tool status formatting (running, complete, failed, unknown), streaming animation cycling, greeting content, and theme color JSON structure.

Tests run on the native target since the logic is platform-independent. WASM-specific functionality (DOM rendering, HTTP calls) is not covered by automated tests.

## See Also

- `rustycode-ui-core` -- Shared session types and markdown rendering
- `rustycode-protocol` -- `ToolCall` / `ToolResult` types used for tool-server communication
- `rustycode-tui` -- Terminal UI (ratatui-based), the native counterpart to this crate
- `rustycode-tool-server` -- External HTTP server that executes tools on behalf of the WASM frontend
- `SPEC.md` -- Feature parity specification between TUI and web versions
