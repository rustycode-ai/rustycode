//! Marketplace browser component

use std::cell::Cell;

use crate::marketplace::client::{fetch_marketplace_index_with_config, RegistryConfig};
use crate::marketplace::index::{ItemType, MarketplaceItem};
use crate::marketplace::installer::{installed_version, is_installed};
use crate::marketplace::registry::RegistryManager;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Tabs, Wrap},
    Frame,
};

/// Marketplace browser actions that should be executed by the TUI event loop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MarketplaceBrowserAction {
    /// Install the selected item.
    Install(String),
    /// Uninstall the selected item.
    Uninstall(String),
    /// Update the selected item.
    Update(String),
}

/// Marketplace browser state.
pub struct MarketplaceBrowser {
    registry: RegistryManager,
    visible: bool,
    query: String,
    selected: usize,
    scroll_offset: usize,
    mode: MarketplaceBrowserMode,
    active_tab: MarketplaceTab,
    viewport_rows: Cell<usize>,
    pending_action: Option<MarketplaceBrowserAction>,
}

/// Browser mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MarketplaceBrowserMode {
    /// List view.
    #[default]
    List,
    /// Detail view.
    Details,
}

/// Browser tabs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MarketplaceTab {
    /// All items.
    #[default]
    All,
    /// Installed items.
    Installed,
    /// Items with updates available.
    Updates,
    /// Skills.
    Skills,
    /// Tools.
    Tools,
    /// MCP servers.
    MCP,
}

impl MarketplaceTab {
    fn all() -> [Self; 6] {
        [
            Self::All,
            Self::Installed,
            Self::Updates,
            Self::Skills,
            Self::Tools,
            Self::MCP,
        ]
    }

    fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Installed => "Installed",
            Self::Updates => "Updates",
            Self::Skills => "Skills",
            Self::Tools => "Tools",
            Self::MCP => "MCP",
        }
    }

    fn next(self) -> Self {
        let tabs = Self::all();
        let idx = tabs.iter().position(|tab| *tab == self).unwrap_or(0);
        tabs[(idx + 1) % tabs.len()]
    }

    fn previous(self) -> Self {
        let tabs = Self::all();
        let idx = tabs.iter().position(|tab| *tab == self).unwrap_or(0);
        tabs[(idx + tabs.len() - 1) % tabs.len()]
    }
}

impl MarketplaceBrowser {
    pub fn new(registry: RegistryManager) -> Self {
        Self {
            registry,
            visible: false,
            query: String::new(),
            selected: 0,
            scroll_offset: 0,
            mode: MarketplaceBrowserMode::List,
            active_tab: MarketplaceTab::All,
            viewport_rows: Cell::new(10),
            pending_action: None,
        }
    }

    /// Open the browser.
    pub fn open(&mut self) {
        self.visible = true;
        self.query.clear();
        self.selected = 0;
        self.scroll_offset = 0;
        self.mode = MarketplaceBrowserMode::List;
        self.active_tab = MarketplaceTab::All;
        self.refresh_builtin_registry();
    }

    /// Close the browser.
    pub fn close(&mut self) {
        self.visible = false;
        self.query.clear();
        self.selected = 0;
        self.scroll_offset = 0;
        self.mode = MarketplaceBrowserMode::List;
        self.pending_action = None;
    }

    /// Check whether the browser is visible.
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Take the next pending action, if any.
    pub fn take_pending_action(&mut self) -> Option<MarketplaceBrowserAction> {
        self.pending_action.take()
    }

    /// Handle keyboard input.
    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        if !self.visible {
            return false;
        }

        match (key.code, key.modifiers) {
            (KeyCode::Esc, _) => {
                if self.mode == MarketplaceBrowserMode::Details {
                    self.mode = MarketplaceBrowserMode::List;
                    self.scroll_offset = 0;
                } else {
                    self.close();
                }
                true
            }
            (KeyCode::Enter, _) => {
                if self.mode == MarketplaceBrowserMode::List {
                    if self.selected_item().is_some() {
                        self.mode = MarketplaceBrowserMode::Details;
                        self.scroll_offset = 0;
                    }
                } else {
                    self.mode = MarketplaceBrowserMode::List;
                    self.scroll_offset = 0;
                }
                true
            }
            (KeyCode::BackTab, _) => {
                self.active_tab = self.active_tab.previous();
                self.reset_selection();
                true
            }
            (KeyCode::Tab, KeyModifiers::CONTROL) => {
                self.active_tab = self.active_tab.next();
                self.reset_selection();
                true
            }
            (KeyCode::Char('l'), KeyModifiers::CONTROL) => {
                self.clear_query();
                true
            }
            (KeyCode::Char('r'), mods) if mods == (KeyModifiers::CONTROL | KeyModifiers::SHIFT) => {
                self.refresh_remote_registry();
                self.reset_selection();
                true
            }
            (KeyCode::Char('r'), KeyModifiers::CONTROL) => {
                if let Some(item) = self.selected_item() {
                    if item.installed && item.has_update() {
                        self.pending_action = Some(MarketplaceBrowserAction::Update(item.id));
                    }
                }
                true
            }
            (KeyCode::Char('i'), KeyModifiers::CONTROL) => {
                if let Some(item) = self.selected_item() {
                    if !item.installed {
                        self.pending_action = Some(MarketplaceBrowserAction::Install(item.id));
                    }
                }
                true
            }
            (KeyCode::Char('u'), KeyModifiers::CONTROL) => {
                if let Some(item) = self.selected_item() {
                    if item.installed {
                        self.pending_action = Some(MarketplaceBrowserAction::Uninstall(item.id));
                    }
                }
                true
            }
            (KeyCode::PageUp, _) => {
                if self.mode == MarketplaceBrowserMode::Details {
                    self.page_up_details();
                } else {
                    self.page_up_list();
                }
                true
            }
            (KeyCode::PageDown, _) => {
                if self.mode == MarketplaceBrowserMode::Details {
                    self.page_down_details();
                } else {
                    self.page_down_list();
                }
                true
            }
            (KeyCode::Home, _) => {
                self.home();
                true
            }
            (KeyCode::End, _) => {
                self.end();
                true
            }
            (KeyCode::Up, _) => {
                self.select_previous();
                true
            }
            (KeyCode::Down, _) => {
                self.select_next();
                true
            }
            (KeyCode::Backspace, _) => {
                self.query.pop();
                self.reset_selection();
                true
            }
            (KeyCode::Char(c), mods) if mods.is_empty() && !c.is_control() => {
                self.query.push(c);
                self.reset_selection();
                true
            }
            _ => false,
        }
    }

    /// Render the browser.
    pub fn render(&self, frame: &mut Frame, area: Rect) {
        if !self.visible {
            return;
        }
        if area.width == 0 || area.height == 0 {
            return;
        }

        let width = ((area.width as usize * 88) / 100)
            .max(48)
            .min(area.width as usize) as u16;
        let height = ((area.height as usize * 84) / 100)
            .max(16)
            .min(area.height as usize) as u16;
        let x = area.x + (area.width.saturating_sub(width)) / 2;
        let y = area.y + (area.height.saturating_sub(height)) / 2;
        let modal_area = Rect::new(x, y, width, height);

        frame.render_widget(Clear, modal_area);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints(
                [
                    Constraint::Length(3),
                    Constraint::Length(3),
                    Constraint::Min(0),
                    Constraint::Length(3),
                ]
                .as_ref(),
            )
            .split(modal_area);

        self.render_search(frame, chunks[0]);
        self.render_tabs(frame, chunks[1]);

        let body_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(54), Constraint::Percentage(46)].as_ref())
            .split(chunks[2]);
        self.set_viewport_rows(body_chunks[0].height.saturating_sub(2) as usize);
        self.render_list(frame, body_chunks[0]);
        self.render_details(frame, body_chunks[1]);

        self.render_footer(frame, chunks[3]);
    }

    fn render_search(&self, frame: &mut Frame, area: Rect) {
        let items = self.filtered_items();
        let installed = items.iter().filter(|item| item.installed).count();
        let updates = items.iter().filter(|item| item.has_update()).count();
        let title = format!(
            " Marketplace  {} items · {} installed · {} updates ",
            items.len(),
            installed,
            updates
        );

        let paragraph = Paragraph::new(Text::from(vec![
            Line::from(vec![
                Span::styled("Search: ", Style::default().fg(Color::DarkGray)),
                Span::styled(self.query.clone(), Style::default().fg(Color::Yellow)),
            ]),
            Line::from(vec![
                Span::styled("Shortcuts: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    "Ctrl+I install  Ctrl+U uninstall  Ctrl+R update  Ctrl+Shift+R refresh",
                    Style::default().fg(Color::Gray),
                ),
            ]),
        ]))
        .block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan)),
        );

        frame.render_widget(paragraph, area);
    }

    fn render_tabs(&self, frame: &mut Frame, area: Rect) {
        let titles = MarketplaceTab::all()
            .into_iter()
            .map(|tab| Line::from(tab.label()))
            .collect::<Vec<_>>();

        let selected = MarketplaceTab::all()
            .iter()
            .position(|tab| *tab == self.active_tab)
            .unwrap_or(0);

        let tabs = Tabs::new(titles)
            .select(selected)
            .block(
                Block::default()
                    .title(" Filters ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::DarkGray)),
            )
            .style(Style::default().fg(Color::Gray))
            .highlight_style(
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            );

        frame.render_widget(tabs, area);
    }

    fn render_list(&self, frame: &mut Frame, area: Rect) {
        let items = self.filtered_items();
        if items.is_empty() {
            let empty = Paragraph::new("No marketplace items found")
                .block(
                    Block::default()
                        .title(" Items ")
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(Color::DarkGray)),
                )
                .alignment(Alignment::Center)
                .style(Style::default().fg(Color::DarkGray));
            frame.render_widget(empty, area);
            return;
        }

        let visible_rows = self.viewport_rows.get().max(1);
        let start = self.scroll_offset.min(items.len().saturating_sub(1));
        let end = (start + visible_rows).min(items.len());
        let selected_rel = self.selected.saturating_sub(start).min(end - start - 1);

        let list_items: Vec<ListItem> = items[start..end]
            .iter()
            .map(|item| {
                let mut lines = vec![Line::from(vec![
                    Span::styled(
                        format!("{} {} ", item.item_type.icon(), item.name),
                        Style::default().fg(Color::White),
                    ),
                    Span::styled(
                        format!(
                            "{} · {} · {} · {}",
                            item.rating_stars(),
                            item.format_downloads(),
                            item.category,
                            item.status_indicator()
                        ),
                        Style::default().fg(Color::DarkGray),
                    ),
                ])];

                if !item.description.is_empty() {
                    lines.push(Line::from(vec![Span::styled(
                        item.description.clone(),
                        Style::default().fg(Color::Gray),
                    )]));
                }

                let mut badges = Vec::new();
                if item.installed {
                    badges.push("installed".to_string());
                }
                if item.has_update() {
                    badges.push("update available".to_string());
                }
                if !badges.is_empty() {
                    lines.push(Line::from(vec![Span::styled(
                        badges.join("  •  "),
                        Style::default().fg(Color::Cyan),
                    )]));
                }

                ListItem::new(Text::from(lines))
            })
            .collect();

        let mut state = ListState::default().with_selected(Some(selected_rel));
        let list = List::new(list_items)
            .block(
                Block::default()
                    .title(" Marketplace Items ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Cyan)),
            )
            .highlight_style(
                Style::default()
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            );

        frame.render_stateful_widget(list, area, &mut state);
    }

    fn render_details(&self, frame: &mut Frame, area: Rect) {
        let Some(item) = self.selected_item() else {
            let paragraph = Paragraph::new("Select an item to see details")
                .block(
                    Block::default()
                        .title(" Details ")
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(Color::DarkGray)),
                )
                .alignment(Alignment::Center)
                .style(Style::default().fg(Color::DarkGray));
            frame.render_widget(paragraph, area);
            return;
        };

        let mut lines = vec![
            Line::from(vec![
                Span::styled("Name: ", Style::default().fg(Color::Cyan)),
                Span::styled(&item.name, Style::default().fg(Color::White)),
            ]),
            Line::from(vec![
                Span::styled("Type: ", Style::default().fg(Color::Cyan)),
                Span::raw(item.item_type.display_name()),
            ]),
            Line::from(vec![
                Span::styled("Category: ", Style::default().fg(Color::Cyan)),
                Span::raw(&item.category),
            ]),
            Line::from(vec![
                Span::styled("Version: ", Style::default().fg(Color::Cyan)),
                Span::raw(&item.version),
            ]),
            Line::from(vec![
                Span::styled("Author: ", Style::default().fg(Color::Cyan)),
                Span::raw(&item.author),
            ]),
            Line::from(vec![
                Span::styled("Rating: ", Style::default().fg(Color::Cyan)),
                Span::raw(format!("{} ({:.1}/5)", item.rating_stars(), item.rating)),
            ]),
            Line::from(vec![
                Span::styled("Downloads: ", Style::default().fg(Color::Cyan)),
                Span::raw(item.format_downloads()),
            ]),
            Line::from(vec![
                Span::styled("Status: ", Style::default().fg(Color::Cyan)),
                Span::styled(
                    if item.installed {
                        if item.has_update() {
                            "Installed, update available"
                        } else {
                            "Installed"
                        }
                    } else {
                        "Not installed"
                    },
                    Style::default().fg(if item.installed {
                        Color::Green
                    } else {
                        Color::Red
                    }),
                ),
            ]),
        ];

        if let Some(installed_version) = &item.installed_version {
            lines.push(Line::from(vec![
                Span::styled("Installed version: ", Style::default().fg(Color::Cyan)),
                Span::raw(installed_version),
            ]));
        }

        if let Some(homepage) = &item.homepage {
            lines.push(Line::from(vec![
                Span::styled("Homepage: ", Style::default().fg(Color::Cyan)),
                Span::raw(homepage),
            ]));
        }

        if !item.tags.is_empty() {
            lines.push(Line::from(vec![
                Span::styled("Tags: ", Style::default().fg(Color::Cyan)),
                Span::raw(item.tags.join(", ")),
            ]));
        }

        if !item.dependencies.is_empty() {
            lines.push(Line::from(vec![
                Span::styled("Dependencies: ", Style::default().fg(Color::Cyan)),
                Span::raw(item.dependencies.join(", ")),
            ]));
        }

        if let Some(min_version) = &item.min_compatible_version {
            lines.push(Line::from(vec![
                Span::styled("Min compatible: ", Style::default().fg(Color::Cyan)),
                Span::raw(min_version),
            ]));
        }

        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("Description: ", Style::default().fg(Color::Cyan)),
            Span::raw(&item.description),
        ]));

        let action_hint = if item.installed {
            if item.has_update() {
                "Ctrl+R update  Ctrl+U uninstall"
            } else {
                "Ctrl+U uninstall"
            }
        } else {
            "Ctrl+I install"
        };

        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("Actions: ", Style::default().fg(Color::Cyan)),
            Span::raw(action_hint),
        ]));
        lines.push(Line::from(vec![
            Span::styled("Refresh: ", Style::default().fg(Color::Cyan)),
            Span::raw("Ctrl+Shift+R"),
        ]));

        let paragraph = Paragraph::new(Text::from(lines))
            .block(
                Block::default()
                    .title(" Details ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Cyan)),
            )
            .wrap(Wrap { trim: false })
            .scroll((self.scroll_offset.min(u16::MAX as usize) as u16, 0));

        frame.render_widget(paragraph, area);
    }

    fn render_footer(&self, frame: &mut Frame, area: Rect) {
        let help_text = match self.mode {
            MarketplaceBrowserMode::List => {
                "Esc:Close  Enter:Details  Ctrl+Tab:Next tab  Shift+Tab:Prev tab  Ctrl+I:Install  Ctrl+U:Uninstall  Ctrl+R:Update  Ctrl+Shift+R:Refresh  Ctrl+L:Clear"
            }
            MarketplaceBrowserMode::Details => {
                "Esc:Back  Ctrl+Tab:Next tab  Shift+Tab:Prev tab  PgUp/PgDn:Scroll  Ctrl+I:Install  Ctrl+U:Uninstall  Ctrl+R:Update  Ctrl+L:Clear"
            }
        };

        let paragraph = Paragraph::new(help_text)
            .style(Style::default().fg(Color::Gray))
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL));

        frame.render_widget(paragraph, area);
    }

    fn filtered_items(&self) -> Vec<MarketplaceItem> {
        let query = self.query.trim().to_lowercase();
        let mut items: Vec<MarketplaceItem> = self
            .registry
            .items()
            .iter()
            .filter(|item| self.matches_tab(item))
            .filter(|item| self.matches_query(item, &query))
            .cloned()
            .map(|mut item| {
                item.installed = is_installed(&item);
                item.installed_version = installed_version(&item);
                item
            })
            .collect();

        items.sort_by(|a, b| {
            b.installed
                .cmp(&a.installed)
                .then_with(|| b.has_update().cmp(&a.has_update()))
                .then_with(|| {
                    b.rating
                        .partial_cmp(&a.rating)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .then_with(|| b.downloads.cmp(&a.downloads))
                .then_with(|| a.name.cmp(&b.name))
        });

        items
    }

    fn matches_tab(&self, item: &MarketplaceItem) -> bool {
        match self.active_tab {
            MarketplaceTab::All => true,
            MarketplaceTab::Installed => item.installed,
            MarketplaceTab::Updates => item.installed && item.has_update(),
            MarketplaceTab::Skills => item.item_type == ItemType::Skill,
            MarketplaceTab::Tools => item.item_type == ItemType::Tool,
            MarketplaceTab::MCP => item.item_type == ItemType::MCP,
        }
    }

    fn matches_query(&self, item: &MarketplaceItem, query: &str) -> bool {
        if query.is_empty() {
            return true;
        }

        let haystacks = [
            item.id.to_lowercase(),
            item.name.to_lowercase(),
            item.description.to_lowercase(),
            item.category.to_lowercase(),
            item.author.to_lowercase(),
            item.version.to_lowercase(),
            item.url.to_lowercase(),
            item.tags.join(" ").to_lowercase(),
            item.dependencies.join(" ").to_lowercase(),
        ];

        haystacks.iter().any(|haystack| haystack.contains(query))
    }

    pub fn selected_item(&self) -> Option<MarketplaceItem> {
        let items = self.filtered_items();
        items.get(self.selected).cloned()
    }

    fn reset_selection(&mut self) {
        self.selected = 0;
        self.scroll_offset = 0;
        self.mode = MarketplaceBrowserMode::List;
    }

    fn select_previous(&mut self) {
        let count = self.filtered_items().len();
        if count == 0 {
            self.reset_selection();
            return;
        }

        if self.selected > 0 {
            self.selected -= 1;
        }
        self.ensure_visible(count);
    }

    fn select_next(&mut self) {
        let count = self.filtered_items().len();
        if count == 0 {
            self.reset_selection();
            return;
        }

        if self.selected < count.saturating_sub(1) {
            self.selected += 1;
        }
        self.ensure_visible(count);
    }

    fn home(&mut self) {
        self.selected = 0;
        self.scroll_offset = 0;
    }

    fn end(&mut self) {
        let count = self.filtered_items().len();
        if count == 0 {
            self.reset_selection();
            return;
        }
        self.selected = count - 1;
        self.ensure_visible(count);
    }

    fn page_up_list(&mut self) {
        let rows = self.viewport_rows.get().max(1);
        self.selected = self.selected.saturating_sub(rows);
        self.scroll_offset = self.scroll_offset.saturating_sub(rows);
    }

    fn page_down_list(&mut self) {
        let rows = self.viewport_rows.get().max(1);
        self.selected = self.selected.saturating_add(rows);
        self.scroll_offset = self.scroll_offset.saturating_add(rows);
    }

    fn page_up_details(&mut self) {
        let rows = self.viewport_rows.get().max(1);
        self.scroll_offset = self.scroll_offset.saturating_sub(rows);
    }

    fn page_down_details(&mut self) {
        let rows = self.viewport_rows.get().max(1);
        self.scroll_offset = self.scroll_offset.saturating_add(rows);
    }

    fn ensure_visible(&mut self, count: usize) {
        let rows = self.viewport_rows.get().max(1);
        if count == 0 {
            self.reset_selection();
            return;
        }

        if self.selected >= count {
            self.selected = count - 1;
        }

        if self.selected < self.scroll_offset {
            self.scroll_offset = self.selected;
        } else if self.selected >= self.scroll_offset + rows {
            self.scroll_offset = self.selected - rows + 1;
        }

        let max_scroll = count.saturating_sub(rows);
        if self.scroll_offset > max_scroll {
            self.scroll_offset = max_scroll;
        }
    }

    fn clear_query(&mut self) {
        self.query.clear();
        self.reset_selection();
    }

    fn set_viewport_rows(&self, rows: usize) {
        self.viewport_rows.set(rows.max(1));
    }

    fn refresh_builtin_registry(&mut self) {
        self.reload_registry(RegistryConfig::builtin_only());
    }

    fn refresh_remote_registry(&mut self) {
        self.reload_registry(RegistryConfig::default());
    }

    fn reload_registry(&mut self, config: RegistryConfig) {
        match rustycode_shared_runtime::block_on_shared(fetch_marketplace_index_with_config(config))
        {
            Ok(items) => {
                self.registry = RegistryManager::new(items);
                self.reset_selection();
            }
            Err(_) => {
                // Keep the existing registry if refresh fails.
            }
        }
    }
}

impl Default for MarketplaceBrowser {
    fn default() -> Self {
        Self::new(RegistryManager::new(vec![]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_browser_defaults() {
        let browser = MarketplaceBrowser::default();
        assert!(!browser.visible);
        assert_eq!(browser.active_tab, MarketplaceTab::All);
        assert_eq!(browser.mode, MarketplaceBrowserMode::List);
    }

    #[test]
    fn test_tab_navigation() {
        assert_eq!(MarketplaceTab::All.next(), MarketplaceTab::Installed);
        assert_eq!(MarketplaceTab::Installed.previous(), MarketplaceTab::All);
    }
}
