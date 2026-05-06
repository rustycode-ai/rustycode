//! Session Mode

use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Frame,
};
use rustycode_session::{
    CompactionEngine, CompactionError, CompactionReport, CompactionStrategy, Message, MessageRole,
    SerializationFormat, Session, SessionId, SessionSerializer, SessionStatus,
};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Session mode state
#[non_exhaustive]
pub struct SessionMode {
    /// Current working directory
    pub cwd: PathBuf,
    pub current_session: Arc<RwLock<Session>>,
    pub session_history: Vec<SessionHistoryEntry>,
    /// Selected session index
    pub selected_session: usize,
    /// Selected message index
    pub selected_message: usize,
    pub view_mode: SessionViewMode,
    pub compaction_strategy: CompactionStrategy,
    pub last_compaction_report: Option<CompactionReport>,
    /// Search query for messages
    pub search_query: String,
    /// Filter for message roles
    pub role_filter: Option<MessageRole>,
    /// Token usage tracking
    pub token_usage: TokenUsage,
    pub show_compaction_preview: bool,
    pub message_display_mode: MessageDisplayMode,
}

/// Session history entry
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct SessionHistoryEntry {
    /// Session ID
    pub id: SessionId,
    /// Session name
    pub name: String,
    /// Created timestamp
    pub created_at: DateTime<Utc>,
    /// Updated timestamp
    pub updated_at: DateTime<Utc>,
    /// Session status
    pub status: SessionStatus,
    pub message_count: usize,
    pub token_count: usize,
    pub total_cost: f64,
    pub tags: Vec<String>,
}

/// Session view mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum SessionViewMode {
    /// Browse session history
    History,
    /// View messages in current session
    Messages,
    /// Compaction controls
    Compaction,
    /// Token usage dashboard
    Tokens,
    /// Session management
    Manage,
}

/// Message display mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum MessageDisplayMode {
    /// Full message content
    Full,
    /// Compact view (first line only)
    Compact,
    /// With metadata
    WithMetadata,
}

/// Token usage statistics
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct TokenUsage {
    /// Total input tokens
    pub input_tokens: usize,
    /// Total output tokens
    pub output_tokens: usize,
    /// Total cost in USD
    pub total_cost: f64,
    /// Last update timestamp
    pub last_updated: DateTime<Utc>,
}

impl Default for TokenUsage {
    fn default() -> Self {
        Self {
            input_tokens: 0,
            output_tokens: 0,
            total_cost: 0.0,
            last_updated: Utc::now(),
        }
    }
}

impl SessionMode {
    /// Create new session mode
    pub async fn new(cwd: PathBuf) -> Result<Self> {
        let current_session = Arc::new(RwLock::new(Session::new("Current Session")));
        let session_history = Self::load_session_history(&cwd).await?;

        // Calculate token usage from current session
        let session = current_session.read().await;
        let token_usage = TokenUsage {
            input_tokens: session
                .user_messages()
                .iter()
                .map(|m| m.estimate_tokens())
                .sum(),
            output_tokens: session
                .assistant_messages()
                .iter()
                .map(|m| m.estimate_tokens())
                .sum(),
            total_cost: session.metadata.total_cost,
            last_updated: Utc::now(),
        };
        drop(session);

        Ok(Self {
            cwd,
            current_session,
            session_history,
            selected_session: 0,
            selected_message: 0,
            view_mode: SessionViewMode::History,
            compaction_strategy: CompactionStrategy::token_threshold(0.5, 20),
            last_compaction_report: None,
            search_query: String::new(),
            role_filter: None,
            token_usage,
            show_compaction_preview: false,
            message_display_mode: MessageDisplayMode::Compact,
        })
    }

    /// Load session history from directory
    async fn load_session_history(cwd: &Path) -> Result<Vec<SessionHistoryEntry>> {
        let _sessions_path = cwd.join(".rustycode").join("sessions");

        // For now, return example data
        // In real implementation, would load from disk
        let now = Utc::now();
        Ok(vec![
            SessionHistoryEntry {
                id: SessionId::parse("sess_abc123").unwrap_or_default(),
                name: "Feature Implementation".to_string(),
                created_at: now - Duration::hours(2),
                updated_at: now - Duration::minutes(30),
                status: SessionStatus::Active,
                message_count: 47,
                token_count: 12500,
                total_cost: 0.15,
                tags: vec!["feature".to_string(), "rust".to_string()],
            },
            SessionHistoryEntry {
                id: SessionId::parse("sess_def456").unwrap_or_default(),
                name: "Bug Fix: Memory Leak".to_string(),
                created_at: now - Duration::hours(24),
                updated_at: now - Duration::hours(23),
                status: SessionStatus::Archived,
                message_count: 23,
                token_count: 5800,
                total_cost: 0.07,
                tags: vec!["bug-fix".to_string(), "memory".to_string()],
            },
            SessionHistoryEntry {
                id: SessionId::parse("sess_ghi789").unwrap_or_default(),
                name: "Code Review: API Design".to_string(),
                created_at: now - Duration::days(2),
                updated_at: now - Duration::days(2),
                status: SessionStatus::Archived,
                message_count: 31,
                token_count: 8200,
                total_cost: 0.10,
                tags: vec!["review".to_string(), "api".to_string()],
            },
        ])
    }

    /// Switch to different session
    pub async fn switch_session(&mut self, session_id: &SessionId) -> Result<()> {
        // In real implementation, would load from disk
        // For now, create a new session
        let new_session = Session::new(format!("Session {}", session_id.as_str()));
        *self.current_session.write().await = new_session;
        self.selected_session = 0;
        self.view.selected_message = 0;
        Ok(())
    }

    /// Create new session
    pub async fn create_session(&mut self, name: String) -> Result<SessionId> {
        let new_session = Session::new(name.clone());
        let id = new_session.id.clone();

        // Add to history
        let entry = SessionHistoryEntry {
            id: id.clone(),
            name,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            status: SessionStatus::Active,
            message_count: 0,
            token_count: 0,
            total_cost: 0.0,
            tags: Vec::new(),
        };

        self.session_history.insert(0, entry);
        self.selected_session = 0;
        *self.current_session.write().await = new_session;

        Ok(id)
    }

    /// Archive current session
    pub async fn archive_session(&mut self) -> Result<()> {
        self.current_session.write().await.archive();
        if let Some(entry) = self.session_history.get_mut(self.selected_session) {
            entry.status = SessionStatus::Archived;
        }
        Ok(())
    }

    /// Export current session to file
    pub async fn export_session(&self, path: PathBuf) -> Result<()> {
        let session = self.current_session.read().await;
        SessionSerializer::to_file(&session, &path, SerializationFormat::Json)?;
        Ok(())
    }

    /// Import session from file
    pub async fn import_session(&mut self, path: PathBuf) -> Result<()> {
        let session = SessionSerializer::from_file(&path, SerializationFormat::Json)?;
        *self.current_session.write().await = session;
        Ok(())
    }

    /// Compact current session
    pub async fn compact_session(&mut self) -> Result<CompactionReport, CompactionError> {
        let engine =
            CompactionEngine::new(self.compaction_strategy.clone()).with_summarization(true);

        let session = self.current_session.read().await;
        let (compacted_messages, report) = engine.compact(&session)?;
        drop(session);

        // Update session with compacted messages
        let mut session = self.current_session.write().await;
        session.messages = compacted_messages;
        session.metadata.total_tokens = report.new_tokens;

        self.last_compaction_report = Some(report.clone());

        // Update token usage
        self.token_usage.input_tokens = session
            .user_messages()
            .iter()
            .map(|m| m.estimate_tokens())
            .sum();
        self.token_usage.output_tokens = session
            .assistant_messages()
            .iter()
            .map(|m| m.estimate_tokens())
            .sum();

        Ok(report)
    }

    /// Preview compaction without applying
    pub async fn preview_compaction(&self) -> Result<CompactionReport, CompactionError> {
        let engine =
            CompactionEngine::new(self.compaction_strategy.clone()).with_summarization(true);

        let session = self.current_session.read().await;
        let (_, report) = engine.compact(&session)?;
        Ok(report)
    }

    /// Select next session
    pub fn next_session(&mut self) {
        if !self.session_history.is_empty() {
            self.selected_session = (self.selected_session + 1) % self.session_history.len();
        }
    }

    /// Select previous session
    pub fn prev_session(&mut self) {
        if !self.session_history.is_empty() {
            let len = self.session_history.len();
            self.selected_session = if self.selected_session == 0 {
                len - 1
            } else {
                self.selected_session - 1
            };
        }
    }

    /// Select next message
    pub async fn next_message(&mut self) {
        let session = self.current_session.read().await;
        if !session.messages.is_empty() {
            self.view.selected_message = (self.view.selected_message + 1) % session.messages.len();
        }
    }

    /// Select previous message
    pub async fn prev_message(&mut self) {
        let session = self.current_session.read().await;
        if !session.messages.is_empty() {
            let len = session.messages.len();
            self.view.selected_message = if self.view.selected_message == 0 {
                len - 1
            } else {
                self.view.selected_message - 1
            };
        }
    }

    /// Switch view mode
    pub fn switch_view(&mut self, mode: SessionViewMode) {
        self.view_mode = mode;
    }

    /// Toggle compaction preview
    pub fn toggle_compaction_preview(&mut self) {
        self.show_compaction_preview = !self.show_compaction_preview;
    }

    /// Cycle compaction strategy
    pub fn cycle_compaction_strategy(&mut self) {
        self.compaction_strategy = match &self.compaction_strategy {
            CompactionStrategy::TokenThreshold { .. } => {
                CompactionStrategy::message_age(Duration::hours(1), 20)
            }
            CompactionStrategy::MessageAge { .. } => {
                CompactionStrategy::semantic_importance(0.5, 20)
            }
            CompactionStrategy::SemanticImportance { .. } => {
                CompactionStrategy::token_threshold(0.5, 20)
            }
            CompactionStrategy::Custom(_) => CompactionStrategy::token_threshold(0.5, 20),
            #[allow(unreachable_patterns)]
            _ => CompactionStrategy::token_threshold(0.5, 20),
        };
    }

    /// Update search query
    pub fn update_search(&mut self, query: String) {
        self.search_query = query;
    }

    /// Set role filter
    pub fn set_role_filter(&mut self, role: Option<MessageRole>) {
        self.role_filter = role;
    }

    /// Cycle message display mode
    pub fn cycle_display_mode(&mut self) {
        self.message_display_mode = match self.message_display_mode {
            MessageDisplayMode::Full => MessageDisplayMode::Compact,
            MessageDisplayMode::Compact => MessageDisplayMode::WithMetadata,
            MessageDisplayMode::WithMetadata => MessageDisplayMode::Full,
        };
    }

    /// Get filtered messages
    pub fn get_filtered_messages(&self) -> Vec<(usize, Message)> {
        let session = self.current_session.blocking_read();

        session
            .messages
            .iter()
            .enumerate()
            .filter(|(_, msg)| {
                // Apply role filter
                if let Some(filter_role) = self.role_filter.as_ref() {
                    if &msg.role != filter_role {
                        return false;
                    }
                }

                // Apply search filter
                if !self.search_query.is_empty() {
                    let text = msg.text().to_lowercase();
                    let query = self.search_query.to_lowercase();
                    if !text.contains(&query) {
                        return false;
                    }
                }

                true
            })
            .map(|(i, msg)| (i, msg.clone()))
            .collect()
    }

    /// Render session mode UI
    pub fn render(&mut self, frame: &mut Frame<'_>, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Header
                Constraint::Min(0),    // Main content
                Constraint::Length(3), // Footer
            ])
            .split(area);

        // Header
        self.render_header(frame, chunks[0]);

        // Main content based on view mode
        match self.view_mode {
            SessionViewMode::History => self.render_history(frame, chunks[1]),
            SessionViewMode::Messages => self.render_messages(frame, chunks[1]),
            SessionViewMode::Compaction => self.render_compaction(frame, chunks[1]),
            SessionViewMode::Tokens => self.render_tokens(frame, chunks[1]),
            SessionViewMode::Manage => self.render_manage(frame, chunks[1]),
        }

        // Footer
        self.render_footer(frame, chunks[2]);
    }

    /// Render header
    fn render_header(&self, frame: &mut Frame<'_>, area: Rect) {
        let mode_name = match self.view_mode {
            SessionViewMode::History => "Session History",
            SessionViewMode::Messages => "Message Browser",
            SessionViewMode::Compaction => "Compaction Controls",
            SessionViewMode::Tokens => "Token Tracking",
            SessionViewMode::Manage => "Session Management",
        };

        let title = Span::styled(
            mode_name,
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );

        let header = Paragraph::new(Line::from(title))
            .block(Block::default().borders(Borders::ALL))
            .alignment(Alignment::Center);

        frame.render_widget(header, area);
    }

    /// Render session history
    fn render_history(&self, frame: &mut Frame<'_>, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(40), // Session list
                Constraint::Percentage(60), // Session details
            ])
            .split(area);

        self.render_session_list(frame, chunks[0]);
        self.render_session_details(frame, chunks[1]);
    }

    /// Render session list
    fn render_session_list(&self, frame: &mut Frame<'_>, area: Rect) {
        let items: Vec<ListItem> = self
            .session_history
            .iter()
            .enumerate()
            .map(|(i, entry)| {
                let is_selected = i == self.selected_session;
                let style = if is_selected {
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };

                let status_icon = match entry.status {
                    SessionStatus::Active => "●",
                    SessionStatus::Archived => "○",
                    SessionStatus::Deleted => "✗",
                    #[allow(unreachable_patterns)]
                    _ => "•",
                };

                let status_color = match entry.status {
                    SessionStatus::Active => Color::Green,
                    SessionStatus::Archived => Color::Gray,
                    SessionStatus::Deleted => Color::Red,
                    #[allow(unreachable_patterns)]
                    _ => Color::White,
                };

                let content = format!(
                    "{} {} - {} msgs ({} tokens)\n  {}",
                    status_icon,
                    entry.name,
                    entry.message_count,
                    entry.token_count,
                    format_timestamp(entry.updated_at)
                );

                ListItem::new(content)
                    .style(style)
                    .style(Style::default().fg(status_color))
            })
            .collect();

        let list = List::new(items)
            .block(
                Block::default()
                    .title(format!("Sessions ({})", self.session_history.len()))
                    .borders(Borders::ALL),
            )
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED));

        frame.render_widget(list, area);
    }

    /// Render session details
    fn render_session_details(&self, frame: &mut Frame<'_>, area: Rect) {
        if let Some(entry) = self.session_history.get(self.selected_session) {
            let details = vec![
                Line::from(vec![
                    Span::styled("Session: ", Style::default().fg(Color::Cyan)),
                    Span::styled(&entry.name, Style::default().add_modifier(Modifier::BOLD)),
                ]),
                Line::from(""),
                Line::from(vec![
                    Span::styled("ID: ", Style::default().fg(Color::Cyan)),
                    Span::raw(entry.id.as_str()),
                ]),
                Line::from(vec![
                    Span::styled("Status: ", Style::default().fg(Color::Cyan)),
                    Span::raw(format!("{:?}", entry.status)),
                ]),
                Line::from(vec![
                    Span::styled("Created: ", Style::default().fg(Color::Cyan)),
                    Span::raw(format_timestamp(entry.created_at)),
                ]),
                Line::from(vec![
                    Span::styled("Updated: ", Style::default().fg(Color::Cyan)),
                    Span::raw(format_timestamp(entry.updated_at)),
                ]),
                Line::from(vec![
                    Span::styled("Messages: ", Style::default().fg(Color::Cyan)),
                    Span::raw(format!("{}", entry.message_count)),
                ]),
                Line::from(vec![
                    Span::styled("Tokens: ", Style::default().fg(Color::Cyan)),
                    Span::raw(format!("{}", entry.token_count)),
                ]),
                Line::from(vec![
                    Span::styled("Cost: ", Style::default().fg(Color::Cyan)),
                    Span::raw(format!("${:.2}", entry.total_cost)),
                ]),
                Line::from(""),
                Line::from(vec![
                    Span::styled("Tags: ", Style::default().fg(Color::Cyan)),
                    Span::raw(entry.tags.join(", ")),
                ]),
            ];

            let paragraph = Paragraph::new(details)
                .block(
                    Block::default()
                        .title("Session Details")
                        .borders(Borders::ALL),
                )
                .wrap(Wrap { trim: false });

            frame.render_widget(paragraph, area);
        } else {
            let paragraph = Paragraph::new("No session selected").block(
                Block::default()
                    .title("Session Details")
                    .borders(Borders::ALL),
            );

            frame.render_widget(paragraph, area);
        }
    }

    /// Render messages view
    fn render_messages(&self, frame: &mut Frame<'_>, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(6),  // Search/filter bar
                Constraint::Min(0),     // Messages list
                Constraint::Length(10), // Message preview
            ])
            .split(area);

        self.render_message_filters(frame, chunks[0]);
        self.render_message_list(frame, chunks[1]);
        self.render_message_preview(frame, chunks[2]);
    }

    /// Render message filters
    fn render_message_filters(&self, frame: &mut Frame<'_>, area: Rect) {
        let filter_text = vec![Line::from(vec![
            Span::styled("Search: ", Style::default().fg(Color::Cyan)),
            Span::raw(if self.search_query.is_empty() {
                "(none)".to_string()
            } else {
                self.search_query.clone()
            }),
            Span::raw(" | "),
            Span::styled("Filter: ", Style::default().fg(Color::Cyan)),
            Span::raw(match self.role_filter {
                Some(MessageRole::User) => "User",
                Some(MessageRole::Assistant) => "Assistant",
                Some(MessageRole::System) => "System",
                Some(MessageRole::Tool) => "Tool",
                None => "All",
                #[allow(unreachable_patterns)]
                _ => "All",
            }),
            Span::raw(" | "),
            Span::styled("View: ", Style::default().fg(Color::Cyan)),
            Span::raw(format!("{:?}", self.message_display_mode)),
        ])];

        let paragraph = Paragraph::new(filter_text)
            .block(Block::default().title("Filters").borders(Borders::ALL))
            .wrap(Wrap { trim: false });

        frame.render_widget(paragraph, area);
    }

    /// Render message list
    fn render_message_list(&self, frame: &mut Frame<'_>, area: Rect) {
        let messages = self.get_filtered_messages();

        let items: Vec<ListItem> = messages
            .iter()
            .map(|(i, msg)| {
                let is_selected = *i == self.view.selected_message;
                let role_icon = match msg.role {
                    MessageRole::User => "👤",
                    MessageRole::Assistant => "🤖",
                    MessageRole::System => "⚙",
                    MessageRole::Tool => "🔧",
                    #[allow(unreachable_patterns)]
                    _ => "?",
                };

                let role_color = match msg.role {
                    MessageRole::User => Color::Green,
                    MessageRole::Assistant => Color::Blue,
                    MessageRole::System => Color::Yellow,
                    MessageRole::Tool => Color::Magenta,
                    #[allow(unreachable_patterns)]
                    _ => Color::White,
                };

                let style = if is_selected {
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(role_color)
                };

                let content = match self.message_display_mode {
                    MessageDisplayMode::Full => {
                        format!(
                            "{} [{}] {}\n  {}",
                            role_icon,
                            msg.role.as_str(),
                            format_timestamp(msg.timestamp),
                            msg.text()
                        )
                    }
                    MessageDisplayMode::Compact => {
                        let text = msg.text();
                        let preview = if text.chars().count() > 60 {
                            let s: String = text.chars().take(57).collect();
                            format!("{}...", s)
                        } else {
                            text
                        };
                        format!("{} [{}] {}", role_icon, msg.role.as_str(), preview)
                    }
                    MessageDisplayMode::WithMetadata => {
                        let tokens = msg.estimate_tokens();
                        format!(
                            "{} [{}] {} tokens | {}",
                            role_icon,
                            msg.role.as_str(),
                            tokens,
                            msg.text()
                        )
                    }
                };

                ListItem::new(content).style(style)
            })
            .collect();

        let list = List::new(items)
            .block(
                Block::default()
                    .title(format!("Messages ({})", messages.len()))
                    .borders(Borders::ALL),
            )
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED));

        frame.render_widget(list, area);
    }

    /// Render message preview
    fn render_message_preview(&self, frame: &mut Frame<'_>, area: Rect) {
        let session = self.current_session.blocking_read();

        if let Some(msg) = session.messages.get(self.view.selected_message) {
            let content = vec![
                Line::from(vec![
                    Span::styled("Role: ", Style::default().fg(Color::Cyan)),
                    Span::styled(
                        msg.role.as_str(),
                        Style::default().add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(" | "),
                    Span::styled("Tokens: ", Style::default().fg(Color::Cyan)),
                    Span::raw(format!("{}", msg.estimate_tokens())),
                ]),
                Line::from(""),
                Line::from(vec![Span::styled(
                    "Content:",
                    Style::default().fg(Color::Cyan),
                )]),
                Line::from(""),
            ];

            let mut text_content = msg.text();
            if text_content.chars().count() > 500 {
                let s: String = text_content.chars().take(497).collect();
                text_content = format!("{}...", s);
            }

            let mut lines: Vec<Line> = content
                .into_iter()
                .chain(
                    text_content
                        .lines()
                        .map(|line| Line::from(vec![Span::raw(line.to_string())])),
                )
                .collect();

            // Add metadata if available
            if let Some(tokens) = msg.metadata.tokens {
                lines.push(Line::from(""));
                lines.push(Line::from(vec![
                    Span::styled("Actual Tokens: ", Style::default().fg(Color::Cyan)),
                    Span::raw(format!("{}", tokens)),
                ]));
            }

            if let Some(cost) = msg.metadata.cost {
                lines.push(Line::from(vec![
                    Span::styled("Cost: ", Style::default().fg(Color::Cyan)),
                    Span::raw(format!("${:.4}", cost)),
                ]));
            }

            if let Some(ref model) = msg.metadata.model {
                lines.push(Line::from(vec![
                    Span::styled("Model: ", Style::default().fg(Color::Cyan)),
                    Span::raw(model),
                ]));
            }

            let paragraph = Paragraph::new(lines)
                .block(
                    Block::default()
                        .title("Message Preview")
                        .borders(Borders::ALL),
                )
                .wrap(Wrap { trim: false });

            frame.render_widget(paragraph, area);
        } else {
            let paragraph = Paragraph::new("No message selected").block(
                Block::default()
                    .title("Message Preview")
                    .borders(Borders::ALL),
            );

            frame.render_widget(paragraph, area);
        }
    }

    /// Render compaction controls
    fn render_compaction(&self, frame: &mut Frame<'_>, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(8),  // Strategy selection
                Constraint::Length(15), // Preview
                Constraint::Min(0),     // History
            ])
            .split(area);

        self.render_compaction_strategy(frame, chunks[0]);
        self.render_compaction_preview(frame, chunks[1]);
        self.render_compaction_history(frame, chunks[2]);
    }

    /// Render compaction strategy
    fn render_compaction_strategy(&self, frame: &mut Frame<'_>, area: Rect) {
        let strategy_info = match &self.compaction_strategy {
            CompactionStrategy::TokenThreshold {
                target_ratio,
                min_messages,
            } => {
                vec![
                    Line::from(vec![
                        Span::styled("Strategy: ", Style::default().fg(Color::Cyan)),
                        Span::styled(
                            "Token Threshold",
                            Style::default().add_modifier(Modifier::BOLD),
                        ),
                    ]),
                    Line::from(""),
                    Line::from(vec![
                        Span::styled("Target Ratio: ", Style::default().fg(Color::Cyan)),
                        Span::raw(format!("{:.0}%", target_ratio * 100.0)),
                    ]),
                    Line::from(vec![
                        Span::styled("Min Messages: ", Style::default().fg(Color::Cyan)),
                        Span::raw(format!("{}", min_messages)),
                    ]),
                    Line::from(""),
                    Line::from("Compacts when token count exceeds target ratio"),
                    Line::from("while keeping minimum number of messages."),
                ]
            }
            CompactionStrategy::MessageAge {
                max_age,
                keep_recent,
            } => {
                vec![
                    Line::from(vec![
                        Span::styled("Strategy: ", Style::default().fg(Color::Cyan)),
                        Span::styled("Message Age", Style::default().add_modifier(Modifier::BOLD)),
                    ]),
                    Line::from(""),
                    Line::from(vec![
                        Span::styled("Max Age: ", Style::default().fg(Color::Cyan)),
                        Span::raw(format!("{} seconds", max_age.num_seconds())),
                    ]),
                    Line::from(vec![
                        Span::styled("Keep Recent: ", Style::default().fg(Color::Cyan)),
                        Span::raw(format!("{} messages", keep_recent)),
                    ]),
                    Line::from(""),
                    Line::from("Compacts messages older than specified duration"),
                    Line::from("while keeping recent messages intact."),
                ]
            }
            CompactionStrategy::SemanticImportance {
                importance_threshold,
                min_messages,
            } => {
                vec![
                    Line::from(vec![
                        Span::styled("Strategy: ", Style::default().fg(Color::Cyan)),
                        Span::styled(
                            "Semantic Importance",
                            Style::default().add_modifier(Modifier::BOLD),
                        ),
                    ]),
                    Line::from(""),
                    Line::from(vec![
                        Span::styled("Importance Threshold: ", Style::default().fg(Color::Cyan)),
                        Span::raw(format!("{:.2}", importance_threshold)),
                    ]),
                    Line::from(vec![
                        Span::styled("Min Messages: ", Style::default().fg(Color::Cyan)),
                        Span::raw(format!("{}", min_messages)),
                    ]),
                    Line::from(""),
                    Line::from("Compacts based on semantic importance"),
                    Line::from("(keeps user messages, tool calls, errors)."),
                ]
            }
            CompactionStrategy::Custom(_) => {
                vec![
                    Line::from(vec![
                        Span::styled("Strategy: ", Style::default().fg(Color::Cyan)),
                        Span::styled("Custom", Style::default().add_modifier(Modifier::BOLD)),
                    ]),
                    Line::from(""),
                    Line::from("Custom compaction function"),
                ]
            }
            #[allow(unreachable_patterns)]
            _ => vec![
                Line::from(vec![
                    Span::styled("Strategy: ", Style::default().fg(Color::Cyan)),
                    Span::styled("Unknown", Style::default().add_modifier(Modifier::BOLD)),
                ]),
                Line::from(""),
                Line::from("Unsupported compaction strategy"),
            ],
        };

        let paragraph = Paragraph::new(strategy_info)
            .block(
                Block::default()
                    .title("Compaction Strategy")
                    .borders(Borders::ALL),
            )
            .wrap(Wrap { trim: false });

        frame.render_widget(paragraph, area);
    }

    /// Render compaction preview
    fn render_compaction_preview(&self, frame: &mut Frame<'_>, area: Rect) {
        let preview_info = if let Some(ref report) = self.last_compaction_report {
            vec![
                Line::from(vec![Span::styled(
                    "Last Compaction Report",
                    Style::default().add_modifier(Modifier::BOLD),
                )]),
                Line::from(""),
                Line::from(vec![
                    Span::styled("Original Messages: ", Style::default().fg(Color::Cyan)),
                    Span::raw(format!("{}", report.original_count)),
                ]),
                Line::from(vec![
                    Span::styled("New Messages: ", Style::default().fg(Color::Cyan)),
                    Span::raw(format!("{}", report.new_count)),
                ]),
                Line::from(vec![
                    Span::styled("Messages Removed: ", Style::default().fg(Color::Cyan)),
                    Span::raw(format!("{}", report.messages_removed)),
                    Span::raw(format!(" ({:.1}%)", report.message_reduction_percentage())),
                ]),
                Line::from(""),
                Line::from(vec![
                    Span::styled("Original Tokens: ", Style::default().fg(Color::Cyan)),
                    Span::raw(format!("{}", report.original_tokens)),
                ]),
                Line::from(vec![
                    Span::styled("New Tokens: ", Style::default().fg(Color::Cyan)),
                    Span::raw(format!("{}", report.new_tokens)),
                ]),
                Line::from(vec![
                    Span::styled("Tokens Saved: ", Style::default().fg(Color::Cyan)),
                    Span::raw(format!("{}", report.original_tokens - report.new_tokens)),
                    Span::raw(format!(" ({:.1}%)", report.reduction_percentage())),
                ]),
            ]
        } else {
            vec![
                Line::from("No compaction performed yet"),
                Line::from(""),
                Line::from("Press 'P' to preview compaction"),
                Line::from("Press 'C' to compact current session"),
            ]
        };

        let paragraph = Paragraph::new(preview_info)
            .block(
                Block::default()
                    .title("Compaction Preview")
                    .borders(Borders::ALL),
            )
            .wrap(Wrap { trim: false });

        frame.render_widget(paragraph, area);
    }

    /// Render compaction history
    fn render_compaction_history(&self, frame: &mut Frame<'_>, area: Rect) {
        let history_text = vec![
            Line::from("Compaction History"),
            Line::from("─────────────────"),
            Line::from(""),
            Line::from("(Compaction history would be displayed here)"),
            Line::from(""),
            Line::from("In a full implementation, this would show:"),
            Line::from("• Timestamp of each compaction"),
            Line::from("• Strategy used"),
            Line::from("• Tokens saved"),
            Line::from("• Before/after statistics"),
        ];

        let paragraph = Paragraph::new(history_text)
            .block(Block::default().title("History").borders(Borders::ALL))
            .wrap(Wrap { trim: false });

        frame.render_widget(paragraph, area);
    }

    /// Render token tracking
    fn render_tokens(&self, frame: &mut Frame<'_>, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(10), // Summary
                Constraint::Min(0),     // Breakdown
            ])
            .split(area);

        self.render_token_summary(frame, chunks[0]);
        self.render_token_breakdown(frame, chunks[1]);
    }

    /// Render token summary
    fn render_token_summary(&self, frame: &mut Frame<'_>, area: Rect) {
        let session = self.current_session.blocking_read();
        let total_tokens = session.estimate_tokens();

        let summary = vec![
            Line::from(vec![
                Span::styled("Total Tokens: ", Style::default().fg(Color::Cyan)),
                Span::styled(
                    format!("{}", total_tokens),
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(vec![
                Span::styled("Input Tokens: ", Style::default().fg(Color::Cyan)),
                Span::raw(format!("{}", self.token_usage.input_tokens)),
            ]),
            Line::from(vec![
                Span::styled("Output Tokens: ", Style::default().fg(Color::Cyan)),
                Span::raw(format!("{}", self.token_usage.output_tokens)),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("Total Cost: ", Style::default().fg(Color::Cyan)),
                Span::styled(
                    format!("${:.4}", self.token_usage.total_cost),
                    Style::default().fg(Color::Yellow),
                ),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("Messages: ", Style::default().fg(Color::Cyan)),
                Span::raw(format!("{}", session.message_count())),
            ]),
            Line::from(vec![
                Span::styled("Last Updated: ", Style::default().fg(Color::Cyan)),
                Span::raw(format_timestamp(self.token_usage.last_updated)),
            ]),
        ];

        let paragraph = Paragraph::new(summary)
            .block(
                Block::default()
                    .title("Token Summary")
                    .borders(Borders::ALL),
            )
            .wrap(Wrap { trim: false });

        frame.render_widget(paragraph, area);
    }

    /// Render token breakdown
    fn render_token_breakdown(&self, frame: &mut Frame<'_>, area: Rect) {
        let breakdown = vec![
            Line::from("Token Breakdown by Role"),
            Line::from("─────────────────────────"),
            Line::from(""),
            Line::from("(Token breakdown would be displayed here)"),
            Line::from(""),
            Line::from("In a full implementation, this would show:"),
            Line::from("• Tokens per message role"),
            Line::from("• Tokens per message (top consumers)"),
            Line::from("• Cost projections"),
            Line::from("• Trends over time"),
        ];

        let paragraph = Paragraph::new(breakdown)
            .block(Block::default().title("Breakdown").borders(Borders::ALL))
            .wrap(Wrap { trim: false });

        frame.render_widget(paragraph, area);
    }

    /// Render session management
    fn render_manage(&self, frame: &mut Frame<'_>, area: Rect) {
        let actions = vec![
            Line::from("Session Management Actions"),
            Line::from("──────────────────────────"),
            Line::from(""),
            Line::from(vec![
                Span::styled("N", Style::default().fg(Color::Green)),
                Span::raw(": Create new session"),
            ]),
            Line::from(vec![
                Span::styled("S", Style::default().fg(Color::Green)),
                Span::raw(": Switch to selected session"),
            ]),
            Line::from(vec![
                Span::styled("A", Style::default().fg(Color::Green)),
                Span::raw(": Archive current session"),
            ]),
            Line::from(vec![
                Span::styled("E", Style::default().fg(Color::Green)),
                Span::raw(": Export session to file"),
            ]),
            Line::from(vec![
                Span::styled("I", Style::default().fg(Color::Green)),
                Span::raw(": Import session from file"),
            ]),
            Line::from(vec![
                Span::styled("F", Style::default().fg(Color::Green)),
                Span::raw(": Fork current session"),
            ]),
            Line::from(vec![
                Span::styled("D", Style::default().fg(Color::Green)),
                Span::raw(": Delete selected session"),
            ]),
            Line::from(""),
            Line::from("Tags and Metadata:"),
            Line::from("• Add custom tags to sessions"),
            Line::from("• Edit session metadata"),
            Line::from("• Set project context"),
        ];

        let paragraph = Paragraph::new(actions)
            .block(
                Block::default()
                    .title("Manage Sessions")
                    .borders(Borders::ALL),
            )
            .wrap(Wrap { trim: false });

        frame.render_widget(paragraph, area);
    }

    /// Render footer with keybindings
    fn render_footer(&self, frame: &mut Frame<'_>, area: Rect) {
        let help_text = match self.view_mode {
            SessionViewMode::History => vec![
                Span::styled("H", Style::default().fg(Color::Green)),
                Span::raw(": History "),
                Span::styled("M", Style::default().fg(Color::Green)),
                Span::raw(": Messages "),
                Span::styled("C", Style::default().fg(Color::Green)),
                Span::raw(": Compact "),
                Span::styled("T", Style::default().fg(Color::Green)),
                Span::raw(": Tokens "),
                Span::styled("↑↓", Style::default().fg(Color::Green)),
                Span::raw(": Select "),
                Span::styled("Q", Style::default().fg(Color::Green)),
                Span::raw(": Quit"),
            ],
            SessionViewMode::Messages => vec![
                Span::styled("H", Style::default().fg(Color::Green)),
                Span::raw(": History "),
                Span::styled("/", Style::default().fg(Color::Green)),
                Span::raw(": Search "),
                Span::styled("F", Style::default().fg(Color::Green)),
                Span::raw(": Filter "),
                Span::styled("V", Style::default().fg(Color::Green)),
                Span::raw(": View "),
                Span::styled("↑↓", Style::default().fg(Color::Green)),
                Span::raw(": Navigate "),
                Span::styled("Q", Style::default().fg(Color::Green)),
                Span::raw(": Quit"),
            ],
            SessionViewMode::Compaction => vec![
                Span::styled("H", Style::default().fg(Color::Green)),
                Span::raw(": History "),
                Span::styled("S", Style::default().fg(Color::Green)),
                Span::raw(": Strategy "),
                Span::styled("P", Style::default().fg(Color::Green)),
                Span::raw(": Preview "),
                Span::styled("C", Style::default().fg(Color::Green)),
                Span::raw(": Compact "),
                Span::styled("Q", Style::default().fg(Color::Green)),
                Span::raw(": Quit"),
            ],
            SessionViewMode::Tokens => vec![
                Span::styled("H", Style::default().fg(Color::Green)),
                Span::raw(": History "),
                Span::styled("R", Style::default().fg(Color::Green)),
                Span::raw(": Refresh "),
                Span::styled("Q", Style::default().fg(Color::Green)),
                Span::raw(": Quit"),
            ],
            SessionViewMode::Manage => vec![
                Span::styled("H", Style::default().fg(Color::Green)),
                Span::raw(": History "),
                Span::styled("N", Style::default().fg(Color::Green)),
                Span::raw(": New "),
                Span::styled("S", Style::default().fg(Color::Green)),
                Span::raw(": Switch "),
                Span::styled("Q", Style::default().fg(Color::Green)),
                Span::raw(": Quit"),
            ],
            #[allow(unreachable_patterns)]
            _ => vec![
                Span::styled("Q", Style::default().fg(Color::Green)),
                Span::raw(": Quit"),
            ],
        };

        let footer = Paragraph::new(Line::from(help_text))
            .block(Block::default().borders(Borders::ALL))
            .alignment(Alignment::Center);

        frame.render_widget(footer, area);
    }
}

/// Format timestamp for display
fn format_timestamp(ts: DateTime<Utc>) -> String {
    let now = Utc::now();
    let diff = now - ts;

    if diff.num_seconds() < 60 {
        format!("{}s ago", diff.num_seconds())
    } else if diff.num_minutes() < 60 {
        format!("{}m ago", diff.num_minutes())
    } else if diff.num_hours() < 24 {
        format!("{}h ago", diff.num_hours())
    } else if diff.num_days() < 7 {
        format!("{}d ago", diff.num_days())
    } else {
        ts.format("%Y-%m-%d").to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_session_mode_new() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mode = SessionMode::new(temp_dir.path().to_path_buf())
            .await
            .unwrap();

        assert_eq!(mode.selected_session, 0);
        assert_eq!(mode.selected_message, 0);
        assert_eq!(mode.view_mode, SessionViewMode::History);
        assert!(mode.search_query.is_empty());
    }

    #[test]
    fn test_format_timestamp() {
        let now = Utc::now();
        assert_eq!(format_timestamp(now), "0s ago");

        let one_min_ago = now - Duration::seconds(60);
        assert_eq!(format_timestamp(one_min_ago), "1m ago");

        let one_hour_ago = now - Duration::seconds(3600);
        assert_eq!(format_timestamp(one_hour_ago), "1h ago");

        let one_day_ago = now - Duration::seconds(86400);
        assert_eq!(format_timestamp(one_day_ago), "1d ago");
    }

    #[tokio::test]
    async fn test_next_prev_session() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut mode = SessionMode::new(temp_dir.path().to_path_buf())
            .await
            .unwrap();

        let initial = mode.selected_session;
        mode.next_session();
        assert_eq!(
            mode.selected_session,
            (initial + 1) % mode.session_history.len()
        );

        mode.prev_session();
        assert_eq!(mode.selected_session, initial);
    }

    #[tokio::test]
    async fn test_cycle_compaction_strategy() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut mode = SessionMode::new(temp_dir.path().to_path_buf())
            .await
            .unwrap();

        let initial_strategy = format!("{:?}", mode.compaction_strategy);
        mode.cycle_compaction_strategy();
        let new_strategy = format!("{:?}", mode.compaction_strategy);

        assert_ne!(initial_strategy, new_strategy);
    }

    #[tokio::test]
    async fn test_cycle_display_mode() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut mode = SessionMode::new(temp_dir.path().to_path_buf())
            .await
            .unwrap();

        let initial = mode.message_display_mode;
        mode.cycle_display_mode();
        assert_ne!(mode.message_display_mode, initial);

        mode.cycle_display_mode();
        mode.cycle_display_mode();
        assert_eq!(mode.message_display_mode, initial);
    }

    #[tokio::test]
    async fn test_update_search() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut mode = SessionMode::new(temp_dir.path().to_path_buf())
            .await
            .unwrap();

        mode.update_search("test query".to_string());
        assert_eq!(mode.search_query, "test query");

        mode.update_search(String::new());
        assert!(mode.search_query.is_empty());
    }

    #[tokio::test]
    async fn test_set_role_filter() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut mode = SessionMode::new(temp_dir.path().to_path_buf())
            .await
            .unwrap();

        mode.set_role_filter(Some(MessageRole::User));
        assert_eq!(mode.role_filter, Some(MessageRole::User));

        mode.set_role_filter(None);
        assert_eq!(mode.role_filter, None);
    }

    #[tokio::test]
    async fn test_switch_view() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut mode = SessionMode::new(temp_dir.path().to_path_buf())
            .await
            .unwrap();

        mode.switch_view(SessionViewMode::Messages);
        assert_eq!(mode.view_mode, SessionViewMode::Messages);

        mode.switch_view(SessionViewMode::Compaction);
        assert_eq!(mode.view_mode, SessionViewMode::Compaction);
    }

    #[tokio::test]
    async fn test_toggle_compaction_preview() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut mode = SessionMode::new(temp_dir.path().to_path_buf())
            .await
            .unwrap();

        let initial = mode.show_compaction_preview;
        mode.toggle_compaction_preview();
        assert_eq!(mode.show_compaction_preview, !initial);

        mode.toggle_compaction_preview();
        assert_eq!(mode.show_compaction_preview, initial);
    }

    #[tokio::test]
    async fn test_create_session() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut mode = SessionMode::new(temp_dir.path().to_path_buf())
            .await
            .unwrap();

        let initial_count = mode.session_history.len();
        let session_id = mode
            .create_session("Test Session".to_string())
            .await
            .unwrap();

        assert_eq!(mode.session_history.len(), initial_count + 1);
        assert_eq!(mode.session_history[0].name, "Test Session");
        assert_eq!(mode.session_history[0].id.as_str(), session_id.as_str());
    }

    #[tokio::test]
    async fn test_archive_session() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut mode = SessionMode::new(temp_dir.path().to_path_buf())
            .await
            .unwrap();

        mode.archive_session().await.unwrap();

        if let Some(entry) = mode.session_history.get(mode.selected_session) {
            assert_eq!(entry.status, SessionStatus::Archived);
        }
    }
}
