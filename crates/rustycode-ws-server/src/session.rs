use std::collections::HashMap;
use std::sync::Arc;

use chrono::Utc;
use tokio::sync::RwLock;
use tracing::info;

use rustycode_orchestration::config::OrchestrationConfig;
use rustycode_orchestration::pipeline::OrchestrationPipeline;
use rustycode_protocol::SessionId;
use rustycode_ui_model::{FrontendMessageKind, FrontendSession};

use crate::approval::WsPipelineInteraction;
use crate::bridge::EventBridge;
use crate::error::WsError;

#[derive(Debug, Clone, serde::Serialize)]
pub struct McpServerConfig {
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
}

type ApprovalMap =
    std::sync::Mutex<std::collections::HashMap<String, tokio::sync::oneshot::Sender<bool>>>;

#[derive(Debug)]
pub struct SessionState {
    pub id: SessionId,
    pub session: FrontendSession,
    pub seq: u64,
    pub created_at: chrono::DateTime<Utc>,
    pub last_active_at: chrono::DateTime<Utc>,
    pub client_count: usize,
    pub cancel_token: tokio_util::sync::CancellationToken,
    pub pending_tool_approvals: std::sync::Arc<ApprovalMap>,
    pub pending_plan_approvals: std::sync::Arc<ApprovalMap>,
}

impl Clone for SessionState {
    fn clone(&self) -> Self {
        Self {
            id: self.id.clone(),
            session: self.session.clone(),
            seq: self.seq,
            created_at: self.created_at,
            last_active_at: self.last_active_at,
            client_count: self.client_count,
            cancel_token: self.cancel_token.clone(),
            pending_tool_approvals: self.pending_tool_approvals.clone(),
            pending_plan_approvals: self.pending_plan_approvals.clone(),
        }
    }
}

impl SessionState {
    pub fn new(id: SessionId) -> Self {
        let now = Utc::now();
        Self {
            id,
            session: FrontendSession::default(),
            seq: 0,
            created_at: now,
            last_active_at: now,
            client_count: 0,
            cancel_token: tokio_util::sync::CancellationToken::new(),
            pending_tool_approvals: std::sync::Arc::new(std::sync::Mutex::new(
                std::collections::HashMap::new(),
            )),
            pending_plan_approvals: std::sync::Arc::new(std::sync::Mutex::new(
                std::collections::HashMap::new(),
            )),
        }
    }

    #[allow(clippy::missing_const_for_fn)]
    pub fn next_seq(&mut self) -> u64 {
        self.seq = self.seq.saturating_add(1);
        self.seq
    }
}

pub struct SessionManager {
    sessions: Arc<RwLock<HashMap<String, SessionState>>>,
    mcp_servers: Arc<RwLock<HashMap<String, McpServerConfig>>>,
    mcp_client_manager: Arc<std::sync::Mutex<rustycode_mcp::stdio_client::McpClientManager>>,
    pipeline: Arc<RwLock<Arc<OrchestrationPipeline>>>,
    provider_name: RwLock<String>,
    model_name: RwLock<String>,
    event_bridge: Option<Arc<EventBridge>>,
    orchestration_config: OrchestrationConfig,
}

const MAX_SESSIONS: usize = 256;
const MAX_PENDING_APPROVALS: usize = 128;
const MAX_MCP_SERVERS: usize = 64;

impl std::fmt::Debug for SessionManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionManager")
            .field("sessions", &"<locked>")
            .field("mcp_servers", &"<locked>")
            .field("mcp_client_manager", &"<Mutex>")
            .field("pipeline", &"<RwLock<Arc<OrchestrationPipeline>>>")
            .field(
                "provider_name",
                &self
                    .provider_name
                    .try_read()
                    .map_or_else(|_| "<locked>".to_string(), |v| v.clone()),
            )
            .field(
                "model_name",
                &self
                    .model_name
                    .try_read()
                    .map_or_else(|_| "<locked>".to_string(), |v| v.clone()),
            )
            .field(
                "event_bridge",
                &self.event_bridge.as_ref().map_or("None", |_| "Some"),
            )
            .field("orchestration_config", &"..")
            .finish()
    }
}

impl Clone for SessionManager {
    fn clone(&self) -> Self {
        Self {
            sessions: Arc::clone(&self.sessions),
            mcp_servers: Arc::clone(&self.mcp_servers),
            mcp_client_manager: Arc::clone(&self.mcp_client_manager),
            pipeline: Arc::clone(&self.pipeline),
            provider_name: RwLock::new(
                self.provider_name
                    .try_read()
                    .map_or_else(|_| String::new(), |v| v.clone()),
            ),
            model_name: RwLock::new(
                self.model_name
                    .try_read()
                    .map_or_else(|_| String::new(), |v| v.clone()),
            ),
            event_bridge: self.event_bridge.clone(),
            orchestration_config: self.orchestration_config.clone(),
        }
    }
}

#[allow(clippy::significant_drop_tightening)]
impl SessionManager {
    pub fn new(
        pipeline: Arc<OrchestrationPipeline>,
        provider_name: String,
        model_name: String,
    ) -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            mcp_servers: Arc::new(RwLock::new(HashMap::new())),
            mcp_client_manager: Arc::new(std::sync::Mutex::new(
                rustycode_mcp::stdio_client::McpClientManager::new(),
            )),
            pipeline: Arc::new(RwLock::new(pipeline)),
            provider_name: RwLock::new(provider_name),
            model_name: RwLock::new(model_name),
            event_bridge: None,
            orchestration_config: OrchestrationConfig::default(),
        }
    }

    pub fn with_config(
        pipeline: Arc<OrchestrationPipeline>,
        provider_name: String,
        model_name: String,
        config: OrchestrationConfig,
    ) -> Self {
        Self {
            orchestration_config: config,
            ..Self::new(pipeline, provider_name, model_name)
        }
    }

    pub fn set_event_bridge(&mut self, bridge: Arc<EventBridge>) {
        self.event_bridge = Some(bridge);
    }

    pub async fn provider_info(&self) -> ProviderInfo {
        let provider = self.provider_name.read().await.clone();
        let model = self.model_name.read().await.clone();
        ProviderInfo { provider, model }
    }

    pub async fn switch_provider(
        &self,
        provider: String,
        model: String,
    ) -> Result<ProviderInfo, WsError> {
        info!(provider = %provider, model = %model, "switching provider");

        let new_provider = rustycode_llm::create_provider(&provider, &model).map_err(|e| {
            WsError::Internal(format!("failed to create provider '{provider}': {e}"))
        })?;

        let tool_registry = std::sync::Arc::new(rustycode_tools::default_registry());
        let new_pipeline = Arc::new(OrchestrationPipeline::with_provider_model_and_tools(
            self.orchestration_config.clone(),
            new_provider,
            &model,
            tool_registry,
        ));

        let new_bus = new_pipeline.bus_handle();

        {
            let mut guard = self.pipeline.write().await;
            *guard = new_pipeline;
        }

        *self.provider_name.write().await = provider;
        *self.model_name.write().await = model;

        if let Some(bridge) = &self.event_bridge {
            bridge.resubscribe(new_bus).await;
        }

        Ok(self.provider_info().await)
    }

    pub async fn pipeline(&self) -> Arc<OrchestrationPipeline> {
        self.pipeline.read().await.clone()
    }

    pub async fn create_session(&self) -> Result<SessionState, WsError> {
        {
            let sessions = self.sessions.read().await;
            if sessions.len() >= MAX_SESSIONS {
                return Err(WsError::TooManySessions {
                    limit: MAX_SESSIONS,
                });
            }
        }
        let id = SessionId::new();
        let token = id.to_string();
        let state = SessionState::new(id);

        info!(session_id = %token, "created new session");

        self.sessions.write().await.insert(token, state.clone());
        Ok(state)
    }

    pub async fn get_or_create(
        &self,
        token: Option<&str>,
    ) -> Result<(SessionState, bool), WsError> {
        if let Some(token) = token {
            let sessions = self.sessions.read().await;
            if let Some(state) = sessions.get(token) {
                return Ok((state.clone(), true));
            }
        }

        let state = self.create_session().await?;
        Ok((state, false))
    }

    pub async fn session(&self, token: &str) -> Option<SessionState> {
        let sessions = self.sessions.read().await;
        sessions.get(token).cloned()
    }

    pub async fn update_session<F, R>(&self, token: &str, f: F) -> Result<R, WsError>
    where
        F: FnOnce(&mut SessionState) -> R,
    {
        let mut sessions = self.sessions.write().await;
        let state = sessions
            .get_mut(token)
            .ok_or_else(|| WsError::SessionNotFound(token.to_string()))?;
        state.last_active_at = Utc::now();
        let ret = f(state);
        drop(sessions);
        Ok(ret)
    }

    pub async fn client_connected(&self, token: &str) -> Result<(), WsError> {
        let mut sessions = self.sessions.write().await;
        let state = sessions
            .get_mut(token)
            .ok_or_else(|| WsError::SessionNotFound(token.to_string()))?;
        state.client_count = state.client_count.saturating_add(1);
        info!(
            session_id = token,
            clients = state.client_count,
            "client connected"
        );
        drop(sessions);
        Ok(())
    }

    pub async fn client_disconnected(&self, token: &str) {
        let mut sessions = self.sessions.write().await;
        if let Some(state) = sessions.get_mut(token) {
            state.client_count = state.client_count.saturating_sub(1);

            // Clean up pending approval maps to prevent memory leaks from abandoned requests
            let tool_count = {
                let mut map = state
                    .pending_tool_approvals
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                let count = map.len();
                map.clear();
                count
            };
            let plan_count = {
                let mut map = state
                    .pending_plan_approvals
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                let count = map.len();
                map.clear();
                count
            };

            info!(
                session_id = token,
                clients = state.client_count,
                cleared_tool_approvals = tool_count,
                cleared_plan_approvals = plan_count,
                "client disconnected"
            );
        }
    }

    pub async fn submit_input(&self, token: &str, content: &str) -> Result<String, WsError> {
        let (cancel_token, task_id) = {
            let mut sessions = self.sessions.write().await;
            let state = sessions
                .get_mut(token)
                .ok_or_else(|| WsError::SessionNotFound(token.to_string()))?;

            state.session.input = content.to_string();
            let submitted = state.session.submit_input();

            if let rustycode_ui_model::SubmittedInput::ChatMessage(msg) = &submitted {
                state
                    .session
                    .add_message(msg.clone(), FrontendMessageKind::User);
                state.session.start_assistant_request();
            }

            state.last_active_at = Utc::now();

            // Create a new cancel token for this generation
            state.cancel_token = tokio_util::sync::CancellationToken::new();
            let ct = state.cancel_token.clone();
            let tid = uuid::Uuid::new_v4().to_string();
            (ct, tid)
        };

        // Spawn the LLM processing task
        let pipeline = self.pipeline.read().await.clone();
        let content_owned = content.to_string();
        let token_owned = token.to_string();
        let sessions = self.sessions.clone();

        let task_id_clone = task_id.clone();
        tokio::spawn(async move {
            let interaction = Arc::new(WsPipelineInteraction::new());

            // Set the session channel so approvals route through the WS
            {
                let sessions_lock = sessions.read().await;
                if let Some(state) = sessions_lock.get(&token_owned) {
                    interaction.set_session(
                        state.pending_tool_approvals.clone(),
                        state.cancel_token.clone(),
                    );
                }
            }

            let conduct_future = pipeline.conduct_streaming(
                task_id_clone.clone(),
                content_owned,
                interaction.clone()
                    as Arc<dyn rustycode_orchestration::pipeline::PipelineInteraction>,
            );
            let content = tokio::select! {
                result = tokio::time::timeout(std::time::Duration::from_mins(15), conduct_future) => {
                    result.map_or_else(
                        |_| {
                            tracing::error!(session_id = %token_owned, task_id = %task_id_clone, "pipeline timed out");
                            "Error: generation timed out".to_string()
                        },
                        |inner| match inner {
                            Err(e) => {
                                tracing::error!(session_id = %token_owned, task_id = %task_id_clone, "pipeline error: {e}");
                                format!("Error: {e}")
                            }
                            Ok(rustycode_orchestration::pipeline::TaskResult::Success { output, .. }) => output,
                            Ok(rustycode_orchestration::pipeline::TaskResult::Failed { reason, .. }) => {
                                tracing::warn!(session_id = %token_owned, "pipeline failed: {reason}");
                                format!("Failed: {reason}")
                            }
                        }
                    )
                }
                () = cancel_token.cancelled() => {
                    tracing::info!(session_id = %token_owned, "generation cancelled");
                    String::new()
                }
            };

            interaction.clear_session();

            let mut lock = sessions.write().await;
            if let Some(state) = lock.get_mut(&token_owned) {
                state.session.finish_assistant_message(content);
            }
        });

        Ok(task_id)
    }

    pub async fn abort(&self, token: &str) -> Result<(), WsError> {
        let sessions = self.sessions.read().await;
        let state = sessions
            .get(token)
            .ok_or_else(|| WsError::SessionNotFound(token.to_string()))?;
        state.cancel_token.cancel();
        Ok(())
    }

    pub async fn respond_tool_approval(
        &self,
        token: &str,
        request_id: &str,
        approved: bool,
    ) -> Result<(), WsError> {
        let sessions = self.sessions.read().await;
        let state = sessions
            .get(token)
            .ok_or_else(|| WsError::SessionNotFound(token.to_string()))?;
        let mut map = state
            .pending_tool_approvals
            .lock()
            .map_err(|e| WsError::Internal(e.to_string()))?;
        if let Some(sender) = map.remove(request_id) {
            if sender.send(approved).is_err() {
                tracing::warn!(
                    session_id = token,
                    request_id,
                    "tool approval response dropped (receiver gone)"
                );
            }
        }
        Ok(())
    }

    pub async fn respond_plan_approval(
        &self,
        token: &str,
        plan_id: &str,
        approved: bool,
    ) -> Result<(), WsError> {
        let sessions = self.sessions.read().await;
        let state = sessions
            .get(token)
            .ok_or_else(|| WsError::SessionNotFound(token.to_string()))?;
        let mut map = state
            .pending_plan_approvals
            .lock()
            .map_err(|e| WsError::Internal(e.to_string()))?;
        if let Some(sender) = map.remove(plan_id) {
            if sender.send(approved).is_err() {
                tracing::warn!(
                    session_id = token,
                    plan_id,
                    "plan approval response dropped (receiver gone)"
                );
            }
        }
        Ok(())
    }

    /// Register a oneshot channel for a pending tool approval request.
    /// The caller awaits the receiver; the server sends the result via the sender.
    pub async fn register_tool_approval(
        &self,
        token: &str,
        request_id: String,
    ) -> Result<tokio::sync::oneshot::Receiver<bool>, WsError> {
        let sessions = self.sessions.read().await;
        let state = sessions
            .get(token)
            .ok_or_else(|| WsError::SessionNotFound(token.to_string()))?;
        let (tx, rx) = tokio::sync::oneshot::channel();
        let mut map = state
            .pending_tool_approvals
            .lock()
            .map_err(|e| WsError::Internal(e.to_string()))?;
        if map.len() >= MAX_PENDING_APPROVALS {
            return Err(WsError::Internal(format!(
                "too many pending tool approvals (limit: {MAX_PENDING_APPROVALS})"
            )));
        }
        map.insert(request_id, tx);
        Ok(rx)
    }

    /// Register a oneshot channel for a pending plan approval request.
    pub async fn register_plan_approval(
        &self,
        token: &str,
        plan_id: String,
    ) -> Result<tokio::sync::oneshot::Receiver<bool>, WsError> {
        let sessions = self.sessions.read().await;
        let state = sessions
            .get(token)
            .ok_or_else(|| WsError::SessionNotFound(token.to_string()))?;
        let (tx, rx) = tokio::sync::oneshot::channel();
        let mut map = state
            .pending_plan_approvals
            .lock()
            .map_err(|e| WsError::Internal(e.to_string()))?;
        if map.len() >= MAX_PENDING_APPROVALS {
            return Err(WsError::Internal(format!(
                "too many pending plan approvals (limit: {MAX_PENDING_APPROVALS})"
            )));
        }
        map.insert(plan_id, tx);
        Ok(rx)
    }

    pub async fn snapshot(&self, token: &str) -> Result<FrontendSession, WsError> {
        let sessions = self.sessions.read().await;
        let state = sessions
            .get(token)
            .ok_or_else(|| WsError::SessionNotFound(token.to_string()))?;
        Ok(state.session.clone())
    }

    pub async fn list_mcp_servers(&self) -> Vec<McpServerConfig> {
        let servers = self.mcp_servers.read().await;
        servers.values().cloned().collect()
    }

    pub fn connected_mcp_servers(&self) -> Vec<String> {
        self.mcp_client_manager.lock().map_or_else(
            |_| Vec::new(),
            |mgr| {
                mgr.connected_servers()
                    .into_iter()
                    .map(String::from)
                    .collect()
            },
        )
    }

    pub async fn add_mcp_server(&self, config: McpServerConfig) -> Result<(), WsError> {
        let name = config.name.trim();
        let command = config.command.trim();
        if name.is_empty() {
            return Err(WsError::Validation(
                "MCP server name must not be empty".to_string(),
            ));
        }
        if command.is_empty() {
            return Err(WsError::Validation(
                "MCP server command must not be empty".to_string(),
            ));
        }
        let mut servers = self.mcp_servers.write().await;
        if !servers.contains_key(name) && servers.len() >= MAX_MCP_SERVERS {
            return Err(WsError::TooManyMcpServers {
                limit: MAX_MCP_SERVERS,
            });
        }
        info!(name = %name, command = %command, "adding MCP server");

        // Connect to the MCP server process (best-effort — config is stored regardless)
        let mut mcp_config = rustycode_mcp::stdio_client::McpServerConfig::new(name, command);
        mcp_config.args.clone_from(&config.args);
        mcp_config.env.clone_from(&config.env);
        {
            let mut mgr = self
                .mcp_client_manager
                .lock()
                .map_err(|e| WsError::Internal(e.to_string()))?;
            if let Err(e) = mgr.add_server(mcp_config) {
                tracing::warn!(name = %name, "MCP server registered but connection failed: {e}");
            }
        }

        servers.insert(name.to_string(), config);
        Ok(())
    }

    pub async fn remove_mcp_server(&self, name: &str) -> Result<(), WsError> {
        let removed = self.mcp_servers.write().await.remove(name);
        if removed.is_none() {
            return Err(WsError::NotFound(format!("mcp server not found: {name}")));
        }

        // Disconnect the running MCP client process
        {
            let mut mgr = self
                .mcp_client_manager
                .lock()
                .map_err(|e| WsError::Internal(e.to_string()))?;
            if let Err(e) = mgr.remove_server(name) {
                tracing::warn!(name = %name, "MCP server config removed but disconnect failed: {e}");
            }
        }

        info!(name = %name, "removed MCP server");
        Ok(())
    }

    pub async fn restart_mcp_server(&self, name: &str) -> Result<McpServerConfig, WsError> {
        let config = {
            let servers = self.mcp_servers.read().await;
            servers
                .get(name)
                .ok_or_else(|| WsError::NotFound(format!("mcp server not found: {name}")))?
                .clone()
        };

        info!(name = %name, "restarting MCP server");

        // Disconnect old connection, then reconnect
        let mut mcp_config =
            rustycode_mcp::stdio_client::McpServerConfig::new(name, &config.command);
        mcp_config.args.clone_from(&config.args);
        mcp_config.env.clone_from(&config.env);
        {
            let mut mgr = self
                .mcp_client_manager
                .lock()
                .map_err(|e| WsError::Internal(e.to_string()))?;
            let _ = mgr.remove_server(name);
            if let Err(e) = mgr.add_server(mcp_config) {
                tracing::warn!(name = %name, "MCP server restart connection failed: {e}");
            }
        }

        Ok(config)
    }

    pub async fn list_sessions(&self) -> Vec<SessionInfo> {
        let sessions = self.sessions.read().await;
        sessions
            .values()
            .map(|s| SessionInfo {
                id: s.id.to_string(),
                created_at: s.created_at,
                last_active_at: s.last_active_at,
                message_count: s.session.messages.len(),
                client_count: s.client_count,
            })
            .collect()
    }

    pub async fn delete_session(&self, token: &str) -> Result<(), WsError> {
        let mut sessions = self.sessions.write().await;
        sessions
            .remove(token)
            .ok_or_else(|| WsError::SessionNotFound(token.to_string()))?;
        info!(session_id = token, "deleted session");
        Ok(())
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionInfo {
    pub id: String,
    pub created_at: chrono::DateTime<Utc>,
    pub last_active_at: chrono::DateTime<Utc>,
    pub message_count: usize,
    pub client_count: usize,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ProviderInfo {
    pub provider: String,
    pub model: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ProviderEntry {
    pub name: String,
    pub display_name: String,
    pub models: Vec<String>,
    pub default_model: String,
    pub available: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ProviderListResponse {
    pub current: ProviderInfo,
    pub providers: Vec<ProviderEntry>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct SwitchProviderRequest {
    pub provider: String,
    pub model: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SkillInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub categories: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SkillListResponse {
    pub skills: Vec<SkillInfo>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct SkillExecuteRequest {
    pub skill_id: String,
    pub session_token: String,
    #[serde(default)]
    pub args: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct McpServerInfo {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub status: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct McpServerListResponse {
    pub servers: Vec<McpServerInfo>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct McpAddServerRequest {
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: std::collections::HashMap<String, String>,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn test_pipeline() -> Arc<OrchestrationPipeline> {
        Arc::new(OrchestrationPipeline::new(
            rustycode_orchestration::config::OrchestrationConfig::default(),
        ))
    }

    fn test_mgr() -> SessionManager {
        SessionManager::new(
            test_pipeline(),
            "mock".to_string(),
            "test-model".to_string(),
        )
    }

    #[tokio::test]
    async fn creates_session_with_unique_id() {
        let mgr = test_mgr();
        let s1 = mgr.create_session().await.unwrap();
        let s2 = mgr.create_session().await.unwrap();
        assert_ne!(s1.id.to_string(), s2.id.to_string());
    }

    #[tokio::test]
    async fn get_or_create_new_when_no_token() {
        let mgr = test_mgr();
        let (state, resumed) = mgr.get_or_create(None).await.unwrap();
        assert!(!resumed);
        assert!(state.id.to_string().starts_with("sess_"));
    }

    #[tokio::test]
    async fn get_or_create_resumes_existing() {
        let mgr = test_mgr();
        let created = mgr.create_session().await.unwrap();
        let token = created.id.to_string();

        let (state, resumed) = mgr.get_or_create(Some(&token)).await.unwrap();
        assert!(resumed);
        assert_eq!(state.id.to_string(), token);
    }

    #[tokio::test]
    async fn get_or_create_new_on_invalid_token() {
        let mgr = test_mgr();
        let (_state, resumed) = mgr.get_or_create(Some("invalid")).await.unwrap();
        assert!(!resumed);
    }

    #[tokio::test]
    async fn submit_input_adds_user_message() {
        let mgr = test_mgr();
        let created = mgr.create_session().await.unwrap();
        let token = created.id.to_string();

        mgr.submit_input(&token, "hello").await.unwrap();

        let snapshot = mgr.snapshot(&token).await.unwrap();
        assert_eq!(snapshot.messages.len(), 2);
        assert_eq!(snapshot.messages[0].kind, FrontendMessageKind::User);
        assert_eq!(snapshot.messages[0].content, "hello");
        assert_eq!(snapshot.messages[1].kind, FrontendMessageKind::Assistant);
    }

    #[tokio::test]
    async fn submit_input_fails_on_unknown_session() {
        let mgr = test_mgr();
        let result = mgr.submit_input("nonexistent", "hello").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn client_tracking() {
        let mgr = test_mgr();
        let created = mgr.create_session().await.unwrap();
        let token = created.id.to_string();

        mgr.client_connected(&token).await.unwrap();
        mgr.client_connected(&token).await.unwrap();

        let state = mgr.session(&token).await.unwrap();
        assert_eq!(state.client_count, 2);

        mgr.client_disconnected(&token).await;
        let state = mgr.session(&token).await.unwrap();
        assert_eq!(state.client_count, 1);
    }

    #[tokio::test]
    async fn next_seq_monotonically_increases() {
        let mgr = test_mgr();
        let created = mgr.create_session().await.unwrap();
        let token = created.id.to_string();

        let s1 = mgr
            .update_session(&token, SessionState::next_seq)
            .await
            .unwrap();
        let s2 = mgr
            .update_session(&token, SessionState::next_seq)
            .await
            .unwrap();
        assert!(s2 > s1);
    }

    #[tokio::test]
    async fn provider_info_returns_initial_values() {
        let mgr = test_mgr();
        let info = mgr.provider_info().await;
        assert_eq!(info.provider, "mock");
        assert_eq!(info.model, "test-model");
    }

    #[tokio::test]
    async fn switch_provider_updates_values() {
        let mgr = test_mgr();
        let info = mgr
            .switch_provider("ollama".to_string(), "llama3".to_string())
            .await
            .unwrap();
        assert_eq!(info.provider, "ollama");
        assert_eq!(info.model, "llama3");

        let info_after = mgr.provider_info().await;
        assert_eq!(info_after.provider, "ollama");
        assert_eq!(info_after.model, "llama3");
    }

    #[tokio::test]
    async fn add_mcp_server_rejects_empty_name() {
        let mgr = test_mgr();
        let config = McpServerConfig {
            name: "  ".to_string(),
            command: "echo".to_string(),
            args: vec![],
            env: HashMap::new(),
        };
        let err = mgr.add_mcp_server(config).await.unwrap_err();
        assert!(matches!(err, WsError::Validation(_)));
    }

    #[tokio::test]
    async fn add_mcp_server_rejects_empty_command() {
        let mgr = test_mgr();
        let config = McpServerConfig {
            name: "test-server".to_string(),
            command: String::new(),
            args: vec![],
            env: HashMap::new(),
        };
        let err = mgr.add_mcp_server(config).await.unwrap_err();
        assert!(matches!(err, WsError::Validation(_)));
    }

    #[tokio::test]
    async fn add_and_list_mcp_servers() {
        let mgr = test_mgr();
        let config = McpServerConfig {
            name: "my-server".to_string(),
            command: "npx".to_string(),
            args: vec!["-y".to_string(), "some-mcp".to_string()],
            env: HashMap::new(),
        };
        mgr.add_mcp_server(config).await.unwrap();
        let servers = mgr.list_mcp_servers().await;
        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0].name, "my-server");
    }

    #[tokio::test]
    async fn remove_mcp_server_returns_not_found() {
        let mgr = test_mgr();
        let err = mgr.remove_mcp_server("nonexistent").await.unwrap_err();
        assert!(matches!(err, WsError::NotFound(_)));
    }

    #[tokio::test]
    async fn delete_session_removes_it() {
        let mgr = test_mgr();
        let created = mgr.create_session().await.unwrap();
        let token = created.id.to_string();

        let sessions = mgr.list_sessions().await;
        assert_eq!(sessions.len(), 1);

        mgr.delete_session(&token).await.unwrap();

        let sessions = mgr.list_sessions().await;
        assert!(sessions.is_empty());
    }

    #[tokio::test]
    async fn delete_session_not_found() {
        let mgr = test_mgr();
        let err = mgr.delete_session("nonexistent").await.unwrap_err();
        assert!(matches!(err, WsError::SessionNotFound(_)));
    }

    #[tokio::test]
    async fn list_sessions_returns_metadata() {
        let mgr = test_mgr();
        let s1 = mgr.create_session().await.unwrap();
        let _s2 = mgr.create_session().await.unwrap();

        let sessions = mgr.list_sessions().await;
        assert_eq!(sessions.len(), 2);

        let found = sessions.iter().find(|s| s.id == s1.id.to_string()).unwrap();
        assert_eq!(found.message_count, 0);
        assert_eq!(found.client_count, 0);
    }

    #[tokio::test]
    async fn snapshot_not_found() {
        let mgr = test_mgr();
        let result = mgr.snapshot("nonexistent").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn abort_cancels_token() {
        let mgr = test_mgr();
        let created = mgr.create_session().await.unwrap();
        let token = created.id.to_string();

        mgr.abort(&token).await.unwrap();

        let state = mgr.session(&token).await.unwrap();
        assert!(state.cancel_token.is_cancelled());
    }

    #[tokio::test]
    async fn abort_not_found() {
        let mgr = test_mgr();
        let err = mgr.abort("nonexistent").await.unwrap_err();
        assert!(matches!(err, WsError::SessionNotFound(_)));
    }

    #[tokio::test]
    async fn tool_approval_roundtrip() {
        let mgr = test_mgr();
        let created = mgr.create_session().await.unwrap();
        let token = created.id.to_string();

        let rx = mgr
            .register_tool_approval(&token, "req-1".to_string())
            .await
            .unwrap();

        mgr.respond_tool_approval(&token, "req-1", true)
            .await
            .unwrap();

        let approved = rx.await.unwrap();
        assert!(approved);
    }

    #[tokio::test]
    async fn tool_approval_not_found_session() {
        let mgr = test_mgr();
        let err = mgr
            .register_tool_approval("nonexistent", "req-1".to_string())
            .await
            .unwrap_err();
        assert!(matches!(err, WsError::SessionNotFound(_)));
    }

    #[tokio::test]
    async fn plan_approval_roundtrip() {
        let mgr = test_mgr();
        let created = mgr.create_session().await.unwrap();
        let token = created.id.to_string();

        let rx = mgr
            .register_plan_approval(&token, "plan-1".to_string())
            .await
            .unwrap();

        mgr.respond_plan_approval(&token, "plan-1", false)
            .await
            .unwrap();

        let approved = rx.await.unwrap();
        assert!(!approved);
    }

    #[tokio::test]
    async fn restart_mcp_server_not_found() {
        let mgr = test_mgr();
        let err = mgr.restart_mcp_server("nonexistent").await.unwrap_err();
        assert!(matches!(err, WsError::NotFound(_)));
    }

    #[tokio::test]
    async fn restart_mcp_server_returns_config() {
        let mgr = test_mgr();
        let config = McpServerConfig {
            name: "test-srv".to_string(),
            command: "node".to_string(),
            args: vec!["server.js".to_string()],
            env: HashMap::new(),
        };
        mgr.add_mcp_server(config).await.unwrap();

        let restarted = mgr.restart_mcp_server("test-srv").await.unwrap();
        assert_eq!(restarted.name, "test-srv");
        assert_eq!(restarted.args, vec!["server.js".to_string()]);
    }

    #[tokio::test]
    async fn session_limit_enforced() {
        let mgr = test_mgr();
        for _ in 0..MAX_SESSIONS {
            mgr.create_session().await.unwrap();
        }
        let err = mgr.create_session().await.unwrap_err();
        assert!(matches!(
            err,
            WsError::TooManySessions {
                limit: MAX_SESSIONS
            }
        ));
    }

    #[tokio::test]
    async fn mcp_server_limit_enforced() {
        let mgr = test_mgr();
        for i in 0..MAX_MCP_SERVERS {
            mgr.add_mcp_server(McpServerConfig {
                name: format!("srv-{i}"),
                command: "echo".to_string(),
                args: vec![],
                env: HashMap::new(),
            })
            .await
            .unwrap();
        }
        let err = mgr
            .add_mcp_server(McpServerConfig {
                name: "overflow".to_string(),
                command: "echo".to_string(),
                args: vec![],
                env: HashMap::new(),
            })
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            WsError::TooManyMcpServers {
                limit: MAX_MCP_SERVERS
            }
        ));
    }

    #[tokio::test]
    async fn duplicate_mcp_server_upserts() {
        let mgr = test_mgr();
        mgr.add_mcp_server(McpServerConfig {
            name: "fs".to_string(),
            command: "npx".to_string(),
            args: vec!["mcp-fs-v1".to_string()],
            env: HashMap::new(),
        })
        .await
        .unwrap();

        // Re-adding with same name updates the config
        mgr.add_mcp_server(McpServerConfig {
            name: "fs".to_string(),
            command: "npx".to_string(),
            args: vec!["mcp-fs-v2".to_string()],
            env: HashMap::new(),
        })
        .await
        .unwrap();

        let servers = mgr.list_mcp_servers().await;
        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0].args, vec!["mcp-fs-v2".to_string()]);
    }

    #[tokio::test]
    async fn delete_session_frees_slot() {
        let mgr = test_mgr();
        for _ in 0..MAX_SESSIONS {
            mgr.create_session().await.unwrap();
        }
        // Should fail at limit
        assert!(mgr.create_session().await.is_err());
        // Delete one
        let sessions = mgr.list_sessions().await;
        mgr.delete_session(&sessions[0].id).await.unwrap();
        // Should succeed now
        mgr.create_session().await.unwrap();
    }
}
