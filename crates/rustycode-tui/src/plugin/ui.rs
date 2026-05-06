//! Plugin manager UI

use std::cell::Cell;
use std::sync::{Arc, RwLock};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
    Frame,
};

use super::manager::PluginManager;

/// Plugin manager UI state
pub struct PluginManagerUI {
    pub visible: bool,

    /// Search query
    pub query: String,

    /// Currently selected plugin index within the filtered list
    pub selected_index: usize,

    /// Scroll offset for list/details paging
    pub scroll_offset: usize,

    /// Current mode (list/details)
    pub mode: PluginManagerMode,

    /// Number of visible rows in the list/details pane
    viewport_rows: Cell<usize>,
}

/// Plugin manager display mode
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum PluginManagerMode {
    /// List view
    #[default]
    List,

    /// Detail view
    Details,
}

impl PluginManagerUI {
    /// Create new plugin manager UI
    pub fn new() -> Self {
        Self {
            visible: false,
            query: String::new(),
            selected_index: 0,
            scroll_offset: 0,
            mode: PluginManagerMode::List,
            viewport_rows: Cell::new(10),
        }
    }

    /// Show the UI.
    pub fn show(&mut self) {
        self.visible = true;
        self.query.clear();
        self.selected_index = 0;
        self.scroll_offset = 0;
        self.mode = PluginManagerMode::List;
    }

    /// Hide the UI.
    pub fn hide(&mut self) {
        self.visible = false;
    }

    /// Toggle visibility.
    pub fn toggle(&mut self) {
        if self.visible {
            self.hide();
        } else {
            self.show();
        }
    }

    /// Check whether the UI is visible.
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    pub fn set_query(&mut self, query: impl Into<String>) {
        self.query = query.into();
        self.selected_index = 0;
        self.scroll_offset = 0;
    }

    /// Clear the search query.
    pub fn clear_query(&mut self) {
        self.query.clear();
        self.selected_index = 0;
        self.scroll_offset = 0;
    }

    /// Handle keyboard input.
    pub fn handle_key(&mut self, key: KeyEvent, manager: &Arc<RwLock<PluginManager>>) -> bool {
        if !self.visible {
            return false;
        }

        let mut manager = manager.write().unwrap_or_else(|e| e.into_inner());

        match (key.code, key.modifiers) {
            (KeyCode::Esc, _) => {
                if self.mode == PluginManagerMode::Details {
                    self.mode = PluginManagerMode::List;
                    self.scroll_offset = 0;
                } else {
                    self.hide();
                }
                true
            }
            (KeyCode::Enter, _) => {
                if self.mode == PluginManagerMode::List {
                    if self.selected_plugin_name(&manager).is_some() {
                        self.mode = PluginManagerMode::Details;
                        self.scroll_offset = 0;
                    }
                } else {
                    self.mode = PluginManagerMode::List;
                    self.scroll_offset = 0;
                }
                true
            }
            (KeyCode::Backspace, _) => {
                self.query.pop();
                self.reset_selection();
                true
            }
            (KeyCode::Char('l'), KeyModifiers::CONTROL) => {
                self.clear_query();
                true
            }
            (KeyCode::Char('e'), KeyModifiers::CONTROL) => {
                self.with_selected_plugin_mut(&mut manager, |plugin| {
                    plugin.enabled = true;
                });
                true
            }
            (KeyCode::Char('d'), KeyModifiers::CONTROL) => {
                self.with_selected_plugin_mut(&mut manager, |plugin| {
                    plugin.enabled = false;
                });
                true
            }
            (KeyCode::Char('r'), mods) if mods == (KeyModifiers::CONTROL | KeyModifiers::SHIFT) => {
                let _ = manager.reload_from_disk();
                self.reset_selection();
                true
            }
            (KeyCode::Char('r'), KeyModifiers::CONTROL) => {
                self.with_selected_plugin_name(&mut manager, |plugin_manager, name| {
                    let _ = plugin_manager.update_plugin(&name);
                });
                true
            }
            (KeyCode::Char('u'), mods) if mods == (KeyModifiers::CONTROL | KeyModifiers::SHIFT) => {
                self.with_selected_plugin_name(&mut manager, |plugin_manager, name| {
                    let _ = plugin_manager.uninstall_plugin(&name);
                });
                self.reset_selection();
                true
            }
            (KeyCode::PageUp, _) => {
                self.page_up();
                true
            }
            (KeyCode::PageDown, _) => {
                self.page_down();
                true
            }
            (KeyCode::Home, _) => {
                self.home();
                true
            }
            (KeyCode::End, _) => {
                self.end(&manager);
                true
            }
            (KeyCode::Up, _) => {
                self.select_previous(&manager);
                true
            }
            (KeyCode::Down, _) => {
                self.select_next(&manager);
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

    /// Render the plugin manager UI.
    pub fn render(&self, frame: &mut Frame, area: Rect, manager: &PluginManager) {
        if !self.visible {
            return;
        }

        let width = ((area.width as usize * 86) / 100)
            .max(32)
            .min(area.width as usize) as u16;
        let height = ((area.height as usize * 78) / 100)
            .max(12)
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
                    Constraint::Min(0),
                    Constraint::Length(3),
                ]
                .as_ref(),
            )
            .split(modal_area);

        self.render_search(frame, chunks[0], manager);

        let body_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(52), Constraint::Percentage(48)].as_ref())
            .split(chunks[1]);
        self.set_viewport_rows(body_chunks[0].height.saturating_sub(2) as usize);
        self.render_list(frame, body_chunks[0], manager);
        self.render_details(frame, body_chunks[1], manager);

        self.render_footer(frame, chunks[2]);
    }

    fn render_search(&self, frame: &mut Frame, area: Rect, manager: &PluginManager) {
        let total = self.sorted_filtered_plugins(manager).len();
        let current = if total == 0 {
            0
        } else {
            self.selected_index.min(total.saturating_sub(1)) + 1
        };
        let title = format!(" Plugin Manager  {}/{} ", current, total.max(1));
        let paragraph = Paragraph::new(Text::from(vec![
            Line::from(vec![
                Span::styled("Search: ", Style::default().fg(Color::DarkGray)),
                Span::styled(self.query.clone(), Style::default().fg(Color::Yellow)),
            ]),
            Line::from(vec![
                Span::styled("Source: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    "/plugin open | /plugin install <source>",
                    Style::default().fg(Color::Gray),
                ),
            ]),
        ]))
        .block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan)),
        )
        .wrap(Wrap { trim: false });

        frame.render_widget(paragraph, area);
    }

    fn render_list(&self, frame: &mut Frame, area: Rect, manager: &PluginManager) {
        let plugins = self.sorted_filtered_plugins(manager);
        if plugins.is_empty() {
            let empty = Paragraph::new("No plugins found")
                .block(
                    Block::default()
                        .title(" Plugins ")
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(Color::DarkGray)),
                )
                .alignment(Alignment::Center)
                .style(Style::default().fg(Color::DarkGray));
            frame.render_widget(empty, area);
            return;
        }

        let visible_rows = self.viewport_rows.get().max(1);
        let start = self.scroll_offset.min(plugins.len().saturating_sub(1));
        let end = (start + visible_rows).min(plugins.len());
        let selected_rel = self
            .selected_index
            .saturating_sub(start)
            .min(end - start - 1);

        let items: Vec<ListItem> = plugins[start..end]
            .iter()
            .map(|plugin| {
                let status = if plugin.enabled {
                    "enabled"
                } else {
                    "disabled"
                };
                let command_count = plugin.manifest.slash_commands.len();
                let mut lines = vec![Line::from(vec![
                    Span::styled(
                        format!("{} ", plugin.manifest.name),
                        Style::default().fg(Color::White),
                    ),
                    Span::styled(
                        format!(
                            "v{} • {} • {} cmd{}",
                            plugin.manifest.version,
                            status,
                            command_count,
                            if command_count == 1 { "" } else { "s" }
                        ),
                        Style::default().fg(Color::DarkGray),
                    ),
                ])];

                if !plugin.manifest.description.is_empty() {
                    lines.push(Line::from(vec![Span::styled(
                        plugin.manifest.description.clone(),
                        Style::default().fg(Color::Gray),
                    )]));
                }

                if plugin.install_source.is_some() || plugin.updated_at.is_some() {
                    let mut meta_bits = Vec::new();
                    if let Some(source) = &plugin.install_source {
                        meta_bits.push(format!("source {}", source));
                    }
                    if let Some(updated_at) = &plugin.updated_at {
                        meta_bits.push(format!("updated {}", updated_at));
                    }
                    lines.push(Line::from(vec![Span::styled(
                        meta_bits.join("  •  "),
                        Style::default().fg(Color::DarkGray),
                    )]));
                }

                ListItem::new(lines)
            })
            .collect();

        let mut state = ListState::default().with_selected(Some(selected_rel));
        let list = List::new(items)
            .block(
                Block::default()
                    .title(" Installed Plugins ")
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

    fn render_details(&self, frame: &mut Frame, area: Rect, manager: &PluginManager) {
        let Some(plugin_name) = self.selected_plugin_name(manager) else {
            let paragraph = Paragraph::new("Select a plugin to see details")
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

        let Some(plugin) = manager.plugin(&plugin_name) else {
            return;
        };

        let mut lines = vec![
            Line::from(vec![
                Span::styled("Name: ", Style::default().fg(Color::Cyan)),
                Span::styled(&plugin.manifest.name, Style::default().fg(Color::White)),
            ]),
            Line::from(vec![
                Span::styled("Version: ", Style::default().fg(Color::Cyan)),
                Span::raw(&plugin.manifest.version),
            ]),
            Line::from(vec![
                Span::styled("Author: ", Style::default().fg(Color::Cyan)),
                Span::raw(&plugin.manifest.author),
            ]),
            Line::from(vec![
                Span::styled("Status: ", Style::default().fg(Color::Cyan)),
                Span::styled(
                    if plugin.enabled {
                        "Enabled"
                    } else {
                        "Disabled"
                    },
                    Style::default().fg(if plugin.enabled {
                        Color::Green
                    } else {
                        Color::Red
                    }),
                ),
            ]),
            Line::from(vec![
                Span::styled("Type: ", Style::default().fg(Color::Cyan)),
                Span::raw(format!("{:?}", plugin.manifest.plugin_type)),
            ]),
            Line::from(vec![
                Span::styled("Description: ", Style::default().fg(Color::Cyan)),
                Span::raw(&plugin.manifest.description),
            ]),
            Line::from(""),
        ];

        if let Some(source) = &plugin.install_source {
            lines.push(Line::from(vec![
                Span::styled("Source: ", Style::default().fg(Color::Cyan)),
                Span::raw(source),
            ]));
        }
        if let Some(installed_at) = &plugin.installed_at {
            lines.push(Line::from(vec![
                Span::styled("Installed: ", Style::default().fg(Color::Cyan)),
                Span::raw(installed_at),
            ]));
        }
        if let Some(updated_at) = &plugin.updated_at {
            lines.push(Line::from(vec![
                Span::styled("Updated: ", Style::default().fg(Color::Cyan)),
                Span::raw(updated_at),
            ]));
        }

        if !plugin.manifest.permissions.is_empty() {
            lines.push(Line::from(vec![Span::styled(
                "Permissions:",
                Style::default().fg(Color::Cyan),
            )]));
            for perm in plugin.permissions.describe() {
                lines.push(Line::from(vec![Span::raw("  • "), Span::raw(perm)]));
            }
            lines.push(Line::from(""));
        }

        if !plugin.manifest.slash_commands.is_empty() {
            lines.push(Line::from(vec![Span::styled(
                "Commands:",
                Style::default().fg(Color::Cyan),
            )]));
            for cmd in &plugin.manifest.slash_commands {
                lines.push(Line::from(vec![
                    Span::raw("  • /"),
                    Span::styled(&cmd.name, Style::default().fg(Color::Yellow)),
                    Span::raw(": "),
                    Span::raw(&cmd.description),
                ]));
            }
            lines.push(Line::from(""));
        }

        lines.push(Line::from(vec![
            Span::styled("Actions: ", Style::default().fg(Color::Cyan)),
            Span::raw("Ctrl+E enable  Ctrl+D disable  Ctrl+U uninstall  Ctrl+R update"),
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
            .scroll((self.scroll_offset as u16, 0));

        frame.render_widget(paragraph, area);
    }

    fn render_footer(&self, frame: &mut Frame, area: Rect) {
        let help_text = match self.mode {
            PluginManagerMode::List => {
                "Esc:Close  Enter:Details  Ctrl+E:Enable  Ctrl+D:Disable  Ctrl+U:Uninstall  Ctrl+R:Update  Ctrl+Shift+R:Reload  Ctrl+L:Clear"
            }
            PluginManagerMode::Details => {
                "Esc:Back  Ctrl+E:Enable  Ctrl+D:Disable  Ctrl+U:Uninstall  Ctrl+R:Update  PgUp/PgDn:Scroll  Ctrl+L:Clear"
            }
        };

        let paragraph = Paragraph::new(help_text)
            .style(Style::default().fg(Color::Gray))
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL));

        frame.render_widget(paragraph, area);
    }

    fn plugin_matches(&self, plugin: &super::manager::Plugin, query: &str) -> bool {
        if query.is_empty() {
            return true;
        }

        let haystacks = [
            plugin.manifest.name.to_lowercase(),
            plugin.manifest.description.to_lowercase(),
            plugin.manifest.author.to_lowercase(),
            format!("{:?}", plugin.manifest.plugin_type).to_lowercase(),
            plugin
                .manifest
                .slash_commands
                .iter()
                .map(|cmd| format!("{} {}", cmd.name, cmd.description).to_lowercase())
                .collect::<Vec<_>>()
                .join(" "),
            plugin
                .permissions
                .describe()
                .into_iter()
                .map(|perm| perm.to_lowercase())
                .collect::<Vec<_>>()
                .join(" "),
        ];

        haystacks.iter().any(|haystack| haystack.contains(query))
    }

    fn sorted_filtered_plugins<'a>(
        &self,
        manager: &'a PluginManager,
    ) -> Vec<&'a super::manager::Plugin> {
        let query = self.query.trim().to_lowercase();
        let mut plugins: Vec<&super::manager::Plugin> = manager
            .plugins()
            .into_iter()
            .filter(|plugin| self.plugin_matches(plugin, &query))
            .collect();

        plugins.sort_by(|a, b| {
            b.enabled
                .cmp(&a.enabled)
                .then_with(|| {
                    b.updated_at
                        .as_deref()
                        .unwrap_or("")
                        .cmp(a.updated_at.as_deref().unwrap_or(""))
                })
                .then_with(|| {
                    b.installed_at
                        .as_deref()
                        .unwrap_or("")
                        .cmp(a.installed_at.as_deref().unwrap_or(""))
                })
                .then_with(|| a.manifest.name.cmp(&b.manifest.name))
        });

        plugins
    }

    fn selected_plugin_name(&self, manager: &PluginManager) -> Option<String> {
        let plugins = self.sorted_filtered_plugins(manager);
        plugins
            .get(self.selected_index)
            .map(|plugin| plugin.manifest.name.clone())
    }

    fn with_selected_plugin_mut(
        &mut self,
        manager: &mut PluginManager,
        action: impl FnOnce(&mut super::manager::Plugin),
    ) {
        let Some(name) = self.selected_plugin_name(manager) else {
            return;
        };

        if let Some(plugin) = manager.plugin_mut(&name) {
            action(plugin);
        }
    }

    fn with_selected_plugin_name(
        &mut self,
        manager: &mut PluginManager,
        action: impl FnOnce(&mut PluginManager, String),
    ) {
        let Some(name) = self.selected_plugin_name(manager) else {
            return;
        };
        action(manager, name);
    }

    fn select_previous(&mut self, manager: &PluginManager) {
        let count = self.sorted_filtered_plugins(manager).len();
        if count == 0 {
            self.reset_selection();
            return;
        }

        if self.selected_index > 0 {
            self.selected_index -= 1;
        }
        self.ensure_visible(count);
    }

    fn select_next(&mut self, manager: &PluginManager) {
        let count = self.sorted_filtered_plugins(manager).len();
        if count == 0 {
            self.reset_selection();
            return;
        }

        if self.selected_index < count.saturating_sub(1) {
            self.selected_index += 1;
        }
        self.ensure_visible(count);
    }

    fn home(&mut self) {
        self.selected_index = 0;
        self.scroll_offset = 0;
    }

    fn end(&mut self, manager: &PluginManager) {
        let count = self.sorted_filtered_plugins(manager).len();
        if count == 0 {
            self.reset_selection();
            return;
        }
        self.selected_index = count - 1;
        self.ensure_visible(count);
    }

    fn page_up(&mut self) {
        let rows = self.viewport_rows.get().max(1);
        self.selected_index = self.selected_index.saturating_sub(rows);
        self.scroll_offset = self.scroll_offset.saturating_sub(rows);
    }

    fn page_down(&mut self) {
        let rows = self.viewport_rows.get().max(1);
        self.selected_index = self.selected_index.saturating_add(rows);
        self.scroll_offset = self.scroll_offset.saturating_add(rows);
    }

    fn ensure_visible(&mut self, count: usize) {
        let rows = self.viewport_rows.get().max(1);
        if count == 0 {
            self.reset_selection();
            return;
        }

        if self.selected_index >= count {
            self.selected_index = count - 1;
        }

        if self.selected_index < self.scroll_offset {
            self.scroll_offset = self.selected_index;
        } else if self.selected_index >= self.scroll_offset + rows {
            self.scroll_offset = self.selected_index - rows + 1;
        }

        let max_scroll = count.saturating_sub(rows);
        if self.scroll_offset > max_scroll {
            self.scroll_offset = max_scroll;
        }
    }

    fn reset_selection(&mut self) {
        self.selected_index = 0;
        self.scroll_offset = 0;
        self.mode = PluginManagerMode::List;
    }

    /// Update viewport rows based on the current render area.
    pub fn set_viewport_rows(&self, rows: usize) {
        self.viewport_rows.set(rows.max(1));
    }
}

impl Default for PluginManagerUI {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_plugin_manager_ui_new() {
        let ui = PluginManagerUI::new();
        assert!(!ui.visible);
        assert_eq!(ui.selected_index, 0);
        assert_eq!(ui.mode, PluginManagerMode::List);
    }

    #[test]
    fn test_plugin_manager_ui_toggle() {
        let mut ui = PluginManagerUI::new();

        ui.toggle();
        assert!(ui.visible);

        ui.toggle();
        assert!(!ui.visible);
    }

    #[test]
    fn test_select_next_previous() {
        let temp_dir = TempDir::new().unwrap();
        let plugin_dir = temp_dir.path().join("alpha");
        std::fs::create_dir_all(&plugin_dir).unwrap();
        std::fs::write(
            plugin_dir.join("plugin.toml"),
            r#"
            name = "alpha"
            version = "0.1.0"
            description = "Alpha"
            permissions = []
            entry_point = "libalpha.so"
        "#,
        )
        .unwrap();

        let mut manager = PluginManager::new().unwrap();
        manager
            .load_plugin(&plugin_dir.join("plugin.toml"))
            .unwrap();

        let mut ui = PluginManagerUI::new();
        ui.show();
        ui.select_next(&manager);
        assert_eq!(ui.selected_index, 0);
        ui.select_previous(&manager);
        assert_eq!(ui.selected_index, 0);
    }
}
