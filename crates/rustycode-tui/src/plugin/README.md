# RustyCode Plugin System

## Overview

The RustyCode plugin system makes the TUI extensible and hackable. Plugins can add slash commands, themes, hooks, and custom functionality.

## Architecture

```
~/.rustycode/plugins/
├── timer/
│   ├── plugin.toml          # Plugin manifest
│   └── libtimer.so          # Compiled plugin
├── notes/
│   ├── plugin.toml
│   └── libnotes.so
└── myplugin/
    ├── plugin.toml
    └── libmyplugin.so
```

## Features

### Plugin Types

1. **Command Plugins**: Add slash commands
2. **Theme Plugins**: Provide color schemes
3. **Hook Plugins**: Respond to events
4. **Hybrid Plugins**: Multiple types combined

### Permission System

- Fine-grained permissions for security
- User confirmation on first use
- Permission audit log

### Plugin Manager UI

- List all installed plugins
- Enable/disable plugins
- View plugin details
- Manage permissions

## Slash Commands

### Built-in Plugin Commands

- `/plugin list` - Show all plugins
- `/plugin enable <name>` - Enable plugin
- `/plugin disable <name>` - Disable plugin
- `/plugin info <name>` - Show plugin details

### Example Plugin Commands

Timer plugin:
- `/pomodoro` - Start 25-minute Pomodoro timer
- `/break` - Start 5-minute break
- `/timer <minutes>` - Start custom timer

Notes plugin:
- `/note <text>` - Save a note
- `/notes` - Show all notes
- `/clear_notes` - Clear all notes

## Plugin API

### Core API

```rust
pub struct PluginAPI {
    pub config: PluginConfig,
    pub ui: PluginUI,
    pub commands: PluginCommands,
    pub context: PluginContext,
}
```

### Available Methods

- `show_message(&str)` - Show message to user
- `get_input() -> String` - Get current input
- `set_input(&str)` - Set input text
- `get_config(&str) -> Option<String>` - Get config value
- `set_config(String, String)` - Set config value
- `context.get_workspace() -> String` - Get workspace context
- `context.get_cwd() -> PathBuf` - Get current directory
- `context.get_history() -> Vec<String>` - Get conversation history

## Plugin Manifest

```toml
name = "myplugin"
version = "0.1.0"
description = "My awesome plugin"
author = "Your Name"
permissions = ["read_file", "write_file"]
entry_point = "libmyplugin.so"

[[slash_commands]]
name = "mycommand"
description = "My custom command"
handler = "my_handler"
```

## Permissions

### Available Permissions

- `read_file` - Read files
- `write_file` - Write files
- `execute_command` - Execute shell commands
- `network_request` - Make network requests
- `notification` - Show notifications
- `clipboard` - Access clipboard
- `workspace_context` - Access workspace
- `ui_control` - Control UI
- `conversation_history` - Access history

## Example Plugins

### Timer Plugin

Provides Pomodoro timer and break reminders.

### Notes Plugin

Quick note-taking and retrieval.

### Theme Plugin

Custom color schemes for the TUI.

## Development

See [PLUGIN_DEVELOPMENT.md](/PLUGIN_DEVELOPMENT.md) for complete plugin development guide.

## Security

### Sandboxing

Plugins run in a controlled environment with:
- Permission checks before operations
- Filesystem access restricted to granted paths
- Network access controlled by permissions
- Command execution requires explicit permission

### Best Practices

1. Only grant necessary permissions
2. Review plugin code before installation
3. Keep plugins updated
4. Monitor plugin behavior in audit log

## Future Enhancements

- WASM plugin support for cross-platform plugins
- Plugin marketplace
- Hot-reload for development
- Plugin dependencies
- Version compatibility checking
- Plugin signing and verification

## Contributing

Contributions welcome! See:
- [PLUGIN_DEVELOPMENT.md](/PLUGIN_DEVELOPMENT.md)
- Source code: `crates/rustycode-tui/src/plugin/`
- Example plugins: `/tmp/rustycode-plugins/`

## License

MIT License - see main project LICENSE file.
