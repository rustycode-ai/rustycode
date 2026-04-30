# RustyCode Web Parity Specification

## Overview

This document defines the feature parity between RustyCode TUI and Web (WASM) versions. The goal is to bring the web version to feature-parity with the TUI while working within browser constraints.

## Current State

| Category | TUI Features | Web Features |
|----------|--------------|---------------|
| **UI Framework** | ratatui + tty | ratatui + DOM (ratzilla) |
| **Core** | Full session management | Basic session (FrontendSession) |
| **Modules** | 100+ Rust files | 2 files |
| **Slash Commands** | 15+ (compact, copilot, hook, load, marketplace, mcp, memory, review, save, skill, stats, theme, etc.) | None |
| **Skills** | Full lifecycle (manager, loader, installer, updater, activation, as_tool, composition, preferences, search, suggester) | None |
| **Marketplace** | Registry, index, installer, updates | None |
| **Memory** | Full system (injection, command, auto, relevance) | None |
| **MCP Mode** | Yes | No |
| **Session Recovery** | Yes | No |
| **Tool Execution** | Direct via rustycode-tools | External tool-server via HTTP |
| **Clipboard** | Yes | Partial |

## Architecture Constraints

1. **WASM Limitations**: No `std::fs`, no `std::process::Command`, no direct TCP sockets
2. **Tool Execution**: Must go through external `rustycode-tool-server` 
3. **Persistence**: IndexedDB instead of filesystem
4. **Networking**: HTTP via `gloo-net`, WebSocket for streaming

## Target Features

### Phase 1: Core Parity (High Priority)

#### 1.1 Slash Command Support
- [ ] `/skill` - List and manage skills
- [ ] `/skills` - Skill suggestions based on context
- [ ] `/memory` - Memory operations (add, list, clear)
- [ ] `/marketplace` - Browse marketplace
- [ ] `/theme` - Theme switching
- [ ] `/stats` - Session statistics
- [ ] `/compact` - Context compaction
- [ ] `/save` - Save conversation
- [ ] `/load` - Load saved conversation
- [ ] `/mcp` - MCP server management

#### 1.2 Session State Enhancement
- [ ] Add `tool_iteration_count` tracking
- [ ] Add session metadata (created_at, last_updated)
- [ ] Add retry state management

### Phase 2: Skills System (Medium Priority)

#### 2.1 Skill Data Model
- [ ] Define `Skill` struct compatible with WASM
- [ ] Define `SkillStatus` enum
- [ ] Define `TriggerCondition` enum

#### 2.2 Skill UI Components
- [ ] Skill list panel (right side)
- [ ] Skill activation/deactivation toggle
- [ ] Skill status indicators

#### 2.3 Skill Execution
- [ ] Execute skills via tool-server
- [ ] Handle skill parameters via UI prompts
- [ ] Display skill output in conversation

### Phase 3: Marketplace (Medium Priority)

#### 3.1 Marketplace Data Model
- [ ] `MarketplaceItem` struct (name, description, author, version, type)
- [ ] `ItemType` enum (Skill, Tool, MCP)

#### 3.2 Marketplace UI
- [ ] Browse items in right panel
- [ ] Search/filter functionality
- [ ] Install button (triggers tool-server)
- [ ] Update check functionality

### Phase 4: Memory System (Lower Priority)

#### 4.1 Memory Data Model
- [ ] `MemoryEntry` struct
- [ ] In-memory store with IndexedDB persistence

#### 4.2 Memory UI
- [ ] Display active memories
- [ ] Add memory command
- [ ] Clear memory command

### Phase 5: Advanced Features (Nice to Have)

#### 5.1 Session Recovery
- [ ] Serialize session to IndexedDB
- [ ] Restore on page load

#### 5.2 MCP Mode
- [ ] Connect to MCP server via WebSocket
- [ ] Handle MCP protocol messages

## Implementation Notes

### Shared Code Strategy
Leverage `rustycode-ui-core` for:
- `FrontendSession` - Already shared
- `SubmittedInput` parsing - Already shared
- `RunController` trait - Already shared
- Add: Slash command parsing helpers
- Add: Skill/Marketplace data models

### Tool Server Communication
```
Web (WASM) → HTTP POST /call → rustycode-tool-server → LLM → Response
```

### IndexedDB Schema
- `sessions`: Serialized FrontendSession
- `memories`: Memory entries
- `preferences`: User preferences

## Success Criteria

- [ ] All 15+ slash commands functional
- [ ] Skills can be browsed, activated, and executed
- [ ] Marketplace items can be browsed and installed
- [ ] Memory entries persist across sessions
- [ ] Session recovery works on page reload
- [ ] 80%+ code reuse from shared crates