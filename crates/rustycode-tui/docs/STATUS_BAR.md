# Status Bar Component Documentation

## Overview

The status bar is a comprehensive, mode-aware UI component that provides real-time system information at the bottom of the TUI screen. It adapts its display based on available width, current mode, and application state.

## Architecture

### Core Components

1. **StatusBar**: Main rendering component
2. **StatusBarState**: Complete state information
3. **StatusBarConfig**: Display configuration
4. **StatusBarStateBuilder**: Fluent API for building state

### Information Displayed

The status bar shows the following information (from left to right):

1. **Current Mode**: Mode name with icon (e.g., "💬 Chat")
2. **Connection Status**: Provider connectivity (e.g., "✓ Anthropic")
3. **Token Usage**: Current tokens and cost (e.g., "12.5K ($0.15)")
4. **Agent Status**: Active/queued agents (e.g., "2 active")
5. **Session Info**: Message count and compaction (e.g., "47 msgs")
6. **System Metrics**: Memory usage (optional)
7. **Time**: Current time in HH:MM format
8. **Key Hints**: Context-sensitive keybindings (second line)

## Usage

### Basic Rendering

```rust
use rustycode_tui::ui::StatusBar;
use ratatui::Frame;

let status_bar = StatusBar::default();
let state = build_status_bar_state();

status_bar.render(frame, area, &state);
```

### Building Status State

```rust
use rustycode_tui::ui::{StatusBarStateBuilder, AppMode, ConnectionStatus};

let state = StatusBarStateBuilder::new()
    .mode(AppMode::Chat)
    .connection(ConnectionStatus::Connected {
        provider: "Anthropic".to_string(),
        model: "claude-sonnet-4".to_string(),
    })
    .tokens(Some(token_info))
    .agents(Some(agent_status))
    .session(Some(session_status))
    .hint(KeyHint::new("Ctrl+Q", "Quit"))
    .hint(KeyHint::new("Ctrl+S", "Save"))
    .build();
```

### Integration with Existing State

```rust
// From conversation state
let token_info = TokenInfo::from_monitor(&conversation_state.context_monitor, total_cost);

let state = StatusBarStateBuilder::new()
    .mode(current_mode)
    .connection(get_connection_status())
    .tokens(Some(token_info))
    .agents(get_agent_status())
    .session(get_session_status()))
    .hints(get_contextual_hints(current_mode))
    .build();
```

## Mode-Specific Behavior

### Chat Mode
- Shows token usage prominently
- Displays agent activity
- Shows message count
- Hints: `Ctrl+Q: Quit`, `Ctrl+S: Save`, `Ctrl+N: New Chat`

### Config Mode
- Shows validation status
- Displays current config file
- Hints: `Ctrl+E: Edit`, `Ctrl+R: Reload`, `Esc: Back`

### Learning Mode
- Shows pattern count
- Displays learning progress
- Hints: `Ctrl+P: Patterns`, `Ctrl+T: Train`, `Esc: Back`

### Provider Mode
- Shows active provider
- Displays cost summary
- Hints: `Ctrl+C: Configure`, `Ctrl+T: Test`, `Esc: Back`

### Agent Mode
- Shows running/queued agents
- Displays queue size
- Hints: `Ctrl+A: Add Agent`, `Ctrl+K: Kill`, `Esc: Back`

### Session Mode
- Shows message count
- Displays compaction status
- Hints: `Ctrl+S: Save`, `Ctrl+C: Compact`, `Ctrl+L: Load`, `Esc: Back`

### MCP Mode
- Shows connected servers
- Displays available tool count
- Hints: `Ctrl+N: New Server`, `Ctrl+D: Disconnect`, `Esc: Back`

### Performance Mode
- Shows current metric
- Displays metric value
- Hints: `Ctrl+M: Switch Metric`, `Ctrl+R: Reset`, `Esc: Back`

## Responsive Design

The status bar adapts to different screen widths:

### Width >= 100px (Full Display)
```
[💬 Chat] │ Anthropic ✓ │ Tokens: 12.5K ($0.15) │ Agents: 2 active │ 47 msgs │ 750MB │ 14:32
[Ctrl+Q: Quit  Ctrl+S: Save  Ctrl+N: New Chat]
```

### Width 60-99px (Reduced Display)
```
[💬 Chat] │ Anthropic ✓ │ Tokens: 12.5K │ Agents: 2 │ 14:32
```

### Width 40-59px (Minimal Display)
```
[💬 Chat] │ Anthropic ✓ │ Tokens: 12.5K
```

### Width < 40px (Critical Display)
```
[💬 Chat] │ ✓
```

## Configuration

### Custom Configuration

```rust
use rustycode_tui::ui::{StatusBar, StatusBarConfig, WidthThresholds};

let config = StatusBarConfig {
    show_mode: true,
    show_connection: true,
    show_tokens: true,
    show_agents: true,
    show_session: true,
    show_metrics: false,
    show_time: true,
    show_hints: true,
    min_full_width: 100,
    width_thresholds: WidthThresholds {
        hide_hints: 80,
        hide_time: 60,
        hide_metrics: 50,
        hide_session: 40,
        hide_agents: 30,
    },
};

let status_bar = StatusBar::new(config);
```

### Disabling Hints

```rust
let config = StatusBarConfig {
    show_hints: false,
    ..StatusBarConfig::default()
};
```

## API Reference

### StatusBar

Main status bar component.

#### Methods

- `new(config: StatusBarConfig) -> Self`: Create with custom config
- `default() -> Self`: Create with default config
- `render(&self, f: &mut Frame, area: Rect, state: &StatusBarState)`: Render the status bar
- `height(&self, state: &StatusBarState) -> u16`: Get total height needed (1 or 2 lines)

### StatusBarState

Complete status bar state.

#### Fields

- `mode: AppMode`: Current application mode
- `connection: ConnectionStatus`: Provider connection status
- `tokens: Option<TokenInfo>`: Token usage information
- `agents: Option<AgentStatus>`: Agent activity status
- `session: Option<SessionStatus>`: Session information
- `metrics: Option<SystemMetrics>`: System performance metrics
- `hints: Vec<KeyHint>`: Keybinding hints
- `mode_data: Option<ModeData>`: Mode-specific data

### StatusBarStateBuilder

Fluent builder for creating status bar state.

#### Methods

- `new() -> Self`: Create new builder
- `mode(mode: AppMode) -> Self`: Set mode
- `connection(connection: ConnectionStatus) -> Self`: Set connection status
- `tokens(tokens: Option<TokenInfo>) -> Self`: Set token info
- `agents(agents: Option<AgentStatus>) -> Self`: Set agent status
- `session(session: Option<SessionStatus>) -> Self`: Set session status
- `metrics(metrics: Option<SystemMetrics>) -> Self`: Set metrics
- `hint(hint: KeyHint) -> Self`: Add a hint
- `hints(hints: Vec<KeyHint>) -> Self`: Set all hints
- `mode_data(data: Option<ModeData>) -> Self`: Set mode-specific data
- `build() -> StatusBarState`: Build the state

## State Sources

### Connection Status

```rust
pub enum ConnectionStatus {
    Connected { provider: String, model: String },
    Connecting,
    Disconnected,
    Error { message: String },
}
```

### Token Information

```rust
pub struct TokenInfo {
    pub current_tokens: usize,
    pub max_tokens: usize,
    pub usage_percentage: f64,
    pub cost_usd: f64,
}
```

### Agent Status

```rust
pub struct AgentStatus {
    pub active_count: usize,
    pub queued_count: usize,
    pub total_count: usize,
}
```

### Session Status

```rust
pub struct SessionStatus {
    pub message_count: usize,
    pub compaction_ratio: Option<f64>,
    pub title: Option<String>,
}
```

### System Metrics

```rust
pub struct SystemMetrics {
    pub memory_mb: f64,
    pub cpu_percent: f64,
    pub frame_time_ms: f64,
}
```

## Color Coding

The status bar uses color coding for quick visual feedback:

- **Green**: Good/Connected (e.g., "✓ Anthropic")
- **Yellow**: Warning/Connecting (e.g., token usage > 50%)
- **Red**: Error/High usage (e.g., token usage > 80%)
- **Cyan**: Chat mode
- **Magenta**: Learning mode
- **Blue**: Agent mode
- **Gray**: Idle/disconnected states

## Performance Considerations

1. **Efficient Rendering**: Only re-renders when state changes
2. **Width-Aware**: Hides less critical elements on narrow screens
3. **No Heavy Computation**: All formatting is pre-computed
4. **Lazy Evaluation**: Optional data is only processed when present

## Testing

The status bar includes comprehensive tests:

```bash
# Run all status bar tests
cargo test --package rustycode-tui --lib ui::status_bar

# Run specific test
cargo test --package rustycode-tui --lib test_app_mode_display
```

## Future Enhancements

Potential improvements for the status bar:

1. **Customizable Layout**: Allow users to choose element order
2. **More Metrics**: Add network usage, disk I/O, etc.
3. **Clickable Elements**: Make status bar elements interactive
4. **Progress Bars**: Visual progress for long operations
5. **Notifications**: Toast-style notifications overlay
6. **Multi-line Hints**: Support more hints by wrapping

## Example Integration

See the event loop integration in `app/event_loop_render.rs` for a complete example of how to integrate the status bar into the main TUI rendering loop.

## Troubleshooting

### Status Bar Not Showing

Check that:
1. Status bar is being rendered in the main loop
2. Area is calculated correctly (bottom of screen)
3. State is being updated properly

### Elements Not Appearing

Check that:
1. Width thresholds are appropriate for screen size
2. Elements are enabled in config
3. Data is provided in state (use `Some()` for optional fields)

### Colors Not Showing

Check that:
1. Terminal supports color
2. Theme is configured correctly
3. Color values are valid

## License

MIT License - See LICENSE file for details
