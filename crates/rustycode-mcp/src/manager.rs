//! Enterprise-grade MCP server lifecycle management

use crate::client::McpClient;
use crate::headers_helper::merge_headers;
use crate::http_transport::HttpTransport;
use crate::oauth::OAuthManager;
use crate::protocol::McpEvent;
use crate::server_enablement::ServerEnablementManager;
use crate::types::McpTool;
use crate::{McpError, McpResult, Transport};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Configuration for server manager
#[derive(Debug, Clone)]
pub struct ManagerConfig {
    pub health_check_interval: Duration,
    /// Maximum restart attempts
    pub max_restart_attempts: usize,
    pub restart_backoff_multiplier: f64,
    pub initial_restart_delay: Duration,
    pub request_timeout: Duration,
    pub connection_timeout: Duration,
}

impl Default for ManagerConfig {
    fn default() -> Self {
        Self {
            health_check_interval: Duration::from_secs(30),
            max_restart_attempts: 5,
            restart_backoff_multiplier: 2.0,
            initial_restart_delay: Duration::from_millis(500),
            request_timeout: Duration::from_secs(30),
            connection_timeout: Duration::from_secs(10),
        }
    }
}

/// MCP transport type for remote server connections
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum McpTransportType {
    Stdio,
    Http,
    Sse,
}

/// OAuth configuration for remote MCP servers
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpOAuthConfig {
    pub client_id: String,
    #[serde(default)]
    pub scopes: Option<String>,
    #[serde(default)]
    pub callback_port: Option<u16>,
}

/// Server configuration for spawning
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// Unique server identifier
    #[serde(alias = "server_id")]
    pub server_id: String,
    /// Server name
    pub name: String,
    /// Transport type — auto-detected if omitted
    #[serde(default, rename = "type", alias = "transport_type")]
    pub transport_type: Option<McpTransportType>,
    /// Command to spawn (stdio)
    #[serde(default, alias = "command")]
    pub command: Option<String>,
    /// Arguments for command (stdio)
    #[serde(default)]
    pub args: Vec<String>,
    /// Remote server URL (http/sse)
    #[serde(default)]
    pub url: Option<String>,
    /// HTTP headers for remote servers
    #[serde(default)]
    pub headers: Option<HashMap<String, String>>,
    /// Path to script that outputs dynamic headers as JSON
    #[serde(default, rename = "headersHelper", alias = "headers_helper")]
    pub headers_helper: Option<String>,
    /// Server description
    #[serde(default)]
    pub description: Option<String>,
    /// OAuth configuration for remote servers
    #[serde(default)]
    pub oauth: Option<McpOAuthConfig>,
    /// Enabled capabilities
    #[serde(default, alias = "enable_tools")]
    pub enable_tools: bool,
    #[serde(default, alias = "enable_resources")]
    pub enable_resources: bool,
    #[serde(default, alias = "enable_prompts")]
    pub enable_prompts: bool,
    /// Whether this server is enabled (can be toggled per-session)
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Optional: only load these tools from the server (empty = all tools)
    #[serde(default, alias = "tools_allowlist")]
    pub tools_allowlist: Vec<String>,
    /// Optional: never load these tools from the server
    #[serde(default, alias = "tools_denylist")]
    pub tools_denylist: Vec<String>,
    /// Optional: tags for filtering servers by context
    #[serde(default)]
    pub tags: Vec<String>,
}

const fn default_true() -> bool {
    true
}

fn resolve_transport_type(config: &ServerConfig) -> McpResult<McpTransportType> {
    if let Some(transport_type) = &config.transport_type {
        return Ok(transport_type.clone());
    }

    if config.command.is_some() {
        return Ok(McpTransportType::Stdio);
    }

    if config.url.is_some() {
        return Ok(McpTransportType::Http);
    }

    Err(McpError::InvalidRequest(format!(
        "Server '{}' must declare either a command or a url",
        config.server_id
    )))
}

async fn build_transport_for_server(
    oauth_manager: &OAuthManager,
    config: &ServerConfig,
) -> McpResult<Box<dyn Transport>> {
    match resolve_transport_type(config)? {
        McpTransportType::Stdio => {
            let command = config.command.as_deref().ok_or_else(|| {
                McpError::InvalidRequest(format!(
                    "Server '{}' has no command (required for stdio transport)",
                    config.server_id
                ))
            })?;
            let args: Vec<&str> = config.args.iter().map(String::as_str).collect();
            let transport = crate::StdioTransport::spawn(command, &args)?;
            Ok(Box::new(transport))
        }
        McpTransportType::Http | McpTransportType::Sse => {
            let url = config.url.as_deref().ok_or_else(|| {
                McpError::InvalidRequest(format!(
                    "Server '{}' has no url (required for remote transport)",
                    config.server_id
                ))
            })?;

            let static_headers = config.headers.clone().unwrap_or_default();
            let mut headers =
                merge_headers(&static_headers, config.headers_helper.as_deref()).await;

            if oauth_manager.has_oauth_config(&config.server_id).await {
                let token = oauth_manager.access_token(&config.server_id).await?;
                headers.insert("Authorization".to_string(), format!("Bearer {token}"));
            }

            match resolve_transport_type(config)? {
                McpTransportType::Http => Ok(Box::new(HttpTransport::new(url, headers)?)),
                McpTransportType::Sse => Ok(Box::new(HttpTransport::new(url, headers)?)),
                McpTransportType::Stdio => unreachable!(),
            }
        }
    }
}

/// MCP config file structure (`.mcp.json`)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct McpConfigFile {
    /// MCP servers configuration — accepts both "mcpServers" and "servers"
    #[serde(default, alias = "mcpServers")]
    pub servers: HashMap<String, ServerConfig>,
}

impl McpConfigFile {
    /// Load MCP config from a file path
    pub fn load_from_path(path: &Path) -> Result<Self, String> {
        let content =
            fs::read_to_string(path).map_err(|e| format!("Failed to read MCP config file: {e}"))?;

        let config: Self = serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse MCP config file: {e}"))?;

        Ok(config)
    }

    /// Load MCP config from standard locations
    /// Checks: ./mcp.json, ~/.rustycode/mcp-servers.json, ~/.mcp.json
    pub fn load_from_standard_locations() -> Vec<(PathBuf, Self)> {
        let mut configs = Vec::new();

        let mut locations: Vec<PathBuf> = vec![PathBuf::from("mcp.json")];

        // Add home directory locations
        if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
            let mut rustycode_config = PathBuf::from(&home);
            rustycode_config.push(".rustycode");
            rustycode_config.push("mcp-servers.json");
            locations.push(rustycode_config);

            let mut home_config = PathBuf::from(&home);
            home_config.push(".mcp.json");
            locations.push(home_config);
        }

        for location in locations {
            if location.exists() {
                if let Ok(config) = Self::load_from_path(&location) {
                    configs.push((location, config));
                }
            }
        }

        configs
    }

    /// Get all servers from this config
    pub fn servers(&self) -> Vec<&ServerConfig> {
        self.servers.values().collect()
    }
}

/// Server health status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[non_exhaustive]
pub enum HealthStatus {
    /// Server is healthy
    Healthy,
    /// Server is unhealthy but running
    Unhealthy(String),
    /// Server is stopped
    Stopped,
    /// Server is starting
    Starting,
    /// Server is restarting
    Restarting,
}

/// Server state
struct ServerState {
    config: ServerConfig,
    status: HealthStatus,
    restart_count: usize,
    last_health_check: Option<Instant>,
    last_error: Option<String>,
    /// Cached tool definitions (populated at connect time)
    cached_tools: Vec<McpTool>,
    /// When tools were last fetched/refreshed
    last_tools_check: Option<Instant>,
    /// Whether the cached tools are stale
    tools_stale: bool,
    /// Consecutive reconnection attempts (for backoff)
    reconnection_attempts: usize,
    /// When the last reconnection was attempted
    last_reconnection_attempt: Option<Instant>,
    /// Receiver for MCP events (tools changed, progress, etc.)
    notification_rx: tokio::sync::broadcast::Receiver<McpEvent>,
}

/// Managed MCP server
#[derive(Clone)]
pub struct McpServer {
    server_id: String,
    client: Arc<RwLock<McpClient>>,
    state: Arc<RwLock<ServerState>>,
}

impl std::fmt::Debug for McpServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McpServer")
            .field("server_id", &self.server_id)
            .field("state", &"<state>")
            .finish()
    }
}

/// Enterprise-grade MCP server manager
pub struct McpServerManager {
    servers: Arc<RwLock<HashMap<String, McpServer>>>,
    config: ManagerConfig,
    oauth_manager: OAuthManager,
    health_check_handle: Option<tokio::task::JoinHandle<()>>,
    enablement_manager: ServerEnablementManager,
}

impl McpServerManager {
    pub fn new(config: ManagerConfig) -> Self {
        Self {
            servers: Arc::new(RwLock::new(HashMap::new())),
            config,
            oauth_manager: OAuthManager::new(),
            health_check_handle: None,
            enablement_manager: ServerEnablementManager::default(),
        }
    }

    /// Create with default configuration
    pub fn default_config() -> Self {
        Self::new(ManagerConfig::default())
    }

    /// Access the OAuth manager used for remote server authentication.
    pub const fn oauth_manager(&self) -> &OAuthManager {
        &self.oauth_manager
    }

    async fn build_transport(&self, config: &ServerConfig) -> McpResult<Box<dyn Transport>> {
        build_transport_for_server(&self.oauth_manager, config).await
    }

    /// Start an MCP server
    pub async fn start_server(&mut self, config: ServerConfig) -> McpResult<McpServer> {
        info!("Starting MCP server '{}'", config.server_id);

        // Check server enablement
        let load_result = self.enablement_manager.can_load_server(&config.server_id);
        if !load_result.allowed {
            let reason = load_result
                .reason
                .unwrap_or_else(|| "Server is disabled".to_string());
            warn!("Server '{}' cannot be loaded: {}", config.server_id, reason);
            return Err(McpError::InvalidRequest(reason));
        }

        let servers = self.servers.read().await;
        if servers.contains_key(&config.server_id) {
            return Err(McpError::InvalidRequest(format!(
                "Server '{}' already exists",
                config.server_id
            )));
        }
        drop(servers);

        let client_config = crate::client::McpClientConfig {
            timeout_secs: self.config.request_timeout.as_secs(),
            ..Default::default()
        };

        let mut client = McpClient::new(client_config);
        let transport = self.build_transport(&config).await?;

        // Connect with timeout
        tokio::time::timeout(
            self.config.connection_timeout,
            client.connect_with_transport(&config.server_id, transport),
        )
        .await
        .map_err(|_| McpError::Timeout)?
        .map_err(|e| {
            McpError::ConnectionError(format!(
                "Failed to connect to server '{}': {}",
                config.server_id, e
            ))
        })?;

        let notification_rx = client.subscribe_events();

        let state = ServerState {
            config: config.clone(),
            status: HealthStatus::Healthy,
            restart_count: 0,
            last_health_check: Some(Instant::now()),
            last_error: None,
            cached_tools: Vec::new(),
            last_tools_check: None,
            tools_stale: true, // Mark as stale initially, will be populated after connect
            reconnection_attempts: 0,
            last_reconnection_attempt: None,
            notification_rx,
        };

        let server = McpServer {
            server_id: config.server_id.clone(),
            client: Arc::new(RwLock::new(client)),
            state: Arc::new(RwLock::new(state)),
        };

        let mut servers = self.servers.write().await;
        servers.insert(config.server_id.clone(), server.clone());
        drop(servers);

        // Populate the tools cache after server starts
        // This is done after inserting to avoid borrow issues
        if config.enable_tools {
            match server.refresh_cached_tools().await {
                Ok(()) => {
                    info!("Cached tools for MCP server '{}'", config.server_id);
                }
                Err(e) => {
                    warn!(
                        "Failed to cache tools for server '{}': {}",
                        config.server_id, e
                    );
                    // Don't fail the server start, tools can be cached later
                }
            }
        }

        info!("MCP server '{}' started successfully", config.server_id);
        Ok(server)
    }

    /// Stop an MCP server
    pub async fn stop_server(&mut self, server_id: &str) -> McpResult<()> {
        info!("Stopping MCP server '{}'", server_id);

        let mut servers = self.servers.write().await;
        let server = servers
            .get(server_id)
            .ok_or_else(|| McpError::ServerNotFound(server_id.to_string()))?;

        // Disconnect client
        let mut client = server.client.write().await;
        client.disconnect(server_id).await?;
        drop(client);

        // Remove from servers
        servers.remove(server_id);

        info!("MCP server '{}' stopped", server_id);
        Ok(())
    }

    /// Restart an MCP server
    pub async fn restart_server(&mut self, server_id: &str) -> McpResult<()> {
        info!("Restarting MCP server '{}'", server_id);

        // Get current config before stopping
        let config = {
            let servers = self.servers.read().await;
            let server = servers
                .get(server_id)
                .ok_or_else(|| McpError::ServerNotFound(server_id.to_string()))?;
            let state = server.state.read().await;
            state.config.clone()
        };

        // Stop the server
        self.stop_server(server_id).await?;

        // Start the server again
        self.start_server(config).await?;

        info!("MCP server '{}' restarted", server_id);
        Ok(())
    }

    /// Health check for a server
    pub async fn health_check(&self, server_id: &str) -> HealthStatus {
        let servers = self.servers.read().await;
        let server = match servers.get(server_id) {
            Some(s) => s,
            None => return HealthStatus::Stopped,
        };

        // Check if client is connected
        let client = server.client.read().await;
        let is_connected = client.is_connected(server_id).await;
        drop(client);

        if !is_connected {
            return HealthStatus::Unhealthy("Not connected".to_string());
        }

        // Update health check time
        {
            let mut state = server.state.write().await;
            state.last_health_check = Some(Instant::now());
            state.status.clone()
        }
    }

    /// Get server by ID
    pub async fn server(&self, server_id: &str) -> Option<McpServer> {
        let servers = self.servers.read().await;
        servers.get(server_id).cloned()
    }

    /// List all servers
    pub async fn list_servers(&self) -> Vec<String> {
        let servers = self.servers.read().await;
        servers.keys().cloned().collect()
    }

    /// Stop all running MCP servers. Call on app exit to prevent orphaned processes.
    pub async fn shutdown(&self) {
        let server_ids: Vec<String> = {
            let servers = self.servers.read().await;
            servers.keys().cloned().collect()
        };

        if server_ids.is_empty() {
            return;
        }

        info!("Shutting down {} MCP server(s)", server_ids.len());

        for server_id in server_ids {
            let mut servers = self.servers.write().await;
            if let Some(server) = servers.remove(&server_id) {
                let mut client = server.client.write().await;
                if let Err(e) = client.disconnect(&server_id).await {
                    warn!("Error disconnecting MCP server '{}': {}", server_id, e);
                }
            }
        }

        info!("All MCP servers shut down");
    }

    /// Load and start servers from a config file
    pub async fn load_from_config_file(&mut self, config_path: &Path) -> Result<usize, String> {
        let config_file = McpConfigFile::load_from_path(config_path)?;
        let mut started = 0;

        for (server_id, server_config) in config_file.servers {
            info!("Loading MCP server '{}' from config", server_id);
            match self.start_server(server_config).await {
                Ok(_) => {
                    started += 1;
                    info!("Successfully started MCP server '{}'", server_id);
                }
                Err(e) => {
                    warn!("Failed to start MCP server '{}': {}", server_id, e);
                }
            }
        }

        Ok(started)
    }

    /// Load servers from standard config locations
    pub async fn load_from_standard_locations(&mut self) -> Result<usize, String> {
        let configs = McpConfigFile::load_from_standard_locations();
        let mut total_started = 0;

        for (path, config_file) in configs {
            info!("Loading MCP servers from {:?}", path);
            for (server_id, server_config) in config_file.servers {
                info!("Loading MCP server '{}' from {:?}", server_id, path);
                match self.start_server(server_config).await {
                    Ok(_) => {
                        total_started += 1;
                        info!("Successfully started MCP server '{}'", server_id);
                    }
                    Err(e) => {
                        warn!("Failed to start MCP server '{}': {}", server_id, e);
                    }
                }
            }
        }

        Ok(total_started)
    }

    /// Start health monitoring task with auto-reconnect
    pub fn start_health_monitoring(&mut self) {
        if self.health_check_handle.is_some() {
            warn!("Health monitoring already started");
            return;
        }

        let servers = self.servers.clone();
        let oauth_manager = self.oauth_manager.clone();
        let interval = self.config.health_check_interval;
        let max_restart_attempts = self.config.max_restart_attempts;
        let restart_backoff_multiplier = self.config.restart_backoff_multiplier;
        let initial_restart_delay = self.config.initial_restart_delay;

        let handle = tokio::spawn(async move {
            let mut interval_timer = tokio::time::interval(interval);
            loop {
                interval_timer.tick().await;

                let server_ids: Vec<String> = {
                    let servers_read = servers.read().await;
                    servers_read.keys().cloned().collect()
                };

                for server_id in server_ids {
                    let servers_guard = servers.read().await;
                    if let Some(server) = servers_guard.get(&server_id) {
                        let client_lock = server.client();
                        let client = client_lock.read().await;
                        let is_connected = client.is_connected(&server_id).await;
                        drop(client);

                        if is_connected {
                            // Server is connected - reset reconnection counter
                            let mut state = server.state.write().await;
                            if state.reconnection_attempts > 0 {
                                info!("Server '{}' reconnected successfully, resetting attempt counter", server_id);
                                state.reconnection_attempts = 0;
                                state.last_reconnection_attempt = None;
                                state.status = HealthStatus::Healthy;
                            }
                            drop(state);

                            // Drain pending notifications from the transport inbox
                            // and process tool-change events
                            {
                                let client = client_lock.read().await;
                                client.drain_notifications(&server_id).await;
                            }

                            // Check for dispatched events (non-blocking)
                            let needs_refresh = {
                                let mut state = server.state.write().await;
                                let mut refresh = false;
                                loop {
                                    match state.notification_rx.try_recv() {
                                        Ok(McpEvent::ToolsListChanged { .. }) => {
                                            info!("Server '{}' sent tools/list_changed, marking stale", server_id);
                                            state.tools_stale = true;
                                            refresh = true;
                                        }
                                        Ok(_) => {} // ignore other events
                                        Err(_) => break,
                                    }
                                }
                                refresh
                            };

                            if needs_refresh {
                                if let Err(e) = server.refresh_cached_tools().await {
                                    warn!(
                                        "Failed to refresh tools for server '{}': {}",
                                        server_id, e
                                    );
                                }
                            }
                        } else {
                            warn!("Server '{}' is not connected", server_id);

                            // Get server config and check if we should attempt reconnection
                            let (
                                should_reconnect,
                                server_config,
                                reconnection_attempts,
                                last_reconnection_attempt,
                            ) = {
                                let state = server.state.read().await;
                                (
                                    state.reconnection_attempts < max_restart_attempts,
                                    state.config.clone(),
                                    state.reconnection_attempts,
                                    state.last_reconnection_attempt,
                                )
                            };

                            if should_reconnect {
                                // Calculate backoff delay (capped at 5 minutes)
                                let backoff_ms = (initial_restart_delay.as_millis() as u64)
                                    .saturating_mul(
                                        restart_backoff_multiplier
                                            .powf(reconnection_attempts as f64)
                                            as u64,
                                    )
                                    .min(300_000);
                                let backoff_duration = Duration::from_millis(backoff_ms);

                                // Check if enough time has passed since last attempt
                                let can_reconnect = last_reconnection_attempt
                                    .is_none_or(|last| last.elapsed() >= backoff_duration);

                                if can_reconnect {
                                    info!(
                                        "Marking server '{}' for reconnection (attempt {})",
                                        server_id,
                                        reconnection_attempts + 1
                                    );

                                    // Update state to restarting
                                    {
                                        let mut state = server.state.write().await;
                                        state.reconnection_attempts = reconnection_attempts + 1;
                                        state.last_reconnection_attempt = Some(Instant::now());
                                        state.status = HealthStatus::Restarting;
                                    }

                                    // Actually attempt reconnection
                                    let server = server.clone();
                                    let oauth_manager = oauth_manager.clone();
                                    if let Err(e) = server
                                        .reconnect(
                                            &server_config,
                                            &oauth_manager,
                                            Duration::from_secs(10),
                                            Duration::from_secs(30),
                                        )
                                        .await
                                    {
                                        warn!(
                                            "Reconnection failed for server '{}': {}",
                                            server_id, e
                                        );
                                        let mut state = server.state.write().await;
                                        state.status = HealthStatus::Unhealthy(e.to_string());
                                    } else {
                                        info!("Successfully reconnected to server '{}'", server_id);
                                    }
                                }
                            } else {
                                warn!(
                                    "Server '{}' exceeded max reconnection attempts ({})",
                                    server_id, max_restart_attempts
                                );
                                let mut state = server.state.write().await;
                                state.status = HealthStatus::Unhealthy(
                                    "Max reconnection attempts reached".to_string(),
                                );
                            }
                        }
                    }
                }
            }
        });

        self.health_check_handle = Some(handle);
        info!("Health monitoring with auto-reconnect started");
    }

    /// Stop health monitoring
    pub async fn stop_health_monitoring(&mut self) {
        if let Some(handle) = self.health_check_handle.take() {
            handle.abort();
            info!("Health monitoring stopped");
        }
    }

    /// Get the enablement manager
    pub const fn enablement_manager(&self) -> &ServerEnablementManager {
        &self.enablement_manager
    }

    /// Get the enablement manager (mutable)
    pub const fn enablement_manager_mut(&mut self) -> &mut ServerEnablementManager {
        &mut self.enablement_manager
    }

    /// Set admin configuration for server enablement
    pub fn set_admin_enablement(
        &mut self,
        enabled: bool,
        allowlist: Option<Vec<String>>,
        excludelist: Option<Vec<String>>,
    ) {
        self.enablement_manager
            .set_admin_config(enabled, allowlist, excludelist);
    }

    /// Check if a server can be loaded
    pub async fn can_load_server(&self, server_id: &str) -> bool {
        self.enablement_manager.can_load_server(server_id).allowed
    }

    /// Get display state for a server
    pub async fn server_display_state(
        &self,
        server_id: &str,
    ) -> crate::server_enablement::ServerDisplayState {
        self.enablement_manager.display_state(server_id).await
    }
}

impl McpServer {
    /// Get server ID
    pub fn id(&self) -> &str {
        &self.server_id
    }

    /// Get server status
    pub async fn status(&self) -> HealthStatus {
        let state = self.state.read().await;
        state.status.clone()
    }

    /// Get server config
    pub async fn config(&self) -> ServerConfig {
        let state = self.state.read().await;
        state.config.clone()
    }

    /// Get access to the underlying client
    pub fn client(&self) -> Arc<RwLock<McpClient>> {
        self.client.clone()
    }

    /// Get restart count
    pub async fn restart_count(&self) -> usize {
        let state = self.state.read().await;
        state.restart_count
    }

    /// Get last error
    pub async fn last_error(&self) -> Option<String> {
        let state = self.state.read().await;
        state.last_error.clone()
    }

    /// Get cached tools (zero network calls)
    pub async fn cached_tools(&self) -> Vec<McpTool> {
        let state = self.state.read().await;
        state.cached_tools.clone()
    }

    /// Check if cached tools are stale
    pub async fn are_tools_stale(&self) -> bool {
        let state = self.state.read().await;
        state.tools_stale
    }

    /// Refresh the cached tools from the server
    pub async fn refresh_cached_tools(&self) -> McpResult<()> {
        debug!("Refreshing cached tools for server '{}'", self.server_id);

        let client = self.client.read().await;
        let tools = client.list_tools(&self.server_id).await?;
        drop(client);

        let mut state = self.state.write().await;
        state.cached_tools = tools;
        state.last_tools_check = Some(Instant::now());
        state.tools_stale = false;

        debug!(
            "Cached {} tools for server '{}'",
            state.cached_tools.len(),
            self.server_id
        );
        Ok(())
    }

    /// Mark cached tools as stale (triggering refresh on next access)
    pub async fn mark_tools_stale(&self) {
        let mut state = self.state.write().await;
        state.tools_stale = true;
    }

    /// Get tools, refreshing from cache if stale
    pub async fn tools(&self) -> McpResult<Vec<McpTool>> {
        // If tools are stale or empty, refresh them
        if self.are_tools_stale().await {
            self.refresh_cached_tools().await?;
        }
        Ok(self.cached_tools().await)
    }

    /// Get reconnection attempts
    pub async fn reconnection_attempts(&self) -> usize {
        let state = self.state.read().await;
        state.reconnection_attempts
    }

    /// Check if server needs reconnection
    pub async fn needs_reconnection(&self) -> bool {
        let state = self.state.read().await;
        state.status == HealthStatus::Restarting
    }

    /// Reconnect the server client (called from health monitoring loop)
    /// This is a helper that the `McpServerManager` calls during auto-reconnect
    pub async fn reconnect(
        &self,
        config: &ServerConfig,
        oauth_manager: &OAuthManager,
        connection_timeout: Duration,
        request_timeout: Duration,
    ) -> McpResult<()> {
        info!("Attempting to reconnect to server '{}'", self.server_id);

        // Create new client
        let client_config = crate::client::McpClientConfig {
            timeout_secs: request_timeout.as_secs(),
            ..Default::default()
        };

        let mut client = McpClient::new(client_config);
        let transport = build_transport_for_server(oauth_manager, config).await?;

        // Connect with timeout
        tokio::time::timeout(
            connection_timeout,
            client.connect_with_transport(&config.server_id, transport),
        )
        .await
        .map_err(|_| McpError::Timeout)?
        .map_err(|e| {
            McpError::ConnectionError(format!(
                "Failed to reconnect to server '{}': {}",
                config.server_id, e
            ))
        })?;

        // Update the server's client
        {
            let mut server_client = self.client.write().await;
            *server_client = client;
        }

        // Refresh tools cache
        if config.enable_tools {
            match self.refresh_cached_tools().await {
                Ok(()) => {
                    info!(
                        "Refreshed tools cache for reconnected server '{}'",
                        self.server_id
                    );
                }
                Err(e) => {
                    warn!(
                        "Failed to refresh tools cache for server '{}': {}",
                        self.server_id, e
                    );
                }
            }
        }

        // Update state to healthy
        {
            let mut state = self.state.write().await;
            state.status = HealthStatus::Healthy;
            state.last_error = None;
        }

        info!("Successfully reconnected to server '{}'", self.server_id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_manager_creation() {
        let manager = McpServerManager::default_config();
        let servers = manager.list_servers().await;
        assert!(servers.is_empty());
    }

    #[tokio::test]
    async fn test_health_status() {
        let status1 = HealthStatus::Healthy;
        let status2 = HealthStatus::Healthy;
        assert_eq!(status1, status2);

        let status3 = HealthStatus::Unhealthy("error".to_string());
        assert_ne!(status1, status3);
    }

    #[tokio::test]
    async fn test_server_config() {
        let config = ServerConfig {
            server_id: "test-server".to_string(),
            name: "Test Server".to_string(),
            command: Some("echo".to_string()),
            args: vec!["hello".to_string()],
            transport_type: None,
            url: None,
            headers: None,
            headers_helper: None,
            description: None,
            oauth: None,
            enable_tools: true,
            enable_resources: false,
            enable_prompts: false,
            enabled: true,
            tools_allowlist: vec![],
            tools_denylist: vec![],
            tags: vec![],
        };

        assert_eq!(config.server_id, "test-server");
        assert_eq!(config.args.len(), 1);
    }

    #[test]
    fn test_mcp_config_file_default() {
        let config = McpConfigFile::default();
        assert!(config.servers.is_empty());
    }

    #[test]
    fn test_mcp_config_file_from_json() {
        use serde_json::json;

        let json = json!({
            "servers": {
                "test": {
                    "server_id": "test",
                    "name": "Test",
                    "command": "echo",
                    "args": [],
                    "enable_tools": true,
                    "enable_resources": false,
                    "enable_prompts": false
                }
            }
        });

        let config: McpConfigFile = serde_json::from_value(json).unwrap();
        assert_eq!(config.servers.len(), 1);
        assert!(config.servers.contains_key("test"));
    }

    #[test]
    fn test_mcp_config_get_servers() {
        let mut config = McpConfigFile::default();
        config.servers.insert(
            "server1".to_string(),
            ServerConfig {
                server_id: "server1".to_string(),
                name: "Server 1".to_string(),
                command: Some("echo".to_string()),
                args: vec![],
                transport_type: None,
                url: None,
                headers: None,
                headers_helper: None,
                description: None,
                oauth: None,
                enable_tools: true,
                enable_resources: false,
                enable_prompts: false,
                enabled: true,
                tools_allowlist: vec![],
                tools_denylist: vec![],
                tags: vec![],
            },
        );

        let servers = config.servers();
        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0].name, "Server 1");
    }

    #[test]
    fn test_initialize_response_deserialization() {
        use crate::types::InitializeResponse;
        use serde_json::json;

        // Test that camelCase JSON is correctly deserialized to snake_case struct
        let json = json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {
                "tools": {
                    "listChanged": true
                }
            },
            "serverInfo": {
                "name": "test-server",
                "version": "1.0.0"
            }
        });

        let response: InitializeResponse = serde_json::from_value(json).unwrap();
        assert_eq!(response.protocol_version, "2024-11-05");
        assert_eq!(response.server_info.name, "test-server");
    }

    #[tokio::test]
    async fn test_server_enablement_integration() {
        use tempfile::TempDir;

        // Create a temp directory for the enablement config
        let temp_dir = TempDir::new().unwrap();
        let mut manager = McpServerManager::new(ManagerConfig::default());

        // Override the enablement manager with a test config path
        manager.enablement_manager =
            ServerEnablementManager::with_config_path(Some(temp_dir.path().to_path_buf())).unwrap();

        // By default, all servers should be enabled
        assert!(manager.can_load_server("test-server").await);

        // Disable the server
        manager
            .enablement_manager
            .disable_server("test-server")
            .unwrap();
        assert!(!manager.can_load_server("test-server").await);

        // Re-enable the server
        manager
            .enablement_manager
            .enable_server("test-server")
            .unwrap();
        assert!(manager.can_load_server("test-server").await);
    }

    #[tokio::test]
    async fn test_server_enablement_admin_kill_switch() {
        let mut manager = McpServerManager::new(ManagerConfig::default());

        // Disable admin kill switch (all servers blocked)
        manager.set_admin_enablement(false, None, None);

        assert!(!manager.can_load_server("any-server").await);
    }

    #[tokio::test]
    async fn test_server_enablement_allowlist() {
        let mut manager = McpServerManager::new(ManagerConfig::default());

        // Set allowlist with only "allowed-server"
        manager.set_admin_enablement(true, Some(vec!["allowed-server".to_string()]), None);

        assert!(manager.can_load_server("allowed-server").await);
        assert!(!manager.can_load_server("other-server").await);
    }

    #[tokio::test]
    async fn test_server_enablement_excludelist() {
        let mut manager = McpServerManager::new(ManagerConfig::default());

        // Set excludelist with "blocked-server"
        manager.set_admin_enablement(true, None, Some(vec!["blocked-server".to_string()]));

        assert!(manager.can_load_server("allowed-server").await);
        assert!(!manager.can_load_server("blocked-server").await);
    }

    #[test]
    fn test_manager_config_defaults() {
        let config = ManagerConfig::default();
        assert_eq!(config.health_check_interval, Duration::from_secs(30));
        assert_eq!(config.max_restart_attempts, 5);
        assert!((config.restart_backoff_multiplier - 2.0).abs() < f64::EPSILON);
        assert_eq!(config.initial_restart_delay, Duration::from_millis(500));
        assert_eq!(config.request_timeout, Duration::from_secs(30));
        assert_eq!(config.connection_timeout, Duration::from_secs(10));
    }

    #[test]
    fn test_server_config_serialization_full() {
        let config = ServerConfig {
            server_id: "test".to_string(),
            name: "Test".to_string(),
            command: Some("node".to_string()),
            args: vec!["server.js".to_string()],
            transport_type: None,
            url: None,
            headers: None,
            headers_helper: None,
            description: None,
            oauth: None,
            enable_tools: true,
            enable_resources: true,
            enable_prompts: true,
            enabled: true,
            tools_allowlist: vec!["tool1".to_string()],
            tools_denylist: vec!["tool2".to_string()],
            tags: vec!["database".to_string(), "prod".to_string()],
        };
        let json = serde_json::to_string(&config).unwrap();
        let parsed: ServerConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.server_id, "test");
        assert_eq!(parsed.tools_allowlist, vec!["tool1"]);
        assert_eq!(parsed.tools_denylist, vec!["tool2"]);
        assert_eq!(parsed.tags, vec!["database", "prod"]);
    }

    #[test]
    fn test_server_config_deserialization_defaults() {
        // Minimal JSON should use defaults for optional fields
        let json = r#"{
            "server_id": "s",
            "name": "S",
            "command": "echo",
            "args": [],
            "enable_tools": true,
            "enable_resources": false,
            "enable_prompts": false
        }"#;
        let config: ServerConfig = serde_json::from_str(json).unwrap();
        assert!(config.enabled); // default_true
        assert!(config.tools_allowlist.is_empty());
        assert!(config.tools_denylist.is_empty());
        assert!(config.tags.is_empty());
    }

    #[test]
    fn test_health_status_equality() {
        assert_eq!(HealthStatus::Healthy, HealthStatus::Healthy);
        assert_eq!(
            HealthStatus::Unhealthy("err".to_string()),
            HealthStatus::Unhealthy("err".to_string())
        );
        assert_eq!(HealthStatus::Stopped, HealthStatus::Stopped);
        assert_eq!(HealthStatus::Starting, HealthStatus::Starting);
        assert_eq!(HealthStatus::Restarting, HealthStatus::Restarting);

        assert_ne!(HealthStatus::Healthy, HealthStatus::Stopped);
        assert_ne!(
            HealthStatus::Unhealthy("a".to_string()),
            HealthStatus::Unhealthy("b".to_string())
        );
    }

    #[test]
    fn test_health_status_serialization() {
        let status = HealthStatus::Healthy;
        let json = serde_json::to_string(&status).unwrap();
        let parsed: HealthStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, HealthStatus::Healthy);

        let status = HealthStatus::Unhealthy("error msg".to_string());
        let json = serde_json::to_string(&status).unwrap();
        let parsed: HealthStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, status);
    }

    #[tokio::test]
    async fn test_manager_get_server_not_found() {
        let manager = McpServerManager::default_config();
        assert!(manager.server("nonexistent").await.is_none());
    }

    #[tokio::test]
    async fn test_manager_health_check_not_found() {
        let manager = McpServerManager::default_config();
        let status = manager.health_check("nonexistent").await;
        assert_eq!(status, HealthStatus::Stopped);
    }

    #[tokio::test]
    async fn test_manager_shutdown_empty() {
        let manager = McpServerManager::default_config();
        // Shutdown on empty should not panic
        manager.shutdown().await;
    }

    #[tokio::test]
    async fn test_manager_stop_health_monitoring_when_none() {
        let mut manager = McpServerManager::default_config();
        // No monitoring started, should not panic
        manager.stop_health_monitoring().await;
    }

    #[test]
    fn test_mcp_config_file_empty_servers_json() {
        let json = r#"{}"#;
        let config: McpConfigFile = serde_json::from_str(json).unwrap();
        assert!(config.servers.is_empty());
    }

    #[test]
    fn test_mcp_config_file_mcp_servers_key() {
        let json = r#"{
            "mcpServers": {
                "github": {
                    "server_id": "github",
                    "name": "GitHub",
                    "command": "npx",
                    "args": ["-y", "@modelcontextprotocol/server-github"]
                }
            }
        }"#;
        let config: McpConfigFile = serde_json::from_str(json).unwrap();
        assert_eq!(config.servers.len(), 1);
        assert!(config.servers.contains_key("github"));
    }

    #[test]
    fn test_mcp_config_file_claude_http_server() {
        let json = r#"{
            "mcpServers": {
                "vercel": {
                    "server_id": "vercel",
                    "name": "Vercel",
                    "type": "http",
                    "url": "https://mcp.vercel.com",
                    "headers": {"Authorization": "Bearer token123"},
                    "description": "Vercel deployments"
                }
            }
        }"#;
        let config: McpConfigFile = serde_json::from_str(json).unwrap();
        let server = config.servers.get("vercel").unwrap();
        assert_eq!(server.transport_type, Some(McpTransportType::Http));
        assert_eq!(server.url.as_deref(), Some("https://mcp.vercel.com"));
        assert_eq!(server.description.as_deref(), Some("Vercel deployments"));
        assert!(server.command.is_none());
    }

    #[test]
    fn test_mcp_config_file_mixed_transports() {
        let json = r#"{
            "mcpServers": {
                "local-fs": {
                    "server_id": "local-fs",
                    "name": "Filesystem",
                    "command": "npx",
                    "args": ["-y", "server-filesystem"]
                },
                "remote-api": {
                    "server_id": "remote-api",
                    "name": "API",
                    "type": "http",
                    "url": "https://api.example.com/mcp",
                    "oauth": {
                        "clientId": "my-client",
                        "scopes": "read write"
                    }
                }
            }
        }"#;
        let config: McpConfigFile = serde_json::from_str(json).unwrap();
        assert_eq!(config.servers.len(), 2);

        let local = config.servers.get("local-fs").unwrap();
        assert!(local.command.is_some());
        assert!(local.url.is_none());

        let remote = config.servers.get("remote-api").unwrap();
        assert!(remote.command.is_none());
        assert_eq!(remote.url.as_deref(), Some("https://api.example.com/mcp"));
        let oauth = remote.oauth.as_ref().unwrap();
        assert_eq!(oauth.client_id, "my-client");
    }
}
