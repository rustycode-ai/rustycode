//! Model Selector Popup Component
//!
//! This module provides a VS Code-style model selector popup for the TUI.
//!
//! ## Features
//!
//! - **Quick model switching**: Select from available models
//! - **Fuzzy matching**: Search models by name or description
//! - **Keyboard shortcuts**: Ctrl+1-4 for quick switching
//! - **Model details**: Show capabilities, cost, and context window
//! - **Modal dialog**: Centered overlay with search and list
//!
//! ## Usage
//!
//! ```rust,no_run

// Complete implementation - pending integration with keyboard shortcuts
//! use rustycode_tui::ui::model_selector::{ModelSelector, ModelInfo};
//! use crossterm::event::{KeyCode, KeyEvent};
//!
//! // Create model selector with available models
//! let mut selector = ModelSelector::new();
//!
//! // Handle keyboard input
//! selector.handle_key(KeyEvent::new(KeyCode::Char('g'), crossterm::event::KeyModifiers::NONE));
//! selector.handle_key(KeyEvent::new(KeyCode::Down, crossterm::event::KeyModifiers::NONE));
//! selector.handle_key(KeyEvent::new(KeyCode::Enter, crossterm::event::KeyModifiers::NONE));
//!
//! // Check if a model was selected
//! if let Some(model) = selector.take_selected() {
//!     // Switch to selected model
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

// Re-export the shared fuzzy matcher types for convenience
pub use super::fuzzy_matcher::{FuzzyMatcher as ModelFuzzyMatcher, MatchScore};

// MODEL INFO

/// Information about an available model
#[derive(Clone, Debug)]
pub struct ModelInfo {
    /// Model identifier (e.g., "claude-sonnet-4-20250514")
    pub id: String,

    /// Display name (e.g., "Claude Sonnet 4")
    pub name: String,

    /// Provider (e.g., "anthropic")
    pub provider: String,

    /// Model description
    pub description: String,

    /// Context window size (in tokens)
    pub context_window: usize,

    /// Input cost per million tokens
    pub input_cost: f64,

    /// Output cost per million tokens
    pub output_cost: f64,

    /// Capabilities
    pub capabilities: Vec<String>,

    /// Quick shortcut number (1-4)
    pub shortcut: Option<usize>,
}

impl ModelInfo {
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        provider: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            provider: provider.into(),
            description: description.into(),
            context_window: 200000,
            input_cost: 0.0,
            output_cost: 0.0,
            capabilities: Vec::new(),
            shortcut: None,
        }
    }

    /// Set context window
    pub fn with_context_window(mut self, tokens: usize) -> Self {
        self.context_window = tokens;
        self
    }

    /// Set costs
    pub fn with_costs(mut self, input: f64, output: f64) -> Self {
        self.input_cost = input;
        self.output_cost = output;
        self
    }

    /// Set capabilities
    pub fn with_capabilities(mut self, caps: Vec<String>) -> Self {
        self.capabilities = caps;
        self
    }

    /// Set quick shortcut
    pub fn with_shortcut(mut self, num: usize) -> Self {
        self.shortcut = Some(num);
        self
    }

    /// Format cost display
    pub fn cost_display(&self) -> String {
        if self.input_cost > 0.0 || self.output_cost > 0.0 {
            format!(
                "${:.2}/M in, ${:.2}/M out",
                self.input_cost, self.output_cost
            )
        } else {
            "Free".to_string()
        }
    }

    /// Format context window display
    pub fn context_display(&self) -> String {
        if self.context_window >= 1_000_000 {
            format!("{}M tokens", self.context_window / 1_000_000)
        } else if self.context_window >= 1_000 {
            format!("{}K tokens", self.context_window / 1_000)
        } else {
            format!("{} tokens", self.context_window)
        }
    }
}

// MODEL SELECTOR STATE

/// Model selector state
#[derive(Debug, Clone)]
pub struct ModelSelectorState {
    /// Current search query
    pub query: String,

    /// Available models
    pub models: Vec<ModelInfo>,

    /// Filtered and ranked model indices (index into models)
    pub filtered_indices: Vec<usize>,

    /// Currently selected index (into filtered_indices)
    pub selected_index: usize,

    /// Whether the selector is visible
    pub visible: bool,

    /// Whether the selection was confirmed (Enter/Ctrl+1-4) vs dismissed (Escape)
    confirmed: bool,

    /// Fuzzy matcher
    matcher: ModelFuzzyMatcher,
}

impl ModelSelectorState {
    /// Create new model selector state
    pub fn new(models: Vec<ModelInfo>) -> Self {
        let filtered_indices = (0..models.len()).collect();

        Self {
            query: String::new(),
            models,
            filtered_indices,
            selected_index: 0,
            visible: false,
            confirmed: false,
            matcher: ModelFuzzyMatcher::new(),
        }
    }

    /// Calculate match score for a query against a model
    fn match_score(&self, query: &str, model: &ModelInfo) -> MatchScore {
        let query_lower = query.to_lowercase();
        let name_lower = model.name.to_lowercase();
        let desc_lower = model.description.to_lowercase();
        let provider_lower = model.provider.to_lowercase();

        // Use the shared matcher for name matching
        let name_score = self.matcher.match_score(query, &name_lower);
        if name_score == MatchScore::Exact {
            return MatchScore::Exact;
        }
        if name_score == MatchScore::Prefix {
            return MatchScore::Prefix;
        }
        if name_score == MatchScore::Substring {
            return MatchScore::Substring;
        }

        // Check provider match
        if provider_lower.contains(&query_lower) {
            return MatchScore::Substring;
        }

        // Check description match
        if desc_lower.contains(&query_lower) {
            return MatchScore::Substring;
        }

        MatchScore::None
    }

    /// Show the selector
    pub fn show(&mut self) {
        self.visible = true;
        self.confirmed = false;
        self.query.clear();
        self.selected_index = 0;
        self.update_filtered();
    }

    /// Hide the selector
    pub fn hide(&mut self) {
        self.visible = false;
    }

    /// Toggle selector visibility
    pub fn toggle(&mut self) {
        if self.visible {
            self.hide();
        } else {
            self.show();
        }
    }

    /// Update filtered models based on current query
    fn update_filtered(&mut self) {
        self.filtered_indices = if self.query.is_empty() {
            // Show all models when query is empty
            (0..self.models.len()).collect()
        } else {
            // Filter and rank by relevance using the custom match scoring
            let query = self.query.clone();
            self.matcher
                .filter_and_rank(&query, &self.models, |model| {
                    self.match_score(&query, model)
                })
                .into_iter()
                .map(|(idx, _)| idx)
                .collect()
        };

        // Clamp selected index
        if self.filtered_indices.is_empty() {
            self.selected_index = 0;
        } else {
            self.selected_index = self.selected_index.min(self.filtered_indices.len() - 1);
        }
    }

    /// Get currently selected model (if any)
    pub fn selected_model(&self) -> Option<&ModelInfo> {
        self.filtered_indices
            .get(self.selected_index)
            .and_then(|&idx| self.models.get(idx))
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

    /// Clear the query
    pub fn clear_query(&mut self) {
        self.query.clear();
        self.update_filtered();
    }

    /// Move selection up
    pub fn move_up(&mut self) {
        if !self.filtered_indices.is_empty() && self.selected_index > 0 {
            self.selected_index -= 1;
        }
    }

    /// Move selection down
    pub fn move_down(&mut self) {
        if !self.filtered_indices.is_empty() {
            self.selected_index = (self.selected_index + 1).min(self.filtered_indices.len() - 1);
        }
    }

    /// Quick select by shortcut number (1-4)
    pub fn quick_select(&mut self, num: usize) -> Option<&ModelInfo> {
        // Find model with this shortcut
        self.models
            .iter()
            .find(|&model| model.shortcut == Some(num))
            .map(|v| v as _)
    }

    /// Get number of filtered models
    pub fn filtered_count(&self) -> usize {
        self.filtered_indices.len()
    }
}

// MODEL SELECTOR RENDERER

/// Model selector renderer
pub struct ModelSelectorRenderer {
    /// Visual state
    state: ModelSelectorState,
}

impl ModelSelectorRenderer {
    pub fn new() -> Self {
        Self::with_models(Self::default_models())
    }

    /// Create a new model selector with custom models
    pub fn with_models(models: Vec<ModelInfo>) -> Self {
        Self {
            state: ModelSelectorState::new(models),
        }
    }

    /// Get default built-in models
    fn default_models() -> Vec<ModelInfo> {
        vec![
            ModelInfo::new(
                "claude-sonnet-4-6",
                "Claude Sonnet 4",
                "anthropic",
                "Best balance of intelligence and speed",
            )
            .with_context_window(200_000)
            .with_costs(3.0, 15.0)
            .with_capabilities(vec![
                "Coding".to_string(),
                "Analysis".to_string(),
                "Writing".to_string(),
            ])
            .with_shortcut(1),
            ModelInfo::new(
                "claude-opus-4-6",
                "Claude Opus 4",
                "anthropic",
                "Maximum intelligence for complex tasks",
            )
            .with_context_window(200_000)
            .with_costs(15.0, 75.0)
            .with_capabilities(vec![
                "Complex Reasoning".to_string(),
                "Research".to_string(),
                "Architecture".to_string(),
            ])
            .with_shortcut(2),
            ModelInfo::new(
                "claude-haiku-4-20250514",
                "Claude Haiku 4",
                "anthropic",
                "Fastest model, great for simple tasks",
            )
            .with_context_window(200_000)
            .with_costs(0.25, 1.25)
            .with_capabilities(vec![
                "Quick Tasks".to_string(),
                "Classification".to_string(),
                "Formatting".to_string(),
            ])
            .with_shortcut(3),
            ModelInfo::new(
                "gpt-4-turbo",
                "GPT-4 Turbo",
                "openai",
                "OpenAI's most capable model",
            )
            .with_context_window(128_000)
            .with_costs(10.0, 30.0)
            .with_capabilities(vec![
                "Coding".to_string(),
                "Reasoning".to_string(),
                "Multimodal".to_string(),
            ])
            .with_shortcut(4),
        ]
    }

    /// Get mutable reference to state
    pub fn state_mut(&mut self) -> &mut ModelSelectorState {
        &mut self.state
    }

    /// Get reference to state
    pub fn state(&self) -> &ModelSelectorState {
        &self.state
    }

    /// Show the selector
    pub fn show(&mut self) {
        self.state.show();
    }

    /// Hide the selector
    pub fn hide(&mut self) {
        self.state.hide();
    }

    /// Toggle selector visibility
    pub fn toggle(&mut self) {
        self.state.toggle();
    }

    /// Handle a key event
    ///
    /// Returns true if the event was handled
    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        match (key.code, key.modifiers) {
            // Close selector on Escape
            (KeyCode::Esc, KeyModifiers::NONE) => {
                self.hide();
                true
            }

            // Quick select with Ctrl+1-4
            (KeyCode::Char(c @ '1'..='4'), KeyModifiers::CONTROL) => {
                let num = c as usize - '0' as usize;
                if let Some(model) = self.state.quick_select(num) {
                    tracing::info!("Model quick-selected: {:?}", model.name);
                    self.state.confirmed = true;
                    self.hide();
                }
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

            // Select model on Enter
            (KeyCode::Enter, KeyModifiers::NONE) => {
                if let Some(model) = self.state.selected_model() {
                    tracing::info!("Model selected: {:?}", model.name);
                    self.state.confirmed = true;
                    self.hide();
                }
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

            // Clear query on Ctrl+U
            (KeyCode::Char('u'), KeyModifiers::CONTROL) => {
                self.state.clear_query();
                true
            }

            _ => false,
        }
    }

    /// Take the selected model (removes it from state)
    pub fn take_selected_model(&mut self) -> Option<ModelInfo> {
        if !self.state.confirmed {
            return None;
        }
        self.state.confirmed = false;
        if let Some(_model) = self.state.selected_model() {
            let idx = self.state.filtered_indices[self.state.selected_index];
            Some(self.state.models[idx].clone())
        } else {
            None
        }
    }

    /// Render the model selector
    pub fn render(&self, f: &mut Frame, area: Rect) {
        if !self.state.visible {
            return;
        }

        // Calculate modal size (60% width, 50% height)
        let width = (area.width * 60) / 100;
        let height = (area.height * 50) / 100;

        // Center the modal
        let x = area.x + (area.width - width) / 2;
        let y = area.y + (area.height - height) / 2;
        let modal_area = Rect::new(x, y, width, height);

        // Clear the area behind the modal
        f.render_widget(Clear, modal_area);

        // Split into search input and model list
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(0)].as_ref())
            .split(modal_area);

        // Render search input
        self.render_search_input(f, chunks[0]);

        // Render model list
        self.render_model_list(f, chunks[1]);
    }

    /// Render the search input field
    fn render_search_input(&self, f: &mut Frame, area: Rect) {
        let paragraph = Paragraph::new(Line::from(vec![
            Span::styled("> ", Style::default().fg(Color::Gray)),
            Span::styled(
                &self.state.query,
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
        ]))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan))
                .title("Model Selector")
                .title_style(
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
        )
        .wrap(Wrap { trim: false });

        f.render_widget(paragraph, area);
    }

    /// Render the filtered model list
    fn render_model_list(&self, f: &mut Frame, area: Rect) {
        if self.state.filtered_indices.is_empty() {
            // Show "no results" message
            let paragraph = Paragraph::new(Line::from(vec![Span::styled(
                "No models found",
                Style::default().fg(Color::DarkGray),
            )]))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::DarkGray)),
            )
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: false });

            f.render_widget(paragraph, area);
            return;
        }

        // Create list items
        let items: Vec<ListItem> = self
            .state
            .filtered_indices
            .iter()
            .map(|&model_idx| {
                let model = &self.state.models[model_idx];

                // Create name span (highlight if query matches)
                let name_span = if self.state.query.is_empty() {
                    Span::raw(model.name.clone())
                } else {
                    Span::styled(
                        &model.name,
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    )
                };

                // Create shortcut indicator
                let shortcut_span = if let Some(num) = model.shortcut {
                    Span::styled(
                        format!(" Ctrl+{} ", num),
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    )
                } else {
                    Span::raw("       ".to_string())
                };

                // Create cost and context info
                let info_span = Span::styled(
                    format!(
                        " {} | {} | {}",
                        model.provider,
                        model.cost_display(),
                        model.context_display()
                    ),
                    Style::default().fg(Color::DarkGray),
                );

                // Create description line
                let desc_span = Span::styled(
                    &model.description,
                    Style::default().fg(Color::Rgb(180, 180, 180)),
                );

                ListItem::new(vec![
                    Line::from(vec![shortcut_span, name_span]),
                    Line::from(desc_span),
                    Line::from(info_span),
                ])
            })
            .collect();

        // Render list
        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Cyan)),
            )
            .highlight_style(
                Style::default()
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            );

        // Create list state
        let mut list_state = ListState::default();
        list_state.select(Some(self.state.selected_index));

        f.render_stateful_widget(list, area, &mut list_state);
    }
}

impl Default for ModelSelectorRenderer {
    fn default() -> Self {
        Self::new()
    }
}

// MODEL SELECTOR (HIGH-LEVEL API)

/// High-level model selector API
///
/// This combines state and rendering into a single convenient interface.
pub struct ModelSelector {
    /// Renderer with embedded state
    renderer: ModelSelectorRenderer,
}

impl ModelSelector {
    pub fn new() -> Self {
        Self {
            renderer: ModelSelectorRenderer::new(),
        }
    }

    /// Create a new model selector with custom models
    pub fn with_models(models: Vec<ModelInfo>) -> Self {
        Self {
            renderer: ModelSelectorRenderer::with_models(models),
        }
    }

    /// Check if selector is visible
    pub fn is_visible(&self) -> bool {
        self.renderer.state().visible
    }

    /// Show the selector
    pub fn show(&mut self) {
        self.renderer.show();
    }

    /// Hide the selector
    pub fn hide(&mut self) {
        self.renderer.hide();
    }

    /// Toggle selector visibility
    pub fn toggle(&mut self) {
        self.renderer.toggle();
    }

    /// Handle a key event
    ///
    /// Returns true if the event was handled
    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        self.renderer.handle_key(key)
    }

    /// Take the selected model (removes it from state)
    pub fn take_selected(&mut self) -> Option<ModelInfo> {
        self.renderer.take_selected_model()
    }

    /// Render the model selector
    pub fn render(&self, f: &mut Frame, area: Rect) {
        self.renderer.render(f, area);
    }

    /// Get mutable reference to state
    pub fn state_mut(&mut self) -> &mut ModelSelectorState {
        self.renderer.state_mut()
    }

    /// Get reference to state
    pub fn state(&self) -> &ModelSelectorState {
        self.renderer.state()
    }
}

impl Default for ModelSelector {
    fn default() -> Self {
        Self::new()
    }
}

// TESTS

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_info_creation() {
        let model = ModelInfo::new(
            "test-model",
            "Test Model",
            "test-provider",
            "Test description",
        );

        assert_eq!(model.id, "test-model");
        assert_eq!(model.name, "Test Model");
        assert_eq!(model.provider, "test-provider");
        assert_eq!(model.description, "Test description");
    }

    #[test]
    fn test_model_info_with_context() {
        let model =
            ModelInfo::new("test-model", "Test Model", "test", "desc").with_context_window(128000);

        assert_eq!(model.context_window, 128000);
        assert_eq!(model.context_display(), "128K tokens");
    }

    #[test]
    fn test_model_info_with_costs() {
        let model =
            ModelInfo::new("test-model", "Test Model", "test", "desc").with_costs(3.0, 15.0);

        assert_eq!(model.input_cost, 3.0);
        assert_eq!(model.output_cost, 15.0);
        assert_eq!(model.cost_display(), "$3.00/M in, $15.00/M out");
    }

    #[test]
    fn test_fuzzy_matcher() {
        let state = ModelSelectorState::new(vec![ModelInfo::new(
            "claude-sonnet-4",
            "Claude Sonnet 4",
            "anthropic",
            "Best balance",
        )]);
        let model = &state.models[0];

        // Exact match
        assert_eq!(
            state.match_score("Claude Sonnet 4", model),
            MatchScore::Exact
        );

        // Prefix match
        assert_eq!(state.match_score("Claude", model), MatchScore::Prefix);

        // Substring match
        assert_eq!(state.match_score("Sonnet", model), MatchScore::Substring);

        // Provider match
        assert_eq!(state.match_score("anthropic", model), MatchScore::Substring);

        // No match
        assert_eq!(state.match_score("xyz", model), MatchScore::None);
    }

    #[test]
    fn test_model_selector_state() {
        let models = vec![
            ModelInfo::new("model-1", "Model 1", "test", "desc1"),
            ModelInfo::new("model-2", "Model 2", "test", "desc2"),
        ];

        let mut state = ModelSelectorState::new(models);

        assert!(!state.visible);
        assert_eq!(state.filtered_count(), 2);

        state.show();
        assert!(state.visible);

        state.insert_char('m');
        assert_eq!(state.query, "m");
        assert_eq!(state.filtered_count(), 2); // Both match

        state.backspace();
        assert_eq!(state.query, "");
        assert_eq!(state.filtered_count(), 2);
    }

    #[test]
    fn test_keyboard_navigation() {
        let mut selector = ModelSelector::new();
        selector.show();

        assert_eq!(selector.state().selected_index, 0);

        // Navigate down
        selector.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(selector.state().selected_index, 1);

        // Navigate up
        selector.handle_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
        assert_eq!(selector.state().selected_index, 0);

        // Escape hides selector
        selector.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(!selector.is_visible());
    }

    #[test]
    fn test_query_filtering() {
        let mut selector = ModelSelector::new();
        selector.show();

        // Type "claude" to filter
        selector.handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE));
        selector.handle_key(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE));
        selector.handle_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));

        assert_eq!(selector.state().query, "cla");
        assert!(selector.state().filtered_count() > 0); // At least one Claude model

        // Backspace
        selector.handle_key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
        assert_eq!(selector.state().query, "cl");

        // Clear with Ctrl+U
        selector.handle_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
        assert_eq!(selector.state().query, "");
    }

    #[test]
    fn test_quick_select() {
        let mut selector = ModelSelector::new();
        selector.show();

        // Quick select with Ctrl+1
        selector.handle_key(KeyEvent::new(KeyCode::Char('1'), KeyModifiers::CONTROL));

        // Should hide selector
        assert!(!selector.is_visible());
    }
}
