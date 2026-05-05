//! Conversation History System
//!
//! JSON file-based conversation persistence with search and export.
//! Stores complete conversations with metadata, cost tracking, and tags.
//!
//! This is a lightweight alternative to SQLite-based storage, suitable for:
//! - Simple conversation archival
//! - Easy backup and migration
//! - Development and testing
//! - User-accessible conversation management

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Context;

/// A saved conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conversation {
    pub id: String,
    pub title: String,
    pub created_at: u64,
    pub updated_at: u64,
    pub model: String,
    pub provider: String,
    pub messages: Vec<SavedMessage>,
    pub tags: Vec<String>,
    pub total_tokens: u64,
    pub total_cost_cents: u32,
    pub workspace_path: Option<String>,
}

/// A message in a saved conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedMessage {
    pub role: String,
    pub content: String,
    pub timestamp: u64,
    pub tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
}

/// Filter for searching conversations
#[derive(Debug, Clone, Default)]
pub struct ConversationFilter {
    pub query: Option<String>,
    pub tags: Vec<String>,
    pub model: Option<String>,
    pub since: Option<u64>,
    pub until: Option<u64>,
    pub limit: usize,
    pub offset: usize,
}

/// Export format
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum ExportFormat {
    Json,
    Markdown,
}

/// Manages conversation history stored as JSON files
pub struct ConversationHistory {
    storage_dir: PathBuf,
}

impl ConversationHistory {
    /// Create a new conversation history manager with a custom storage directory
    ///
    pub fn new(storage_dir: impl AsRef<Path>) -> anyhow::Result<Self> {
        let dir = storage_dir.as_ref().to_path_buf();
        std::fs::create_dir_all(&dir).with_context(|| {
            format!(
                "failed to create conversation history directory at {}",
                dir.display()
            )
        })?;
        Ok(Self { storage_dir: dir })
    }

    /// Use the default storage directory (~/.rustycode/conversations)
    ///
    pub fn default_dir() -> anyhow::Result<Self> {
        let home =
            dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Cannot determine home directory"))?;
        let dir = home.join(".rustycode").join("conversations");
        Self::new(dir)
    }

    /// Save a conversation to disk
    ///
    pub fn save(&self, conversation: &Conversation) -> anyhow::Result<()> {
        validate_id(&conversation.id)?;
        let filename = format!("{}.json", conversation.id);
        let path = self.storage_dir.join(&filename);
        let json = serde_json::to_string_pretty(conversation)
            .context("failed to serialize conversation")?;

        // Atomic write: temp file + rename to avoid corruption on crash
        let tmp_path = path.with_extension("json.tmp");
        std::fs::write(&tmp_path, &json)
            .with_context(|| format!("failed to write temp file to {}", tmp_path.display()))?;
        if let Err(e) = std::fs::rename(&tmp_path, &path) {
            let _ = std::fs::remove_file(&tmp_path);
            return Err(e)
                .with_context(|| format!("failed to rename temp file to {}", path.display()));
        }
        Ok(())
    }

    /// Load a conversation by ID
    ///
    pub fn load(&self, id: &str) -> anyhow::Result<Conversation> {
        validate_id(id)?;
        let filename = format!("{id}.json");
        let path = self.storage_dir.join(&filename);
        let json = std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read conversation from {}", path.display()))?;
        let conversation: Conversation = serde_json::from_str(&json).with_context(|| {
            format!("failed to parse conversation JSON from {}", path.display())
        })?;
        Ok(conversation)
    }

    /// Delete a conversation by ID
    ///
    pub fn delete(&self, id: &str) -> anyhow::Result<()> {
        validate_id(id)?;
        let filename = format!("{id}.json");
        let path = self.storage_dir.join(&filename);
        std::fs::remove_file(&path)
            .with_context(|| format!("failed to delete conversation at {}", path.display()))?;
        Ok(())
    }

    /// List all conversations, sorted by `updated_at` descending
    ///
    pub fn list(&self, limit: usize) -> anyhow::Result<Vec<ConversationSummary>> {
        let mut summaries = Vec::new();

        let entries = std::fs::read_dir(&self.storage_dir)
            .with_context(|| format!("failed to read directory {}", self.storage_dir.display()))?;

        for entry in entries {
            let entry = entry.context("failed to read directory entry")?;
            if entry.path().extension().and_then(|e| e.to_str()) == Some("json") {
                if let Ok(json) = std::fs::read_to_string(entry.path()) {
                    if let Ok(conv) = serde_json::from_str::<Conversation>(&json) {
                        summaries.push(ConversationSummary {
                            id: conv.id,
                            title: conv.title,
                            model: conv.model,
                            updated_at: conv.updated_at,
                            message_count: conv.messages.len(),
                            tags: conv.tags,
                            total_cost_cents: conv.total_cost_cents,
                        });
                    }
                }
            }
        }

        summaries.sort_by_key(|b| std::cmp::Reverse(b.updated_at));
        summaries.truncate(limit);
        Ok(summaries)
    }

    /// Search conversations by query text, tags, model, or date range
    ///
    pub fn search(&self, filter: &ConversationFilter) -> anyhow::Result<Vec<ConversationSummary>> {
        let mut results = Vec::new();

        let entries = std::fs::read_dir(&self.storage_dir)
            .with_context(|| format!("failed to read directory {}", self.storage_dir.display()))?;

        for entry in entries {
            let entry = entry.context("failed to read directory entry")?;
            if entry.path().extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }

            let json = std::fs::read_to_string(entry.path())
                .context("failed to read conversation JSON")?;
            let conv: Conversation = match serde_json::from_str(&json) {
                Ok(c) => c,
                Err(_) => continue,
            };

            // Apply filters
            if let Some(query) = &filter.query {
                let query_lower = query.to_lowercase();
                let matches = conv.title.to_lowercase().contains(&query_lower)
                    || conv
                        .messages
                        .iter()
                        .any(|m| m.content.to_lowercase().contains(&query_lower));
                if !matches {
                    continue;
                }
            }

            if !filter.tags.is_empty() && !filter.tags.iter().all(|t| conv.tags.contains(t)) {
                continue;
            }

            if let Some(model) = &filter.model {
                if &conv.model != model {
                    continue;
                }
            }

            if let Some(since) = filter.since {
                if conv.updated_at < since {
                    continue;
                }
            }

            if let Some(until) = filter.until {
                if conv.updated_at > until {
                    continue;
                }
            }

            results.push(ConversationSummary {
                id: conv.id,
                title: conv.title,
                model: conv.model,
                updated_at: conv.updated_at,
                message_count: conv.messages.len(),
                tags: conv.tags,
                total_cost_cents: conv.total_cost_cents,
            });
        }

        results.sort_by_key(|b| std::cmp::Reverse(b.updated_at));
        let limit = if filter.limit == 0 { 50 } else { filter.limit };
        results.truncate(limit);
        Ok(results)
    }

    /// Export a conversation to the specified format
    ///
    pub fn export(&self, id: &str, format: ExportFormat, output_path: &Path) -> anyhow::Result<()> {
        let conv = self.load(id)?;

        let content = match format {
            ExportFormat::Json => serde_json::to_string_pretty(&conv)
                .context("failed to serialize conversation to JSON")?,
            ExportFormat::Markdown => self.to_markdown(&conv),
        };

        std::fs::write(output_path, content)
            .with_context(|| format!("failed to write export to {}", output_path.display()))?;
        Ok(())
    }

    /// Convert a conversation to Markdown format
    fn to_markdown(&self, conv: &Conversation) -> String {
        let mut md = String::new();
        md.push_str(&format!("# {}\n\n", conv.title));
        md.push_str(&format!("**Model:** {}  \n", conv.model));
        md.push_str(&format!("**Provider:** {}  \n", conv.provider));
        md.push_str(&format!(
            "**Date:** {}  \n",
            format_timestamp(conv.created_at)
        ));
        md.push_str(&format!("**Messages:** {}  \n", conv.messages.len()));
        if conv.total_tokens > 0 {
            md.push_str(&format!("**Total Tokens:** {}  \n", conv.total_tokens));
        }
        if conv.total_cost_cents > 0 {
            md.push_str(&format!(
                "**Cost:** ${:.2}  \n",
                f64::from(conv.total_cost_cents) / 100.0
            ));
        }
        if !conv.tags.is_empty() {
            md.push_str(&format!("**Tags:** {}  \n", conv.tags.join(", ")));
        }
        if let Some(workspace) = &conv.workspace_path {
            md.push_str(&format!("**Workspace:** {workspace}  \n"));
        }
        md.push_str("\n---\n\n");

        for msg in &conv.messages {
            md.push_str(&format!(
                "## {} ({})\n\n",
                capitalize(&msg.role),
                format_timestamp(msg.timestamp)
            ));
            md.push_str(&msg.content);
            md.push_str("\n\n");
            if let Some(model) = &msg.model {
                md.push_str(&format!("*Model: {model}*\n\n"));
            }
            if let Some(provider) = &msg.provider {
                md.push_str(&format!("*Provider: {provider}*\n\n"));
            }
            if let Some(tokens) = msg.tokens {
                md.push_str(&format!("*Tokens: {tokens}*\n\n"));
            }
        }

        md
    }
}

/// Summary of a conversation for listings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationSummary {
    pub id: String,
    pub title: String,
    pub model: String,
    pub updated_at: u64,
    pub message_count: usize,
    pub tags: Vec<String>,
    pub total_cost_cents: u32,
}

/// Create a new unique conversation ID using UUID v4
///
pub fn new_conversation_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// Get the current Unix timestamp in seconds
///
pub fn now_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Format a Unix timestamp as a human-readable date string
fn format_timestamp(ts: u64) -> String {
    match chrono::DateTime::from_timestamp(ts as i64, 0) {
        Some(dt) => dt.format("%Y-%m-%d %H:%M").to_string(),
        None => ts.to_string(),
    }
}

/// Capitalize the first character of a string
fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
    }
}

/// Validate that a conversation ID doesn't contain path traversal characters.
fn validate_id(id: &str) -> anyhow::Result<()> {
    if id.contains('/') || id.contains('\\') || id == ".." {
        anyhow::bail!("invalid conversation ID: {id}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn test_conversation() -> Conversation {
        Conversation {
            id: new_conversation_id(),
            title: "Test Conversation".into(),
            created_at: now_timestamp(),
            updated_at: now_timestamp(),
            model: "claude-3.5".into(),
            provider: "anthropic".into(),
            messages: vec![
                SavedMessage {
                    role: "user".into(),
                    content: "Hello".into(),
                    timestamp: now_timestamp(),
                    tokens: Some(5),
                    model: Some("claude-3.5".into()),
                    provider: Some("anthropic".into()),
                },
                SavedMessage {
                    role: "assistant".into(),
                    content: "Hi there!".into(),
                    timestamp: now_timestamp(),
                    tokens: Some(10),
                    model: Some("claude-3.5".into()),
                    provider: Some("anthropic".into()),
                },
            ],
            tags: vec!["test".into()],
            total_tokens: 15,
            total_cost_cents: 1,
            workspace_path: None,
        }
    }

    #[test]
    fn test_save_and_load() {
        let dir = TempDir::new().unwrap();
        let history = ConversationHistory::new(dir.path()).unwrap();

        let conv = test_conversation();
        let id = conv.id.clone();

        history.save(&conv).unwrap();
        let loaded = history.load(&id).unwrap();

        assert_eq!(loaded.title, "Test Conversation");
        assert_eq!(loaded.messages.len(), 2);
        assert_eq!(loaded.total_tokens, 15);
        assert_eq!(loaded.tags.len(), 1);
    }

    #[test]
    fn test_list() {
        let dir = TempDir::new().unwrap();
        let history = ConversationHistory::new(dir.path()).unwrap();

        history.save(&test_conversation()).unwrap();
        history.save(&test_conversation()).unwrap();

        let list = history.list(10).unwrap();
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn test_search_by_query() {
        let dir = TempDir::new().unwrap();
        let history = ConversationHistory::new(dir.path()).unwrap();

        let mut conv = test_conversation();
        conv.title = "Rust refactoring task".into();
        history.save(&conv).unwrap();

        let results = history
            .search(&ConversationFilter {
                query: Some("rust".into()),
                limit: 10,
                ..Default::default()
            })
            .unwrap();

        assert_eq!(results.len(), 1);
        assert!(results[0].title.to_lowercase().contains("rust"));
    }

    #[test]
    fn test_search_by_tag() {
        let dir = TempDir::new().unwrap();
        let history = ConversationHistory::new(dir.path()).unwrap();

        let mut conv = test_conversation();
        conv.tags = vec!["refactor".into(), "rust".into()];
        history.save(&conv).unwrap();

        let results = history
            .search(&ConversationFilter {
                tags: vec!["refactor".into()],
                limit: 10,
                ..Default::default()
            })
            .unwrap();

        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_delete() {
        let dir = TempDir::new().unwrap();
        let history = ConversationHistory::new(dir.path()).unwrap();

        let conv = test_conversation();
        let id = conv.id.clone();
        history.save(&conv).unwrap();

        history.delete(&id).unwrap();
        assert!(history.load(&id).is_err());
    }

    #[test]
    fn test_export_markdown() {
        let dir = TempDir::new().unwrap();
        let history = ConversationHistory::new(dir.path()).unwrap();

        let conv = test_conversation();
        let id = conv.id.clone();
        history.save(&conv).unwrap();

        let output = dir.path().join("export.md");
        history
            .export(&id, ExportFormat::Markdown, &output)
            .unwrap();

        let content = std::fs::read_to_string(&output).unwrap();
        assert!(content.contains("# Test Conversation"));
        assert!(content.contains("Hello"));
        assert!(content.contains("## User"));
    }

    #[test]
    fn test_export_json() {
        let dir = TempDir::new().unwrap();
        let history = ConversationHistory::new(dir.path()).unwrap();

        let conv = test_conversation();
        let id = conv.id.clone();
        history.save(&conv).unwrap();

        let output = dir.path().join("export.json");
        history.export(&id, ExportFormat::Json, &output).unwrap();

        let content = std::fs::read_to_string(&output).unwrap();
        let exported: Conversation = serde_json::from_str(&content).unwrap();
        assert_eq!(exported.id, conv.id);
        assert_eq!(exported.title, conv.title);
    }

    #[test]
    fn test_search_with_multiple_filters() {
        let dir = TempDir::new().unwrap();
        let history = ConversationHistory::new(dir.path()).unwrap();

        let mut conv = test_conversation();
        conv.title = "Python script optimization".into();
        conv.model = "gpt-4".into();
        conv.tags = vec!["python".into(), "optimization".into()];
        conv.created_at = now_timestamp();
        conv.updated_at = now_timestamp();
        history.save(&conv).unwrap();

        // Search by model and tag
        let results = history
            .search(&ConversationFilter {
                model: Some("gpt-4".into()),
                tags: vec!["python".into()],
                limit: 10,
                ..Default::default()
            })
            .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].model, "gpt-4");
    }

    #[test]
    fn test_conversation_summary_fields() {
        let dir = TempDir::new().unwrap();
        let history = ConversationHistory::new(dir.path()).unwrap();

        let conv = test_conversation();
        history.save(&conv).unwrap();

        let summaries = history.list(10).unwrap();
        let summary = &summaries[0];

        assert_eq!(summary.title, "Test Conversation");
        assert_eq!(summary.message_count, 2);
        assert_eq!(summary.total_cost_cents, 1);
        assert_eq!(summary.tags.len(), 1);
    }

    #[test]
    fn test_default_dir() {
        let history = ConversationHistory::default_dir();
        assert!(history.is_ok());
        // The directory should be created
        let history = history.unwrap();
        assert!(history.storage_dir.exists());
    }

    #[test]
    fn test_conversation_filter_default() {
        let filter = ConversationFilter::default();
        assert!(filter.query.is_none());
        assert!(filter.tags.is_empty());
        assert!(filter.model.is_none());
        assert!(filter.since.is_none());
        assert!(filter.until.is_none());
        assert_eq!(filter.limit, 0);
        assert_eq!(filter.offset, 0);
    }

    #[test]
    fn test_export_format_equality() {
        assert_eq!(ExportFormat::Json, ExportFormat::Json);
        assert_eq!(ExportFormat::Markdown, ExportFormat::Markdown);
        assert_ne!(ExportFormat::Json, ExportFormat::Markdown);
    }

    #[test]
    fn test_new_conversation_id_unique() {
        let id1 = new_conversation_id();
        let id2 = new_conversation_id();
        assert_ne!(id1, id2);
        assert!(!id1.is_empty());
        // UUID v4 format: 8-4-4-4-12
        assert_eq!(id1.len(), 36);
    }

    #[test]
    fn test_now_timestamp_reasonable() {
        let ts = now_timestamp();
        // Should be after 2020-01-01 and before 2100-01-01
        assert!(ts > 1577836800);
        assert!(ts < 4102444800);
    }

    #[test]
    fn test_conversation_serialization() {
        let conv = test_conversation();
        let json = serde_json::to_string(&conv).unwrap();
        let decoded: Conversation = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.id, conv.id);
        assert_eq!(decoded.title, conv.title);
        assert_eq!(decoded.model, conv.model);
        assert_eq!(decoded.provider, conv.provider);
        assert_eq!(decoded.messages.len(), conv.messages.len());
        assert_eq!(decoded.tags, conv.tags);
        assert_eq!(decoded.total_tokens, conv.total_tokens);
        assert_eq!(decoded.total_cost_cents, conv.total_cost_cents);
    }

    #[test]
    fn test_saved_message_serialization() {
        let msg = SavedMessage {
            role: "user".into(),
            content: "Hello world".into(),
            timestamp: 1234567890,
            tokens: Some(42),
            model: Some("gpt-4".into()),
            provider: Some("openai".into()),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let decoded: SavedMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.role, "user");
        assert_eq!(decoded.content, "Hello world");
        assert_eq!(decoded.timestamp, 1234567890);
        assert_eq!(decoded.tokens, Some(42));
        assert_eq!(decoded.model, Some("gpt-4".into()));
        assert_eq!(decoded.provider, Some("openai".into()));
    }

    #[test]
    fn test_saved_message_no_tokens() {
        let msg = SavedMessage {
            role: "system".into(),
            content: "prompt".into(),
            timestamp: 0,
            tokens: None,
            model: None,
            provider: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        let decoded: SavedMessage = serde_json::from_str(&json).unwrap();
        assert!(decoded.tokens.is_none());
        assert!(decoded.model.is_none());
        assert!(decoded.provider.is_none());
    }

    #[test]
    fn test_conversation_summary_serialization() {
        let summary = ConversationSummary {
            id: "test-id".to_string(),
            title: "Test".to_string(),
            model: "gpt-4".to_string(),
            updated_at: 12345,
            message_count: 5,
            tags: vec!["tag1".to_string()],
            total_cost_cents: 99,
        };
        let json = serde_json::to_string(&summary).unwrap();
        let decoded: ConversationSummary = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.id, "test-id");
        assert_eq!(decoded.message_count, 5);
        assert_eq!(decoded.total_cost_cents, 99);
    }

    #[test]
    fn test_search_by_model() {
        let dir = TempDir::new().unwrap();
        let history = ConversationHistory::new(dir.path()).unwrap();

        let mut conv1 = test_conversation();
        conv1.model = "gpt-4".into();
        history.save(&conv1).unwrap();

        let mut conv2 = test_conversation();
        conv2.model = "claude-3.5".into();
        history.save(&conv2).unwrap();

        let results = history
            .search(&ConversationFilter {
                model: Some("gpt-4".into()),
                limit: 10,
                ..Default::default()
            })
            .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].model, "gpt-4");
    }

    #[test]
    fn test_search_by_date_range() {
        let dir = TempDir::new().unwrap();
        let history = ConversationHistory::new(dir.path()).unwrap();

        let mut conv_old = test_conversation();
        conv_old.updated_at = 1000;
        history.save(&conv_old).unwrap();

        let mut conv_new = test_conversation();
        conv_new.updated_at = 5000;
        history.save(&conv_new).unwrap();

        let results = history
            .search(&ConversationFilter {
                since: Some(2000),
                until: Some(6000),
                limit: 10,
                ..Default::default()
            })
            .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].updated_at, 5000);
    }

    #[test]
    fn test_search_no_results() {
        let dir = TempDir::new().unwrap();
        let history = ConversationHistory::new(dir.path()).unwrap();

        let results = history
            .search(&ConversationFilter {
                query: Some("nonexistent_xyzzy".into()),
                limit: 10,
                ..Default::default()
            })
            .unwrap();

        assert!(results.is_empty());
    }

    #[test]
    fn test_list_with_limit() {
        let dir = TempDir::new().unwrap();
        let history = ConversationHistory::new(dir.path()).unwrap();

        for _ in 0..5 {
            history.save(&test_conversation()).unwrap();
        }

        let list = history.list(3).unwrap();
        assert_eq!(list.len(), 3);
    }

    #[test]
    fn test_list_empty() {
        let dir = TempDir::new().unwrap();
        let history = ConversationHistory::new(dir.path()).unwrap();

        let list = history.list(10).unwrap();
        assert!(list.is_empty());
    }

    #[test]
    fn test_load_nonexistent() {
        let dir = TempDir::new().unwrap();
        let history = ConversationHistory::new(dir.path()).unwrap();

        let result = history.load("nonexistent-id");
        assert!(result.is_err());
    }

    #[test]
    fn test_delete_nonexistent() {
        let dir = TempDir::new().unwrap();
        let history = ConversationHistory::new(dir.path()).unwrap();

        let result = history.delete("nonexistent-id");
        assert!(result.is_err());
    }

    #[test]
    fn test_conversation_with_workspace_path() {
        let dir = TempDir::new().unwrap();
        let history = ConversationHistory::new(dir.path()).unwrap();

        let mut conv = test_conversation();
        conv.workspace_path = Some("/home/user/project".into());
        let id = conv.id.clone();
        history.save(&conv).unwrap();

        let loaded = history.load(&id).unwrap();
        assert_eq!(loaded.workspace_path, Some("/home/user/project".into()));
    }

    #[test]
    fn test_export_markdown_includes_metadata() {
        let dir = TempDir::new().unwrap();
        let history = ConversationHistory::new(dir.path()).unwrap();

        let mut conv = test_conversation();
        conv.workspace_path = Some("/dev/project".into());
        let id = conv.id.clone();
        history.save(&conv).unwrap();

        let output = dir.path().join("export.md");
        history
            .export(&id, ExportFormat::Markdown, &output)
            .unwrap();

        let content = std::fs::read_to_string(&output).unwrap();
        assert!(content.contains("**Model:** claude-3.5"));
        assert!(content.contains("**Provider:** anthropic"));
        assert!(content.contains("**Messages:** 2"));
        assert!(content.contains("**Total Tokens:** 15"));
        assert!(content.contains("**Cost:**"));
        assert!(content.contains("**Tags:** test"));
        assert!(content.contains("**Workspace:** /dev/project"));
    }

    #[test]
    fn test_search_case_insensitive() {
        let dir = TempDir::new().unwrap();
        let history = ConversationHistory::new(dir.path()).unwrap();

        let mut conv = test_conversation();
        conv.title = "Rust Refactoring".into();
        history.save(&conv).unwrap();

        let results_lower = history
            .search(&ConversationFilter {
                query: Some("rust".into()),
                limit: 10,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(results_lower.len(), 1);

        let results_upper = history
            .search(&ConversationFilter {
                query: Some("RUST".into()),
                limit: 10,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(results_upper.len(), 1);
    }

    #[test]
    fn test_search_in_message_content() {
        let dir = TempDir::new().unwrap();
        let history = ConversationHistory::new(dir.path()).unwrap();

        let mut conv = test_conversation();
        conv.title = "Generic title".into();
        conv.messages.push(SavedMessage {
            role: "user".into(),
            content: "How do I implement async streams in Rust?".into(),
            timestamp: now_timestamp(),
            tokens: Some(20),
            model: Some("claude-3.5".into()),
            provider: Some("anthropic".into()),
        });
        history.save(&conv).unwrap();

        let results = history
            .search(&ConversationFilter {
                query: Some("async streams".into()),
                limit: 10,
                ..Default::default()
            })
            .unwrap();

        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_search_tag_must_match_all() {
        let dir = TempDir::new().unwrap();
        let history = ConversationHistory::new(dir.path()).unwrap();

        let mut conv = test_conversation();
        conv.tags = vec!["rust".into(), "async".into()];
        history.save(&conv).unwrap();

        // Both tags present → match
        let results = history
            .search(&ConversationFilter {
                tags: vec!["rust".into(), "async".into()],
                limit: 10,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(results.len(), 1);

        // Missing one tag → no match
        let results = history
            .search(&ConversationFilter {
                tags: vec!["rust".into(), "missing".into()],
                limit: 10,
                ..Default::default()
            })
            .unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_conversation_empty_messages() {
        let dir = TempDir::new().unwrap();
        let history = ConversationHistory::new(dir.path()).unwrap();

        let conv = Conversation {
            id: new_conversation_id(),
            title: "Empty".into(),
            created_at: now_timestamp(),
            updated_at: now_timestamp(),
            model: "test".into(),
            provider: "test".into(),
            messages: vec![],
            tags: vec![],
            total_tokens: 0,
            total_cost_cents: 0,
            workspace_path: None,
        };
        let id = conv.id.clone();
        history.save(&conv).unwrap();

        let loaded = history.load(&id).unwrap();
        assert_eq!(loaded.messages.len(), 0);
        assert_eq!(loaded.total_tokens, 0);
    }

    #[test]
    fn test_save_rejects_path_traversal() {
        let dir = TempDir::new().unwrap();
        let history = ConversationHistory::new(dir.path()).unwrap();

        let mut conv = test_conversation();
        conv.id = "../etc/passwd".to_string();
        assert!(history.save(&conv).is_err());

        conv.id = "foo\\bar".to_string();
        assert!(history.save(&conv).is_err());

        conv.id = "..".to_string();
        assert!(history.save(&conv).is_err());
    }

    #[test]
    fn test_load_rejects_path_traversal() {
        let dir = TempDir::new().unwrap();
        let history = ConversationHistory::new(dir.path()).unwrap();

        assert!(history.load("../etc/passwd").is_err());
        assert!(history.load("foo\\bar").is_err());
        assert!(history.load("..").is_err());
    }

    #[test]
    fn test_delete_rejects_path_traversal() {
        let dir = TempDir::new().unwrap();
        let history = ConversationHistory::new(dir.path()).unwrap();

        assert!(history.delete("../etc/passwd").is_err());
        assert!(history.delete("foo\\bar").is_err());
        assert!(history.delete("..").is_err());
    }
}
