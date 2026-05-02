//! Help topics for the TUI help system
//!
//! Comprehensive documentation of all features, shortcuts, and commands.

use super::{HelpCategory, HelpTopic};

/// Get all help topics
pub fn get_all_topics() -> Vec<HelpTopic> {
    vec![
        // Navigation topics
        HelpTopic {
            title: "Scroll messages".to_string(),
            category: HelpCategory::Navigation,
            content: "Scroll up and down through the message history. Use Page Up/Down for full-viewport scrolling.".to_string(),
            key_bindings: vec!["↑/↓".to_string(), "PgUp/PgDn".to_string()],
        },
        HelpTopic {
            title: "Navigate to top/bottom".to_string(),
            category: HelpCategory::Navigation,
            content: "Jump to the first or last message. Vim keys j/k work when input is empty, gg jumps to top, G to bottom. Shift+Up/Down jumps between user message boundaries (turn navigation). Ctrl+Shift+Z jumps back to previous scroll position.".to_string(),
            key_bindings: vec!["Home".to_string(), "End".to_string(), "j/k".to_string(), "g/G".to_string(), "Shift+Up/Down".to_string(), "Ctrl+Shift+Z".to_string()],
        },
        HelpTopic {
            title: "Toggle message collapse".to_string(),
            category: HelpCategory::Navigation,
            content: "Press Space with empty input to toggle collapse on the selected message. Tab cycles through tool expansion, thinking display, and collapsed. Click on any message to toggle its collapse state.".to_string(),
            key_bindings: vec!["Space".to_string(), "Tab".to_string(), "Mouse click".to_string()],
        },
        HelpTopic {
            title: "Expand/collapse all".to_string(),
            category: HelpCategory::Navigation,
            content: "Expand all messages or collapse all except user messages to reduce visual noise.".to_string(),
            key_bindings: vec!["Alt+E".to_string(), "Alt+W".to_string()],
        },
        HelpTopic {
            title: "Expand/collapse tool blocks".to_string(),
            category: HelpCategory::Navigation,
            content: "Expand or collapse all tool execution blocks across all messages at once.".to_string(),
            key_bindings: vec!["Alt+Shift+E".to_string(), "Alt+Shift+W".to_string()],
        },
        HelpTopic {
            title: "Search messages".to_string(),
            category: HelpCategory::Navigation,
            content: "Toggle search mode to find text across all messages. Use arrow keys to navigate results.".to_string(),
            key_bindings: vec!["Ctrl+F".to_string()],
        },
        HelpTopic {
            title: "Session sidebar".to_string(),
            category: HelpCategory::Navigation,
            content: "Toggle the session sidebar to browse and switch between conversation sessions. Use Ctrl+Shift+N/P to navigate sessions directly.".to_string(),
            key_bindings: vec!["Ctrl+B".to_string(), "Ctrl+Shift+N/P".to_string()],
        },
        HelpTopic {
            title: "File finder".to_string(),
            category: HelpCategory::Navigation,
            content: "Toggle the fuzzy file finder overlay to search for files by name in your workspace.".to_string(),
            key_bindings: vec!["Ctrl+O".to_string()],
        },
        HelpTopic {
            title: "Exit TUI".to_string(),
            category: HelpCategory::Navigation,
            content: "Quit the TUI and return to the terminal. Ctrl+Z suspends the process (resume with `fg`). Ctrl+C no longer quits — use Ctrl+D or Ctrl+Q instead.".to_string(),
            key_bindings: vec!["Ctrl+D".to_string(), "Ctrl+Q".to_string(), "Ctrl+Z (suspend)".to_string()],
        },

        // Editing topics
        HelpTopic {
            title: "Send message".to_string(),
            category: HelpCategory::Editing,
            content: "Send the current input to the AI assistant.".to_string(),
            key_bindings: vec!["Enter".to_string()],
        },
        HelpTopic {
            title: "Multi-line input".to_string(),
            category: HelpCategory::Editing,
            content: "Toggle between single-line and multi-line input modes. In multi-line mode, Enter adds new lines, Alt+Enter sends the message.".to_string(),
            key_bindings: vec!["Alt+Enter".to_string()],
        },
        HelpTopic {
            title: "Clear input".to_string(),
            category: HelpCategory::Editing,
            content: "Clear the current input field by double-tapping Escape quickly, or use Ctrl+L to clear the current line. When input is empty, Ctrl+U scrolls half-page up (Vim) and Ctrl+L forces a screen redraw.".to_string(),
            key_bindings: vec!["Double Esc".to_string(), "Ctrl+L".to_string(), "Ctrl+U (scroll when empty)".to_string()],
        },
        HelpTopic {
            title: "Stash input".to_string(),
            category: HelpCategory::Editing,
            content: "Temporarily save and clear the current input. Press Ctrl+S again to restore the stashed input.".to_string(),
            key_bindings: vec!["Ctrl+S".to_string()],
        },
        HelpTopic {
            title: "Input navigation".to_string(),
            category: HelpCategory::Editing,
            content: "Readline-style navigation: Ctrl+A (beginning of line), Ctrl+E (end of line), Ctrl+W (delete word backward), Ctrl+K (kill to end of line).".to_string(),
            key_bindings: vec!["Ctrl+A/E".to_string(), "Ctrl+W/K".to_string()],
        },
        HelpTopic {
            title: "Input history".to_string(),
            category: HelpCategory::Editing,
            content: "Navigate command history with Up/Down arrows. Use Ctrl+R for reverse search through history (type to filter, Ctrl+R to cycle matches).".to_string(),
            key_bindings: vec!["↑/↓".to_string(), "Ctrl+R".to_string()],
        },
        HelpTopic {
            title: "Paste from clipboard".to_string(),
            category: HelpCategory::Editing,
            content: "Use Ctrl+V for clipboard paste, or terminal-native paste: Ctrl+Shift+V (Linux/Windows) or Cmd+V (macOS). Bracketed paste is supported for multi-line content.".to_string(),
            key_bindings: vec!["Ctrl+V".to_string(), "Ctrl+Shift+V".to_string()],
        },
        HelpTopic {
            title: "External editor".to_string(),
            category: HelpCategory::Editing,
            content: "Open the current input in $EDITOR (defaults to nano). Edit and save to load the result back into the input field. Press Enter to send.".to_string(),
            key_bindings: vec!["Ctrl+X".to_string()],
        },

        // Tools topics
        HelpTopic {
            title: "Tool panel".to_string(),
            category: HelpCategory::Tools,
            content: "Toggle the tool execution panel to see all tool calls with status, duration, and results. Press Enter on a tool to see its output. Press F to toggle between truncated and full output view. Press Ctrl+C on a running tool to cancel it.".to_string(),
            key_bindings: vec!["Ctrl+P".to_string(), "Enter".to_string(), "F".to_string(), "Ctrl+C cancel".to_string(), "Esc".to_string()],
        },
        HelpTopic {
            title: "Team panel".to_string(),
            category: HelpCategory::Tools,
            content: "Toggle the team agent timeline panel to see orchestrated agent activity, current turn, and trust values.".to_string(),
            key_bindings: vec!["Ctrl+G".to_string()],
        },
        HelpTopic {
            title: "Worker panel".to_string(),
            category: HelpCategory::Tools,
            content: "Toggle the worker status panel to see spawned sub-agents and their states (spawning, running, finished, failed). Only toggles when input is empty.".to_string(),
            key_bindings: vec!["Ctrl+W (empty input)".to_string()],
        },
        HelpTopic {
            title: "Tool execution".to_string(),
            category: HelpCategory::Tools,
            content: "The AI can execute tools like reading files, running commands, and editing code. In brutalist mode, tools appear inline with messages.".to_string(),
            key_bindings: vec!["Automatic".to_string()],
        },
        HelpTopic {
            title: "Tool approval".to_string(),
            category: HelpCategory::Tools,
            content: "Some tools require approval before execution. Press Y to approve, N to reject, or A to always approve that tool.".to_string(),
            key_bindings: vec!["Y/N/A".to_string()],
        },
        HelpTopic {
            title: "Copy message".to_string(),
            category: HelpCategory::Tools,
            content: "Copy the selected message, the last AI response, or the entire conversation to clipboard.".to_string(),
            key_bindings: vec!["Ctrl+Shift+C".to_string(), "Ctrl+Y".to_string(), "Ctrl+Shift+K".to_string()],
        },
        HelpTopic {
            title: "Regenerate response".to_string(),
            category: HelpCategory::Tools,
            content: "Regenerate the last AI response with a fresh completion. The original user prompt is re-sent.".to_string(),
            key_bindings: vec!["Ctrl+Shift+R".to_string(), "/r".to_string()],
        },
        HelpTopic {
            title: "Export conversation".to_string(),
            category: HelpCategory::Tools,
            content: "Export the current conversation to a markdown file in ~/.rustycode/exports/. Also available as /export command.".to_string(),
            key_bindings: vec!["Ctrl+Shift+E".to_string(), "/export".to_string()],
        },
        HelpTopic {
            title: "Undo file changes".to_string(),
            category: HelpCategory::Tools,
            content: "Undo the last file write operation by restoring the previous file contents. Works for write_file, edit_file, and apply_patch tools.".to_string(),
            key_bindings: vec!["/undo".to_string()],
        },
        HelpTopic {
            title: "View changes".to_string(),
            category: HelpCategory::Tools,
            content: "Show git diff of uncommitted changes in the workspace.".to_string(),
            key_bindings: vec!["/diff".to_string()],
        },
        HelpTopic {
            title: "Cancel generation".to_string(),
            category: HelpCategory::Tools,
            content: "Cancel the current AI generation or tool execution by pressing Esc or Ctrl+C. Content received so far is preserved. When not streaming, Ctrl+C dismisses overlays. Use Ctrl+D or Ctrl+Q to quit.".to_string(),
            key_bindings: vec!["Esc".to_string(), "Ctrl+C".to_string()],
        },
        HelpTopic {
            title: "Undo task extraction".to_string(),
            category: HelpCategory::Tools,
            content: "Undo the last automatic task/todo extraction that was performed on an AI response.".to_string(),
            key_bindings: vec!["Ctrl+Shift+U".to_string()],
        },

        // Commands topics
        HelpTopic {
            title: "Slash commands".to_string(),
            category: HelpCategory::Commands,
            content: "Type '/' or press Ctrl+K to access slash commands like /help, /save, /load, /theme, /stats, /track (compact by default, /track full for details), /compact, and more.".to_string(),
            key_bindings: vec!["/".to_string(), "Ctrl+K".to_string()],
        },
        HelpTopic {
            title: "Bash commands".to_string(),
            category: HelpCategory::Commands,
            content: "Execute shell commands directly by prefixing with '!'. Output is displayed as system messages. Example: !ls, !pwd, !git status.".to_string(),
            key_bindings: vec!["!<command>".to_string()],
        },
        HelpTopic {
            title: "Help".to_string(),
            category: HelpCategory::Commands,
            content: "Show this help screen with all available commands and shortcuts.".to_string(),
            key_bindings: vec!["?".to_string(), "/help".to_string()],
        },
        HelpTopic {
            title: "Save conversation".to_string(),
            category: HelpCategory::Commands,
            content: "Save the current conversation to a file.".to_string(),
            key_bindings: vec!["/save".to_string()],
        },
        HelpTopic {
            title: "Load conversation".to_string(),
            category: HelpCategory::Commands,
            content: "Load a previously saved conversation.".to_string(),
            key_bindings: vec!["/load".to_string()],
        },
        HelpTopic {
            title: "Change theme".to_string(),
            category: HelpCategory::Commands,
            content: "Switch between different color themes using Alt+T to cycle forward or Alt+Shift+T backward. Ctrl+T toggles theme preview.".to_string(),
            key_bindings: vec!["Alt+T".to_string(), "Alt+Shift+T".to_string(), "Ctrl+T".to_string()],
        },
        HelpTopic {
            title: "Model selection".to_string(),
            category: HelpCategory::Commands,
            content: "Open the model selector to choose which AI model to use, or use /model <number> to switch directly.".to_string(),
            key_bindings: vec!["Alt+P".to_string(), "/model".to_string()],
        },
        HelpTopic {
            title: "Provider selection".to_string(),
            category: HelpCategory::Commands,
            content: "Select which AI provider to use (Anthropic, OpenAI, Ollama, etc.). Use /provider to list available providers.".to_string(),
            key_bindings: vec!["/provider".to_string()],
        },
        HelpTopic {
            title: "Plugin management".to_string(),
            category: HelpCategory::Commands,
            content: "Inspect and manage installed plugins with /plugin list, /plugin install <source>, /plugin update [name|all], /plugin uninstall <name>, /plugin info <name>, /plugin enable <name>, and /plugin disable <name>.".to_string(),
            key_bindings: vec!["/plugin".to_string()],
        },
        HelpTopic {
            title: "Memory commands".to_string(),
            category: HelpCategory::Commands,
            content: "Manage automatic memory/context injection. Use /memory to show available memories.".to_string(),
            key_bindings: vec!["/memory".to_string()],
        },
        HelpTopic {
            title: "Skill palette".to_string(),
            category: HelpCategory::Commands,
            content: "Toggle the skill palette to browse and use available skills.".to_string(),
            key_bindings: vec!["Ctrl+Shift+S".to_string()],
        },
        HelpTopic {
            title: "Task commands".to_string(),
            category: HelpCategory::Commands,
            content: "Manage workspace tasks. Commands: /task list (show all), /task create <description> (new task), /task complete <number> (mark done), /task start <number> (start working), /task delete <number> (remove task).".to_string(),
            key_bindings: vec!["/task".to_string()],
        },
        HelpTopic {
            title: "Todo commands".to_string(),
            category: HelpCategory::Commands,
            content: "Manage todo items. Commands: /todo list (show all), /todo add <text> (new item), /todo done <number> (mark complete), /todo uncheck <number> (mark incomplete), /todo delete <number> (remove item).".to_string(),
            key_bindings: vec!["/todo".to_string()],
        },
        HelpTopic {
            title: "Worker status".to_string(),
            category: HelpCategory::Commands,
            content: "View status of spawned agents (workers). Workers are automatically tracked when using spawn_agent tool. Commands: /workers list (show all workers by status), /workers help (usage docs). Status: 🔄 Spawning, ⚙️ Running, ✅ Finished, ❌ Failed.".to_string(),
            key_bindings: vec!["/workers".to_string()],
        },
        HelpTopic {
            title: "Scheduled tasks".to_string(),
            category: HelpCategory::Commands,
            content: "Manage cron-based scheduled autonomous tasks. Commands: /cron list (show all scheduled tasks), /cron help (usage docs). Schedule format: 5-field cron expression (e.g., \"0 9 * * *\" = daily at 9am).".to_string(),
            key_bindings: vec!["/cron".to_string()],
        },
        HelpTopic {
            title: "Session cost".to_string(),
            category: HelpCategory::Commands,
            content: "Show session token usage and estimated cost. Displays input/output token counts, context usage percentage, and accumulated cost.".to_string(),
            key_bindings: vec!["/cost".to_string(), "/usage".to_string()],
        },
        HelpTopic {
            title: "LSP servers".to_string(),
            category: HelpCategory::Commands,
            content: "Check LSP server availability and status. Use /lsp status to see which language servers (rust-analyzer, pyright, gopls, etc.) are installed.".to_string(),
            key_bindings: vec!["/lsp status".to_string()],
        },

        // Settings topics
        HelpTopic {
            title: "Auto-continue mode".to_string(),
            category: HelpCategory::Settings,
            content: "Toggle auto-continue mode to have the AI keep working on pending tasks automatically. Shows remaining tasks/todos in status bar.".to_string(),
            key_bindings: vec!["Ctrl+Shift+A".to_string()],
        },
        HelpTopic {
            title: "Agent mode".to_string(),
            category: HelpCategory::Settings,
            content: "Cycle through agent modes (e.g., default, plan, code). Ctrl+M cycles forward, Ctrl+Shift+M cycles backward. Current mode shown in status bar.".to_string(),
            key_bindings: vec!["Ctrl+M".to_string(), "Ctrl+Shift+M".to_string()],
        },
        HelpTopic {
            title: "Toggle UI sections".to_string(),
            category: HelpCategory::Settings,
            content: "Toggle visibility of UI sections like status bar and footer for distraction-free mode.".to_string(),
            key_bindings: vec!["Ctrl+Shift+H".to_string()],
        },
        HelpTopic {
            title: "Brutalist mode".to_string(),
            category: HelpCategory::Settings,
            content: "Toggle between classic and brutalist UI styles. Brutalist mode features asymmetric borders, compact layout, inline tools, and lowercase typography.".to_string(),
            key_bindings: vec!["Alt+B".to_string()],
        },
        HelpTopic {
            title: "Token usage".to_string(),
            category: HelpCategory::Settings,
            content: "Current token usage is shown in the status bar. Green: <50%, Yellow: 50-80%, Red: >80%.".to_string(),
            key_bindings: vec!["Auto".to_string()],
        },
        HelpTopic {
            title: "Context window".to_string(),
            category: HelpCategory::Settings,
            content: "The AI has a limited context window. Old messages are automatically compacted when approaching the limit.".to_string(),
            key_bindings: vec!["Auto".to_string()],
        },
        HelpTopic {
            title: "Streaming responses".to_string(),
            category: HelpCategory::Settings,
            content: "AI responses stream in real-time as they are generated. You will see text appear character by character.".to_string(),
            key_bindings: vec!["Auto".to_string()],
        },
    ]
}
