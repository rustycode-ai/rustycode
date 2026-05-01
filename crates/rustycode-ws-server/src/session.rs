use std::collections::HashMap;
use std::sync::Arc;

use chrono::Utc;
use tokio::sync::RwLock;
use tracing::info;

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
        }
    }

    pub fn next_seq(&mut self) -> u64 {
        self.seq = self.seq.saturating_add(1);
        self.seq
    }
}

#[derive(Debug, Clone)]
pub struct SessionManager {
    sessions: Arc<RwLock<HashMap<String, SessionState>>>,
}

impl SessionManager {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
        }
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
        Ok(f(state))
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

    pub async fn submit_input(&self, token: &str, content: &str) -> Result<(), WsError> {
        let mut sessions = self.sessions.write().await;
        let state = sessions
            .get_mut(token)
            .ok_or_else(|| WsError::SessionNotFound(token.to_string()))?;

        state.session.input = content.to_string();
        let submitted = state.session.submit_input();

        if let rustycode_ui_model::SubmittedInput::ChatMessage(msg) = &submitted {
            state.session.add_message(msg.clone(), FrontendMessageKind::User);
            state.session.start_assistant_request();
        }

        state.last_active_at = Utc::now();
        Ok(())
    }

    pub async fn snapshot(&self, token: &str) -> Result<FrontendSession, WsError> {
        let sessions = self.sessions.read().await;
        let state = sessions
            .get(token)
            .ok_or_else(|| WsError::SessionNotFound(token.to_string()))?;
        Ok(state.session.clone())
    }
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn creates_session_with_unique_id() {
        let mgr = SessionManager::new();
        let s1 = mgr.create_session().await;
        let s2 = mgr.create_session().await;
        assert_ne!(s1.id.to_string(), s2.id.to_string());
    }

    #[tokio::test]
    async fn get_or_create_new_when_no_token() {
        let mgr = SessionManager::new();
        let (state, resumed) = mgr.get_or_create(None).await.unwrap();
        assert!(!resumed);
        assert!(state.id.to_string().starts_with("sess_"));
    }

    #[tokio::test]
    async fn get_or_create_resumes_existing() {
        let mgr = SessionManager::new();
        let created = mgr.create_session().await;
        let token = created.id.to_string();

        let (state, resumed) = mgr.get_or_create(Some(&token)).await.unwrap();
        assert!(resumed);
        assert_eq!(state.id.to_string(), token);
    }

    #[tokio::test]
    async fn get_or_create_new_on_invalid_token() {
        let mgr = SessionManager::new();
        let (_state, resumed) = mgr.get_or_create(Some("invalid")).await.unwrap();
        assert!(!resumed);
    }

    #[tokio::test]
    async fn submit_input_adds_user_message() {
        let mgr = SessionManager::new();
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
        let mgr = SessionManager::new();
        let result = mgr.submit_input("nonexistent", "hello").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn client_tracking() {
        let mgr = SessionManager::new();
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
        let mgr = SessionManager::new();
        let created = mgr.create_session().await;
        let token = created.id.to_string();

        let s1 = mgr.update_session(&token, |s| s.next_seq()).await.unwrap();
        let s2 = mgr.update_session(&token, |s| s.next_seq()).await.unwrap();
        assert!(s2 > s1);
    }
}
