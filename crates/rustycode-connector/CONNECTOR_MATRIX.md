# Terminal Connector Capability Matrix

This document compares the capabilities and performance of terminal connectors supported by rustycode.

## Overview

| Connector | Platform | Type | Status |
|-----------|----------|------|--------|
| tmux | Cross-platform | Terminal multiplexer | **Production Ready** |
| it2 CLI | macOS only | iTerm2 Python API CLI | **Working (slow)** |
| iTerm2 (AppleScript) | macOS only | Terminal emulator | **Limited Support** |
| iTerm2 (Native) | macOS only | Unix socket + Protobuf | **New - Fastest iTerm2 option** |

## Capability Comparison

| Feature | tmux | it2 CLI | iTerm2 (AppleScript) | iTerm2 (Native) |
|---------|------|---------|----------------------|-----------------|
| `create_session` | ✅ Full support | ✅ Supported (slow) | ✅ Supported (slow) | ✅ Supported (AppleScript fallback) |
| `split_pane` (horizontal) | ✅ Full support | ✅ Supported | ✅ Supported | ✅ Supported (native) |
| `split_pane` (vertical) | ✅ Full support | ✅ Supported | ✅ Supported | ✅ Supported (native) |
| `send_keys` | ✅ Full support | ✅ Supported (very slow) | ✅ Supported | ✅ Supported (native) |
| `capture_output` | ✅ Full support | ✅ Supported (slow) | ❌ Not supported | ✅ Supported (native) |
| `set_pane_title` | ✅ Full support | ✅ Supported | ⚠️ Window title only | ✅ Supported (native) |
| `select_pane` | ✅ Full support | ⚠️ Session focus only | ✅ Supported | ✅ Supported (AppleScript fallback) |
| `kill_pane` | ✅ Full support | ❌ Not supported | ❌ Not supported | ❌ Not supported |
| `close_session` | ✅ Full support | ✅ Supported | ✅ Supported | ✅ Supported (AppleScript fallback) |
| `wait_for_output` | ✅ Full support | ✅ Supported (slow) | ❌ Not supported | ✅ Supported (native) |
| `list_sessions` | ✅ Full support | ⚠️ Tracked sessions only | ⚠️ Tracked sessions only | ⚠️ Tracked sessions only |
| `session_info` | ✅ Full support | ⚠️ Tracked sessions only | ⚠️ Tracked sessions only | ⚠️ Tracked sessions only |

## Performance Benchmarks

*Run on macOS with tmux 3.4, it2 CLI 0.2.1, and iTerm2 3.5*

### Operation Latency (milliseconds, lower is better)

| Operation | tmux | it2 CLI | iTerm2 (AppleScript) | iTerm2 (Native)* | Slowdown (it2) | Slowdown (AppleScript) |
|-----------|------|---------|----------------------|------------------|----------------|------------------------|
| Session create | 5ms | 733ms | 272ms | ~270ms | 147x slower | 54x slower |
| Split pane | 8ms | 310ms | 93ms | ~10-20ms | 39x slower | 12x slower |
| Send keys | 4ms | 288ms | 34ms | ~5-10ms | 72x slower | 9x slower |
| Capture output | 8ms | 575ms | N/A | ~10-20ms | 72x slower | N/A |
| Set pane title | 4ms | 304ms | 36ms | ~5-10ms | 76x slower | 9x slower |
| Select pane | 4ms | 153ms | 34ms | ~30-50ms | 38x slower | 9x slower |
| Kill pane | 5ms | N/A | N/A | N/A | N/A | N/A |
| Close session | 5ms | 281ms | 102ms | ~100ms | 56x slower | 20x slower |
| **Total** | **43ms** | **2644ms** | **571ms** | **~500-600ms** | **61x slower** | **13x slower |

*Note: iTerm2 Native uses AppleScript for window creation/close (same latency as AppleScript), but native Unix socket + Protobuf for split/send/capture operations (expected 5-20ms). Actual benchmarks pending.

### Success Rate

| Connector | Success Rate | Notes |
|-----------|--------------|-------|
| tmux | 100% | All operations supported |
| it2 CLI | 87.5% | kill_pane not supported |
| iTerm2 (AppleScript) | 75% | Missing capture_output, kill_pane |
| iTerm2 (Native) | 87.5% | kill_pane not supported (iTerm2 API limitation) |

## Architecture Differences

### tmux Architecture

```
┌─────────────────────────────────────────┐
│           Tmux Server                   │
│  ┌─────────┬─────────┬─────────────┐   │
│  │ Session │ Session │   Session   │   │
│  │   :0    │   :1    │     :2      │   │
│  │ ┌────┐  │ ┌────┐  │  ┌────┐     │   │
│  │ │Pane│  │ │Pane│  │  │Pane│     │   │
│  │ │ 0  │  │ │ 0  │  │  │ 0  │     │   │
│  │ └────┘  │ └────┘  │  └────┘     │   │
│  └─────────┴─────────┴─────────────┘   │
└─────────────────────────────────────────┘
         │
         │ tmux CLI (tmux command)
         │ (direct IPC, very fast)
         │
┌─────────────────┐
│ TmuxConnector   │
│ - Fast CLI API  │
│ - Full features │
└─────────────────┘
```

### it2 CLI Architecture (iTerm2 Python API)

```
┌─────────────────────────────────────────┐
│           iTerm2 Application            │
│  ┌─────────────────────────────────┐   │
│  │           Window 0              │   │
│  │  ┌───────────────────────────┐  │   │
│  │  │         Tab 0             │  │   │
│  │  │  ┌──────┬──────────────┐  │  │   │
│  │  │  │Pane 0│    Pane 1    │  │  │   │
│  │  │  └──────┴──────────────┘  │  │   │
│  │  └───────────────────────────┘  │   │
│  └─────────────────────────────────┘   │
└─────────────────────────────────────────┘
         │
         │ iTerm2 Python API (RPC)
         │ (significant overhead)
         │
┌─────────────────┐
│    it2 CLI      │
│ (Python-based)  │
│                 │
└─────────────────┘
         │
         │ Process spawning
         │
┌─────────────────┐
│  It2Connector   │
│ - Python API    │
│ - Slow (RPC)    │
└─────────────────┘
```

### iTerm2 AppleScript Architecture

```
┌─────────────────────────────────────────┐
│           iTerm2 Application            │
│  ┌─────────────────────────────────┐   │
│  │           Window 0              │   │
│  │  ┌───────────────────────────┐  │   │
│  │  │         Tab 0             │  │   │
│  │  │  ┌──────┬──────────────┐  │  │   │
│  │  │  │Pane 0│    Pane 1    │  │  │   │
│  │  │  └──────┴──────────────┘  │  │   │
│  │  └───────────────────────────┘  │   │
│  └─────────────────────────────────┘   │
└─────────────────────────────────────────┘
         │
         │ AppleScript (osascript)
         │ (moderate overhead)
         │
┌─────────────────┐
│ ITermConnector  │
│ - AppleScript   │
│ - Limited API   │
└─────────────────┘
```

### iTerm2 Native Architecture

```
┌─────────────────────────────────────────┐
│           iTerm2 Application            │
│  ┌─────────────────────────────────┐   │
│  │           Window 0              │   │
│  │  ┌───────────────────────────┐  │   │
│  │  │         Tab 0             │  │   │
│  │  │  ┌──────┬──────────────┐  │  │   │
│  │  │  │Pane 0│    Pane 1    │  │  │   │
│  │  │  └──────┴──────────────┘  │  │   │
│  │  └───────────────────────────┘  │   │
│  └─────────────────────────────────┘   │
└─────────────────────────────────────────┘
         │
         │ Unix Domain Socket
         │ Protocol Buffers encoding
         │ (minimal overhead)
         │
┌─────────────────────────┐
│ ITerm2NativeConnector   │
│ - Native Rust client    │
│ - Unix socket + Protobuf│
│ - AppleScript fallback  │
│   (window create/close) │
└─────────────────────────┘
```

**Key Design Notes:**
- iTerm2's native API does NOT support creating windows programmatically
- Window creation and close use AppleScript as fallback
- All other operations (split, send, capture) use native Unix socket
- Authentication via `ITERM2_COOKIE` / `ITERM2_KEY` environment variables

## Recommendations

### When to Use tmux

**Use tmux when you need:**
- Fast, responsive pane operations
- Output capture and verification
- Fine-grained pane management (kill individual panes)
- Cross-platform compatibility
- Server/remote development
- Automated testing
- Complex multi-pane layouts

**tmux is the recommended connector for production use.**

### When to Use it2 CLI

**Use it2 CLI when:**
- You're on macOS and must use iTerm2
- You need capture_output (which AppleScript doesn't support)
- You prefer iTerm2's terminal features
- Performance is not critical

**Note:** it2 CLI is significantly slower than AppleScript due to Python API overhead.

### When to Use iTerm2 (AppleScript)

**Use iTerm2 AppleScript when:**
- You're on macOS and prefer native terminal experience
- You don't need output capture
- Simple pane layouts are sufficient
- You're doing local development only
- You want better performance than it2 CLI

**iTerm2 AppleScript is suitable for basic local development but lacks capture_output.**

### When to Use iTerm2 (Native)

**Use iTerm2 Native when:**
- You're on macOS and prefer iTerm2 terminal
- You need better performance than it2 CLI or AppleScript
- You need capture_output (which AppleScript doesn't support)
- You want near-tmux speeds for most operations
- You're doing local macOS development

**iTerm2 Native is the recommended connector for iTerm2 users.** It provides:
- 10-50x faster operations than it2 CLI
- 2-5x faster than AppleScript for most operations
- Full capture_output support
- Native Rust implementation using Unix socket + Protobuf

**Limitations:**
- Window creation/close still uses AppleScript (iTerm2 API limitation)
- kill_pane not supported (iTerm2 doesn't expose this API)

## Implementation Notes

### tmux Implementation

- Uses `tmux` CLI commands
- Fast execution (direct IPC to tmux server)
- Full feature parity with tmux capabilities
- Session tracking via tmux server

### it2 CLI Implementation

- Uses `it2` Python CLI tool
- Communicates with iTerm2 via Python API (RPC)
- Significant overhead from:
  - Process spawning for each command
  - Python API RPC communication
  - iTerm2 AppleScript bridge internally
- Supports capture_output (unlike AppleScript)
- Cannot kill individual panes

### iTerm2 AppleScript Implementation

- Uses AppleScript via `osascript`
- Moderate execution overhead
- Limited by iTerm2's AppleScript API
- Cannot capture pane content
- Cannot kill individual panes
- Window management is 1-indexed

## Future Improvements

### iTerm2

1. **Implement iTerm2 REST API** - iTerm2 now has a proprietary REST API that could provide:
   - Faster operations
   - Output capture support
   - Better pane management

2. **Add PowerShell scripting** - Alternative to AppleScript for macOS automation

3. **Hybrid approach** - Fall back to tmux when advanced features needed

### General

1. **Add WezTerm connector** - Modern GPU-accelerated terminal with Lua API
2. **Add Windows Terminal connector** - PowerShell-based automation
3. **Add zellij connector** - Rust-based terminal multiplexer

## Running Benchmarks

```bash
# Run the connector benchmark
cargo run --package rustycode-connector --example connector_benchmark

# Run connector comparison tests
cargo test --package rustycode-connector --test connector_comparison
```

## See Also

- [`rustycode-connector` API docs](../crates/rustycode-connector/README.md)
- [tmux documentation](https://man.openbsd.org/tmux.1)
- [iTerm2 AppleScript documentation](https://iterm2.com/applescript.html)
