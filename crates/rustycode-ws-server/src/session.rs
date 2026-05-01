use std::collections::HashMap;
use std::sync::Arc;

use chrono::Utc;
use tokio::sync::RwLock;
use tracing::info;

use rustycode_orchestration::pipeline::OrchestrationPipeline;
use rustycode_protocol::SessionId;
use rustycode_ui_model::{FrontendMessageKind, FrontendSession};

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

type ApprovalMap = std::sync::Mutex<std::collections::HashMap<String, tokio::sync::oneshot::Sender<bool>>>;

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
            pending_tool_approvals: std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
            pending_plan_approvals: std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
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
    pipeline: Arc<OrchestrationPipeline>,
    provider_name: RwLock<String>,
    model_name: RwLock<String>,
}

impl std::fmt::Debug for SessionManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionManager")
            .field("sessions", &"<locked>")
            .field("mcp_servers", &"<locked>")
            .field("pipeline", &"<OrchestrationPipeline>")
            .field("provider_name", &self.provider_name.try_read().map_or_else(|_| "<locked>".to_string(), |v| v.clone()))
            .field("model_name", &self.model_name.try_read().map_or_else(|_| "<locked>".to_string(), |v| v.clone()))
            .finish()
    }
}

impl Clone for SessionManager {
    fn clone(&self) -> Self {
        Self {
            sessions: Arc::clone(&self.sessions),
            mcp_servers: Arc::clone(&self.mcp_servers),
            pipeline: Arc::clone(&self.pipeline),
            provider_name: RwLock::new(self.provider_name.try_read().map_or_else(|_| String::new(), |v| v.clone())),
            model_name: RwLock::new(self.model_name.try_read().map_or_else(|_| String::new(), |v| v.clone())),
        }
    }
}

#[allow(clippy::significant_drop_tightening)]
impl SessionManager {
    pub fn new(pipeline: Arc<OrchestrationPipeline>, provider_name: String, model_name: String) -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            mcp_servers: Arc::new(RwLock::new(HashMap::new())),
            pipeline,
            provider_name: RwLock::new(provider_name),
            model_name: RwLock::new(model_name),
        }
    }

    pub async fn provider_info(&self) -> ProviderInfo {
        let provider = self.provider_name.read().await.clone();
        let model = self.model_name.read().await.clone();
        ProviderInfo { provider, model }
    }

    pub async fn switch_provider(&self, provider: String, model: String) -> ProviderInfo {
        info!(provider = %provider, model = %model, "switching provider");
        *self.provider_name.write().await = provider;
        *self.model_name.write().await = model;
        self.provider_info().await
    }

    pub const fn pipeline(&self) -> &Arc<OrchestrationPipeline> {
        &self.pipeline
    }

    pub async fn create_session(&self) -> SessionState {
        let id = SessionId::new();
        let token = id.to_string();
        let state = SessionState::new(id);

        info!(session_id = %token, "created new session");

        self.sessions.write().await.insert(token, state.clone());
        state
    }

    pub async fn get_or_create(&self, token: Option<&str>) -> Result<(SessionState, bool), WsError> {
        if let Some(token) = token {
            let sessions = self.sessions.read().await;
            if let Some(state) = sessions.get(token) {
                return Ok((state.clone(), true));
            }
        }

        let state = self.create_session().await;
        Ok((state, false))
    }

    pub async fn get_session(&self, token: &str) -> Option<SessionState> {
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
            info!(
                session_id = token,
                clients = state.client_count,
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
        let pipeline = Arc::clone(&self.pipeline);
        let content_owned = content.to_string();
        let token_owned = token.to_string();
        let sessions = self.sessions.clone();

        let task_id_clone = task_id.clone();
        tokio::spawn(async move {
            let content = tokio::select! {
                result = pipeline.conduct(task_id_clone.clone(), content_owned) => {
                    match &result {
                        Err(e) => {
                            tracing::error!(session_id = %token_owned, task_id = %task_id_clone, "pipeline error: {e}");
                            format!("Error: {e}")
                        }
                        Ok(rustycode_orchestration::pipeline::TaskResult::Success { output, .. }) => {
                            output.clone()
                        }
                        Ok(rustycode_orchestration::pipeline::TaskResult::Failed { reason, .. }) => {
                            tracing::warn!(session_id = %token_owned, "pipeline failed: {reason}");
                            format!("Failed: {reason}")
                        }
                    }
                }
                () = cancel_token.cancelled() => {
                    tracing::info!(session_id = %token_owned, "generation cancelled");
                    String::new()
                }
            };
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
        let mut map = state.pending_tool_approvals.lock().map_err(|e| WsError::Internal(e.to_string()))?;
        if let Some(sender) = map.remove(request_id) {
            let _ = sender.send(approved);
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
        let mut map = state.pending_plan_approvals.lock().map_err(|e| WsError::Internal(e.to_string()))?;
        if let Some(sender) = map.remove(plan_id) {
            let _ = sender.send(approved);
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
        let mut map = state.pending_tool_approvals.lock().map_err(|e| WsError::Internal(e.to_string()))?;
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
        let mut map = state.pending_plan_approvals.lock().map_err(|e| WsError::Internal(e.to_string()))?;
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

    pub async fn add_mcp_server(&self, config: McpServerConfig) {
        let name = config.name.clone();
        info!(name = %name, command = %config.command, "adding MCP server");
        self.mcp_servers.write().await.insert(name, config);
    }

    pub async fn remove_mcp_server(&self, name: &str) -> Result<(), WsError> {
        let removed = self.mcp_servers.write().await.remove(name);
        if removed.is_some() {
            info!(name = %name, "removed MCP server");
            Ok(())
        } else {
            Err(WsError::NotFound(format!(
                "mcp server not found: {name}"
            )))
        }
    }

    pub async fn restart_mcp_server(&self, name: &str) -> Result<McpServerConfig, WsError> {
        let servers = self.mcp_servers.read().await;
        let config = servers.get(name).ok_or_else(|| {
            WsError::NotFound(format!("mcp server not found: {name}"))
        })?;
        info!(name = %name, "restarting MCP server");
        Ok(config.clone())
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
        SessionManager::new(test_pipeline(), "mock".to_string(), "test-model".to_string())
    }

    #[tokio::test]
    async fn creates_session_with_unique_id() {
        let mgr = test_mgr();
        let s1 = mgr.create_session().await;
        let s2 = mgr.create_session().await;
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
        let created = mgr.create_session().await;
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
        let created = mgr.create_session().await;
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
        let created = mgr.create_session().await;
        let token = created.id.to_string();

        mgr.client_connected(&token).await.unwrap();
        mgr.client_connected(&token).await.unwrap();

        let state = mgr.get_session(&token).await.unwrap();
        assert_eq!(state.client_count, 2);

        mgr.client_disconnected(&token).await;
        let state = mgr.get_session(&token).await.unwrap();
        assert_eq!(state.client_count, 1);
    }

    #[tokio::test]
    async fn next_seq_monotonically_increases() {
        let mgr = test_mgr();
        let created = mgr.create_session().await;
        let token = created.id.to_string();

        let s1 = mgr.update_session(&token, SessionState::next_seq).await.unwrap();
        let s2 = mgr.update_session(&token, SessionState::next_seq).await.unwrap();
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
        let info = mgr.switch_provider("anthropic".to_string(), "claude-sonnet-4-6".to_string()).await;
        assert_eq!(info.provider, "anthropic");
        assert_eq!(info.model, "claude-sonnet-4-6");

        let info_after = mgr.provider_info().await;
        assert_eq!(info_after.provider, "anthropic");
        assert_eq!(info_after.model, "claude-sonnet-4-6");
    }
}
