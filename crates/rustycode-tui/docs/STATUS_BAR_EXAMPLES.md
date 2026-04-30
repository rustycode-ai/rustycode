# Status Bar Visual Examples

This document shows visual examples of how the status bar appears in different scenarios.

## Screen Layout

```
┌─────────────────────────────────────────────────────────────┐
│                                                             │
│                    Main Content Area                         │
│                     (Messages, etc.)                         │
│                                                             │
│                                                             │
│                                                             │
└─────────────────────────────────────────────────────────────┘
┌─────────────────────────────────────────────────────────────┐
│ [💬 Chat] │ Anthropic ✓ │ Tokens: 12.5K ($0.15) │ 14:32    │  ← Status Bar
└─────────────────────────────────────────────────────────────┘
```

## Example 1: Chat Mode (Full Width)

```
┌──────────────────────────────────────────────────────────────────────────┐
│ [💬 Chat] │ Anthropic ✓ │ Tokens: 12.5K ($0.15) │ Agents: 2 active │ 47 msgs │ 14:32 │
├──────────────────────────────────────────────────────────────────────────┤
│ [Ctrl+Q: Quit  Ctrl+S: Save  Ctrl+N: New Chat]                          │
└──────────────────────────────────────────────────────────────────────────┘
```

**Status:**
- Mode: Chat (💬)
- Connection: Connected to Anthropic (✓)
- Tokens: 12.5K used, $0.15 cost (green, healthy)
- Agents: 2 active
- Messages: 47 in session
- Time: 14:32

**Hints:**
- Ctrl+Q: Quit application
- Ctrl+S: Save session
- Ctrl+N: New chat

## Example 2: Config Mode (Full Width)

```
┌──────────────────────────────────────────────────────────────────────────┐
│ [⚙️ Config] │ rustycode.json ✓ │ Validation: OK │ 2 files loaded │ 14:33 │
├──────────────────────────────────────────────────────────────────────────┤
│ [Ctrl+E: Edit  Ctrl+R: Reload  Esc: Back]                                │
└──────────────────────────────────────────────────────────────────────────┘
```

**Status:**
- Mode: Config (⚙️)
- File: rustycode.json (✓ valid)
- Validation: OK
- Files: 2 config files loaded
- Time: 14:33

**Hints:**
- Ctrl+E: Edit config
- Ctrl+R: Reload config
- Esc: Back to chat

## Example 3: Learning Mode (High Token Usage)

```
┌──────────────────────────────────────────────────────────────────────────┐
│ [🧠 Learning] │ Anthropic ✓ │ Tokens: 85.2K ($1.25) │ 42 patterns │ 75% │
├──────────────────────────────────────────────────────────────────────────┤
│ [Ctrl+P: Patterns  Ctrl+T: Train  Esc: Back]                             │
└──────────────────────────────────────────────────────────────────────────┘
```

**Status:**
- Mode: Learning (🧠)
- Connection: Connected to Anthropic (✓)
- Tokens: 85.2K used (⚠ red, high usage)
- Cost: $1.25
- Patterns: 42 learned
- Progress: 75% trained

**Warning:** Token usage at 85% (red color, needs compaction soon)

## Example 4: Agent Mode (With Queue)

```
┌──────────────────────────────────────────────────────────────────────────┐
│ [🤖 Agent] │ Anthropic ✓ │ Agents: 3 active, 5 queued │ 2 running │ 14:35 │
├──────────────────────────────────────────────────────────────────────────┤
│ [Ctrl+A: Add Agent  Ctrl+K: Kill  Esc: Back]                             │
└──────────────────────────────────────────────────────────────────────────┘
```

**Status:**
- Mode: Agent (🤖)
- Connection: Connected to Anthropic (✓)
- Agents: 3 active, 5 in queue
- Running: 2 currently executing
- Time: 14:35

**Hints:**
- Ctrl+A: Add new agent
- Ctrl+K: Kill agent
- Esc: Back to chat

## Example 5: Provider Mode (Cost Summary)

```
┌──────────────────────────────────────────────────────────────────────────┐
│ [🔌 Provider] │ OpenAI │ Model: gpt-4 │ Total: $2.50 │ Requests: 15 │ 14:36│
├──────────────────────────────────────────────────────────────────────────┤
│ [Ctrl+C: Configure  Ctrl+T: Test  Esc: Back]                             │
└──────────────────────────────────────────────────────────────────────────┘
```

**Status:**
- Mode: Provider (🔌)
- Provider: OpenAI
- Model: gpt-4
- Total Cost: $2.50 this session
- Requests: 15 made
- Time: 14:36

## Example 6: Session Mode (Compaction Needed)

```
┌──────────────────────────────────────────────────────────────────────────┐
│ [📋 Session] │ Anthropic ✓ │ Messages: 127 │ ⚠ 85% full │ Compact needed │ 14:37│
├──────────────────────────────────────────────────────────────────────────┤
│ [Ctrl+S: Save  Ctrl+C: Compact  Ctrl+L: Load  Esc: Back]                 │
└──────────────────────────────────────────────────────────────────────────┘
```

**Status:**
- Mode: Session (📋)
- Connection: Connected to Anthropic (✓)
- Messages: 127 in session
- ⚠ Warning: 85% of token limit
- Action: Compaction recommended

## Example 7: MCP Mode

```
┌──────────────────────────────────────────────────────────────────────────┐
│ [🔗 MCP] │ 3 servers │ 12 tools │ Connected │ 14:38                     │
├──────────────────────────────────────────────────────────────────────────┤
│ [Ctrl+N: New Server  Ctrl+D: Disconnect  Esc: Back]                      │
└──────────────────────────────────────────────────────────────────────────┘
```

**Status:**
- Mode: MCP (🔗)
- Servers: 3 connected
- Tools: 12 available
- Status: Connected
- Time: 14:38

## Example 8: Performance Mode

```
┌──────────────────────────────────────────────────────────────────────────┐
│ [📊 Performance] │ Memory: 750MB │ CPU: 45% │ Frame: 16ms │ 60 FPS │ 14:39│
├──────────────────────────────────────────────────────────────────────────┤
│ [Ctrl+M: Switch Metric  Ctrl+R: Reset  Esc: Back]                        │
└──────────────────────────────────────────────────────────────────────────┘
```

**Status:**
- Mode: Performance (📊)
- Memory: 750MB (⚠ yellow, moderate)
- CPU: 45% usage
- Frame Time: 16ms
- FPS: 60
- Time: 14:39

## Example 9: Narrow Screen (80 chars)

```
┌────────────────────────────────────────────────────────────────────────┐
│ [💬 Chat] │ Anthropic ✓ │ Tokens: 12.5K │ Agents: 2 │ 14:32           │
└────────────────────────────────────────────────────────────────────────┘
```

**Adaptations:**
- Hints hidden (no second line)
- Cost hidden
- Session info hidden
- Time shown

## Example 10: Very Narrow Screen (50 chars)

```
┌────────────────────────────────────────────────┐
│ [💬 Chat] │ Anthropic ✓ │ Tokens: 12.5K        │
└────────────────────────────────────────────────┘
```

**Adaptations:**
- Only critical info shown
- Agents, session, time hidden
- Mode and connection always visible

## Example 11: Connecting State

```
┌──────────────────────────────────────────────────────────────────────────┐
│ [💬 Chat] │ ⏳ Connecting... │ Tokens: 0 ($0.00) │ Ready │ 14:40         │
└──────────────────────────────────────────────────────────────────────────┘
```

**Status:**
- Connection: ⏳ Connecting (yellow)
- Tokens: 0 (new session)
- Status: Ready

## Example 12: Error State

```
┌──────────────────────────────────────────────────────────────────────────┐
│ [💬 Chat] │ ⚠ Error: Connection failed │ Retry │ Check settings │ 14:41 │
└──────────────────────────────────────────────────────────────────────────┘
```

**Status:**
- Connection: ⚠ Error (red)
- Error: Connection failed
- Suggestions shown

## Color Legend

- **Green**: Good/Healthy (token usage < 50%, connected, low memory)
- **Yellow**: Warning (token usage 50-80%, connecting, moderate memory)
- **Red**: Error/Critical (token usage > 80%, connection failed, high memory)
- **Cyan**: Chat mode
- **Magenta**: Learning mode
- **Blue**: Agent mode
- **White**: Performance mode
- **Gray**: Idle/disconnected states

## Responsive Behavior

### Width ≥ 100 chars
```
[Mode] │ Connection │ Tokens │ Agents │ Session │ Time
[Hints line]
```

### Width 60-99 chars
```
[Mode] │ Connection │ Tokens │ Agents │ Time
```

### Width 40-59 chars
```
[Mode] │ Connection │ Tokens
```

### Width < 40 chars
```
[Mode] │ Connection icon
```

## Status Update Flow

```
Initial State:
[💬 Chat] │ Disconnected │ Tokens: 0 ($0.00) │ Ready

User sends message:
[💬 Chat] │ ⏳ Thinking │ Tokens: 2.5K ($0.03) │ Processing

AI responding:
[💬 Chat] │ ✓ Connected │ Tokens: 8.2K ($0.10) │ Streaming

Response complete:
[💬 Chat] │ ✓ Connected │ Tokens: 12.5K ($0.15) │ Ready │ 47 msgs

Mode change:
[⚙️ Config] │ rustycode.json ✓ │ Validation: OK
```

## Summary

The status bar provides:
- **At-a-glance information**: Mode, connection, tokens, activity
- **Visual feedback**: Color coding for quick status assessment
- **Context-aware hints**: Relevant keybindings for current mode
- **Responsive design**: Adapts to screen width automatically
- **Real-time updates**: Current time, token usage, agent activity

All examples demonstrate the status bar's ability to provide comprehensive, mode-aware system information in a clean, unobtrusive format.
