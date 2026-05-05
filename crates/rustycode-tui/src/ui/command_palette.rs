//! Command Palette Component
//!
//! This module provides a VS Code-style command palette for the TUI.
//!
//! ## Features
//!
//! - **Fuzzy matching**: Substring search with relevance ranking
//! - **Keyboard navigation**: Arrow keys, Enter to select, Esc to close
//! - **Modal dialog**: Centered overlay with ~60% width, ~40% height
//! - **Built-in commands**: Help, clear, quit, theme, model, save, load
//! - **Extensible**: Easy to add custom commands
//!
//! ## Usage
//!
//! ```rust,no_run

// Complete implementation - pending integration with keyboard shortcuts
//! use rustycode_tui::ui::command_palette::{CommandPalette, Command};
//! use crossterm::event::{KeyCode, KeyEvent};
//!
//! // Create command palette with default commands
//! let mut palette = CommandPalette::new();
//!
//! // Handle keyboard input
//! palette.handle_key(KeyEvent::new(KeyCode::Char('h'), crossterm::event::KeyModifiers::NONE));
//! palette.handle_key(KeyEvent::new(KeyCode::Down, crossterm::event::KeyModifiers::NONE));
//! palette.handle_key(KeyEvent::new(KeyCode::Enter, crossterm::event::KeyModifiers::NONE));
//!
//! // Check if a command was selected
//! if let Some(command) = palette.take_selected() {
//!     (command.handler)();  // Execute command
//! }
//! ```

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
    Frame,
};
use std::cell::Cell;
use std::fmt;

// COMMAND SYSTEM

/// Command that can be executed from the palette
#[derive(Clone)]
pub struct Command {
    /// Unique command identifier (e.g., "help", "clear")
    pub name: String,

    /// Human-readable description (e.g., "Show help dialog")
    pub description: String,

    /// Argument hint shown inline (e.g., `"<subcommand> [args]"`)
    pub argument_hint: String,

    /// Function to execute when command is selected
    pub handler: CommandHandler,
}

impl fmt::Debug for Command {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Command")
            .field("name", &self.name)
            .field("description", &self.description)
            .field("argument_hint", &self.argument_hint)
            .field("handler", &"<function>")
            .finish()
    }
}

/// Command handler function type
///
/// This is a callable that executes the command logic.
/// It returns a `CommandResult` indicating success or failure.
pub type CommandHandler = fn() -> CommandResult;

/// Result of executing a command
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum CommandResult {
    /// Command executed successfully
    Success,

    /// Command executed with a message
    SuccessWithMessage(String),

    /// Command failed
    Error(String),

    /// Command should close the palette
    Close,
}

impl CommandResult {
    /// Check if result indicates the palette should close
    pub fn should_close(&self) -> bool {
        matches!(self, Self::Close)
    }
}

impl Command {
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        handler: CommandHandler,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            argument_hint: String::new(),
            handler,
        }
    }

    /// Create a new command with an argument hint
    pub fn with_hint(
        name: impl Into<String>,
        description: impl Into<String>,
        argument_hint: impl Into<String>,
        handler: CommandHandler,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            argument_hint: argument_hint.into(),
            handler,
        }
    }

    /// Execute this command
    pub fn execute(&self) -> CommandResult {
        (self.handler)()
    }
}

// FUZZY MATCHING

/// Match relevance score
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[non_exhaustive]
pub enum MatchScore {
    /// No match
    None = 0,
    /// Substring match
    Substring = 1,
    /// Prefix match
    Prefix = 2,
    /// Exact match
    Exact = 3,
}

/// Palette tabs for grouping commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteTab {
    All,
    Core,
    Files,
    Agents,
    Extensions,
    Settings,
    Utilities,
}

impl PaletteTab {
    /// Ordered list of palette tabs.
    pub const fn all() -> [PaletteTab; 7] {
        [
            PaletteTab::All,
            PaletteTab::Core,
            PaletteTab::Files,
            PaletteTab::Agents,
            PaletteTab::Extensions,
            PaletteTab::Settings,
            PaletteTab::Utilities,
        ]
    }

    /// Human-readable label.
    pub fn label(self) -> &'static str {
        match self {
            PaletteTab::All => "All",
            PaletteTab::Core => "Core",
            PaletteTab::Files => "Files",
            PaletteTab::Agents => "Agents",
            PaletteTab::Extensions => "Extensions",
            PaletteTab::Settings => "Settings",
            PaletteTab::Utilities => "Utilities",
        }
    }

    /// Short icon used in the tab strip.
    pub fn icon(self) -> &'static str {
        match self {
            PaletteTab::All => "⌘",
            PaletteTab::Core => "●",
            PaletteTab::Files => "↔",
            PaletteTab::Agents => "⚙",
            PaletteTab::Extensions => "✦",
            PaletteTab::Settings => "◌",
            PaletteTab::Utilities => "▣",
        }
    }
}

fn command_tab(command: &Command) -> PaletteTab {
    let name = command.name.as_str();

    if matches!(
        name,
        "/clear" | "/save" | "/load" | "/quit" | "/exit" | "/q" | "/help"
    ) {
        PaletteTab::Core
    } else if matches!(
        name,
        "/undo" | "/diff" | "/export" | "/workspace" | "/rename"
    ) {
        PaletteTab::Files
    } else if matches!(
        name,
        "/agent" | "/team" | "/plan" | "/orchestra" | "/workers" | "/cron"
    ) {
        PaletteTab::Agents
    } else if name == "/plugin"
        || name == "/plugins"
        || name.starts_with("/plugin ")
        || name == "/skill"
        || name.starts_with("/skill ")
        || name == "/marketplace"
        || name.starts_with("/marketplace ")
        || name == "/mcp"
        || name.starts_with("/mcp ")
        || name == "/hook"
    {
        PaletteTab::Extensions
    } else if name == "/model"
        || name.starts_with("/model ")
        || name == "/provider"
        || name.starts_with("/provider ")
        || matches!(
            name,
            "/theme" | "/copilot-login" | "/model list" | "/provider list"
        )
    {
        PaletteTab::Settings
    } else {
        PaletteTab::Utilities
    }
}

/// Fuzzy matcher for command search
#[derive(Debug, Clone)]
pub struct FuzzyMatcher;

impl FuzzyMatcher {
    pub fn new() -> Self {
        Self
    }

    /// Calculate match score for a query against a command
    pub fn match_score(&self, query: &str, command: &Command) -> MatchScore {
        let query_lower = query.to_lowercase();
        let name_lower = command.name.to_lowercase();
        let desc_lower = command.description.to_lowercase();
        let hint_lower = command.argument_hint.to_lowercase();

        // Exact match in name
        if name_lower == query_lower {
            return MatchScore::Exact;
        }

        // Prefix match in name
        if name_lower.starts_with(&query_lower) {
            return MatchScore::Prefix;
        }

        // Substring match in name
        if name_lower.contains(&query_lower) {
            return MatchScore::Substring;
        }

        // Substring match in description
        if desc_lower.contains(&query_lower) {
            return MatchScore::Substring;
        }

        // Substring match in argument hint
        if !hint_lower.is_empty() && hint_lower.contains(&query_lower) {
            return MatchScore::Substring;
        }

        MatchScore::None
    }

    /// Filter and rank commands by query
    pub fn filter_commands(&self, query: &str, commands: &[Command]) -> Vec<(usize, MatchScore)> {
        let mut matches: Vec<(usize, MatchScore)> = commands
            .iter()
            .enumerate()
            .filter_map(|(idx, cmd)| {
                let score = self.match_score(query, cmd);
                if score != MatchScore::None {
                    Some((idx, score))
                } else {
                    None
                }
            })
            .collect();

        // Sort by score (descending)
        matches.sort_by_key(|a| std::cmp::Reverse(a.1));

        matches
    }

    /// Highlight matching characters in text (Unicode-safe)
    pub fn highlight_matches(&self, text: &str, query: &str) -> Line<'_> {
        if query.is_empty() {
            return Line::from(text.to_string());
        }

        let query_lower: Vec<char> = query.to_lowercase().chars().collect();
        let query_len = query_lower.len();
        let text_lower_chars: Vec<char> = text.to_lowercase().chars().collect();

        // Map each char in text_lower back to the char index in original text.
        // to_lowercase() can expand one char into multiple, so we build a forward map.
        let mut lower_to_text = Vec::with_capacity(text_lower_chars.len());
        for (text_idx, ch) in text.chars().enumerate() {
            for _ in ch.to_lowercase() {
                lower_to_text.push(text_idx);
            }
        }

        // Byte offsets for each char position in original text
        let byte_offsets: Vec<usize> = text.char_indices().map(|(i, _)| i).collect();
        let text_len = text.len();

        let mut spans = Vec::new();
        let mut last_text_char = 0; // last processed char index in text
        let mut i = 0;

        while i + query_len <= text_lower_chars.len() {
            if text_lower_chars[i..i + query_len] == query_lower[..] {
                // Match at text_lower char positions [i..i+query_len)
                let text_start_char = lower_to_text[i];
                // The match may span multiple original chars; find the last one
                let text_end_char = lower_to_text[i + query_len - 1];

                let byte_start = byte_offsets
                    .get(text_start_char)
                    .copied()
                    .unwrap_or(text_len);
                // End byte is the start of the char AFTER the last matched char
                let byte_end = byte_offsets
                    .get(text_end_char + 1)
                    .copied()
                    .unwrap_or(text_len);

                // Text before match
                let prev_byte = byte_offsets
                    .get(last_text_char)
                    .copied()
                    .unwrap_or(text_len);
                if byte_start > prev_byte {
                    spans.push(Span::raw(text[prev_byte..byte_start].to_string()));
                }

                // Highlighted match
                if byte_start < byte_end {
                    spans.push(Span::styled(
                        text[byte_start..byte_end].to_string(),
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    ));
                }

                last_text_char = text_end_char + 1;
                i += query_len;
            } else {
                i += 1;
            }
        }

        // Remaining text
        let remaining_byte = byte_offsets
            .get(last_text_char)
            .copied()
            .unwrap_or(text_len);
        if remaining_byte < text_len {
            spans.push(Span::raw(text[remaining_byte..].to_string()));
        }

        if spans.is_empty() {
            Line::from(text.to_string())
        } else {
            Line::from(spans)
        }
    }
}

impl Default for FuzzyMatcher {
    fn default() -> Self {
        Self::new()
    }
}

// COMMAND PALETTE STATE

/// Command palette state
#[derive(Debug, Clone)]
pub struct CommandPaletteState {
    /// Current search query
    pub query: String,

    /// Available commands
    pub commands: Vec<Command>,

    /// Recently used command names, newest first.
    pub recent_commands: Vec<String>,

    /// Filtered and ranked command indices (index into commands)
    pub filtered_indices: Vec<usize>,

    /// Currently selected index (into filtered_indices)
    pub selected_index: usize,

    /// Whether the palette is visible
    pub visible: bool,

    /// Active palette tab
    pub active_tab: PaletteTab,

    /// Scroll offset into the filtered list
    pub scroll_offset: usize,

    /// Number of rows the viewport can display
    pub viewport_rows: Cell<usize>,

    /// Fuzzy matcher
    matcher: FuzzyMatcher,
}

impl CommandPaletteState {
    /// Create new command palette state
    pub fn new(commands: Vec<Command>) -> Self {
        let filtered_indices = (0..commands.len()).collect();

        Self {
            query: String::new(),
            commands,
            recent_commands: Vec::new(),
            filtered_indices,
            selected_index: 0,
            visible: false,
            active_tab: PaletteTab::All,
            scroll_offset: 0,
            viewport_rows: Cell::new(8),
            matcher: FuzzyMatcher::new(),
        }
    }

    /// Show the palette
    pub fn show(&mut self) {
        self.visible = true;
        self.query.clear();
        self.selected_index = 0;
        self.scroll_offset = 0;
        self.active_tab = PaletteTab::All;
        self.update_filtered();
    }

    /// Hide the palette
    pub fn hide(&mut self) {
        self.visible = false;
        self.query.clear();
        self.selected_index = 0;
        self.scroll_offset = 0;
        self.update_filtered();
    }

    /// Mark a command as recently used.
    fn record_recent(&mut self, command_name: &str) {
        self.recent_commands.retain(|name| name != command_name);
        self.recent_commands.insert(0, command_name.to_string());
        self.recent_commands.truncate(5);
    }

    fn recent_rank(&self, command_name: &str) -> usize {
        self.recent_commands
            .iter()
            .position(|name| name == command_name)
            .unwrap_or(usize::MAX)
    }

    /// Toggle palette visibility
    pub fn toggle(&mut self) {
        if self.visible {
            self.hide();
        } else {
            self.show();
        }
    }

    /// Update filtered commands based on current query
    pub fn update_filtered(&mut self) {
        let mut matches: Vec<(usize, MatchScore)> = self
            .commands
            .iter()
            .enumerate()
            .filter(|(_, command)| {
                self.active_tab == PaletteTab::All || command_tab(command) == self.active_tab
            })
            .filter_map(|(idx, command)| {
                if self.query.is_empty() {
                    Some((idx, MatchScore::Substring))
                } else {
                    let score = self.matcher.match_score(&self.query, command);
                    if score == MatchScore::None {
                        None
                    } else {
                        Some((idx, score))
                    }
                }
            })
            .collect();

        matches.sort_by(|a, b| {
            let a_cmd = &self.commands[a.0];
            let b_cmd = &self.commands[b.0];
            let a_recent = self.recent_rank(&a_cmd.name);
            let b_recent = self.recent_rank(&b_cmd.name);

            if self.query.is_empty() {
                a_recent.cmp(&b_recent).then_with(|| {
                    let a_name = a_cmd.name.to_lowercase();
                    let b_name = b_cmd.name.to_lowercase();
                    a_name.cmp(&b_name)
                })
            } else {
                b.1.cmp(&a.1)
                    .then_with(|| a_recent.cmp(&b_recent))
                    .then_with(|| {
                        let a_name = a_cmd.name.to_lowercase();
                        let b_name = b_cmd.name.to_lowercase();
                        a_name.cmp(&b_name)
                    })
            }
        });

        self.filtered_indices = matches.into_iter().map(|(idx, _)| idx).collect();

        // Clamp selected index
        if self.filtered_indices.is_empty() {
            self.selected_index = 0;
            self.scroll_offset = 0;
        } else {
            self.selected_index = self.selected_index.min(self.filtered_indices.len() - 1);
            self.ensure_visible();
        }
    }

    /// Set the active tab and refresh the filtered list.
    pub fn set_active_tab(&mut self, tab: PaletteTab) {
        self.active_tab = tab;
        self.selected_index = 0;
        self.scroll_offset = 0;
        self.update_filtered();
    }

    /// Sync the query from the input field text (everything after `/`).
    /// Returns true if the query changed (caller should set dirty).
    pub fn sync_query_from_input(&mut self, input: &str) -> bool {
        let new_query = input.strip_prefix('/').unwrap_or(input).to_string();
        if self.query != new_query {
            self.query = new_query;
            self.update_filtered();
            true
        } else {
            false
        }
    }

    /// Advance to the next tab.
    pub fn next_tab(&mut self) {
        let tabs = PaletteTab::all();
        let idx = tabs
            .iter()
            .position(|tab| *tab == self.active_tab)
            .unwrap_or(0);
        self.set_active_tab(tabs[(idx + 1) % tabs.len()]);
    }

    /// Move to the previous tab.
    pub fn prev_tab(&mut self) {
        let tabs = PaletteTab::all();
        let idx = tabs
            .iter()
            .position(|tab| *tab == self.active_tab)
            .unwrap_or(0);
        let next = if idx == 0 { tabs.len() - 1 } else { idx - 1 };
        self.set_active_tab(tabs[next]);
    }

    /// Replace the current search query.
    pub fn set_query(&mut self, query: impl Into<String>) {
        self.query = query.into();
        self.selected_index = 0;
        self.scroll_offset = 0;
        self.update_filtered();
    }

    /// Clear the current search query.
    pub fn clear_query(&mut self) {
        self.set_query(String::new());
    }

    /// Move selection one page up.
    pub fn page_up(&mut self) {
        if self.filtered_indices.is_empty() {
            return;
        }
        self.selected_index = self
            .selected_index
            .saturating_sub(self.viewport_rows.get().max(1));
        self.ensure_visible();
    }

    /// Move selection one page down.
    pub fn page_down(&mut self) {
        if self.filtered_indices.is_empty() {
            return;
        }
        let last = self.filtered_indices.len().saturating_sub(1);
        self.selected_index = (self.selected_index + self.viewport_rows.get().max(1)).min(last);
        self.ensure_visible();
    }

    /// Jump to the first result.
    pub fn home(&mut self) {
        if self.filtered_indices.is_empty() {
            return;
        }
        self.selected_index = 0;
        self.ensure_visible();
    }

    /// Jump to the last result.
    pub fn end(&mut self) {
        if self.filtered_indices.is_empty() {
            return;
        }
        self.selected_index = self.filtered_indices.len().saturating_sub(1);
        self.ensure_visible();
    }

    /// Get currently selected command (if any)
    pub fn selected_command(&self) -> Option<&Command> {
        self.filtered_indices
            .get(self.selected_index)
            .and_then(|&idx| self.commands.get(idx))
    }

    fn selected_command_index(&self) -> Option<usize> {
        self.filtered_indices.get(self.selected_index).copied()
    }

    /// Add a character to the query
    pub fn insert_char(&mut self, c: char) {
        self.query.push(c);
        self.update_filtered();
    }

    /// Remove last character from query (backspace)
    pub fn backspace(&mut self) {
        self.query.pop();
        self.update_filtered();
    }

    /// Move selection up
    pub fn move_up(&mut self) {
        if !self.filtered_indices.is_empty() && self.selected_index > 0 {
            self.selected_index -= 1;
            self.ensure_visible();
        }
    }

    /// Move selection down
    pub fn move_down(&mut self) {
        if !self.filtered_indices.is_empty() {
            self.selected_index = (self.selected_index + 1).min(self.filtered_indices.len() - 1);
            self.ensure_visible();
        }
    }

    /// Get number of filtered commands
    pub fn filtered_count(&self) -> usize {
        self.filtered_indices.len()
    }

    /// Register the current selection as recently used.
    pub fn mark_selected_recent(&mut self) {
        if let Some(command) = self.selected_command() {
            let name = command.name.clone();
            self.record_recent(&name);
        }
    }

    /// Update the current viewport size.
    pub fn set_viewport_rows(&self, rows: usize) {
        self.viewport_rows.set(rows.max(1));
    }

    fn ensure_visible(&mut self) {
        if self.filtered_indices.is_empty() {
            self.scroll_offset = 0;
            return;
        }

        let viewport = self.viewport_rows.get().max(1);
        if self.selected_index < self.scroll_offset {
            self.scroll_offset = self.selected_index;
        } else if self.selected_index >= self.scroll_offset + viewport {
            self.scroll_offset = self.selected_index + 1 - viewport;
        }
    }
}

// COMMAND PALETTE RENDERER

/// Command palette renderer
pub struct CommandPaletteRenderer {
    /// Visual state
    state: CommandPaletteState,
}

impl CommandPaletteRenderer {
    pub fn new() -> Self {
        Self::with_commands(Self::default_commands())
    }

    /// Create a new command palette with custom commands
    pub fn with_commands(commands: Vec<Command>) -> Self {
        Self {
            state: CommandPaletteState::new(commands),
        }
    }

    /// Get default built-in commands
    ///
    /// These match the registered slash commands in `REGISTERED_SLASH_COMMANDS`.
    /// The palette inserts the command name into the input field; actual execution
    /// happens through the normal slash command dispatch path.
    fn default_commands() -> Vec<Command> {
        vec![
            // ── Conversation ──────────────────────────────────────
            Command::new("/clear", "Clear conversation and reset session", || {
                CommandResult::Close
            }),
            Command::new("/save", "Save current conversation", || {
                CommandResult::Close
            }),
            Command::with_hint("/load", "Load a saved conversation", "<name>", || {
                CommandResult::Close
            }),
            Command::with_hint("/rename", "Rename the current session", "<name>", || {
                CommandResult::Close
            }),
            Command::with_hint(
                "/compact",
                "Summarize and compress conversation history",
                "[preview|threshold]",
                || CommandResult::Close,
            ),
            Command::new("/cost", "Show session token usage and cost", || {
                CommandResult::Close
            }),
            Command::new("/regenerate", "Regenerate the last AI response", || {
                CommandResult::Close
            }),
            // ── Files ──────────────────────────────────────────────
            Command::new("/undo", "Undo the last file write operation", || {
                CommandResult::Close
            }),
            Command::new("/diff", "Show git diff of recent changes", || {
                CommandResult::Close
            }),
            Command::new("/export", "Export conversation to markdown file", || {
                CommandResult::Close
            }),
            Command::with_hint(
                "/workspace",
                "Rescan workspace context",
                "[rescan|reload]",
                || CommandResult::Close,
            ),
            Command::with_hint(
                "/extract",
                "Extract tasks/todos from text",
                "<text>",
                || CommandResult::Close,
            ),
            // ── Agents & Teams ─────────────────────────────────────
            Command::with_hint(
                "/agent",
                "Manage AI agents",
                "list | spawn <role> <task> | cancel <id>",
                || CommandResult::Close,
            ),
            Command::with_hint(
                "/team",
                "Start or manage a team task",
                "<task description>",
                || CommandResult::Close,
            ),
            Command::with_hint(
                "/plan",
                "Enter plan mode for structured planning",
                "[task]",
                || CommandResult::Close,
            ),
            // ── AI Configuration ───────────────────────────────────
            Command::new("/model list", "Show available models", || {
                CommandResult::Close
            }),
            Command::with_hint("/model", "Switch LLM model", "<model-name>", || {
                CommandResult::Close
            }),
            Command::with_hint(
                "/provider",
                "Switch LLM provider",
                "anthropic|openai|ollama|local",
                || CommandResult::Close,
            ),
            Command::new("/provider list", "Show available providers", || {
                CommandResult::Close
            }),
            Command::with_hint(
                "/provider connect",
                "Show setup instructions for a provider",
                "<number>",
                || CommandResult::Close,
            ),
            Command::with_hint(
                "/provider validate",
                "Test provider credentials",
                "<number>",
                || CommandResult::Close,
            ),
            Command::with_hint(
                "/provider disconnect",
                "Remove provider config",
                "<number>",
                || CommandResult::Close,
            ),
            Command::new("/theme", "Cycle through color themes", || {
                CommandResult::Close
            }),
            // ── Memory & Knowledge ─────────────────────────────────
            Command::with_hint(
                "/memory",
                "Manage persistent memory",
                "save|recall|search|list|delete",
                || CommandResult::Close,
            ),
            Command::with_hint("/review", "Analyze code for issues", "[path]", || {
                CommandResult::Close
            }),
            Command::new("/learnings", "Show accumulated learnings", || {
                CommandResult::Close
            }),
            // ── Tasks & Todos ──────────────────────────────────────
            Command::with_hint("/task", "Manage tasks", "create|list|status|done", || {
                CommandResult::Close
            }),
            Command::with_hint("/todo", "Manage todos", "add|list|done", || {
                CommandResult::Close
            }),
            Command::with_hint(
                "/track",
                "Show workspace progress",
                "full|detail|tasks|todos",
                || CommandResult::Close,
            ),
            // ── Skills & Extensions ────────────────────────────────
            Command::with_hint(
                "/skill",
                "Manage skills",
                "list|install|activate|run|reload",
                || CommandResult::Close,
            ),
            Command::new("/skills", "Open the skill browser", || CommandResult::Close),
            Command::new("/skill list", "List installed skills", || {
                CommandResult::Close
            }),
            Command::with_hint(
                "/skill install",
                "Install a skill from the marketplace",
                "<name>",
                || CommandResult::Close,
            ),
            Command::with_hint(
                "/skill uninstall",
                "Remove an installed skill",
                "<name>",
                || CommandResult::Close,
            ),
            Command::with_hint(
                "/skill activate",
                "Enable a skill for auto-triggering",
                "<name>",
                || CommandResult::Close,
            ),
            Command::with_hint("/skill deactivate", "Disable a skill", "<name>", || {
                CommandResult::Close
            }),
            Command::with_hint("/skill update", "Update a skill", "<name>", || {
                CommandResult::Close
            }),
            Command::with_hint(
                "/skill run",
                "Run a skill immediately",
                "<name> [args...]",
                || CommandResult::Close,
            ),
            Command::with_hint("/skill info", "Show skill details", "<name>", || {
                CommandResult::Close
            }),
            Command::new("/skill reload", "Reload skills from disk", || {
                CommandResult::Close
            }),
            Command::with_hint(
                "/skill suggestions",
                "Configure automatic skill suggestions",
                "[status|on|off|quiet|normal|aggressive|reset]",
                || CommandResult::Close,
            ),
            // ── Plugins ───────────────────────────────────────────
            Command::with_hint(
                "/plugin",
                "Manage installed plugins",
                "list|reload|info <name>|install <source>|update [name|all]|uninstall <name>",
                || CommandResult::Close,
            ),
            Command::new("/plugin open", "Open the plugin browser", || {
                CommandResult::Close
            }),
            Command::new("/plugins", "Alias for /plugin list", || {
                CommandResult::Close
            }),
            Command::new("/plugin list", "List installed plugins", || {
                CommandResult::Close
            }),
            Command::new("/plugin reload", "Reload plugins from disk", || {
                CommandResult::Close
            }),
            Command::with_hint("/plugin info", "Show plugin details", "<name>", || {
                CommandResult::Close
            }),
            Command::with_hint(
                "/plugin install",
                "Install a plugin from git or a local path",
                "<git-url|path>",
                || CommandResult::Close,
            ),
            Command::with_hint(
                "/plugin update",
                "Update one plugin or all plugins",
                "[name|all]",
                || CommandResult::Close,
            ),
            Command::with_hint(
                "/plugin uninstall",
                "Remove an installed plugin",
                "<name>",
                || CommandResult::Close,
            ),
            Command::with_hint("/plugin enable", "Enable a plugin", "<name>", || {
                CommandResult::Close
            }),
            Command::with_hint("/plugin disable", "Disable a plugin", "<name>", || {
                CommandResult::Close
            }),
            Command::with_hint(
                "/marketplace",
                "Browse skill marketplace",
                "list|search|install",
                || CommandResult::Close,
            ),
            Command::new("/marketplace search", "Search the marketplace", || {
                CommandResult::Close
            }),
            Command::new("/marketplace list", "List marketplace items", || {
                CommandResult::Close
            }),
            Command::new("/marketplace info", "Show marketplace item details", || {
                CommandResult::Close
            }),
            Command::new("/marketplace update", "Update marketplace items", || {
                CommandResult::Close
            }),
            Command::with_hint(
                "/marketplace install",
                "Install a marketplace item",
                "<item-id>",
                || CommandResult::Close,
            ),
            Command::with_hint(
                "/marketplace uninstall",
                "Remove a marketplace item",
                "<item-id>",
                || CommandResult::Close,
            ),
            Command::with_hint("/mcp list", "List configured MCP servers", "", || {
                CommandResult::Close
            }),
            Command::new("/mcp status", "Show MCP connection status", || {
                CommandResult::Close
            }),
            Command::new("/mcp debug", "Show MCP diagnostics", || {
                CommandResult::Close
            }),
            Command::new("/mcp reload", "Reload MCP servers from config", || {
                CommandResult::Close
            }),
            Command::with_hint("/mcp enable", "Enable an MCP server", "<server>", || {
                CommandResult::Close
            }),
            Command::with_hint("/mcp disable", "Disable an MCP server", "<server>", || {
                CommandResult::Close
            }),
            Command::with_hint("/mcp toggle", "Toggle an MCP server", "<server>", || {
                CommandResult::Close
            }),
            Command::new("/mcp open", "Open the MCP manager overlay", || {
                CommandResult::Close
            }),
            Command::with_hint(
                "/mcp call",
                "Call an MCP tool directly",
                "<server> <tool> [json-args]",
                || CommandResult::Close,
            ),
            Command::with_hint(
                "/mcp exec",
                "Alias for /mcp call",
                "<server> <tool> [json-args]",
                || CommandResult::Close,
            ),
            Command::with_hint(
                "/mcp allowlist",
                "Manage MCP allowlist",
                "add|remove|list",
                || CommandResult::Close,
            ),
            Command::with_hint("/mcp", "Manage MCP servers", "list|status", || {
                CommandResult::Close
            }),
            Command::with_hint("/hook", "Manage hooks", "list|status", || {
                CommandResult::Close
            }),
            // ── Autonomous Development ─────────────────────────────
            Command::with_hint(
                "/orchestra",
                "Orchestra project management",
                "progress|state|health|plan|execute",
                || CommandResult::Close,
            ),
            Command::with_hint(
                "/workers",
                "Manage background workers",
                "list|status|cancel",
                || CommandResult::Close,
            ),
            Command::with_hint("/cron", "Manage scheduled jobs", "list|add|remove", || {
                CommandResult::Close
            }),
            // ── Misc ───────────────────────────────────────────────
            Command::new("/help", "Show keyboard shortcuts and help", || {
                CommandResult::Close
            }),
            Command::new("/copilot-login", "Sign in to GitHub Copilot", || {
                CommandResult::Close
            }),
            Command::new("/quit", "Exit the TUI (Ctrl+D/Ctrl+Q)", || {
                CommandResult::Close
            }),
        ]
    }

    /// Get mutable reference to state
    pub fn state_mut(&mut self) -> &mut CommandPaletteState {
        &mut self.state
    }

    /// Get reference to state
    pub fn state(&self) -> &CommandPaletteState {
        &self.state
    }

    /// Show the palette
    pub fn show(&mut self) {
        self.state.show();
    }

    /// Hide the palette
    pub fn hide(&mut self) {
        self.state.hide();
    }

    /// Toggle palette visibility
    pub fn toggle(&mut self) {
        self.state.toggle();
    }

    /// Handle a key event
    ///
    /// Returns true if the event was handled
    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        match (key.code, key.modifiers) {
            // Close palette on Escape
            (KeyCode::Esc, KeyModifiers::NONE) => {
                self.hide();
                true
            }

            // Switch tabs
            (KeyCode::Tab, KeyModifiers::CONTROL) => {
                self.state.next_tab();
                true
            }
            (KeyCode::BackTab, _) => {
                self.state.prev_tab();
                true
            }

            // Navigate up
            (KeyCode::Up | KeyCode::Char('k'), KeyModifiers::NONE) => {
                self.state.move_up();
                true
            }

            // Navigate down
            (KeyCode::Down | KeyCode::Char('j'), KeyModifiers::NONE) => {
                self.state.move_down();
                true
            }

            // Paging
            (KeyCode::PageUp, KeyModifiers::NONE) => {
                self.state.page_up();
                true
            }
            (KeyCode::PageDown, KeyModifiers::NONE) => {
                self.state.page_down();
                true
            }
            (KeyCode::Home, KeyModifiers::NONE) => {
                self.state.home();
                true
            }
            (KeyCode::End, KeyModifiers::NONE) => {
                self.state.end();
                true
            }

            // Select command on Enter
            (KeyCode::Enter, KeyModifiers::NONE) => {
                if let Some(command) = self.state.selected_command() {
                    command.execute();
                }
                self.hide();
                true
            }

            // Typing characters
            (KeyCode::Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) => {
                self.state.insert_char(c);
                true
            }

            // Backspace
            (KeyCode::Backspace, KeyModifiers::NONE) => {
                self.state.backspace();
                true
            }

            // Jump back one word with Ctrl+Backspace
            (KeyCode::Backspace, KeyModifiers::CONTROL) => {
                let trimmed = self.state.query.trim_end();
                let cut = trimmed
                    .rfind(char::is_whitespace)
                    .map(|idx| idx + 1)
                    .unwrap_or(0);
                self.state.query.truncate(cut);
                self.state.update_filtered();
                true
            }

            // Clear query on Ctrl+U
            (KeyCode::Char('u'), KeyModifiers::CONTROL) => {
                self.state.clear_query();
                true
            }

            _ => false,
        }
    }

    /// Take the selected command (removes it from state)
    pub fn take_selected_command(&mut self) -> Option<Command> {
        if let Some(idx) = self.state.selected_command_index() {
            let command = self.state.commands[idx].clone();
            self.state.mark_selected_recent();
            Some(command)
        } else {
            None
        }
    }

    /// Render the command palette
    pub fn render(&self, f: &mut Frame, area: Rect) {
        if !self.state.visible {
            return;
        }

        let count = self.state.filtered_indices.len();
        if area.width < 40 || area.height < 12 {
            return; // Terminal too small to show palette
        }

        let width = ((area.width as f32) * 0.82).round() as u16;
        let width = width.clamp(54, area.width.saturating_sub(2));
        let height = ((area.height as f32) * 0.64).round() as u16;
        let height = height.clamp(14, area.height.saturating_sub(2));
        let x = area.x + (area.width.saturating_sub(width)) / 2;
        let y = area.y + (area.height.saturating_sub(height)) / 2;
        let palette_area = Rect::new(x, y, width, height);

        f.render_widget(Clear, palette_area);

        let outer = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Rgb(80, 200, 220)))
            .title(Line::from(vec![
                Span::styled(
                    " Command Dialog ",
                    Style::default().fg(Color::Rgb(80, 200, 220)),
                ),
                Span::styled(
                    format!("{} results", count),
                    Style::default().fg(Color::DarkGray),
                ),
            ]));
        f.render_widget(outer.clone(), palette_area);

        let inner = outer.inner(palette_area);
        if inner.width < 4 || inner.height < 4 {
            return;
        }

        self.state
            .set_viewport_rows(inner.height.saturating_sub(4) as usize);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(2),
                Constraint::Length(1),
                Constraint::Min(4),
                Constraint::Length(2),
            ])
            .split(inner);

        self.render_search(f, chunks[0]);
        self.render_recent(f, chunks[1]);
        self.render_body(f, chunks[2]);
        self.render_footer(f, chunks[3]);
    }

    fn render_search(&self, f: &mut Frame, area: Rect) {
        let selected = self
            .state
            .selected_command()
            .map(|cmd| cmd.name.as_str())
            .unwrap_or("no match");

        let search = Paragraph::new(Line::from(vec![
            Span::styled("Search ", Style::default().fg(Color::DarkGray)),
            Span::styled(&self.state.query, Style::default().fg(Color::White)),
            Span::styled(
                if self.state.query.is_empty() {
                    "  Type to filter".to_string()
                } else {
                    format!("  → {}", selected)
                },
                Style::default().fg(Color::DarkGray),
            ),
        ]))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Search ")
                .border_style(Style::default().fg(Color::DarkGray)),
        )
        .wrap(Wrap { trim: false });

        f.render_widget(search, area);
    }

    fn render_recent(&self, f: &mut Frame, area: Rect) {
        let mut spans = vec![Span::styled(
            "Recent: ",
            Style::default().fg(Color::DarkGray),
        )];

        if self.state.recent_commands.is_empty() {
            spans.push(Span::styled(
                "none yet",
                Style::default().fg(Color::DarkGray),
            ));
        } else {
            for (idx, name) in self.state.recent_commands.iter().enumerate() {
                if idx > 0 {
                    spans.push(Span::styled("  ", Style::default().fg(Color::DarkGray)));
                }
                spans.push(Span::styled(
                    format!("[{}]", name),
                    Style::default()
                        .fg(Color::Rgb(80, 200, 220))
                        .add_modifier(Modifier::BOLD),
                ));
            }
        }

        let recent = Paragraph::new(Line::from(spans)).wrap(Wrap { trim: true });
        f.render_widget(recent, area);
    }

    fn render_body(&self, f: &mut Frame, area: Rect) {
        if self.state.filtered_indices.is_empty() {
            let empty = Paragraph::new(vec![
                Line::from(Span::styled(
                    "No commands matched",
                    Style::default().fg(Color::DarkGray),
                )),
                Line::from(Span::styled(
                    "Try a different query or clear the search with Ctrl+U.",
                    Style::default().fg(Color::DarkGray),
                )),
            ])
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::DarkGray)),
            )
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true });
            f.render_widget(empty, area);
            return;
        }

        let visible_start = self.state.scroll_offset;
        let visible_end = (visible_start + self.state.viewport_rows.get().max(1))
            .min(self.state.filtered_indices.len());

        let items: Vec<ListItem> = self.state.filtered_indices[visible_start..visible_end]
            .iter()
            .enumerate()
            .map(|(offset, &cmd_idx)| {
                let command = &self.state.commands[cmd_idx];
                let absolute_index = visible_start + offset;
                let is_selected = absolute_index == self.state.selected_index;

                let mut label_spans = vec![Span::styled(
                    if is_selected { "▸ " } else { "  " },
                    Style::default().fg(if is_selected {
                        Color::Cyan
                    } else {
                        Color::DarkGray
                    }),
                )];

                label_spans.extend(
                    self.state
                        .matcher
                        .highlight_matches(&command.name, &self.state.query)
                        .spans,
                );

                if !command.argument_hint.is_empty() {
                    label_spans.push(Span::styled(
                        format!(" {}", command.argument_hint),
                        Style::default().fg(Color::DarkGray),
                    ));
                }

                label_spans.push(Span::styled(
                    format!("  · {}", command.description),
                    Style::default().fg(Color::DarkGray),
                ));

                ListItem::new(Line::from(label_spans))
            })
            .collect();

        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Commands ")
                    .border_style(Style::default().fg(Color::DarkGray)),
            )
            .highlight_style(
                Style::default()
                    .bg(Color::Rgb(50, 60, 68))
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            );

        let mut list_state = ListState::default();
        list_state.select(Some(
            self.state.selected_index.saturating_sub(visible_start),
        ));
        f.render_stateful_widget(list, area, &mut list_state);
    }

    fn render_footer(&self, f: &mut Frame, area: Rect) {
        let footer = Paragraph::new(Line::from(vec![
            Span::styled("Esc", Style::default().fg(Color::DarkGray)),
            Span::raw(" close  "),
            Span::styled("Enter", Style::default().fg(Color::DarkGray)),
            Span::raw(" insert  "),
            Span::styled("Ctrl+K/P", Style::default().fg(Color::DarkGray)),
            Span::raw(" open  "),
            Span::styled("PgUp/PgDn", Style::default().fg(Color::DarkGray)),
            Span::raw(" scroll  "),
            Span::styled("Ctrl+U", Style::default().fg(Color::DarkGray)),
            Span::raw(" clear"),
        ]))
        .alignment(Alignment::Left)
        .wrap(Wrap { trim: true });

        f.render_widget(footer, area);
    }
}

impl Default for CommandPaletteRenderer {
    fn default() -> Self {
        Self::new()
    }
}

// COMMAND PALETTE (HIGH-LEVEL API)

/// High-level command palette API
///
/// This combines state and rendering into a single convenient interface.
pub struct CommandPalette {
    /// Renderer with embedded state
    renderer: CommandPaletteRenderer,
}

impl CommandPalette {
    pub fn new() -> Self {
        Self {
            renderer: CommandPaletteRenderer::new(),
        }
    }

    /// Create a new command palette with custom commands
    pub fn with_commands(commands: Vec<Command>) -> Self {
        Self {
            renderer: CommandPaletteRenderer::with_commands(commands),
        }
    }

    /// Check if palette is visible
    pub fn is_visible(&self) -> bool {
        self.renderer.state().visible
    }

    /// Show the palette
    pub fn show(&mut self) {
        self.renderer.show();
    }

    /// Hide the palette
    pub fn hide(&mut self) {
        self.renderer.hide();
    }

    /// Toggle palette visibility
    pub fn toggle(&mut self) {
        self.renderer.toggle();
    }

    /// Handle a key event
    ///
    /// Returns true if the event was handled
    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        self.renderer.handle_key(key)
    }

    /// Take the selected command (removes it from state)
    pub fn take_selected(&mut self) -> Option<Command> {
        self.renderer.take_selected_command()
    }

    /// Render the command palette
    pub fn render(&self, f: &mut Frame, area: Rect) {
        self.renderer.render(f, area);
    }

    /// Get mutable reference to state
    pub fn state_mut(&mut self) -> &mut CommandPaletteState {
        self.renderer.state_mut()
    }

    /// Get reference to state
    pub fn state(&self) -> &CommandPaletteState {
        self.renderer.state()
    }

    /// Sync query from input text (everything after `/`)
    pub fn sync_query_from_input(&mut self, input: &str) -> bool {
        self.renderer.state_mut().sync_query_from_input(input)
    }
}

impl Default for CommandPalette {
    fn default() -> Self {
        Self::new()
    }
}

// EXAMPLE USAGE

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_creation() {
        let handler = || CommandResult::Success;
        let cmd = Command::new("test", "Test command", handler);

        assert_eq!(cmd.name, "test");
        assert_eq!(cmd.description, "Test command");
    }

    #[test]
    fn test_fuzzy_matcher() {
        let matcher = FuzzyMatcher::new();
        let cmd = Command::new("help", "Show help dialog", || CommandResult::Success);

        // Exact match
        assert_eq!(matcher.match_score("help", &cmd), MatchScore::Exact);

        // Prefix match
        assert_eq!(matcher.match_score("hel", &cmd), MatchScore::Prefix);

        // Substring match
        assert_eq!(matcher.match_score("el", &cmd), MatchScore::Substring);

        // Description match
        assert_eq!(matcher.match_score("dialog", &cmd), MatchScore::Substring);

        // No match
        assert_eq!(matcher.match_score("xyz", &cmd), MatchScore::None);
    }

    #[test]
    fn test_command_palette_state() {
        let commands = vec![
            Command::new("help", "Show help", || CommandResult::Success),
            Command::new("clear", "Clear history", || CommandResult::Success),
        ];

        let mut state = CommandPaletteState::new(commands);

        assert!(!state.visible);
        assert_eq!(state.filtered_count(), 2);

        state.show();
        assert!(state.visible);

        state.insert_char('h');
        assert_eq!(state.query, "h");
        assert_eq!(state.filtered_count(), 2); // Both "help" (name) and "clear" (description "history") match

        state.backspace();
        assert_eq!(state.query, "");
        assert_eq!(state.filtered_count(), 2);
    }

    #[test]
    fn test_highlight_matches() {
        let matcher = FuzzyMatcher::new();

        // Exact match
        let line = matcher.highlight_matches("help", "help");
        assert!(!line.spans.is_empty());

        // Substring match
        let line = matcher.highlight_matches("el", "help");
        assert!(!line.spans.is_empty());

        // No match
        let line = matcher.highlight_matches("xyz", "help");
        // Should return original text as single span
        assert_eq!(line.spans.len(), 1);
    }

    #[test]
    fn test_highlight_matches_unicode() {
        let matcher = FuzzyMatcher::new();

        // Case-insensitive match with multi-byte chars — must not panic
        let line = matcher.highlight_matches("über", "ÜBER");
        assert!(!line.spans.is_empty());

        // CJK characters — must not panic
        let line = matcher.highlight_matches("東", "東京都");
        assert!(!line.spans.is_empty());

        // Turkish İ (expands to i + combining dot above in lowercase)
        let line = matcher.highlight_matches("istanbul", "İstanbul");
        // Should not panic even if highlighting is partial
        assert!(!line.spans.is_empty());

        // Mixed ASCII + multi-byte
        let line = matcher.highlight_matches("café", "CAFÉ");
        assert!(!line.spans.is_empty());

        // Verify ASCII still works correctly
        let line = matcher.highlight_matches("test_command", "test");
        assert!(line.spans.len() >= 2);
    }

    #[test]
    fn test_keyboard_navigation() {
        let mut palette = CommandPalette::new();
        palette.show();

        assert_eq!(palette.state().selected_index, 0);

        // Navigate down
        palette.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(palette.state().selected_index, 1);

        // Navigate up
        palette.handle_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
        assert_eq!(palette.state().selected_index, 0);

        // Escape hides palette
        palette.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(!palette.is_visible());
    }

    #[test]
    fn test_query_filtering() {
        let mut palette = CommandPalette::new();
        palette.show();

        // Type "he" to filter for "help" and commands with "he" in description
        palette.handle_key(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE));
        palette.handle_key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE));

        assert_eq!(palette.state().query, "he");
        // "he" matches "help" (name) and commands with "he" in their descriptions
        // like "theme" ("Switch between dark and light theme"), "model", "save", "load"
        assert!(palette.state().filtered_count() >= 1);

        // Backspace
        palette.handle_key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
        assert_eq!(palette.state().query, "h");

        // Clear with Ctrl+U
        palette.handle_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
        assert_eq!(palette.state().query, "");
    }

    #[test]
    fn test_tab_filtering() {
        let commands = vec![
            Command::new("/clear", "Clear conversation", || CommandResult::Success),
            Command::new("/undo", "Undo a file write", || CommandResult::Success),
            Command::new("/agent", "Manage agents", || CommandResult::Success),
            Command::new("/plugin list", "List plugins", || CommandResult::Success),
            Command::new("/theme", "Switch theme", || CommandResult::Success),
        ];

        let mut palette = CommandPalette::with_commands(commands);
        palette.show();

        palette.state_mut().set_active_tab(PaletteTab::Files);
        assert_eq!(palette.state().filtered_count(), 1);
        assert_eq!(palette.state().selected_command().unwrap().name, "/undo");

        palette.state_mut().set_active_tab(PaletteTab::Agents);
        assert_eq!(palette.state().filtered_count(), 1);
        assert_eq!(palette.state().selected_command().unwrap().name, "/agent");

        palette.state_mut().set_active_tab(PaletteTab::Extensions);
        assert_eq!(palette.state().filtered_count(), 1);
        assert_eq!(
            palette.state().selected_command().unwrap().name,
            "/plugin list"
        );
    }

    #[test]
    fn test_paging_keeps_selection_in_view() {
        let commands = (0..12)
            .map(|idx| {
                Command::new(format!("cmd{idx}"), "Paging test", || {
                    CommandResult::Success
                })
            })
            .collect::<Vec<_>>();

        let mut palette = CommandPalette::with_commands(commands);
        palette.show();
        palette.state_mut().set_viewport_rows(4);

        palette.state_mut().page_down();
        assert_eq!(palette.state().selected_index, 4);
        assert!(palette.state().scroll_offset <= palette.state().selected_index);

        palette.state_mut().page_down();
        assert_eq!(palette.state().selected_index, 8);

        palette.state_mut().page_down();
        assert_eq!(palette.state().selected_index, 11);
    }

    #[test]
    fn test_command_execution() {
        let mut palette = CommandPalette::new();
        palette.show();

        // Select first command and execute
        let cmd = palette.take_selected();
        assert!(cmd.is_some());

        if let Some(cmd) = cmd {
            let result = cmd.execute();
            // All palette commands return Close (actual execution goes through slash dispatch)
            assert!(matches!(result, CommandResult::Close));
        }
    }

    #[test]
    fn test_recent_commands_tracking() {
        let commands = vec![
            Command::new("help", "Show help", || CommandResult::Success),
            Command::new("clear", "Clear history", || CommandResult::Success),
        ];

        let mut palette = CommandPalette::with_commands(commands);
        palette.show();

        palette.state_mut().set_query("help");
        let _ = palette.take_selected();

        assert_eq!(palette.state().recent_commands.len(), 1);
        assert_eq!(palette.state().recent_commands[0], "help");
    }
}
