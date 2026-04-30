//! File Selector Component
//! Provides an overlay for selecting files from the project workspace.

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, List, ListItem, ListState},
    Frame,
};

pub struct FileSelector {
    files: Vec<String>,
    state: ListState,
    visible: bool,
    filter: String,
}

impl FileSelector {
    pub fn new(files: Vec<String>) -> Self {
        Self {
            files,
            state: ListState::default(),
            visible: false,
            filter: String::new(),
        }
    }

    pub fn show(&mut self) {
        self.visible = true;
        self.state.select(Some(0));
    }

    pub fn hide(&mut self) {
        self.visible = false;
    }

    pub fn is_visible(&self) -> bool {
        self.visible
    }

    pub fn take_selected(&mut self) -> Option<String> {
        let selected = self.state.selected()?;
        let filtered: Vec<String> = self
            .files
            .iter()
            .filter(|f| f.contains(&self.filter))
            .cloned()
            .collect();
        filtered.get(selected).cloned()
    }

    pub fn handle_key(&mut self, key: crossterm::event::KeyEvent) {
        use crossterm::event::KeyCode;
        let filtered: Vec<String> = self
            .files
            .iter()
            .filter(|f| f.contains(&self.filter))
            .cloned()
            .collect();

        match key.code {
            KeyCode::Up => {
                let i = self.state.selected().unwrap_or(0);
                self.state.select(Some(i.saturating_sub(1)));
            }
            KeyCode::Down => {
                let i = self.state.selected().unwrap_or(0);
                if i < filtered.len().saturating_sub(1) {
                    self.state.select(Some(i + 1));
                }
            }
            KeyCode::PageUp => {
                let i = self.state.selected().unwrap_or(0);
                self.state.select(Some(i.saturating_sub(10)));
            }
            KeyCode::PageDown => {
                let i = self.state.selected().unwrap_or(0);
                self.state
                    .select(Some((i + 10).min(filtered.len().saturating_sub(1))));
            }
            KeyCode::Char(c) => {
                self.filter.push(c);
                self.state.select(Some(0));
            }
            KeyCode::Backspace => {
                self.filter.pop();
                self.state.select(Some(0));
            }
            _ => {}
        }
    }

    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        if !self.visible {
            return;
        }

        let area = centered_rect(50, 60, area);
        let items: Vec<ListItem> = self
            .files
            .iter()
            .filter(|f| f.contains(&self.filter))
            .map(|f| ListItem::new(f.as_str()))
            .collect();

        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Select File (@) "),
            )
            .highlight_style(
                Style::default()
                    .bg(Color::Blue)
                    .add_modifier(Modifier::BOLD),
            );

        frame.render_stateful_widget(list, area, &mut self.state);
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let px = percent_x.min(100);
    let py = percent_y.min(100);
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - py) / 2),
            Constraint::Percentage(py),
            Constraint::Percentage((100 - py) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - px) / 2),
            Constraint::Percentage(px),
            Constraint::Percentage((100 - px) / 2),
        ])
        .split(popup_layout[1])[1]
}
