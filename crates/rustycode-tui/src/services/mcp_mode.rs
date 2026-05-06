//! MCP Tools Mode

use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Frame,
};

use anyhow::Result;
use rustycode_mcp::proxy::{ProxyConfig, ToolProxy};
use rustycode_mcp::{HealthStatus, ManagerConfig, McpConfigFile, McpServerManager};
use rustycode_tools::Tool;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// MCP mode state
#[non_exhaustive]
pub struct McpMode {
    /// MCP server manager
    pub manager: Arc<RwLock<McpServerManager>>,
    /// Selected server index
    pub selected_server: usize,
    /// Selected tool index
    pub selected_tool: usize,
    /// Show resources panel
    pub show_resources: bool,
    /// Server health status
    pub server_health: Vec<ServerHealth>,
    /// Available tools
    pub tools: Vec<ToolInfo>,
    /// Tool execution results
    pub execution_results: Vec<ToolExecutionResult>,
    pub search_query: String,
    pub execution_mode: ExecutionMode,
    /// Tool proxies per server (for execution)
    pub server_proxies: HashMap<String, ToolProxy>,
    /// Resources per server
    pub server_resources: HashMap<String, Vec<ResourceInfo>>,
    /// Loading state (tracks initialization progress)
    pub loading_state: Option<LoadingState>,
}

/// Loading state for MCP initialization
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct LoadingState {
    /// Whether loading is in progress
    pub is_loading: bool,
    /// Current server being loaded
    pub current_server: Option<String>,
    /// Number of servers loaded so far
    pub servers_loaded: usize,
    /// Total number of servers to load
    pub servers_total: usize,
    /// Current step description
    pub current_step: String,
}

/// Server health information
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ServerHealth {
    /// Server name
    pub name: String,
    /// Health status
    pub status: HealthStatus,
    pub tool_count: usize,
    pub resource_count: usize,
    /// Last check timestamp
    pub last_check: chrono::DateTime<chrono::Utc>,
}

/// Tool information
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ToolInfo {
    /// Tool name
    pub name: String,
    /// Tool description
    pub description: String,
    pub server_name: String,
    /// Input schema (JSON Schema)
    pub input_schema: Option<Value>,
}

/// Tool execution result
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ToolExecutionResult {
    pub tool_name: String,
    /// Execution status
    pub status: ExecutionStatus,
    /// Result data
    pub result: Option<Value>,
    /// Error message (if failed)
    pub error: Option<String>,
    /// Execution time (ms)
    pub execution_time: u128,
}

/// Execution status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ExecutionStatus {
    Pending,
    Running,
    Success,
    Failed,
}

/// Execution mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ExecutionMode {
    Single,
    Parallel,
}

/// Resource information
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ResourceInfo {
    /// Resource URI
    pub uri: String,
    /// Resource name
    pub name: String,
    /// Human-readable description
    pub description: String,
    pub mime_type: Option<String>,
}

impl McpMode {
    /// Create new MCP mode (loads all enabled servers)
    pub async fn new() -> Result<Self> {
        Self::new_with_filters(None, None).await
    }

    /// Create new MCP mode with filters
    ///
    pub async fn new_with_filters(
        enabled_tags: Option<Vec<String>>,
        enabled_server_ids: Option<Vec<String>>,
    ) -> Result<Self> {
        let manager = McpServerManager::new(ManagerConfig::default());

        // Load MCP configs from standard locations
        let configs = McpConfigFile::load_from_standard_locations();

        let mut server_health = Vec::new();
        let mut tools = Vec::new();
        let mut server_proxies = HashMap::new();
        let mut server_resources = HashMap::new();

        // Count total servers to load (for progress tracking)
        let mut total_servers = 0;
        if !configs.is_empty() {
            for (_config_path, config_file) in &configs {
                for (server_id, server_config) in &config_file.servers {
                    if !server_config.enabled {
                        continue;
                    }
                    // Apply tag filter
                    if let Some(ref tags) = enabled_tags {
                        if !tags.is_empty() && server_config.tags.iter().all(|t| !tags.contains(t))
                        {
                            continue;
                        }
                    }
                    // Apply server ID filter
                    if let Some(ref ids) = enabled_server_ids {
                        if !ids.is_empty() && !ids.contains(server_id) {
                            continue;
                        }
                    }
                    total_servers += 1;
                }
            }
        }

        // Initialize loading state
        let mut loading_state = Some(LoadingState {
            is_loading: true,
            current_server: None,
            servers_loaded: 0,
            servers_total: total_servers,
            current_step: "Initializing".to_string(),
        });

        if configs.is_empty() {
            info!("No MCP server configurations found");
        } else {
            info!("Loading {} MCP server configuration(s)", configs.len());

            // Load servers and tools from each config
            for (_config_path, config_file) in &configs {
                for (server_id, server_config) in &config_file.servers {
                    // Filter by enabled flag
                    if !server_config.enabled {
                        info!("Skipping disabled MCP server '{}'", server_id);
                        continue;
                    }

                    // Filter by tags if specified
                    if let Some(ref tags) = enabled_tags {
                        if !tags.is_empty() && server_config.tags.iter().all(|t| !tags.contains(t))
                        {
                            info!("Skipping MCP server '{}' (no matching tags)", server_id);
                            continue;
                        }
                    }

                    // Filter by server IDs if specified
                    if let Some(ref ids) = enabled_server_ids {
                        if !ids.is_empty() && !ids.contains(server_id) {
                            info!("Skipping MCP server '{}' (not in allowlist)", server_id);
                            continue;
                        }
                    }

                    // Update loading state
                    if let Some(ref mut state) = loading_state {
                        state.current_server = Some(server_id.clone());
                        state.current_step = "Connecting...".to_string();
                    }

                    let command = match server_config.command.clone() {
                        Some(cmd) => cmd,
                        None => {
                            debug!(
                                "Skipping MCP server '{}': no command (remote transport)",
                                server_id
                            );
                            continue;
                        }
                    };

                    let proxy_config = ProxyConfig {
                        server_name: server_id.clone(),
                        command,
                        args: server_config.args.clone(),
                        tool_prefix: None,
                        cache_tools: true,
                    };

                    match ToolProxy::with_discovery(proxy_config).await {
                        Ok(proxy) => {
                            info!("Connected to MCP server '{}'", server_id);

                            // Update loading state - discovering tools
                            if let Some(ref mut state) = loading_state {
                                state.current_step = "Discovering tools...".to_string();
                            }

                            // Get tools from this server
                            let proxied_tools = proxy.tools().await;

                            // Apply tool filtering
                            let filtered_tools: Vec<_> = proxied_tools
                                .into_iter()
                                .filter(|tool| {
                                    let name = tool.name();
                                    // Check denylist first
                                    if server_config.tools_denylist.iter().any(|d| d == name) {
                                        return false;
                                    }
                                    // Check allowlist if specified
                                    if !server_config.tools_allowlist.is_empty() {
                                        return server_config
                                            .tools_allowlist
                                            .iter()
                                            .any(|a| a == name);
                                    }
                                    true
                                })
                                .collect();

                            for proxied_tool in &filtered_tools {
                                tools.push(ToolInfo {
                                    name: proxied_tool.name().to_string(),
                                    description: proxied_tool.description().to_string(),
                                    server_name: server_id.clone(),
                                    input_schema: Some(proxied_tool.parameters_schema()),
                                });
                            }

                            // Try to load resources from this server
                            let resources = Self::load_resources(&proxy, server_id).await;
                            let resource_count = resources.len();

                            server_proxies.insert(server_id.clone(), proxy);
                            server_resources.insert(server_id.clone(), resources);

                            // Add server health
                            server_health.push(ServerHealth {
                                name: server_id.clone(),
                                status: HealthStatus::Healthy,
                                tool_count: filtered_tools.len(),
                                resource_count,
                                last_check: chrono::Utc::now(),
                            });

                            // Update loading state - server loaded
                            if let Some(ref mut state) = loading_state {
                                state.servers_loaded += 1;
                                state.current_step = format!(
                                    "Loaded {}/{}",
                                    state.servers_loaded, state.servers_total
                                );
                            }
                        }
                        Err(e) => {
                            warn!("Failed to connect to MCP server '{}': {}", server_id, e);
                            // Add server as stopped
                            server_health.push(ServerHealth {
                                name: server_id.clone(),
                                status: HealthStatus::Stopped,
                                tool_count: 0,
                                resource_count: 0,
                                last_check: chrono::Utc::now(),
                            });

                            // Update loading state - server failed
                            if let Some(ref mut state) = loading_state {
                                state.servers_loaded += 1;
                                state.current_step = format!("Failed: {}", e);
                            }
                        }
                    }
                }
            }
        }

        // Mark loading as complete
        if let Some(ref mut state) = loading_state {
            state.is_loading = false;
            state.current_step = "Complete".to_string();
        }

        Ok(Self {
            manager: Arc::new(RwLock::new(manager)),
            selected_server: 0,
            selected_tool: 0,
            show_resources: false,
            server_health,
            tools,
            execution_results: Vec::new(),
            search_query: String::new(),
            execution_mode: ExecutionMode::Single,
            server_proxies,
            server_resources,
            loading_state,
        })
    }

    /// Load resources from a server
    async fn load_resources(proxy: &ToolProxy, server_id: &str) -> Vec<ResourceInfo> {
        // Access the client from proxy to list resources
        match proxy.list_resources(server_id).await {
            Ok(resources) => resources
                .into_iter()
                .map(|r| ResourceInfo {
                    uri: r.uri,
                    name: r.name,
                    description: r.description,
                    mime_type: r.mime_type,
                })
                .collect(),
            Err(_) => {
                // Resources may not be enabled for this server
                Vec::new()
            }
        }
    }

    /// Refresh server health status
    pub async fn refresh_health(&mut self) -> Result<()> {
        // Refresh health by checking if proxies are still connected
        for health in &mut self.server_health {
            health.last_check = chrono::Utc::now();
            // Check if server is still connected via the proxy
            if let Some(proxy) = self.server_proxies.get(&health.name) {
                if proxy.is_connected().await {
                    health.status = HealthStatus::Healthy;
                } else {
                    health.status = HealthStatus::Unhealthy("Not connected".to_string());
                }
            }
        }

        let total_tools = self.tools.len();
        self.selected_tool = self.selected_tool.min(total_tools.saturating_sub(1));

        Ok(())
    }

    /// Execute selected tool
    pub async fn execute_tool(&mut self, tool_name: &str, params: Value) -> Result<()> {
        // Check if tool requires confirmation
        let requires_confirmation = self.tool_requires_confirmation(tool_name).await;

        if requires_confirmation {
            // For now, just execute with a warning - actual UI confirmation would go here
            warn!(
                "Tool '{}' requires confirmation (not yet implemented in UI)",
                tool_name
            );
        }

        let start = std::time::Instant::now();

        // Find the server for this tool
        let server_name = self
            .tools
            .iter()
            .find(|t| t.name == tool_name)
            .map(|t| t.server_name.clone())
            .ok_or_else(|| anyhow::anyhow!("Tool '{}' not found", tool_name))?;

        // Get the proxy for this server
        let proxy = self
            .server_proxies
            .get(&server_name)
            .ok_or_else(|| anyhow::anyhow!("Server '{}' not connected", server_name))?;

        // Execute the tool
        let result = proxy.call_tool(tool_name, params).await;

        let execution_result = match result {
            Ok(mcp_result) => {
                // Convert MCP content to text
                let text = mcp_result
                    .content
                    .iter()
                    .filter_map(|c| match c {
                        rustycode_mcp::types::McpContent::Text { text } => Some(text.clone()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("\n");

                ToolExecutionResult {
                    tool_name: tool_name.to_string(),
                    status: ExecutionStatus::Success,
                    result: Some(serde_json::json!({ "output": text })),
                    error: None,
                    execution_time: start.elapsed().as_millis(),
                }
            }
            Err(e) => ToolExecutionResult {
                tool_name: tool_name.to_string(),
                status: ExecutionStatus::Failed,
                result: None,
                error: Some(format!("Tool execution failed: {}", e)),
                execution_time: start.elapsed().as_millis(),
            },
        };

        self.execution_results.push(execution_result);
        Ok(())
    }

    async fn tool_requires_confirmation(&self, tool_name: &str) -> bool {
        let server_name = self
            .tools
            .iter()
            .find(|t| t.name == tool_name)
            .map(|t| t.server_name.clone());

        if let Some(server) = server_name {
            if let Some(proxy) = self.server_proxies.get(&server) {
                return proxy.requires_confirmation(tool_name).await;
            }
        }
        false
    }

    /// Execute multiple tools in parallel
    pub async fn execute_tools_parallel(&mut self, tools: Vec<(String, Value)>) -> Result<()> {
        // Clone the tools to execute (we need owned values for parallel execution)
        let tool_calls: Vec<(String, Value)> = tools.to_vec();

        // Execute all tools sequentially (true parallel would require spawning tasks)
        for (tool_name, params) in tool_calls {
            let result = self.execute_tool(&tool_name, params).await;
            if let Err(e) = result {
                warn!("Parallel tool execution error: {}", e);
            }
        }

        Ok(())
    }

    /// Select next server
    pub fn next_server(&mut self) {
        if !self.server_health.is_empty() {
            self.selected_server = (self.selected_server + 1) % self.server_health.len();
            self.selected_tool = 0;
        }
    }

    /// Select previous server
    pub fn prev_server(&mut self) {
        if !self.server_health.is_empty() {
            self.selected_server = if self.selected_server == 0 {
                self.server_health.len() - 1
            } else {
                self.selected_server - 1
            };
            self.selected_tool = 0;
        }
    }

    /// Select next tool
    pub fn next_tool(&mut self) {
        if self.selected_server >= self.server_health.len() {
            return;
        }
        let server_name = &self.server_health[self.selected_server].name;
        let server_tools = self
            .tools
            .iter()
            .filter(|t| &t.server_name == server_name)
            .count();

        if server_tools > 0 {
            self.selected_tool = (self.selected_tool + 1) % server_tools;
        }
    }

    /// Select previous tool
    pub fn prev_tool(&mut self) {
        if self.selected_server >= self.server_health.len() {
            return;
        }
        let server_name = &self.server_health[self.selected_server].name;
        let server_tools = self
            .tools
            .iter()
            .filter(|t| &t.server_name == server_name)
            .count();

        if server_tools > 0 {
            self.selected_tool = if self.selected_tool == 0 {
                server_tools - 1
            } else {
                self.selected_tool - 1
            };
        }
    }

    /// Toggle resources panel
    pub fn toggle_resources(&mut self) {
        self.show_resources = !self.show_resources;
    }

    /// Switch execution mode
    pub fn switch_execution_mode(&mut self) {
        self.execution_mode = match self.execution_mode {
            ExecutionMode::Single => ExecutionMode::Parallel,
            ExecutionMode::Parallel => ExecutionMode::Single,
        };
    }

    /// Update search query
    pub fn update_search(&mut self, query: String) {
        self.search_query = query;
    }

    /// Render MCP mode UI
    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
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

        // Main content
        let main_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(25), // Servers
                Constraint::Percentage(40), // Tools
                Constraint::Percentage(35), // Details/Results
            ])
            .split(chunks[1]);

        self.render_servers(frame, main_chunks[0]);
        self.render_tools(frame, main_chunks[1]);

        if self.show_resources {
            self.render_resources(frame, main_chunks[2]);
        } else {
            self.render_tool_details(frame, main_chunks[2]);
        }

        // Footer
        self.render_footer(frame, chunks[2]);
    }

    /// Render header
    fn render_header(&self, frame: &mut Frame, area: Rect) {
        // Check if loading is in progress
        let loading_line = if let Some(ref state) = self.loading_state {
            if state.is_loading {
                // Spinner animation frames
                let frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
                let frame_idx = (frame.count() / 5) % frames.len();
                let spinner = frames[frame_idx];

                let progress = if state.servers_total > 0 {
                    format!("{}/{}", state.servers_loaded, state.servers_total)
                } else {
                    String::new()
                };

                let server = state.current_server.as_deref().unwrap_or("...");
                let step = &state.current_step;

                Some(format!(
                    "{} Loading {} - {} ({})",
                    spinner, server, step, progress
                ))
            } else {
                None
            }
        } else {
            None
        };

        let healthy_count = self
            .server_health
            .iter()
            .filter(|h| h.status == HealthStatus::Healthy)
            .count();

        let title = if loading_line.is_some() {
            "MCP Tools (Loading...)".to_string()
        } else {
            format!(
                "MCP Tools ({}/{} healthy)",
                healthy_count,
                self.server_health.len()
            )
        };

        let mut lines = vec![Line::from(Span::styled(
            title,
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ))];

        if let Some(loading_text) = loading_line {
            lines.push(Line::from(Span::styled(
                loading_text,
                Style::default().fg(Color::Yellow),
            )));
        }

        let header = Paragraph::new(lines)
            .block(Block::default().borders(Borders::ALL))
            .alignment(Alignment::Center);

        frame.render_widget(header, area);
    }

    /// Render servers list
    fn render_servers(&self, frame: &mut Frame, area: Rect) {
        let items: Vec<ListItem> = self
            .server_health
            .iter()
            .enumerate()
            .map(|(i, health)| {
                let is_selected = i == self.selected_server;
                let status_icon = match &health.status {
                    HealthStatus::Healthy => "✓",
                    HealthStatus::Unhealthy(_) => "✗",
                    HealthStatus::Stopped => "○",
                    HealthStatus::Starting => "⟳",
                    HealthStatus::Restarting => "↻",
                    #[allow(unreachable_patterns)]
                    _ => "?",
                };

                let style = if is_selected {
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };

                let content = format!(
                    "{} {} ({} tools)",
                    status_icon, health.name, health.tool_count
                );

                ListItem::new(content).style(style)
            })
            .collect();

        let list = List::new(items)
            .block(Block::default().title("Servers").borders(Borders::ALL))
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED));

        frame.render_widget(list, area);
    }

    /// Render tools list
    fn render_tools(&self, frame: &mut Frame, area: Rect) {
        if self.selected_server >= self.server_health.len() {
            let paragraph = Paragraph::new("No server selected")
                .block(Block::default().title("Tools").borders(Borders::ALL));
            frame.render_widget(paragraph, area);
            return;
        }
        let current_server = &self.server_health[self.selected_server];
        let server_tools: Vec<_> = self
            .tools
            .iter()
            .filter(|t| t.server_name == current_server.name)
            .collect();

        let items: Vec<ListItem> = server_tools
            .iter()
            .enumerate()
            .map(|(i, tool)| {
                let is_selected = i == self.selected_tool;
                let style = if is_selected {
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };

                let content = format!("{} - {}", tool.name, tool.description);
                ListItem::new(content).style(style)
            })
            .collect();

        let list = List::new(items)
            .block(Block::default().title("Tools").borders(Borders::ALL))
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED));

        frame.render_widget(list, area);
    }

    /// Render tool details
    fn render_tool_details(&self, frame: &mut Frame, area: Rect) {
        if self.selected_server >= self.server_health.len() {
            let paragraph = Paragraph::new("No server selected")
                .block(Block::default().title("Tool Details").borders(Borders::ALL));
            frame.render_widget(paragraph, area);
            return;
        }
        let current_server = &self.server_health[self.selected_server];
        let server_tools: Vec<_> = self
            .tools
            .iter()
            .filter(|t| t.server_name == current_server.name)
            .collect();

        if server_tools.is_empty() {
            let paragraph = Paragraph::new("No tools available")
                .block(Block::default().title("Tool Details").borders(Borders::ALL));
            frame.render_widget(paragraph, area);
            return;
        }

        let idx = self.selected_tool.min(server_tools.len() - 1);
        let tool = &server_tools[idx];

        let details = vec![
            Line::from(vec![
                Span::styled("Name: ", Style::default().fg(Color::Cyan)),
                Span::styled(&tool.name, Style::default().add_modifier(Modifier::BOLD)),
            ]),
            Line::from(vec![
                Span::styled("Description: ", Style::default().fg(Color::Cyan)),
                Span::raw(&tool.description),
            ]),
            Line::from(vec![
                Span::styled("Server: ", Style::default().fg(Color::Cyan)),
                Span::raw(&tool.server_name),
            ]),
            Line::from(""),
            Line::from("Execution:"),
            Line::from(vec![
                Span::styled("Mode: ", Style::default().fg(Color::Cyan)),
                Span::raw(match self.execution_mode {
                    ExecutionMode::Single => "Single",
                    ExecutionMode::Parallel => "Parallel",
                }),
            ]),
            Line::from(vec![
                Span::styled("Recent Results: ", Style::default().fg(Color::Cyan)),
                Span::raw(format!("{}", self.execution_results.len())),
            ]),
        ];

        let paragraph = Paragraph::new(details)
            .block(Block::default().title("Tool Details").borders(Borders::ALL))
            .wrap(Wrap { trim: false });

        frame.render_widget(paragraph, area);
    }

    /// Render resources panel
    fn render_resources(&self, frame: &mut Frame, area: Rect) {
        if self.selected_server >= self.server_health.len() {
            let paragraph = Paragraph::new("No server selected")
                .block(Block::default().title("Resources").borders(Borders::ALL));
            frame.render_widget(paragraph, area);
            return;
        }
        let current_server = &self.server_health[self.selected_server];

        let resources = vec![
            Line::from(vec![
                Span::styled("Resources for: ", Style::default().fg(Color::Cyan)),
                Span::styled(
                    &current_server.name,
                    Style::default().add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(""),
            Line::from(format!(
                "Total resources: {}",
                current_server.resource_count
            )),
            Line::from(""),
            Line::from("Example resources:"),
            Line::from("• config://app/settings"),
            Line::from("• file://README.md"),
            Line::from("• prompt://system"),
        ];

        let paragraph = Paragraph::new(resources)
            .block(Block::default().title("Resources").borders(Borders::ALL))
            .wrap(Wrap { trim: false });

        frame.render_widget(paragraph, area);
    }

    /// Render footer with keybindings
    fn render_footer(&self, frame: &mut Frame, area: Rect) {
        let help_text = vec![
            Span::styled("E", Style::default().fg(Color::Green)),
            Span::raw(": Execute "),
            Span::styled("P", Style::default().fg(Color::Green)),
            Span::raw(": Parallel "),
            Span::styled("R", Style::default().fg(Color::Green)),
            Span::raw(": Resources "),
            Span::styled("H", Style::default().fg(Color::Green)),
            Span::raw(": Health "),
            Span::styled("↑↓", Style::default().fg(Color::Green)),
            Span::raw(": Navigate "),
            Span::styled("Q", Style::default().fg(Color::Green)),
            Span::raw(": Quit"),
        ];

        let footer = Paragraph::new(Line::from(help_text))
            .block(Block::default().borders(Borders::ALL))
            .alignment(Alignment::Center);

        frame.render_widget(footer, area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mcp_mode_new() {
        let mode = McpMode::new().await.unwrap();
        assert_eq!(mode.selected_server, 0);
        assert_eq!(mode.selected_tool, 0);
        assert!(!mode.show_resources);
        // Server count depends on config files present - just verify it initializes
    }

    #[test]
    fn test_execution_mode_switch() {
        let mut mode = McpMode {
            manager: Arc::new(RwLock::new(McpServerManager::new(ManagerConfig::default()))),
            selected_server: 0,
            selected_tool: 0,
            show_resources: false,
            server_health: Vec::new(),
            tools: Vec::new(),
            execution_results: Vec::new(),
            search_query: String::new(),
            execution_mode: ExecutionMode::Single,
            server_proxies: HashMap::new(),
            server_resources: HashMap::new(),
            loading_state: None,
        };

        mode.switch_execution_mode();
        assert_eq!(mode.execution_mode, ExecutionMode::Parallel);

        mode.switch_execution_mode();
        assert_eq!(mode.execution_mode, ExecutionMode::Single);
    }

    #[test]
    fn test_loading_state() {
        let loading = LoadingState {
            is_loading: true,
            current_server: Some("test-server".to_string()),
            servers_loaded: 2,
            servers_total: 5,
            current_step: "Discovering tools...".to_string(),
        };

        assert!(loading.is_loading);
        assert_eq!(loading.servers_loaded, 2);
        assert_eq!(loading.servers_total, 5);
        assert_eq!(loading.current_server, Some("test-server".to_string()));
    }
}
