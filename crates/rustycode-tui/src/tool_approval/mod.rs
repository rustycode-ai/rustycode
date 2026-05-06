//! Tool Approval System
//!
//! Risk-based tool classification and approval UI for safe tool execution.

use ratatui::layout::Alignment;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

pub mod risk;

/// Tool approval state
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ApprovalState {
    /// Tool has not been approved yet
    Pending,
    /// Tool approved for execution
    Approved,
    /// Tool rejected by user
    Rejected,
    /// Tool approved for all future uses (session)
    ApprovedAll,
    /// Tool rejected and blocked for rest of session
    RejectedAll,
}

/// Scroll state for diff preview within the approval dialog.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct DiffScrollState {
    pub scroll_offset: usize,
}

/// Tool approval request
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ApprovalRequest {
    pub tool_name: String,
    pub tool_type: risk::ToolType,
    pub risk_level: risk::RiskLevel,
    pub description: String,
    pub command: String,
    pub state: ApprovalState,
    pub diff_scroll: DiffScrollState,
}

impl ApprovalRequest {
    pub fn new(
        tool_name: String,
        tool_type: risk::ToolType,
        description: String,
        command: String,
    ) -> Self {
        let risk_level = risk::classify_tool_risk(&tool_type, &command);

        Self {
            tool_name,
            tool_type,
            risk_level,
            description,
            command,
            state: ApprovalState::Pending,
            diff_scroll: DiffScrollState::default(),
        }
    }

    /// Check if the command field contains diff content (git diff or unified diff format).
    pub fn has_diff_content(&self) -> bool {
        crate::ui::diff_renderer::looks_like_git_diff(&self.command)
    }

    pub fn scroll_diff_up(&mut self) {
        if self.diff_scroll.scroll_offset > 0 {
            self.diff_scroll.scroll_offset -= 1;
        }
    }

    pub fn scroll_diff_down(&mut self, visible_lines: usize) {
        let total = self.command.lines().count();
        let max_offset = total.saturating_sub(visible_lines);
        if self.diff_scroll.scroll_offset < max_offset {
            self.diff_scroll.scroll_offset += 1;
        }
    }

    pub fn approve(&mut self) {
        self.state = ApprovalState::Approved;
    }

    pub fn reject(&mut self) {
        self.state = ApprovalState::Rejected;
    }

    pub fn reject_all(&mut self) {
        self.state = ApprovalState::RejectedAll;
    }

    pub fn approve_all(&mut self) {
        self.state = ApprovalState::ApprovedAll;
    }

    pub fn is_approved(&self) -> bool {
        matches!(
            self.state,
            ApprovalState::Approved | ApprovalState::ApprovedAll
        )
    }
}

/// Tool approval manager
#[derive(Debug)]
#[non_exhaustive]
pub struct ToolApprovalManager {
    pub session_approvals: Vec<(String, ApprovalState)>,
    pub auto_approve_safe: bool,
}

impl ToolApprovalManager {
    pub fn new() -> Self {
        Self {
            session_approvals: Vec::new(),
            auto_approve_safe: true, // Auto-approve safe tools
        }
    }

    pub fn requires_approval(&self, tool_name: &str, risk_level: risk::RiskLevel) -> bool {
        // Check if we've already approved this tool in the session
        if let Some((_, state)) = self
            .session_approvals
            .iter()
            .find(|(name, _)| name == tool_name)
        {
            return !matches!(state, ApprovalState::Approved | ApprovalState::ApprovedAll);
        }

        // Auto-approve safe tools if enabled
        if self.auto_approve_safe && matches!(risk_level, risk::RiskLevel::Safe) {
            return false;
        }

        true
    }

    /// Record approval decision for session
    pub fn record_approval(&mut self, tool_name: String, state: ApprovalState) {
        // Remove existing approval for this tool if any
        self.session_approvals
            .retain(|(name, _)| name != &tool_name);

        // Add new approval
        self.session_approvals.push((tool_name, state));
    }

    /// Get approval state for a tool
    pub fn approval_state(&self, tool_name: &str) -> Option<&ApprovalState> {
        self.session_approvals
            .iter()
            .find(|(name, _)| name == tool_name)
            .map(|(_, state)| state)
    }

    pub fn is_blocked(&self, tool_name: &str) -> bool {
        self.session_approvals
            .iter()
            .any(|(name, state)| name == tool_name && matches!(state, ApprovalState::RejectedAll))
    }
}

impl Default for ToolApprovalManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Compute dynamic panel dimensions for the approval dialog.
///
/// Returns `(panel_height, panel_width)`. When the request contains diff content,
/// the panel expands to show more lines (up to half the terminal height).
/// Otherwise, returns the compact 7x70 default.
pub fn approval_panel_size(request: &ApprovalRequest, terminal_size: ratatui::layout::Rect) -> (u16, u16) {
    if request.has_diff_content() {
        let diff_lines = request.command.lines().count();
        let max_height = (terminal_size.height / 2).max(7) as usize;
        let needed = diff_lines + 6; // header (2) + footer (2) + borders (2)
        let height = (needed.min(max_height)) as u16;
        let width = 80u16.min(terminal_size.width.saturating_sub(4));
        (height, width)
    } else {
        let height = 7u16.min(terminal_size.height.saturating_sub(4));
        let width = 70u16.min(terminal_size.width.saturating_sub(4));
        (height, width)
    }
}

/// Render tool approval UI.
///
/// When the request carries diff content, renders an expanded dialog with a
/// scrollable, color-coded diff preview. Otherwise renders the compact single-line view.
pub fn render_approval_prompt(
    frame: &mut ratatui::Frame,
    area: ratatui::layout::Rect,
    request: &ApprovalRequest,
    _terminal_size: ratatui::layout::Rect,
) {
    // Calculate risk color
    let risk_color = match request.risk_level {
        risk::RiskLevel::Safe => Color::Green,
        risk::RiskLevel::Medium => Color::Yellow,
        risk::RiskLevel::High => Color::Rgb(255, 165, 0),
        risk::RiskLevel::Dangerous => Color::Red,
    };

    let risk_label = match request.risk_level {
        risk::RiskLevel::Safe => "● safe",
        risk::RiskLevel::Medium => "◐ medium",
        risk::RiskLevel::High => "▲ high",
        risk::RiskLevel::Dangerous => "⚠ dangerous",
    };

    let title = format!(" Tool Approval: {} ", request.tool_name);

    let risk_guidance = match request.risk_level {
        risk::RiskLevel::Safe => "This tool is considered safe.",
        risk::RiskLevel::Medium => "This tool will modify files. Review carefully.",
        risk::RiskLevel::High => "This tool executes system commands. Monitor closely.",
        risk::RiskLevel::Dangerous => "This tool is destructive! Use with extreme caution.",
    };

    let header = vec![
        Line::from(vec![
            Span::styled("Risk: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                risk_label,
                Style::default()
                    .fg(risk_color)
                    .add_modifier(ratatui::style::Modifier::BOLD),
            ),
            Span::styled("  ", Style::default()),
            Span::styled(risk_guidance, Style::default().fg(Color::DarkGray)),
        ]),
        Line::from(vec![Span::styled(
            &request.description,
            Style::default().fg(Color::White),
        )]),
    ];

    let footer = vec![Line::from(vec![
        Span::styled("[y] ", Style::default().fg(Color::Green)),
        Span::styled("Yes  ", Style::default().fg(Color::White)),
        Span::styled("[n] ", Style::default().fg(Color::Red)),
        Span::styled("No  ", Style::default().fg(Color::White)),
        Span::styled("[a] ", Style::default().fg(Color::Cyan)),
        Span::styled("Always  ", Style::default().fg(Color::White)),
        Span::styled("[N] ", Style::default().fg(Color::Rgb(255, 100, 100))),
        Span::styled("Block  ", Style::default().fg(Color::White)),
        Span::styled("[Esc] ", Style::default().fg(Color::DarkGray)),
        Span::styled("Cancel", Style::default().fg(Color::DarkGray)),
    ])];

    frame.render_widget(ratatui::widgets::Clear, area);

    if request.has_diff_content() {
        render_diff_approval(frame, area, request, &title, risk_color, &header, &footer);
    } else {
        render_compact_approval(frame, area, request, &title, risk_color, &header, &footer);
    }
}

/// Compact single-line approval (non-diff commands).
fn render_compact_approval(
    frame: &mut ratatui::Frame,
    area: ratatui::layout::Rect,
    request: &ApprovalRequest,
    title: &str,
    risk_color: Color,
    header: &[Line<'_>],
    footer: &[Line<'_>],
) {
    let clean_command = crate::app::tool_output_format::strip_ansi_escapes(&request.command);
    let sanitized: String = clean_command
        .chars()
        .map(|c| {
            if c < '\u{20}' && c != '\n' && c != '\t' || c == '\u{7F}' {
                '�'
            } else {
                c
            }
        })
        .collect();
    let cmd_display = if crate::unicode::display_width(&sanitized) > 80 {
        crate::unicode::truncate_display(&sanitized, 80)
    } else {
        sanitized
    };

    let mut content: Vec<Line<'_>> = header.to_vec();
    content.push(Line::from(vec![
        Span::styled("> ", Style::default().fg(risk_color)),
        Span::styled(&cmd_display, Style::default().fg(Color::Cyan)),
    ]));
    content.extend(footer.iter().cloned());

    let paragraph = Paragraph::new(content)
        .wrap(Wrap { trim: false })
        .block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_style(Style::default().fg(risk_color))
                .style(Style::default().bg(Color::Rgb(20, 20, 25)))
                .padding(ratatui::widgets::Padding::new(0, 1, 0, 1)),
        )
        .alignment(Alignment::Left);

    frame.render_widget(paragraph, area);
}

/// Expanded scrollable diff approval (write/edit tool commands).
fn render_diff_approval(
    frame: &mut ratatui::Frame,
    area: ratatui::layout::Rect,
    request: &ApprovalRequest,
    title: &str,
    risk_color: Color,
    header: &[Line<'_>],
    footer: &[Line<'_>],
) {
    // Render header and footer as static text; diff lines fill the middle.
    let diff_lines = crate::ui::diff_renderer::render_diff(&request.command);
    let total_diff = diff_lines.len();

    // Available vertical space for diff content (area minus header, footer, borders)
    let header_lines = header.len();
    let footer_lines = footer.len();
    let border_lines = 2;
    let available_diff = (area.height as usize)
        .saturating_sub(header_lines + footer_lines + border_lines);
    let visible_diff = available_diff.max(1);

    // Build full content: header + diff slice + footer
    let scroll_offset = request.diff_scroll.scroll_offset.min(total_diff);
    let diff_end = (scroll_offset + visible_diff).min(total_diff);
    let visible_slice = &diff_lines[scroll_offset..diff_end];

    let mut content: Vec<Line<'_>> = header.to_vec();
    content.extend(visible_slice.iter().cloned());

    // Scroll indicator when content overflows
    if total_diff > visible_diff {
        let pct = if scroll_offset + visible_diff >= total_diff {
            "bot"
        } else if scroll_offset == 0 {
            "top"
        } else {
            "mid"
        };
        content.push(Line::from(vec![
            Span::styled(
                format!(" {} lines, j/k to scroll ({}) ", total_diff, pct),
                Style::default().fg(Color::DarkGray),
            ),
        ]));
    }

    content.extend(footer.iter().cloned());

    let paragraph = Paragraph::new(content)
        .block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_style(Style::default().fg(risk_color))
                .style(Style::default().bg(Color::Rgb(20, 20, 25)))
                .padding(ratatui::widgets::Padding::new(0, 1, 0, 1)),
        )
        .alignment(Alignment::Left);

    frame.render_widget(paragraph, area);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_scroll_stops_at_zero() {
        let mut req = ApprovalRequest::new(
            "edit_file".into(),
            risk::ToolType::WriteFile,
            "Edit src/main.rs".into(),
            "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1 +1 @@\n-old\n+new".into(),
        );
        assert!(req.has_diff_content());
        req.scroll_diff_up();
        assert_eq!(req.diff_scroll.scroll_offset, 0);
    }

    #[test]
    fn diff_scroll_clamps_at_max() {
        let content = "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1 +1 @@\n-old\n+new";
        let mut req = ApprovalRequest::new(
            "edit_file".into(),
            risk::ToolType::WriteFile,
            "Edit".into(),
            content.into(),
        );
        // content has 5 lines, visible=2 → max_offset=3
        req.scroll_diff_down(2);
        assert_eq!(req.diff_scroll.scroll_offset, 1);
        req.scroll_diff_down(2);
        assert_eq!(req.diff_scroll.scroll_offset, 2);
        req.scroll_diff_down(2);
        assert_eq!(req.diff_scroll.scroll_offset, 3);
        req.scroll_diff_down(2);
        assert_eq!(req.diff_scroll.scroll_offset, 3, "should clamp at max");
    }

    #[test]
    fn has_diff_content_true_for_git_diff() {
        let req = ApprovalRequest::new(
            "edit_file".into(),
            risk::ToolType::WriteFile,
            "desc".into(),
            "diff --git a/foo b/foo\n--- a/foo\n+++ b/foo".into(),
        );
        assert!(req.has_diff_content());
    }

    #[test]
    fn has_diff_content_false_for_plain_command() {
        let req = ApprovalRequest::new(
            "bash".into(),
            risk::ToolType::Bash,
            "desc".into(),
            "ls -la".into(),
        );
        assert!(!req.has_diff_content());
    }

    #[test]
    fn approval_panel_size_compact_without_diff() {
        let req = ApprovalRequest::new(
            "bash".into(),
            risk::ToolType::Bash,
            "desc".into(),
            "ls -la".into(),
        );
        let size = ratatui::layout::Rect::new(0, 0, 120, 40);
        let (h, w) = approval_panel_size(&req, size);
        assert_eq!(h, 7);
        assert_eq!(w, 70);
    }

    #[test]
    fn approval_panel_size_expanded_with_diff() {
        let req = ApprovalRequest::new(
            "edit_file".into(),
            risk::ToolType::WriteFile,
            "desc".into(),
            "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1 +1 @@\n-old\n+new".into(),
        );
        let size = ratatui::layout::Rect::new(0, 0, 120, 40);
        let (h, w) = approval_panel_size(&req, size);
        // 5 diff lines + 6 overhead = 11, capped at height/2=20 → 11
        assert_eq!(h, 11);
        assert_eq!(w, 80);
    }

    #[test]
    fn approval_state_checks() {
        let mut req = ApprovalRequest::new(
            "bash".into(),
            risk::ToolType::Bash,
            "desc".into(),
            "ls".into(),
        );
        assert!(!req.is_approved());
        req.approve();
        assert!(req.is_approved());
        req.reject();
        assert!(!req.is_approved());
        req.approve_all();
        assert!(req.is_approved());
    }
}
