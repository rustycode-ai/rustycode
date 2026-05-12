//! Runtime session management operations.

use anyhow::{Context, Result};
use rustycode_protocol::{Session, SessionEvent, SessionId};
use rustycode_session::session_manager::SessionStats;
use rustycode_storage::conversation_history::{
    new_conversation_id, now_timestamp, Conversation as HistoryConversation, ConversationHistory,
    SavedMessage,
};
use tracing::info;

use super::Runtime;

impl Runtime {
    /// Save a session to disk
    pub fn save_session(&self, session: &Session) -> Result<()> {
        if let Some(manager) = &self.session_manager {
            manager.save_session(session)?;
            Ok(())
        } else {
            Err(anyhow::anyhow!("Session manager not initialized"))
        }
    }

    /// Load a session by ID
    pub fn load_session(&self, session_id: &SessionId) -> Result<Session> {
        if let Some(manager) = &self.session_manager {
            manager.load_session(session_id)
        } else {
            Err(anyhow::anyhow!("Session manager not initialized"))
        }
    }

    /// Fork an existing session
    pub fn fork_session(&self, session_id: &SessionId) -> Result<SessionId> {
        if let Some(manager) = &self.session_manager {
            manager.fork_session(session_id)
        } else {
            Err(anyhow::anyhow!("Session manager not initialized"))
        }
    }

    /// List all sessions
    pub fn list_sessions(&self) -> Result<Vec<Session>> {
        if let Some(manager) = &self.session_manager {
            manager.list_sessions()
        } else {
            Err(anyhow::anyhow!("Session manager not initialized"))
        }
    }

    /// Delete a session
    pub fn delete_session(&self, session_id: &SessionId) -> Result<()> {
        if let Some(manager) = &self.session_manager {
            manager.delete_session(session_id)
        } else {
            Err(anyhow::anyhow!("Session manager not initialized"))
        }
    }

    /// Clean up old sessions
    pub fn cleanup_old_sessions(&self, days_old: u64) -> Result<usize> {
        if let Some(manager) = &self.session_manager {
            manager.cleanup_old_sessions(days_old)
        } else {
            Err(anyhow::anyhow!("Session manager not initialized"))
        }
    }

    /// Get session statistics
    pub fn session_stats(&self) -> Result<SessionStats> {
        if let Some(manager) = &self.session_manager {
            manager.stats()
        } else {
            Err(anyhow::anyhow!("Session manager not initialized"))
        }
    }

    /// Get recent sessions.
    pub fn recent_sessions(&self, limit: usize) -> Result<Vec<Session>> {
        self.storage.recent_sessions(limit)
    }

    /// Get events for a session.
    pub fn session_events(&self, session_id: &SessionId) -> Result<Vec<SessionEvent>> {
        self.storage.session_events(session_id)
    }

    /// Save conversation history to disk
    pub fn save_conversation(
        &self,
        session_id: &SessionId,
        messages: &[rustycode_llm::provider::ChatMessage],
        task: &str,
        model: &str,
        provider: &str,
        input_tokens: u64,
        output_tokens: u64,
    ) -> Result<()> {
        // Initialize conversation history manager
        let history = ConversationHistory::default_dir()
            .context("Failed to initialize conversation history")?;

        // Extract tags from task (first few words)
        let tags: Vec<String> = task
            .split_whitespace()
            .take(3)
            .map(|s| s.to_lowercase())
            .collect();

        // Convert ChatMessage to SavedMessage
        let saved_messages: Vec<SavedMessage> = messages
            .iter()
            .map(|msg| SavedMessage {
                role: format!("{:?}", msg.role).to_lowercase(),
                content: msg.content.as_text().to_string(),
                timestamp: now_timestamp(),
                tokens: None,
                model: Some(model.to_string()),
                provider: Some(provider.to_string()),
            })
            .collect();

        // Create conversation
        let conversation = HistoryConversation {
            id: new_conversation_id(),
            title: task.chars().take(80).collect::<String>(),
            created_at: now_timestamp(),
            updated_at: now_timestamp(),
            model: model.to_string(),
            provider: provider.to_string(),
            messages: saved_messages,
            tags,
            total_tokens: input_tokens.saturating_add(output_tokens),
            total_cost_cents: 0,
            workspace_path: std::env::current_dir()
                .ok()
                .map(|p| p.display().to_string()),
        };

        // Save to disk
        history
            .save(&conversation)
            .context("Failed to save conversation history")?;

        info!(
            session_id = %session_id.to_string(),
            conversation_id = %conversation.id,
            "Conversation saved to history"
        );

        Ok(())
    }
}
