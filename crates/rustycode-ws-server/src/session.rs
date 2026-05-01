use std::collections::HashMap;
use std::sync::Arc;

use chrono::Utc;
use tokio::sync::RwLock;
use tracing::info;

use rustycode_orchestration::pipeline::OrchestrationPipeline;
use rustycode_protocol::SessionId;
use rustycode_ui_model::{FrontendMessageKind, FrontendSession};

use crate::error::WsError;

#[derive(Debug, Clone)]
pub struct SessionState {
    pub id: SessionId,
    pub session: FrontendSession,
    pub seq: u64,
    pub created_at: chrono::DateTime<Utc>,
    pub last_active_at: chrono::DateTime<Utc>,
    pub client_count: usize,
    pub cancel_token: tokio_util::sync::CancellationToken,
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
    pipeline: Arc<OrchestrationPipeline>,
    provider_name: String,
    model_name: String,
}

impl std::fmt::Debug for SessionManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionManager")
            .field("sessions", &"<locked>")
            .field("pipeline", &"<OrchestrationPipeline>")
            .field("provider_name", &self.provider_name)
            .field("model_name", &self.model_name)
            .finish()
    }
}

impl Clone for SessionManager {
    fn clone(&self) -> Self {
        Self {
            sessions: Arc::clone(&self.sessions),
            pipeline: Arc::clone(&self.pipeline),
            provider_name: self.provider_name.clone(),
            model_name: self.model_name.clone(),
        }
    }
}

#[allow(clippy::significant_drop_tightening)]
impl SessionManager {
    pub fn new(pipeline: Arc<OrchestrationPipeline>, provider_name: String, model_name: String) -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            pipeline,
            provider_name,
            model_name,
        }
    }

    pub fn provider_info(&self) -> ProviderInfo {
        ProviderInfo {
            provider: self.provider_name.clone(),
            model: self.model_name.clone(),
        }
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

    pub async fn snapshot(&self, token: &str) -> Result<FrontendSession, WsError> {
        let sessions = self.sessions.read().await;
        let state = sessions
            .get(token)
            .ok_or_else(|| WsError::SessionNotFound(token.to_string()))?;
        Ok(state.session.clone())
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
}
