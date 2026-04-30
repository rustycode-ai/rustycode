//! Parameter input dialog for skills
//!
//! Provides input dialog for skills that require parameters.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};

/// Parameter input dialog state
pub struct ParamInput {
    /// Skill name
    skill_name: String,

    /// Parameter name
    param_name: String,

    /// Current input value
    input: String,

    /// Whether dialog is visible
    visible: bool,

    /// Input completed (user pressed Enter)
    completed: bool,

    /// Input cancelled (user pressed Esc)
    cancelled: bool,
}

impl ParamInput {
    /// Create new parameter input dialog
    pub fn new(skill_name: String, param_name: String) -> Self {
        Self {
            skill_name,
            param_name,
            input: String::new(),
            visible: false,
            completed: false,
            cancelled: false,
        }
    }

    /// Open the dialog
    pub fn open(&mut self) {
        self.visible = true;
        self.input.clear();
        self.completed = false;
        self.cancelled = false;
    }

    /// Close the dialog
    pub fn close(&mut self) {
        self.visible = false;
        self.completed = false;
        self.cancelled = false;
    }

    /// Check if dialog is visible
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Check if input was completed
    pub fn is_completed(&self) -> bool {
        self.completed
    }

    /// Check if input was cancelled
    pub fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    /// Get input value (and clear it)
    pub fn take_input(&mut self) -> Option<String> {
        if self.completed {
            Some(self.input.clone())
        } else {
            None
        }
    }

    /// Handle keyboard input
    ///
    /// Returns true if input was handled, false otherwise
    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        if !self.visible {
            return false;
        }

        match key.code {
            KeyCode::Esc => {
                self.cancelled = true;
                self.completed = false;
                self.visible = false;
                true
            }

            KeyCode::Enter => {
                if !self.input.is_empty() {
                    self.completed = true;
                    self.cancelled = false;
                    self.visible = false;
                }
                true
            }

            // Ctrl+W - delete word
            KeyCode::Char('w') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.delete_word();
                true
            }

            // Ctrl+U - delete to start
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.input.clear();
                true
            }

            KeyCode::Char(c) => {
                self.input.push(c);
                true
            }

            KeyCode::Backspace => {
                if !self.input.is_empty() {
                    self.input.pop();
                }
                true
            }

            _ => false,
        }
    }

    /// Render the dialog
    pub fn render(&self, frame: &mut Frame, area: Rect) {
        if !self.visible {
            return;
        }

        // Calculate dialog size (50% width, centered, minimal height)
        let width = (area.width * 50 / 100).min(60);
        let height = 10;

        let x = area.x + (area.width.saturating_sub(width)) / 2;
        let y = area.y + (area.height.saturating_sub(height)) / 2;

        let dialog_area = Rect::new(x, y, width, height);

        // Clear the area under the dialog
        frame.render_widget(Clear, dialog_area);

        // Create layout
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints(
                [
                    Constraint::Length(2), // Title
                    Constraint::Length(3), // Input field
                    Constraint::Length(2), // Help text
                ]
                .as_ref(),
            )
            .split(dialog_area);

        // Render title
        self.render_title(frame, chunks[0]);

        // Render input field
        self.render_input(frame, chunks[1]);

        // Render help text
        self.render_help(frame, chunks[2]);
    }

    /// Render dialog title
    fn render_title(&self, frame: &mut Frame, area: Rect) {
        let title = vec![
            Line::from(vec![
                Span::styled("🔧 ", Style::default().fg(Color::Yellow)),
                Span::styled(
                    &self.skill_name,
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(""),
            Line::from(vec![Span::styled(
                format!("{}:", self.param_name),
                Style::default().fg(Color::White),
            )]),
        ];

        let paragraph = Paragraph::new(title)
            .block(Block::default().borders(Borders::ALL))
            .alignment(Alignment::Center);

        frame.render_widget(paragraph, area);
    }

    /// Render input field
    fn render_input(&self, frame: &mut Frame, area: Rect) {
        let input_text = vec![Line::from(vec![
            Span::styled("> ", Style::default().fg(Color::Green)),
            Span::styled(&self.input, Style::default().fg(Color::White)),
            Span::styled(" ", Style::default().fg(Color::White)), // Cursor placeholder
        ])];

        let paragraph = Paragraph::new(input_text)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Blue)),
            )
            .wrap(Wrap { trim: false });

        frame.render_widget(paragraph, area);
    }

    /// Render help text
    fn render_help(&self, frame: &mut Frame, area: Rect) {
        let help = vec![Line::from(vec![
            Span::styled("[", Style::default().fg(Color::DarkGray)),
            Span::styled("Enter", Style::default().fg(Color::Green)),
            Span::styled("] ", Style::default().fg(Color::DarkGray)),
            Span::styled("Run", Style::default().fg(Color::White)),
            Span::styled("  ", Style::default()),
            Span::styled("[", Style::default().fg(Color::DarkGray)),
            Span::styled("Esc", Style::default().fg(Color::Red)),
            Span::styled("] ", Style::default().fg(Color::DarkGray)),
            Span::styled("Cancel", Style::default().fg(Color::White)),
        ])];

        let paragraph = Paragraph::new(help)
            .block(Block::default().borders(Borders::ALL))
            .alignment(Alignment::Center);

        frame.render_widget(paragraph, area);
    }

    /// Delete word (Ctrl+W)
    fn delete_word(&mut self) {
        // Find last delimiter (space or /) and delete from after it
        let last_slash = self.input.rfind('/');
        let last_space = self.input.rfind(' ');

        // Use the rightmost delimiter
        let pos = match (last_slash, last_space) {
            (Some(slash), Some(space)) => Some(slash.max(space)),
            (Some(pos), None) | (None, Some(pos)) => Some(pos),
            (None, None) => None,
        };

        if let Some(pos) = pos {
            // Check if there's content after the delimiter
            let has_content_after = self.input[pos + 1..].chars().any(|c| c != ' ');
            if has_content_after {
                // Keep the delimiter and everything before it
                self.input.truncate(pos + 1);
            } else {
                // Nothing after the delimiter, clear all
                self.input.clear();
            }
        } else {
            // No delimiter found, clear all
            self.input.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_param_input_creation() {
        let input = ParamInput::new("code-review".to_string(), "target".to_string());
        assert!(!input.is_visible());
        assert!(!input.is_completed());
        assert!(!input.is_cancelled());
    }

    #[test]
    fn test_param_input_open_close() {
        let mut input = ParamInput::new("code-review".to_string(), "target".to_string());

        input.open();
        assert!(input.is_visible());

        input.close();
        assert!(!input.is_visible());
    }

    #[test]
    fn test_text_input() {
        let mut input = ParamInput::new("code-review".to_string(), "target".to_string());
        input.open();

        // Test character input
        input.handle_key(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE));
        assert_eq!(input.input, "h");

        input.handle_key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE));
        assert_eq!(input.input, "he");

        // Test backspace
        input.handle_key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
        assert_eq!(input.input, "h");
    }

    #[test]
    fn test_enter_completion() {
        let mut input = ParamInput::new("code-review".to_string(), "target".to_string());
        input.open();

        // Empty input should not complete
        input.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(!input.is_completed());

        // Add text and complete
        input.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE));
        input.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(input.is_completed());
        assert_eq!(input.take_input(), Some("s".to_string()));
    }

    #[test]
    fn test_escape_cancellation() {
        let mut input = ParamInput::new("code-review".to_string(), "target".to_string());
        input.open();

        input.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE));
        input.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));

        assert!(input.is_cancelled());
        assert!(!input.is_completed());
        assert!(input.take_input().is_none());
    }

    #[test]
    fn test_ctrl_w_delete_word() {
        let mut input = ParamInput::new("code-review".to_string(), "target".to_string());
        input.open();

        input.input = "src/main.rs".to_string();

        // Delete word
        input.handle_key(KeyEvent::new(KeyCode::Char('w'), KeyModifiers::CONTROL));
        assert_eq!(input.input, "src/");

        // Delete another word
        input.handle_key(KeyEvent::new(KeyCode::Char('w'), KeyModifiers::CONTROL));
        assert_eq!(input.input, "");
    }

    #[test]
    fn test_ctrl_u_delete_to_start() {
        let mut input = ParamInput::new("code-review".to_string(), "target".to_string());
        input.open();

        input.input = "some text".to_string();

        // Delete to start
        input.handle_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
        assert_eq!(input.input, "");
    }
}
