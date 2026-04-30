//! Skill Palette Component
//!
//! VS Code-style command palette for skills with fuzzy search and keyboard navigation.

// Complete implementation - pending integration with skills UI
#![allow(dead_code)]

use crate::skills::{fuzzy_match, Skill};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
    Frame,
};

/// Skill palette state
pub struct SkillPalette {
    /// All available skills
    skills: Vec<Skill>,

    /// Filtered skills based on search query
    filtered_skills: Vec<Skill>,

    /// Current search query
    query: String,

    /// Selected skill index
    selected: usize,

    /// List state for navigation
    list_state: ListState,

    /// Whether palette is visible
    visible: bool,

    /// Currently selected skill (after Enter)
    selected_skill: Option<Skill>,
}

impl SkillPalette {
    /// Create new skill palette
    pub fn new(skills: Vec<Skill>) -> Self {
        let filtered_skills = skills.clone();

        Self {
            skills,
            filtered_skills,
            query: String::new(),
            selected: 0,
            list_state: ListState::default().with_selected(Some(0)),
            visible: false,
            selected_skill: None,
        }
    }

    /// Open the palette
    pub fn open(&mut self) {
        self.visible = true;
        self.query.clear();
        self.selected = 0;
        self.filter_skills();
        self.list_state.select(Some(0));
    }

    /// Close the palette
    pub fn close(&mut self) {
        self.visible = false;
        self.query.clear();
        self.selected_skill = None;
    }

    /// Check if palette is visible
    pub fn is_visible(&self) -> bool {
        self.visible
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
                self.close();
                true
            }

            KeyCode::Enter => {
                self.select_skill();
                true
            }

            KeyCode::Backspace => {
                if !self.query.is_empty() {
                    self.query.pop();
                    self.filter_skills();
                }
                true
            }

            KeyCode::Down | KeyCode::Char('j') => {
                self.move_selection(1);
                true
            }

            KeyCode::Up | KeyCode::Char('k') => {
                self.move_selection(-1);
                true
            }

            KeyCode::Char('n') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                // Ctrl+N - move down
                self.move_selection(1);
                true
            }

            KeyCode::Char('p') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                // Ctrl+P - move up
                self.move_selection(-1);
                true
            }

            KeyCode::Char(c) => {
                // Handle typing
                self.query.push(c);
                self.filter_skills();
                true
            }

            _ => false,
        }
    }

    /// Get currently selected skill (and clear selection)
    pub fn take_selected(&mut self) -> Option<Skill> {
        self.selected_skill.take()
    }

    /// Render the palette
    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        if !self.visible {
            return;
        }

        // Calculate palette size (60% width, 40% height, centered)
        let width = (area.width * 60 / 100).min(80);
        let height = (area.height * 40 / 100).min(20);

        let x = area.x + (area.width.saturating_sub(width)) / 2;
        let y = area.y + (area.height.saturating_sub(height)) / 2;

        let palette_area = Rect::new(x, y, width, height);

        // Clear the area under the palette
        frame.render_widget(Clear, palette_area);

        // Create layout
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([Constraint::Length(3), Constraint::Min(0)].as_ref())
            .split(palette_area);

        // Render search input
        self.render_search_input(frame, chunks[0]);

        // Render skill list
        self.render_skill_list(frame, chunks[1]);
    }

    /// Render search input
    fn render_search_input(&self, frame: &mut Frame, area: Rect) {
        let input_text = vec![Line::from(vec![
            Span::styled("🔍 ", Style::default().fg(Color::Yellow)),
            Span::raw(&self.query),
        ])];

        let paragraph = Paragraph::new(input_text)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Blue))
                    .title("Skills"),
            )
            .wrap(Wrap { trim: false });

        frame.render_widget(paragraph, area);
    }

    /// Render skill list
    fn render_skill_list(&mut self, frame: &mut Frame, area: Rect) {
        if self.filtered_skills.is_empty() {
            // Show "No skills found" message
            let no_results = Paragraph::new("No skills found")
                .block(Block::default().borders(Borders::ALL))
                .alignment(Alignment::Center)
                .style(Style::default().fg(Color::DarkGray));

            frame.render_widget(no_results, area);
            return;
        }

        // Create list items
        let items: Vec<ListItem> = self
            .filtered_skills
            .iter()
            .enumerate()
            .map(|(i, skill)| {
                let is_selected = i == self.selected;

                // Category icon
                let icon = skill.category.icon();

                // Style based on selection
                let style = if is_selected {
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };

                // Create skill line
                let content = vec![
                    Line::from(vec![
                        Span::styled(format!("{} ", icon), Style::default().fg(Color::Yellow)),
                        Span::styled(&skill.name, style),
                    ]),
                    Line::from(vec![
                        Span::raw("    "),
                        Span::styled(&skill.description, Style::default().fg(Color::DarkGray)),
                    ]),
                ];

                ListItem::new(content)
            })
            .collect();

        // Create list widget
        let list = List::new(items)
            .block(Block::default().borders(Borders::ALL))
            .highlight_style(
                Style::default()
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            );

        frame.render_stateful_widget(list, area, &mut self.list_state);
    }

    /// Filter skills based on query
    fn filter_skills(&mut self) {
        self.filtered_skills = fuzzy_match(&self.query, &self.skills);

        // Reset selection
        self.selected = 0;
        if !self.filtered_skills.is_empty() {
            self.list_state.select(Some(0));
        } else {
            self.list_state.select(None);
        }
    }

    /// Move selection up/down
    fn move_selection(&mut self, direction: isize) {
        if self.filtered_skills.is_empty() {
            return;
        }

        let len = self.filtered_skills.len();
        let new_pos = if direction > 0 {
            (self.selected + 1) % len
        } else {
            self.selected.checked_sub(1).unwrap_or(len - 1)
        };

        self.selected = new_pos;
        self.list_state.select(Some(new_pos));
    }

    /// Select current skill
    fn select_skill(&mut self) {
        if let Some(skill) = self.filtered_skills.get(self.selected) {
            self.selected_skill = Some(skill.clone());
            // Don't close here - let the caller close after taking the selection
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skills::loader::{Skill, SkillCategory};
    use std::path::PathBuf;

    fn create_test_skill(name: &str, description: &str, category: SkillCategory) -> Skill {
        use crate::skills::loader::SkillCommand;

        Skill {
            name: name.to_string(),
            description: description.to_string(),
            category,
            parameters: vec![],
            commands: vec![SkillCommand {
                name: name.to_string(),
                invocation: format!("/{}", name),
                description: description.to_string(),
            }],
            instructions: format!("Use this skill for: {}", description),
            path: PathBuf::from("/test"),
        }
    }

    #[test]
    fn test_palette_creation() {
        let skills = vec![
            create_test_skill("code-review", "Review code", SkillCategory::Agent),
            create_test_skill("tdd-guide", "TDD workflow", SkillCategory::Testing),
        ];

        let palette = SkillPalette::new(skills);
        assert!(!palette.is_visible());
        assert_eq!(palette.filtered_skills.len(), 2);
    }

    #[test]
    fn test_palette_open_close() {
        let skills = vec![];
        let mut palette = SkillPalette::new(skills);

        palette.open();
        assert!(palette.is_visible());

        palette.close();
        assert!(!palette.is_visible());
    }

    #[test]
    fn test_search_filtering() {
        let skills = vec![
            create_test_skill("code-review", "Review code", SkillCategory::Agent),
            create_test_skill("tdd-guide", "TDD workflow", SkillCategory::Testing),
        ];

        let mut palette = SkillPalette::new(skills);
        palette.open();

        palette.query = "code".to_string();
        palette.filter_skills();

        assert_eq!(palette.filtered_skills.len(), 1);
        assert_eq!(palette.filtered_skills[0].name, "code-review");
    }

    #[test]
    fn test_selection_movement() {
        let skills = vec![
            create_test_skill("code-review", "Review code", SkillCategory::Agent),
            create_test_skill("tdd-guide", "TDD workflow", SkillCategory::Testing),
        ];

        let mut palette = SkillPalette::new(skills);
        palette.open();

        assert_eq!(palette.selected, 0);

        palette.move_selection(1);
        assert_eq!(palette.selected, 1);

        palette.move_selection(1); // Should wrap around
        assert_eq!(palette.selected, 0);

        palette.move_selection(-1); // Should wrap to end
        assert_eq!(palette.selected, 1);
    }

    #[test]
    fn test_skill_selection() {
        let skills = vec![
            create_test_skill("code-review", "Review code", SkillCategory::Agent),
            create_test_skill("tdd-guide", "TDD workflow", SkillCategory::Testing),
        ];

        let mut palette = SkillPalette::new(skills);
        palette.open();

        palette.select_skill();

        let selected = palette.take_selected();
        assert!(
            selected.is_some(),
            "Expected skill to be selected but got None"
        );
        assert_eq!(selected.unwrap().name, "code-review");

        // Close after taking selection
        palette.close();
    }
}
