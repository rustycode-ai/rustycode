//! Session Sidebar Component

use lsp_types::DiagnosticSeverity;
use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Clear, Paragraph, Wrap},
    Frame,
};
use rustycode_orchestration::bus::{MilestonePlanProgress, MilestonePlanState};
use rustycode_protocol::MilestoneStatus;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::Instant;

/// Information about a single session
#[derive(Clone, Debug)]
pub struct SessionInfo {
    /// Session ID (directory name)
    pub id: String,
    /// Session title (if set)
    pub title: Option<String>,
    /// Session start time
    pub start_time: Instant,
    pub message_count: usize,
    /// Active tools count
    pub active_tools: usize,
    /// Session state
    pub state: SessionState,
    /// Recovery preview (last messages before crash)
    pub recovery_preview: Option<String>,
    /// Recovery file path (if crashed)
    pub recovery_file: Option<PathBuf>,
    /// Crash timestamp (if crashed)
    pub crash_time: Option<Instant>,
}

/// Session state indicator
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionState {
    /// Session is currently active
    Active,
    /// Session was saved normally
    Saved,
    /// Session crashed (has recovery file)
    Crashed,
}

/// Recovery action options
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RecoveryAction {
    /// Recover the session (load from recovery file)
    Recover,
    /// Discard the recovery data
    Discard,
    /// Inspect the recovery data before deciding
    Inspect,
}

impl RecoveryAction {
    pub fn label(&self) -> &str {
        match self {
            RecoveryAction::Recover => "Recover",
            RecoveryAction::Discard => "Discard",
            RecoveryAction::Inspect => "Inspect",
        }
    }

    pub fn icon(&self) -> &str {
        match self {
            RecoveryAction::Recover => "↺",
            RecoveryAction::Discard => "✕",
            RecoveryAction::Inspect => "🔍",
        }
    }

    pub fn color(&self) -> Color {
        match self {
            RecoveryAction::Recover => Color::Green,
            RecoveryAction::Discard => Color::Red,
            RecoveryAction::Inspect => Color::Cyan,
        }
    }
}

impl SessionState {
    pub fn icon(&self) -> &str {
        match self {
            SessionState::Active => "●",
            SessionState::Saved => "○",
            SessionState::Crashed => "⚠",
        }
    }

    pub fn color(&self) -> Color {
        match self {
            SessionState::Active => Color::Green,
            SessionState::Saved => Color::DarkGray,
            SessionState::Crashed => Color::Red,
        }
    }
}

/// Information about a file conflict
#[derive(Clone, Debug)]
pub struct FileConflict {
    /// Path to the conflicted file
    pub path: PathBuf,
    /// Session IDs that have modified this file
    pub session_ids: Vec<String>,
    /// Timestamp of first modification
    pub first_modified: Instant,
    /// Whether conflict has been resolved
    pub resolved: bool,
}

impl FileConflict {
    pub fn new(path: PathBuf, session_id: String) -> Self {
        Self {
            path,
            session_ids: vec![session_id],
            first_modified: Instant::now(),
            resolved: false,
        }
    }

    /// Add a session to this conflict
    pub fn add_session(&mut self, session_id: String) {
        if !self.session_ids.contains(&session_id) {
            self.session_ids.push(session_id);
        }
    }

    /// Mark conflict as resolved
    pub fn resolve(&mut self) {
        self.resolved = true;
    }

    pub fn involves_session(&self, session_id: &str) -> bool {
        self.session_ids.iter().any(|id| id == session_id)
    }

    /// Get conflict severity based on number of sessions involved
    pub fn severity(&self) -> ConflictSeverity {
        match self.session_ids.len() {
            0..=1 => ConflictSeverity::None,
            2 => ConflictSeverity::Warning,
            _ => ConflictSeverity::Critical,
        }
    }
}

/// Conflict severity level
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConflictSeverity {
    /// No conflict
    None,
    /// Warning - 2 sessions involved
    Warning,
    /// Critical - 3+ sessions involved
    Critical,
}

impl ConflictSeverity {
    pub fn icon(&self) -> &str {
        match self {
            ConflictSeverity::None => "",
            ConflictSeverity::Warning => "⚠",
            ConflictSeverity::Critical => "🔥",
        }
    }

    pub fn color(&self) -> Color {
        match self {
            ConflictSeverity::None => Color::Reset,
            ConflictSeverity::Warning => Color::Yellow,
            ConflictSeverity::Critical => Color::Red,
        }
    }
}

/// Sidebar state with collapsible sections
#[derive(Default)]
pub struct SessionSidebarState {
    /// Which sections are collapsed
    collapsed_sections: HashMap<String, bool>,
    scroll_offset: usize,
    /// Total content lines
    content_lines: usize,
    /// Visible viewport lines
    viewport_lines: usize,
}

impl SessionSidebarState {
    pub fn is_collapsed(&self, key: &str) -> bool {
        self.collapsed_sections.get(key).copied().unwrap_or(false)
    }

    /// Toggle a section's collapsed state
    pub fn toggle_section(&mut self, key: &str) {
        let collapsed = self
            .collapsed_sections
            .entry(key.to_string())
            .or_insert(false);
        *collapsed = !*collapsed;
    }

    /// Scroll up
    pub fn scroll_up(&mut self) {
        if self.scroll_offset > 0 {
            self.scroll_offset -= 1;
        }
    }

    /// Scroll down
    pub fn scroll_down(&mut self) {
        let max_scroll = self.content_lines.saturating_sub(self.viewport_lines);
        if self.scroll_offset < max_scroll {
            self.scroll_offset += 1;
        }
    }
}

/// Sidebar section
struct SidebarSection<'a> {
    key: &'static str,
    title: &'static str,
    lines: Vec<Line<'a>>,
    collapsible: bool,
}

/// Compact milestone progress snapshot shown in the sidebar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MilestoneProgressSnapshot {
    pub milestone_id: String,
    pub milestone_title: String,
    pub status: MilestoneStatus,
    pub plans_total: usize,
    pub plans_completed: usize,
    pub current_plan_summary: String,
    pub action_hint: String,
    pub plan_rows: Vec<MilestonePlanProgress>,
}

/// MCP server lifecycle state shown in the sidebar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum McpServerState {
    Connected,
    Configured,
    Remote,
    Disabled,
    Disconnected,
}

impl McpServerState {
    fn color(&self) -> Color {
        match self {
            McpServerState::Connected => Color::Green,
            McpServerState::Configured => Color::Yellow,
            McpServerState::Remote => Color::Cyan,
            McpServerState::Disabled => Color::DarkGray,
            McpServerState::Disconnected => Color::Red,
        }
    }
}

/// MCP server status entry shown in the sidebar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct McpServerStatus {
    pub name: String,
    pub state: McpServerState,
    pub detail: Option<String>,
}

/// Aggregated LSP diagnostic counts by severity.
#[derive(Default, Clone)]
struct LspDiagnosticCounts {
    errors: usize,
    warnings: usize,
    hints: usize,
    information: usize,
}

/// Session sidebar component
pub struct SessionSidebar {
    state: SessionSidebarState,
    visible: bool,
    message_count: usize,
    active_tools: usize,
    workspace_name: Option<String>,
    rate_limited: bool,
    /// Multi-session support
    current_session_id: String,
    sessions: Vec<SessionInfo>,
    selected_session_index: usize,
    /// Multi-session mode enabled
    multi_session_mode: bool,
    /// Active file conflicts
    conflicts: Vec<FileConflict>,
    /// Files modified by this session (for conflict detection)
    modified_files: HashSet<PathBuf>,
    /// Showing recovery options for crashed session
    showing_recovery: bool,
    /// LSP status
    lsp_connected: bool,
    lsp_servers: Vec<String>,
    lsp_diagnostics: LspDiagnosticCounts,
    /// MCP status
    mcp_connected: bool,
    mcp_servers: Vec<McpServerStatus>,
    /// Tool call summary
    tool_calls_running: usize,
    tool_calls_recent: Option<String>,
    /// Milestone progress summary
    milestone_progress: Option<MilestoneProgressSnapshot>,
}

impl SessionSidebar {
    pub fn new() -> Self {
        Self {
            state: SessionSidebarState::default(),
            visible: true, // Show by default so status sections are immediately visible
            message_count: 0,
            active_tools: 0,
            workspace_name: None,
            rate_limited: false,
            current_session_id: "default".to_string(),
            sessions: Vec::new(),
            selected_session_index: 0,
            multi_session_mode: false,
            conflicts: Vec::new(),
            modified_files: HashSet::new(),
            showing_recovery: false,
            lsp_connected: false,
            lsp_servers: Vec::new(),
            lsp_diagnostics: LspDiagnosticCounts::default(),
            mcp_connected: false,
            mcp_servers: Vec::new(),
            tool_calls_running: 0,
            tool_calls_recent: None,
            milestone_progress: None,
        }
    }

    /// Update LSP status information
    pub fn update_lsp_status(
        &mut self,
        connected: bool,
        servers: Vec<String>,
        diagnostics: HashMap<DiagnosticSeverity, usize>,
    ) {
        self.lsp_connected = connected;
        self.lsp_servers = servers;
        let mut counts = LspDiagnosticCounts::default();
        for (severity, count) in diagnostics {
            if severity == DiagnosticSeverity::ERROR {
                counts.errors = count;
            } else if severity == DiagnosticSeverity::WARNING {
                counts.warnings = count;
            } else if severity == DiagnosticSeverity::HINT {
                counts.hints = count;
            } else if severity == DiagnosticSeverity::INFORMATION {
                counts.information = count;
            }
        }
        self.lsp_diagnostics = counts;
    }

    /// Update MCP status information
    pub fn update_mcp_status(&mut self, connected: bool, servers: Vec<McpServerStatus>) {
        self.mcp_connected = connected;
        self.mcp_servers = servers;
    }

    /// Update tool call summary information
    pub fn update_tool_call_summary(&mut self, running: usize, recent: Option<String>) {
        self.tool_calls_running = running;
        self.tool_calls_recent = recent;
    }

    /// Update the active milestone summary shown in the sidebar.
    pub fn update_milestone_progress(
        &mut self,
        milestone_id: String,
        milestone_title: String,
        status: MilestoneStatus,
        plans_total: usize,
        plans_completed: usize,
        current_plan_summary: String,
        action_hint: String,
        plan_rows: Vec<MilestonePlanProgress>,
    ) {
        self.milestone_progress = Some(MilestoneProgressSnapshot {
            milestone_id,
            milestone_title,
            status,
            plans_total,
            plans_completed,
            current_plan_summary,
            action_hint,
            plan_rows,
        });
    }

    /// Clear the active milestone summary.
    pub fn clear_milestone_progress(&mut self) {
        self.milestone_progress = None;
    }

    pub(crate) fn has_milestone_progress(&self) -> bool {
        self.milestone_progress.is_some()
    }

    /// Show the sidebar
    pub fn show(&mut self) {
        self.visible = true;
    }

    /// Hide the sidebar
    pub fn hide(&mut self) {
        self.visible = false;
    }

    /// Toggle visibility
    pub fn toggle(&mut self) {
        self.visible = !self.visible;
    }

    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Update session info
    pub fn update_session_info(&mut self, message_count: usize, active_tools: usize) {
        self.message_count = message_count;
        self.active_tools = active_tools;
    }

    /// Set workspace name
    pub fn set_workspace(&mut self, name: Option<String>) {
        self.workspace_name = name;
    }

    /// Set rate limit status
    pub fn set_rate_limited(&mut self, limited: bool) {
        self.rate_limited = limited;
    }

    /// Enable multi-session mode
    pub fn enable_multi_session_mode(&mut self) {
        self.multi_session_mode = true;
    }

    /// Disable multi-session mode
    pub fn disable_multi_session_mode(&mut self) {
        self.multi_session_mode = false;
    }

    /// Update the list of available sessions
    pub fn update_sessions(&mut self, sessions: Vec<SessionInfo>) {
        self.sessions = sessions;
        // Keep selected index in bounds
        if !self.sessions.is_empty() && self.selected_session_index >= self.sessions.len() {
            self.selected_session_index = self.sessions.len().saturating_sub(1);
        } else if self.sessions.is_empty() {
            self.selected_session_index = 0;
        }
        self.state.scroll_offset = 0;
    }

    pub fn current_session_id(&self) -> &str {
        &self.current_session_id
    }

    /// Switch to a different session by index
    pub fn switch_to_session(&mut self, index: usize) -> Option<String> {
        if index < self.sessions.len() {
            self.selected_session_index = index;
            Some(self.sessions[index].id.clone())
        } else {
            None
        }
    }

    /// Navigate to previous session
    pub fn prev_session(&mut self) -> bool {
        if !self.sessions.is_empty() && self.selected_session_index > 0 {
            self.selected_session_index -= 1;
            true
        } else {
            false
        }
    }

    /// Navigate to next session
    pub fn next_session(&mut self) -> bool {
        if self.sessions.is_empty() {
            return false;
        }
        let next_idx = self.selected_session_index + 1;
        if next_idx < self.sessions.len() {
            self.selected_session_index = next_idx;
            true
        } else {
            false
        }
    }

    pub fn selected_session(&self) -> Option<&SessionInfo> {
        self.sessions.get(self.selected_session_index)
    }

    /// Record a file modification for conflict detection
    pub fn record_file_modification(&mut self, path: PathBuf) {
        self.modified_files.insert(path);
    }

    /// Check for conflicts with other sessions and add them to the conflict list
    pub fn detect_conflicts(&mut self, other_session_files: &HashMap<String, HashSet<PathBuf>>) {
        self.conflicts.clear();

        for (session_id, files) in other_session_files {
            if session_id == &self.current_session_id {
                continue;
            }

            for file in &self.modified_files {
                if files.contains(file) {
                    // Check if conflict already exists for this file
                    if let Some(conflict) = self.conflicts.iter_mut().find(|c| &c.path == file) {
                        conflict.add_session(session_id.clone());
                    } else {
                        let mut conflict =
                            FileConflict::new(file.clone(), self.current_session_id.clone());
                        conflict.add_session(session_id.clone());
                        self.conflicts.push(conflict);
                    }
                }
            }
        }
    }

    /// Get all active conflicts
    pub fn conflicts(&self) -> &[FileConflict] {
        &self.conflicts
    }

    /// Get conflicts for a specific session
    pub fn session_conflicts(&self, session_id: &str) -> Vec<&FileConflict> {
        self.conflicts
            .iter()
            .filter(|c| c.involves_session(session_id))
            .collect()
    }

    /// Resolve a conflict by marking it as resolved
    pub fn resolve_conflict(&mut self, file_path: &PathBuf) {
        if let Some(conflict) = self.conflicts.iter_mut().find(|c| &c.path == file_path) {
            conflict.resolve();
        }
    }

    pub fn conflict_severity(&self) -> ConflictSeverity {
        self.conflicts
            .iter()
            .filter(|c| c.involves_session(&self.current_session_id))
            .map(|c| c.severity())
            .max_by_key(|&s| match s {
                ConflictSeverity::None => 0,
                ConflictSeverity::Warning => 1,
                ConflictSeverity::Critical => 2,
            })
            .unwrap_or(ConflictSeverity::None)
    }

    /// Get modified files for this session
    pub fn modified_files(&self) -> &HashSet<PathBuf> {
        &self.modified_files
    }

    // Recovery management methods

    /// Show recovery options for crashed sessions
    pub fn show_recovery_options(&mut self) {
        self.showing_recovery = true;
    }

    /// Hide recovery options
    pub fn hide_recovery_options(&mut self) {
        self.showing_recovery = false;
    }

    pub fn is_showing_recovery(&self) -> bool {
        self.showing_recovery
    }

    /// Get all crashed sessions that need recovery
    pub fn crashed_sessions(&self) -> Vec<&SessionInfo> {
        self.sessions
            .iter()
            .filter(|s| s.state == SessionState::Crashed)
            .collect()
    }

    /// Get recovery preview for a session
    pub fn recovery_preview(&self, session_id: &str) -> Option<&str> {
        self.sessions
            .iter()
            .find(|s| s.id == session_id)
            .and_then(|s| s.recovery_preview.as_deref())
    }

    /// Set recovery preview for a session
    pub fn set_recovery_preview(&mut self, session_id: &str, preview: String) {
        if let Some(session) = self.sessions.iter_mut().find(|s| s.id == session_id) {
            // Truncate preview if too long
            let truncated = if preview.len() > 500 {
                format!("{}...", preview.chars().take(497).collect::<String>())
            } else {
                preview
            };
            session.recovery_preview = Some(truncated);
        }
    }

    /// Mark a session as crashed with recovery info
    pub fn mark_session_crashed(
        &mut self,
        session_id: &str,
        recovery_file: PathBuf,
        preview: String,
    ) {
        if let Some(session) = self.sessions.iter_mut().find(|s| s.id == session_id) {
            session.state = SessionState::Crashed;
            session.recovery_file = Some(recovery_file);
            session.crash_time = Some(Instant::now());

            // Truncate preview if too long
            let truncated = if preview.len() > 500 {
                format!("{}...", preview.chars().take(497).collect::<String>())
            } else {
                preview
            };
            session.recovery_preview = Some(truncated);
        }
    }

    /// Get available recovery actions for a session
    pub fn recovery_actions(&self, session_id: &str) -> Vec<RecoveryAction> {
        if self
            .sessions
            .iter()
            .any(|s| s.id == session_id && s.state == SessionState::Crashed)
        {
            vec![
                RecoveryAction::Inspect,
                RecoveryAction::Recover,
                RecoveryAction::Discard,
            ]
        } else {
            Vec::new()
        }
    }

    /// Scroll content up by one line
    pub fn scroll_up(&mut self) {
        self.state.scroll_up();
    }

    /// Scroll content down by one line
    pub fn scroll_down(&mut self) {
        self.state.scroll_down();
    }

    /// Handle a click event
    pub fn handle_click(&mut self, col: u16, row: u16, area: Rect) -> bool {
        if !self.visible {
            return false;
        }

        // Check if click is within sidebar area
        if col < area.x || col >= area.x + area.width || row < area.y || row >= area.y + area.height
        {
            return false;
        }

        // For now, just return false - click handling can be added later
        false
    }

    /// Build plain text for clipboard copy.
    pub fn copyable_text(&self) -> String {
        let mut out = Vec::new();

        for section in self.build_sections(0) {
            if !out.is_empty() {
                out.push(String::new());
            }

            out.push(section.title.to_string());
            for line in section.lines {
                out.push(Self::line_to_text(&line));
            }
        }

        out.join("\n")
    }

    fn line_to_text(line: &Line<'_>) -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>()
    }

    /// Render the sidebar
    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        if !self.visible || area.width < 24 || area.height < 5 {
            return;
        }

        let sidebar_width = area.width.clamp(24, 34).min(area.width);
        let sidebar_area = Rect {
            x: area.x + area.width.saturating_sub(sidebar_width),
            y: area.y,
            width: sidebar_width,
            height: area.height,
        };

        let render_area = Rect {
            x: sidebar_area.x,
            y: sidebar_area.y + 1,
            width: sidebar_area.width,
            height: area.height.saturating_sub(1),
        };
        if render_area.height < 4 {
            return;
        }

        frame.render_widget(Clear, sidebar_area);

        let block = Block::default().style(Style::default().bg(Color::Reset));
        frame.render_widget(block, render_area);

        // Snapshot collapsed state before borrowing sections
        let collapsed_keys: Vec<String> = self
            .state
            .collapsed_sections
            .iter()
            .filter(|(_, &v)| v)
            .map(|(k, _)| k.clone())
            .collect();

        // Snapshot current scroll offset before building sections
        let mut scroll_offset = self.state.scroll_offset;

        // Build sections and lines
        let sections = self.build_sections(render_area.width);

        let mut lines = Vec::new();

        for section in sections {
            if !lines.is_empty() {
                lines.push(Line::from(""));
            }

            let collapsed = collapsed_keys.iter().any(|k| k == section.key);
            let mut header = Vec::new();

            if section.collapsible {
                header.push(Span::styled(
                    if collapsed { "▶ " } else { "▼ " },
                    Style::default().fg(Color::DarkGray),
                ));
            }

            header.push(Span::styled(
                section.title,
                Style::default().fg(Color::Cyan).bold(),
            ));

            lines.push(Line::from(header));

            if !collapsed {
                for line in section.lines {
                    lines.push(line);
                }
            }
        }

        let content_count = lines.len();
        let viewport_count = render_area.height.saturating_sub(2) as usize;
        let max_scroll = content_count.saturating_sub(viewport_count);
        if scroll_offset > max_scroll {
            scroll_offset = max_scroll;
        }

        let inner_width = render_area.width.saturating_sub(1).max(1);
        let display_lines = lines
            .clone()
            .into_iter()
            .map(|line| {
                let mut spans = Vec::with_capacity(line.spans.len() + 1);
                spans.push(Span::raw(" "));
                spans.extend(line.spans.iter().cloned());
                Line::from(spans)
            })
            .collect::<Vec<_>>();

        let paragraph = Paragraph::new(display_lines)
            .scroll((scroll_offset as u16, 0))
            .wrap(Wrap { trim: false })
            .block(Block::default());
        frame.render_widget(paragraph, render_area);

        // Recalculate effective content lines after word-wrap
        // Each logical line may occupy multiple visual rows at the current width
        let wrapped_content_lines: usize = lines
            .iter()
            .map(|line| {
                let line_width = line.width();
                if inner_width == 0 {
                    1
                } else {
                    line_width.div_ceil(inner_width as usize)
                }
            })
            .sum();
        let effective_content = wrapped_content_lines.max(content_count);
        let max_scroll = effective_content.saturating_sub(viewport_count);
        if scroll_offset > max_scroll {
            scroll_offset = max_scroll;
        }

        // Update state after rendering (deferred to avoid borrow conflicts)
        self.state.content_lines = content_count;
        self.state.viewport_lines = viewport_count;
        self.state.scroll_offset = scroll_offset;
    }

    /// Build sidebar sections based on current state
    fn build_sections(&self, _width: u16) -> Vec<SidebarSection<'_>> {
        let mut sections = Vec::new();

        // Multi-session list section (shown at top when in multi-session mode)
        if self.multi_session_mode && !self.sessions.is_empty() {
            let mut session_lines = Vec::new();

            for (idx, session) in self.sessions.iter().enumerate() {
                let is_selected = idx == self.selected_session_index;
                let is_current = session.id == self.current_session_id;

                // Selection indicator
                let prefix = if is_selected {
                    Span::styled("► ", Style::default().fg(Color::Cyan).bold())
                } else {
                    Span::styled("  ", Style::default())
                };

                // State icon
                let state_icon = Span::styled(
                    format!("{} ", session.state.icon()),
                    Style::default().fg(session.state.color()),
                );

                // Session title or ID
                let display_name = session.title.as_ref().unwrap_or(&session.id);
                let truncated = if crate::unicode::display_width(display_name) > 18 {
                    crate::unicode::truncate_display(display_name, 18)
                } else {
                    display_name.to_string()
                };

                let name_span = Span::styled(
                    truncated,
                    Style::default()
                        .fg(if is_current {
                            Color::White
                        } else {
                            Color::DarkGray
                        })
                        .add_modifier(if is_current {
                            ratatui::style::Modifier::BOLD
                        } else {
                            ratatui::style::Modifier::empty()
                        }),
                );

                // Current indicator
                let current_indicator = if is_current {
                    Span::styled(" [CUR]", Style::default().fg(Color::Green))
                } else {
                    Span::styled("", Style::default())
                };

                // Message count badge
                let count_badge = Span::styled(
                    format!(" ({})", session.message_count),
                    Style::default().fg(Color::DarkGray),
                );

                // Conflict indicator
                let session_conflicts: Vec<_> = self
                    .conflicts
                    .iter()
                    .filter(|c| c.involves_session(&session.id))
                    .collect();
                let conflict_icon = if !session_conflicts.is_empty() {
                    let severity = session_conflicts
                        .iter()
                        .map(|c| c.severity())
                        .max_by_key(|&s| match s {
                            ConflictSeverity::None => 0,
                            ConflictSeverity::Warning => 1,
                            ConflictSeverity::Critical => 2,
                        })
                        .unwrap_or(ConflictSeverity::Warning);
                    Span::styled(
                        format!(" {} ", severity.icon()),
                        Style::default().fg(severity.color()),
                    )
                } else {
                    Span::styled("", Style::default())
                };

                let mut line_parts = vec![prefix, state_icon, name_span];
                if is_current {
                    line_parts.push(current_indicator);
                }
                line_parts.push(count_badge);
                line_parts.push(conflict_icon);

                session_lines.push(Line::from(line_parts));
            }

            sections.push(SidebarSection {
                key: "sessions",
                title: "Sessions",
                lines: session_lines,
                collapsible: true,
            });
        }

        // Conflicts section (shown when there are active conflicts)
        if !self.conflicts.is_empty() {
            let active_conflicts: Vec<_> = self
                .conflicts
                .iter()
                .filter(|c| !c.resolved && c.involves_session(&self.current_session_id))
                .collect();

            if !active_conflicts.is_empty() {
                let mut conflict_lines = Vec::new();

                for conflict in active_conflicts {
                    let file_display = if conflict.path.display().to_string().len() > 20 {
                        format!(
                            "...{}",
                            &conflict
                                .path
                                .display()
                                .to_string()
                                .chars()
                                .rev()
                                .take(17)
                                .collect::<String>()
                                .chars()
                                .rev()
                                .collect::<String>()
                        )
                    } else {
                        conflict.path.display().to_string()
                    };

                    let severity = conflict.severity();
                    conflict_lines.push(Line::from(vec![
                        Span::styled(
                            format!("{} ", severity.icon()),
                            Style::default().fg(severity.color()),
                        ),
                        Span::styled(file_display, Style::default().fg(Color::Red)),
                    ]));

                    // Show which sessions are involved
                    let sessions_str = conflict.session_ids.join(", ");
                    conflict_lines.push(Line::from(vec![
                        Span::styled("  ", Style::default()),
                        Span::styled(
                            format!("via: {}", sessions_str),
                            Style::default().fg(Color::DarkGray),
                        ),
                    ]));
                }

                sections.push(SidebarSection {
                    key: "conflicts",
                    title: "⚠ Conflicts",
                    lines: conflict_lines,
                    collapsible: true,
                });
            }
        }

        // Recovery section (shown when there are crashed sessions)
        let crashed_sessions: Vec<_> = self.crashed_sessions();
        if !crashed_sessions.is_empty() && self.showing_recovery {
            let mut recovery_lines = Vec::new();

            for session in crashed_sessions {
                // Session header
                recovery_lines.push(Line::from(vec![
                    Span::styled("⚠ ", Style::default().fg(Color::Red)),
                    Span::styled(
                        session.title.as_ref().unwrap_or(&session.id),
                        Style::default().fg(Color::White).bold(),
                    ),
                ]));

                // Recovery actions
                let actions = self.recovery_actions(&session.id);
                let action_str = actions
                    .iter()
                    .map(|a| format!("{}{}", a.icon(), a.label()))
                    .collect::<Vec<_>>()
                    .join(" | ");
                recovery_lines.push(Line::from(vec![
                    Span::styled("  ", Style::default()),
                    Span::styled(action_str, Style::default().fg(Color::Cyan)),
                ]));

                // Recovery preview (if available)
                if let Some(preview) = &session.recovery_preview {
                    let preview_lines: Vec<&str> = preview.lines().take(3).collect();
                    for line in preview_lines {
                        recovery_lines.push(Line::from(vec![
                            Span::styled("  │ ", Style::default().fg(Color::DarkGray)),
                            Span::styled(
                                if crate::unicode::display_width(line) > 25 {
                                    crate::unicode::truncate_display(line, 25)
                                } else {
                                    line.to_string()
                                },
                                Style::default().fg(Color::DarkGray),
                            ),
                        ]));
                    }
                }
            }

            sections.push(SidebarSection {
                key: "recovery",
                title: "🔧 Recovery",
                lines: recovery_lines,
                collapsible: true,
            });
        }

        // Milestone progress section (shown while a milestone is active)
        if let Some(ref milestone) = self.milestone_progress {
            let mut milestone_lines = Vec::new();
            milestone_lines.push(Line::from(vec![
                Span::styled("◆ ", Style::default().fg(Color::Magenta)),
                Span::styled(
                    format!(
                        "{} {}/{}",
                        milestone.status, milestone.plans_completed, milestone.plans_total
                    ),
                    Style::default()
                        .fg(Color::Magenta)
                        .add_modifier(ratatui::style::Modifier::BOLD),
                ),
            ]));
            milestone_lines.push(Line::from(vec![
                Span::styled("  ", Style::default()),
                Span::styled(
                    milestone.milestone_title.clone(),
                    Style::default().fg(Color::White),
                ),
                Span::styled(
                    format!(" ({})", milestone.milestone_id),
                    Style::default().fg(Color::DarkGray),
                ),
            ]));
            milestone_lines.push(Line::from(vec![
                Span::styled("  ", Style::default()),
                Span::styled(
                    format!("Next: {}", milestone.current_plan_summary),
                    Style::default().fg(Color::Cyan),
                ),
            ]));
            milestone_lines.push(Line::from(vec![
                Span::styled("  ", Style::default()),
                Span::styled(
                    milestone.action_hint.clone(),
                    Style::default().fg(Color::DarkGray),
                ),
            ]));
            if !milestone.plan_rows.is_empty() {
                milestone_lines.push(Line::from(vec![
                    Span::styled("  ", Style::default()),
                    Span::styled("Plans", Style::default().fg(Color::DarkGray)),
                ]));

                for plan in &milestone.plan_rows {
                    let (icon, color) = match plan.state {
                        MilestonePlanState::Completed => ("●", Color::Green),
                        MilestonePlanState::Running => ("◐", Color::Yellow),
                        MilestonePlanState::Ready => ("○", Color::Cyan),
                        MilestonePlanState::Blocked => ("⛔", Color::Red),
                        MilestonePlanState::Failed => ("✗", Color::Red),
                        MilestonePlanState::Draft => ("·", Color::DarkGray),
                    };

                    let mut row = vec![
                        Span::styled("  ", Style::default()),
                        Span::styled(format!("{} ", icon), Style::default().fg(color)),
                        Span::styled(&plan.title, Style::default().fg(color)),
                    ];

                    if !plan.blocked_by.is_empty()
                        && matches!(plan.state, MilestonePlanState::Blocked)
                    {
                        row.push(Span::styled(
                            format!("  blocked by: {}", plan.blocked_by.join(", ")),
                            Style::default().fg(Color::DarkGray),
                        ));
                    }

                    milestone_lines.push(Line::from(row));
                }
            }

            sections.push(SidebarSection {
                key: "milestone",
                title: "Milestone",
                lines: milestone_lines,
                collapsible: true,
            });
        }

        // Current session info section (time shown in header bar)
        sections.push(SidebarSection {
            key: "session",
            title: "Session",
            lines: vec![Line::from(vec![
                Span::styled("Messages ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!("{}", self.message_count),
                    Style::default().fg(Color::White),
                ),
            ])],
            collapsible: false,
        });

        // Workspace section
        if let Some(ref workspace) = self.workspace_name {
            let display_name = if crate::unicode::display_width(workspace) > 25 {
                crate::unicode::truncate_display(workspace, 25)
            } else {
                workspace.clone()
            };

            sections.push(SidebarSection {
                key: "workspace",
                title: "Workspace",
                lines: vec![Line::from(vec![
                    Span::styled("📁 ", Style::default().fg(Color::Cyan)),
                    Span::styled(display_name, Style::default().fg(Color::White)),
                ])],
                collapsible: true,
            });
        }

        // Tool calls summary section (replaces both old "Active Tools" and "Status" —
        // status is already shown in the header bar, active tool count in header ⚡N)
        if self.tool_calls_running > 0 || self.tool_calls_recent.is_some() {
            let mut tool_lines = Vec::new();

            if self.tool_calls_running > 0 {
                tool_lines.push(Line::from(vec![
                    Span::styled("◐ ", Style::default().fg(Color::Yellow)),
                    Span::styled(
                        format!("Running: {} tool(s)", self.tool_calls_running),
                        Style::default().fg(Color::White),
                    ),
                ]));
            } else {
                tool_lines.push(Line::from(vec![
                    Span::styled("● ", Style::default().fg(Color::Green)),
                    Span::styled("Idle", Style::default().fg(Color::DarkGray)),
                ]));
            }

            if let Some(ref recent) = self.tool_calls_recent {
                tool_lines.push(Line::from(vec![
                    Span::styled("  ", Style::default()),
                    Span::styled(recent.clone(), Style::default().fg(Color::DarkGray)),
                ]));
            }

            sections.push(SidebarSection {
                key: "tool-calls",
                title: "Tool Calls",
                lines: tool_lines,
                collapsible: true,
            });
        }

        // LSP status section
        if !self.lsp_servers.is_empty() || self.lsp_connected {
            let mut lsp_lines = Vec::new();

            if self.lsp_connected {
                let has_running = self.lsp_servers.iter().any(|s| s.starts_with("✓ "));
                let color = if has_running {
                    Color::Green
                } else {
                    Color::Yellow
                };
                lsp_lines.push(Line::from(vec![
                    Span::styled("● ", Style::default().fg(color)),
                    Span::styled(
                        self.lsp_servers.join(", "),
                        Style::default().fg(Color::DarkGray),
                    ),
                ]));
            } else {
                lsp_lines.push(Line::from(vec![Span::styled(
                    "● ",
                    Style::default().fg(Color::DarkGray),
                )]));
            }

            // Show diagnostics summary if any
            let total_diagnostics: usize = self.lsp_diagnostics.errors
                + self.lsp_diagnostics.warnings
                + self.lsp_diagnostics.hints
                + self.lsp_diagnostics.information;
            if total_diagnostics > 0 {
                let mut severity_parts = Vec::new();
                if self.lsp_diagnostics.errors > 0 {
                    severity_parts.push(format!("{}E", self.lsp_diagnostics.errors));
                }
                if self.lsp_diagnostics.warnings > 0 {
                    severity_parts.push(format!("{}W", self.lsp_diagnostics.warnings));
                }
                if self.lsp_diagnostics.hints > 0 {
                    severity_parts.push(format!("{}H", self.lsp_diagnostics.hints));
                }
                if self.lsp_diagnostics.information > 0 {
                    severity_parts.push(format!("{}I", self.lsp_diagnostics.information));
                }

                if !severity_parts.is_empty() {
                    lsp_lines.push(Line::from(vec![
                        Span::styled("  ", Style::default()),
                        Span::styled(
                            severity_parts.join(" "),
                            Style::default().fg(Color::DarkGray),
                        ),
                    ]));
                }
            }

            sections.push(SidebarSection {
                key: "lsp",
                title: "LSP",
                lines: lsp_lines,
                collapsible: true,
            });
        }

        // MCP status section
        if !self.mcp_servers.is_empty() || self.mcp_connected {
            let mut mcp_lines = Vec::new();

            for server in &self.mcp_servers {
                // Truncate server name to fit sidebar (width 24-34 minus "● " prefix)
                let max_name_width = 20;
                let display_name = if crate::unicode::display_width(&server.name) > max_name_width {
                    crate::unicode::truncate_display(&server.name, max_name_width.saturating_sub(3))
                        + "..."
                } else {
                    server.name.clone()
                };
                let mut line = vec![
                    Span::styled("● ", Style::default().fg(server.state.color())),
                    Span::styled(display_name, Style::default().fg(server.state.color())),
                ];

                if let Some(detail) = &server.detail {
                    line.push(Span::styled(
                        format!(" {}", detail),
                        Style::default().fg(Color::DarkGray),
                    ));
                }

                mcp_lines.push(Line::from(line));
            }

            sections.push(SidebarSection {
                key: "mcp",
                title: "MCP",
                lines: mcp_lines,
                collapsible: true,
            });
        }

        sections
    }
}

impl Default for SessionSidebar {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sidebar_creation() {
        let sidebar = SessionSidebar::new();
        assert!(sidebar.is_visible()); // Visible by default (Ctrl+B still toggles)
        assert_eq!(sidebar.message_count, 0);
    }

    #[test]
    fn test_sidebar_visibility() {
        let mut sidebar = SessionSidebar::new();
        sidebar.show();
        assert!(sidebar.is_visible());
        sidebar.hide();
        assert!(!sidebar.is_visible());
    }

    #[test]
    fn test_sidebar_toggle() {
        let mut sidebar = SessionSidebar::new();
        assert!(sidebar.is_visible()); // Starts open now
        sidebar.toggle();
        assert!(!sidebar.is_visible());
        sidebar.toggle();
        assert!(sidebar.is_visible());
    }

    #[test]
    fn test_section_collapse() {
        let mut state = SessionSidebarState::default();
        assert!(!state.is_collapsed("session"));
        state.toggle_section("session");
        assert!(state.is_collapsed("session"));
        state.toggle_section("session");
        assert!(!state.is_collapsed("session"));
    }

    #[test]
    fn test_scroll() {
        let mut state = SessionSidebarState {
            content_lines: 100,
            viewport_lines: 10,
            ..Default::default()
        };

        // Scroll down
        for _ in 0..5 {
            state.scroll_down();
        }
        assert_eq!(state.scroll_offset, 5);

        // Scroll up
        state.scroll_up();
        assert_eq!(state.scroll_offset, 4);

        // Can't scroll below 0
        for _ in 0..10 {
            state.scroll_up();
        }
        assert_eq!(state.scroll_offset, 0);
    }

    #[test]
    fn test_update_info() {
        let mut sidebar = SessionSidebar::new();
        sidebar.update_session_info(42, 3);
        assert_eq!(sidebar.message_count, 42);
        assert_eq!(sidebar.active_tools, 3);
    }

    #[test]
    fn test_workspace() {
        let mut sidebar = SessionSidebar::new();
        sidebar.set_workspace(Some("/path/to/workspace".to_string()));
        assert!(sidebar.workspace_name.is_some());
    }

    // Conflict detection tests

    #[test]
    fn test_conflict_creation() {
        let conflict = FileConflict::new(PathBuf::from("/test/file.txt"), "session1".to_string());
        assert_eq!(conflict.session_ids.len(), 1);
        assert_eq!(conflict.session_ids[0], "session1");
        assert!(!conflict.resolved);
    }

    #[test]
    fn test_conflict_add_session() {
        let mut conflict =
            FileConflict::new(PathBuf::from("/test/file.txt"), "session1".to_string());
        conflict.add_session("session2".to_string());
        assert_eq!(conflict.session_ids.len(), 2);
        // Adding same session shouldn't duplicate
        conflict.add_session("session1".to_string());
        assert_eq!(conflict.session_ids.len(), 2);
    }

    #[test]
    fn test_conflict_resolve() {
        let mut conflict =
            FileConflict::new(PathBuf::from("/test/file.txt"), "session1".to_string());
        assert!(!conflict.resolved);
        conflict.resolve();
        assert!(conflict.resolved);
    }

    #[test]
    fn test_conflict_involves_session() {
        let conflict = FileConflict::new(PathBuf::from("/test/file.txt"), "session1".to_string());
        assert!(conflict.involves_session("session1"));
        assert!(!conflict.involves_session("session2"));
    }

    #[test]
    fn test_conflict_severity() {
        let mut conflict =
            FileConflict::new(PathBuf::from("/test/file.txt"), "session1".to_string());
        assert_eq!(conflict.severity(), ConflictSeverity::None);

        conflict.add_session("session2".to_string());
        assert_eq!(conflict.severity(), ConflictSeverity::Warning);

        conflict.add_session("session3".to_string());
        assert_eq!(conflict.severity(), ConflictSeverity::Critical);
    }

    #[test]
    fn test_record_file_modification() {
        let mut sidebar = SessionSidebar::new();
        let path = PathBuf::from("/test/file.txt");
        sidebar.record_file_modification(path.clone());
        assert!(sidebar.modified_files().contains(&path));
    }

    #[test]
    fn test_conflict_detection() {
        let mut sidebar = SessionSidebar::new();
        sidebar.current_session_id = "session1".to_string();

        let path = PathBuf::from("/test/file.txt");
        sidebar.record_file_modification(path.clone());

        let mut other_files = HashMap::new();
        let mut other_session_files = HashSet::new();
        other_session_files.insert(path.clone());
        other_files.insert("session2".to_string(), other_session_files);

        sidebar.detect_conflicts(&other_files);
        assert_eq!(sidebar.conflicts().len(), 1);
        assert_eq!(sidebar.conflicts()[0].path, path);
    }

    #[test]
    fn test_get_session_conflicts() {
        let mut sidebar = SessionSidebar::new();
        sidebar.current_session_id = "session1".to_string();

        let path = PathBuf::from("/test/file.txt");
        sidebar.record_file_modification(path.clone());

        let mut other_files = HashMap::new();
        let mut other_session_files = HashSet::new();
        other_session_files.insert(path.clone());
        other_files.insert("session2".to_string(), other_session_files);

        sidebar.detect_conflicts(&other_files);

        let session1_conflicts = sidebar.session_conflicts("session1");
        assert_eq!(session1_conflicts.len(), 1);

        let session2_conflicts = sidebar.session_conflicts("session2");
        assert_eq!(session2_conflicts.len(), 1);

        let session3_conflicts = sidebar.session_conflicts("session3");
        assert_eq!(session3_conflicts.len(), 0);
    }

    #[test]
    fn test_resolve_conflict() {
        let mut sidebar = SessionSidebar::new();
        sidebar.current_session_id = "session1".to_string();

        let path = PathBuf::from("/test/file.txt");
        sidebar.record_file_modification(path.clone());

        let mut other_files = HashMap::new();
        let mut other_session_files = HashSet::new();
        other_session_files.insert(path.clone());
        other_files.insert("session2".to_string(), other_session_files);

        sidebar.detect_conflicts(&other_files);
        assert_eq!(sidebar.conflicts().len(), 1);

        sidebar.resolve_conflict(&path);
        assert!(sidebar.conflicts()[0].resolved);
    }

    #[test]
    fn test_conflict_severity_color() {
        assert_eq!(ConflictSeverity::None.color(), Color::Reset);
        assert_eq!(ConflictSeverity::Warning.color(), Color::Yellow);
        assert_eq!(ConflictSeverity::Critical.color(), Color::Red);
    }

    #[test]
    fn test_conflict_severity_icon() {
        assert_eq!(ConflictSeverity::None.icon(), "");
        assert_eq!(ConflictSeverity::Warning.icon(), "⚠");
        assert_eq!(ConflictSeverity::Critical.icon(), "🔥");
    }

    // Recovery tests

    #[test]
    fn test_recovery_action_label() {
        assert_eq!(RecoveryAction::Recover.label(), "Recover");
        assert_eq!(RecoveryAction::Discard.label(), "Discard");
        assert_eq!(RecoveryAction::Inspect.label(), "Inspect");
    }

    #[test]
    fn test_recovery_action_icon() {
        assert_eq!(RecoveryAction::Recover.icon(), "↺");
        assert_eq!(RecoveryAction::Discard.icon(), "✕");
        assert_eq!(RecoveryAction::Inspect.icon(), "🔍");
    }

    #[test]
    fn test_recovery_action_color() {
        assert_eq!(RecoveryAction::Recover.color(), Color::Green);
        assert_eq!(RecoveryAction::Discard.color(), Color::Red);
        assert_eq!(RecoveryAction::Inspect.color(), Color::Cyan);
    }

    #[test]
    fn test_mcp_update() {
        let mut sidebar = SessionSidebar::new();
        assert!(!sidebar.mcp_connected);
        assert!(sidebar.mcp_servers.is_empty());

        sidebar.update_mcp_status(
            true,
            vec![
                McpServerStatus {
                    name: "filesystem".to_string(),
                    state: McpServerState::Connected,
                    detail: None,
                },
                McpServerStatus {
                    name: "github".to_string(),
                    state: McpServerState::Configured,
                    detail: Some("configured".to_string()),
                },
            ],
        );
        assert!(sidebar.mcp_connected);
        assert_eq!(sidebar.mcp_servers.len(), 2);
        assert!(matches!(
            sidebar.mcp_servers[0].state,
            McpServerState::Connected
        ));
    }

    #[test]
    fn test_tool_call_summary_update() {
        let mut sidebar = SessionSidebar::new();
        assert_eq!(sidebar.tool_calls_running, 0);
        assert!(sidebar.tool_calls_recent.is_none());

        sidebar.update_tool_call_summary(2, Some("◐ read_file src/main.rs".to_string()));

        assert_eq!(sidebar.tool_calls_running, 2);
        assert_eq!(
            sidebar.tool_calls_recent.as_deref(),
            Some("◐ read_file src/main.rs")
        );
    }

    #[test]
    fn test_milestone_progress_update() {
        let mut sidebar = SessionSidebar::new();
        sidebar.update_milestone_progress(
            "mile_123".to_string(),
            "Auth".to_string(),
            MilestoneStatus::Active,
            5,
            2,
            "Middleware".to_string(),
            "Executing next ready plan...".to_string(),
            vec![
                MilestonePlanProgress {
                    plan_id: rustycode_protocol::PlanId::new(),
                    title: "Types".to_string(),
                    state: MilestonePlanState::Completed,
                    blocked_by: vec![],
                },
                MilestonePlanProgress {
                    plan_id: rustycode_protocol::PlanId::new(),
                    title: "Middleware".to_string(),
                    state: MilestonePlanState::Running,
                    blocked_by: vec![],
                },
                MilestonePlanProgress {
                    plan_id: rustycode_protocol::PlanId::new(),
                    title: "Tests".to_string(),
                    state: MilestonePlanState::Blocked,
                    blocked_by: vec!["Middleware".to_string()],
                },
            ],
        );

        let sections = sidebar.build_sections(30);
        let milestone = sections
            .iter()
            .find(|section| section.key == "milestone")
            .expect("milestone section missing");

        assert!(milestone.title.contains("Milestone"));
        let text = milestone
            .lines
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("Auth"));
        assert!(text.contains("2/5"));
        assert!(text.contains("Next: Middleware"));
        assert!(text.contains("blocked by: Middleware"));
    }

    #[test]
    fn test_show_recovery_options() {
        let mut sidebar = SessionSidebar::new();
        assert!(!sidebar.is_showing_recovery());
        sidebar.show_recovery_options();
        assert!(sidebar.is_showing_recovery());
        sidebar.hide_recovery_options();
        assert!(!sidebar.is_showing_recovery());
    }

    #[test]
    fn test_get_crashed_sessions() {
        let mut sidebar = SessionSidebar::new();
        sidebar.sessions.push(SessionInfo {
            id: "session1".to_string(),
            title: None,
            start_time: Instant::now(),
            message_count: 10,
            active_tools: 0,
            state: SessionState::Active,
            recovery_preview: None,
            recovery_file: None,
            crash_time: None,
        });
        sidebar.sessions.push(SessionInfo {
            id: "session2".to_string(),
            title: None,
            start_time: Instant::now(),
            message_count: 20,
            active_tools: 0,
            state: SessionState::Crashed,
            recovery_preview: Some("Preview".to_string()),
            recovery_file: Some(PathBuf::from("/recovery.json")),
            crash_time: Some(Instant::now()),
        });

        let crashed = sidebar.crashed_sessions();
        assert_eq!(crashed.len(), 1);
        assert_eq!(crashed[0].id, "session2");
    }

    #[test]
    fn test_set_recovery_preview() {
        let mut sidebar = SessionSidebar::new();
        sidebar.sessions.push(SessionInfo {
            id: "session1".to_string(),
            title: None,
            start_time: Instant::now(),
            message_count: 10,
            active_tools: 0,
            state: SessionState::Active,
            recovery_preview: None,
            recovery_file: None,
            crash_time: None,
        });

        sidebar.set_recovery_preview("session1", "This is a recovery preview".to_string());
        assert_eq!(
            sidebar.recovery_preview("session1"),
            Some("This is a recovery preview")
        );
    }

    #[test]
    fn test_recovery_preview_truncation() {
        let mut sidebar = SessionSidebar::new();
        sidebar.sessions.push(SessionInfo {
            id: "session1".to_string(),
            title: None,
            start_time: Instant::now(),
            message_count: 10,
            active_tools: 0,
            state: SessionState::Active,
            recovery_preview: None,
            recovery_file: None,
            crash_time: None,
        });

        let long_preview = "a".repeat(600);
        sidebar.set_recovery_preview("session1", long_preview.clone());
        let preview = sidebar.recovery_preview("session1");
        assert!(preview.is_some());
        assert!(preview.unwrap().len() < long_preview.len());
        assert!(preview.unwrap().ends_with("..."));
    }

    #[test]
    fn test_mark_session_crashed() {
        let mut sidebar = SessionSidebar::new();
        sidebar.sessions.push(SessionInfo {
            id: "session1".to_string(),
            title: None,
            start_time: Instant::now(),
            message_count: 10,
            active_tools: 0,
            state: SessionState::Active,
            recovery_preview: None,
            recovery_file: None,
            crash_time: None,
        });

        sidebar.mark_session_crashed(
            "session1",
            PathBuf::from("/recovery.json"),
            "Crash preview".to_string(),
        );

        let session = &sidebar.sessions[0];
        assert_eq!(session.state, SessionState::Crashed);
        assert_eq!(session.recovery_file, Some(PathBuf::from("/recovery.json")));
        assert_eq!(session.recovery_preview, Some("Crash preview".to_string()));
        assert!(session.crash_time.is_some());
    }

    #[test]
    fn test_get_recovery_actions() {
        let mut sidebar = SessionSidebar::new();
        sidebar.sessions.push(SessionInfo {
            id: "session1".to_string(),
            title: None,
            start_time: Instant::now(),
            message_count: 10,
            active_tools: 0,
            state: SessionState::Crashed,
            recovery_preview: None,
            recovery_file: Some(PathBuf::from("/recovery.json")),
            crash_time: Some(Instant::now()),
        });

        let actions = sidebar.recovery_actions("session1");
        assert_eq!(actions.len(), 3);
        assert!(actions.contains(&RecoveryAction::Inspect));
        assert!(actions.contains(&RecoveryAction::Recover));
        assert!(actions.contains(&RecoveryAction::Discard));

        // Active session should have no recovery actions
        let actions = sidebar.recovery_actions("nonexistent");
        assert_eq!(actions.len(), 0);
    }
}
