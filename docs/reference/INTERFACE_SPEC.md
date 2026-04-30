# RustyCode Interface Specification

This document describes the user interface for RustyCode, focusing on the TUI (Terminal User Interface).

---

## Table of Contents

1. [Philosophy](#1-philosophy)
2. [TUI Implementation](#2-tui-implementation)
3. [Web Implementation (Future)](#3-web-implementation-future)
4. [Data Flow](#4-data-flow)
5. [Implementation Guide](#5-implementation-guide)

---

## 1. Philosophy

### 1.1 Core Principle: Start Simple

> "Don't build it until you need it."

RustyCode follows a pragmatic approach:
- **Phase 1**: Get TUI working first
- **Phase 2**: Add features as needed
- **Phase 3**: Extract shared code ONLY when proven necessary

### 1.2 Why Not Over-Engineer?

| Approach | Pros | Cons |
|----------|------|------|
| **Unified Backend + Adapters** | Future-proof, multi-frontend | Complexity, abstraction overhead, premature design |
| **Direct Integration (Chosen)** | Simple, less code, faster to build | May need refactoring later |

**KiloCode Analogy**: KiloCode imports core directly in both TUI and VS Code - no adapter layer. They compile everything together.

### 1.3 Architecture Decision

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         RustyCode Architecture                              │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  ┌──────────────────┐     ┌──────────────────┐     ┌──────────────────┐   │
│  │    rustycode-cli │     │  rustycode-core  │     │ rustycode-tools  │   │
│  │                  │     │                  │     │                  │   │
│  │ - main.rs        │────▶│ - Session mgmt   │────▶│ - Tool registry  │   │
│  │ - TUI entry     │     │ - Context builder│     │ - Tool execution │   │
│  │                  │     │ - Event bus      │     │ - Permissions    │   │
│  └────────┬─────────┘     └────────┬─────────┘     └────────┬─────────┘   │
│           │                        │                        │              │
│           │         ┌──────────────┼────────────────────────┘              │
│           │         │              │                                       │
│           ▼         ▼              ▼                                       │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │                    rustycode-llm                                     │    │
│  │                                                                      │    │
│  │  Providers: Anthropic | OpenAI | Gemini | Ollama | Bedrock | ...   │    │
│  │  - Request/Response handling                                        │    │
│  │  - Streaming support                                                │    │
│  │  - Token tracking                                                   │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
│                                                                              │
│  ┌─────────────────────────────┐                                          │
│  │         rustycode-tui        │  ◀── Primary Interface                  │
│  │         (ratatui)           │                                          │
│  └─────────────────────────────┘                                          │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## 2. TUI Implementation

### 2.1 Technology Stack

- **Framework**: [Ratatui](https://ratatui.rs/) - Rust TUI library
- **Backend**: Crossterm for terminal handling
- **Async Runtime**: Tokio

### 2.2 Layout Structure

```
┌─────────────────────────────────────────────────────────────────────────────┐
│ HEADER (3 rows)                                                             │
│ 💻 RustyCode   [🔵 Ask ▼]   claude-3-5-sonnet   ⚙️                [?]    │
├────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│           MAIN CONTENT (flexible)                                          │
│                                                                             │
│  - Messages scroll                                                          │
│  - Tool results (indented, collapsible)                                     │
│  - Markdown rendering                                                       │
│                                                                             │
├────────────────────────────────────────────────────────────────────────────┤
│ STATUS BAR (1-2 rows)                                                       │
│ 🌿 main │ 1,234 tokens │ ✓ Connected │ 🔄 Tool running...                │
├────────────────────────────────────────────────────────────────────────────┤
│ INPUT AREA (3 rows default)                                                │
│ > Your message here...                                             [Send] │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 2.3 Components

| Component | Location | Status |
|-----------|----------|--------|
| Header | `ui/header.rs` (to create) | To build |
| Messages | `ui/message.rs` | ✅ Implemented |
| Input | `ui/input.rs` | ✅ Implemented |
| Status Bar | `ui/status.rs` | ✅ Implemented |
| Animator | `ui/animator.rs` | ✅ Implemented |
| Command Palette | `ui/command_palette.rs` | ✅ Implemented |
| Mode Selector | `agent_mode.rs` | ✅ Implemented |

### 2.4 Modes

| Mode | Icon | Description |
|------|------|-------------|
| Ask | 🔵 | Default - ask before any action |
| Plan | 📋 | Only describe what would happen |
| Act | ▶️ | Execute but summarize before destructive |
| Yolo | 🚀 | Fully autonomous |

**Keybindings**:
- `Ctrl+M`: Cycle through modes
- `Ctrl+P`: Command palette
- `Ctrl+S`: Toggle sidebar
- `Ctrl+/`: Help

### 2.5 Command Palette

Commands:
```
/newtask .............. Start new task
/continue ............. Continue last session
/model <name> ........ Switch model
/mode <mode> ......... Switch mode (ask/plan/act/yolo)
/settings ............ Open settings
/history ............. View session history
/clear ............... Clear messages
/help ................ Show help
```

---

## 3. Web Implementation (Future)

### 3.1 When to Add Web

Add web interface when:
1. TUI is stable and feature-complete
2. There's clear user demand for browser interface
3. The complexity is justified

### 3.2 Future Architecture (If Needed)

```
┌─────────────────┐     ┌─────────────────┐
│   rustycode-cli │     │  rustycode-web  │
│                 │     │                 │
│  TUI (ratatui)  │     │  Web (SolidJS)  │
│                 │     │                 │
└────────┬────────┘     └────────┬────────┘
         │                      │
         │         ┌────────────┴────────────┐
         │         │    Shared Core         │
         │         │  (rustycode-core)     │
         │         │  - Session            │
         │         │  - LLM               │
         │         │  - Tools              │
         └────────▶│                        │
                   └────────────────────────┘
```

### 3.3 Minimal Web Implementation

If added later, web could be:
- Simple HTTP API for session management
- Frontend as separate project or crate
- No complex adapter layer needed initially

---

## 4. Data Flow

### 4.1 Current Flow (Direct Integration)

```
User Input
    │
    ▼
┌─────────────────┐
│ InputHandler    │──▶ Parse command (/newtask, /mode, etc.)
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│ Event Loop      │──▶ If LLM call → Send to LLM service
└────────┬────────┘         │
         │                 ▼
         │         ┌─────────────────┐
         │         │ rustycode-llm  │──▶ API call
         │         └────────┬────────┘
         │                  │
         │◀─────────────────┘
         │
         ▼
┌─────────────────┐
│ MessagePanel    │──▶ Render markdown
└────────┬────────┘         │
         │                 ▼
         │         ┌─────────────────┐
         └────────▶│ Tool execution │ (if tool call)
                   └────────┬────────┘
                            │
                    Results returned to
                    message panel
```

### 4.2 Event Handling

```rust
pub enum TuiEvent {
    // Input
    KeyPressed(KeyCode, KeyModifiers),
    
    // Application
    ModeChanged(AiMode),
    MessageReceived(Message),
    MessageSent(String),
    ToolStarted(String),
    ToolCompleted(String, ToolResult),
    ToolError(String, String),
    
    // System
    Connected,
    Disconnected,
    Error(String),
}
```

---

## 5. Implementation Guide

### 5.1 Priority Order

| Priority | Task | Description |
|----------|------|-------------|
| P0 | Fix layout | Update `draw_ui_with_context` with proper header, messages, status, input |
| P0 | Wire event loop | Connect input → LLM → messages |
| P1 | Mode switching | Add mode indicator + keybinding |
| P1 | Tool UI | Show tool execution status |
| P2 | Command palette | Register and handle commands |
| P2 | Session history | Save/load sessions |

### 5.2 Current Code Location

- **Entry**: `crates/rustycode-tui/src/lib.rs::run()`
- **Layout**: `crates/rustycode-tui/src/lib.rs::draw_ui_with_context()`
- **Components**: `crates/rustycode-tui/src/ui/`

### 5.3 Step-by-Step

#### Step 1: Update Layout

File: `crates/rustycode-tui/src/lib.rs`

Replace `draw_ui_with_context` with proper component rendering:
- Header with mode/model
- Message list with scrolling
- Status bar
- Input area

#### Step 2: Wire Event Loop

In `run()` function:
1. Connect InputHandler to LLM service
2. Stream LLM responses to MessagePanel
3. Handle tool execution with status updates

#### Step 3: Add Mode Switching

1. Add mode to header display
2. Handle `Ctrl+M` to cycle modes
3. Pass mode to LLM context

#### Step 4: Tool Execution UI

1. Show "Running [tool]..." in status
2. Display tool result after completion
3. Show errors inline

#### Step 5: Polish

1. Command palette implementation
2. Session history
3. Settings

---

## 6. Reference

- Ratatui: https://ratatui.rs/
- Crossterm: https://docs.rs/crossterm/
- KiloCode TUI: `~/dev/kilocode/packages/opencode/src/cli/cmd/tui/`
