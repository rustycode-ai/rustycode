//! Memory slash commands

use anyhow::Result;
use chrono::Utc;

use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// Memory storage file
const MEMORY_FILE: &str = ".rustycode/memory.json";

/// Maximum key length for memory entries
const MAX_KEY_LENGTH: usize = 100;

/// Maximum value size for memory entries (10KB)
const MAX_VALUE_SIZE: usize = 10 * 1024;

/// Errors that can occur during memory operations
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum MemoryError {
    /// Key format is invalid
    InvalidKey(String),

    /// Key already exists
    KeyExists(String),

    /// Value is too large
    ValueTooLarge { max: usize, actual: usize },

    /// Key not found
    NotFound(String),

    /// File I/O error
    IoError(String),
}

impl std::fmt::Display for MemoryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MemoryError::InvalidKey(key) => {
                write!(f, "❌ Invalid key format: '{}'. Use alphanumeric characters, underscores, and hyphens only.", key)
            }
            MemoryError::KeyExists(key) => {
                write!(
                    f,
                    "❌ Memory key '{}' already exists. Use /memory delete {} first.",
                    key, key
                )
            }
            MemoryError::ValueTooLarge { max, actual } => {
                write!(
                    f,
                    "❌ Value too large (max {} bytes, got {} bytes)",
                    max, actual
                )
            }
            MemoryError::NotFound(key) => {
                write!(f, "❌ Memory not found: {}", key)
            }
            MemoryError::IoError(msg) => {
                write!(f, "❌ I/O error: {}", msg)
            }
        }
    }
}

impl std::error::Error for MemoryError {}

impl From<std::io::Error> for MemoryError {
    fn from(err: std::io::Error) -> Self {
        MemoryError::IoError(err.to_string())
    }
}

/// Convert a `Result<String, MemoryError>` to `Result<String>` (anyhow)
/// This allows the memory commands to use proper error types while still
/// being compatible with code that expects `anyhow::Result`.
pub fn into_anyhow_result(result: Result<String, MemoryError>) -> Result<String> {
    result.map_err(|e| anyhow::anyhow!("{}", e))
}

/// Get memory count for status bar
/// Simple memory entry for key-value storage
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct KeyValueMemory {
    /// Unique key for the memory
    key: String,
    /// Value content
    value: String,
    /// Timestamp when created
    created_at: String,
    /// Timestamp when last accessed
    last_accessed: String,
    /// Number of times accessed
    access_count: usize,
    /// Tags for categorization
    tags: Vec<String>,
}

/// Handle /memory save command
///
/// Saves a key-value pair to memory
///
pub async fn handle_save_command(
    cwd: &Path,
    key: String,
    value: String,
) -> Result<String, MemoryError> {
    // Validate key format
    if !is_valid_key(&key) {
        return Err(MemoryError::InvalidKey(key));
    }

    // Validate value size
    if value.len() > MAX_VALUE_SIZE {
        return Err(MemoryError::ValueTooLarge {
            max: MAX_VALUE_SIZE,
            actual: value.len(),
        });
    }

    // Load existing memories
    let mut memories = load_memories(cwd)?;

    // Check for duplicate key
    if memories.contains_key(&key) {
        return Err(MemoryError::KeyExists(key));
    }

    // Create new memory entry
    let memory = KeyValueMemory {
        key: key.clone(),
        value,
        created_at: Utc::now().to_rfc3339(),
        last_accessed: Utc::now().to_rfc3339(),
        access_count: 0,
        tags: vec![],
    };

    // Save to memories
    memories.insert(key.clone(), memory);

    // Persist to disk
    save_memories(cwd, &memories)?;

    Ok(format!("✓ Saved memory: {}", key))
}

/// Handle /memory recall command
///
/// Retrieves a value from memory by key
///
pub async fn handle_recall_command(cwd: &Path, key: String) -> Result<String, MemoryError> {
    let mut memories = load_memories(cwd)?;

    if let Some(memory) = memories.get_mut(&key) {
        // Update access statistics
        memory.last_accessed = Utc::now().to_rfc3339();
        memory.access_count += 1;

        // Clone needed data before releasing mutable borrow
        let created = format_created_time(&memory.created_at);
        let value = memory.value.clone();
        let access_count = memory.access_count;

        // Release mutable borrow before saving
        let _ = memory;

        // Save updated statistics
        save_memories(cwd, &memories)?;

        Ok(format!(
            "📝 Memory: {}\n  Value: {}\n  Saved: {}\n  Accessed: {} times",
            key, value, created, access_count
        ))
    } else {
        // Return Ok with error message instead of Err
        Ok(format!("❌ Memory '{}' not found", key))
    }
}

/// Handle /memory search command
///
/// Searches memories by key or value content
///
pub async fn handle_search_command(cwd: &Path, query: String) -> Result<String> {
    let memories = load_memories(cwd)?;

    if memories.is_empty() {
        return Ok(
            "ℹ️ No memories saved yet. Use /memory save <key> <value> to create one.".to_string(),
        );
    }

    let query_lower = query.to_lowercase();
    let matches: Vec<_> = memories
        .iter()
        .filter(|(key, memory)| {
            key.to_lowercase().contains(&query_lower)
                || memory.value.to_lowercase().contains(&query_lower)
                || memory
                    .tags
                    .iter()
                    .any(|tag| tag.to_lowercase().contains(&query_lower))
        })
        .collect();

    if matches.is_empty() {
        Ok(format!("ℹ️ No memories found matching '{}'", query))
    } else {
        let mut result = format!("📝 Memories matching '{}':\n", query);
        for (key, memory) in matches {
            result.push_str(&format!(
                "  {}: {}\n",
                key,
                truncate_string(&memory.value, 60)
            ));
        }
        Ok(result)
    }
}

/// Handle /memory list command
///
/// Lists all saved memories
///
pub async fn handle_list_command(cwd: &Path) -> Result<String> {
    let memories = load_memories(cwd)?;

    if memories.is_empty() {
        return Ok(
            "ℹ️ No memories saved yet. Use /memory save <key> <value> to create one.".to_string(),
        );
    }

    let mut result = format!("📝 Saved Memories ({} items):\n", memories.len());

    // Sort by creation time (most recent first)
    let mut sorted_memories: Vec<_> = memories.iter().collect();
    sorted_memories.sort_by_key(|a| std::cmp::Reverse(a.1.created_at.clone()));

    for (key, memory) in sorted_memories {
        result.push_str(&format!(
            "  {}: {}\n",
            key,
            truncate_string(&memory.value, 60)
        ));
    }

    Ok(result)
}

/// Handle /memory delete command
///
/// Deletes a memory by key
///
pub async fn handle_delete_command(cwd: &Path, key: String) -> Result<String, MemoryError> {
    let mut memories = load_memories(cwd)?;

    if memories.remove(&key).is_some() {
        save_memories(cwd, &memories)?;
        Ok(format!("✓ Deleted memory: {}", key))
    } else {
        Err(MemoryError::NotFound(key))
    }
}

/// Handle /memory clear command
///
/// Clears all saved memories
///
pub async fn handle_clear_command(cwd: &Path) -> Result<String, MemoryError> {
    let memories = load_memories(cwd)?;
    let count = memories.len();

    if count == 0 {
        return Ok("ℹ️ No memories to clear.".to_string());
    }

    // Clear all memories
    save_memories(cwd, &HashMap::new())?;

    Ok(format!("✓ Cleared {} memories", count))
}

/// Get memory count for status bar
///
pub fn memory_count(cwd: &Path) -> usize {
    load_memories(cwd).map(|m| m.len()).unwrap_or(0)
}

// ── Helper Functions ─────────────────────────────────────────────────────

/// Load memories from disk
fn load_memories(cwd: &Path) -> Result<HashMap<String, KeyValueMemory>, MemoryError> {
    let memory_path = cwd.join(MEMORY_FILE);

    if !memory_path.exists() {
        return Ok(HashMap::new());
    }

    let content =
        fs::read_to_string(&memory_path).map_err(|e| MemoryError::IoError(e.to_string()))?;
    let memories: HashMap<String, KeyValueMemory> = serde_json::from_str(&content)
        .map_err(|e| MemoryError::IoError(format!("Failed to parse memory file: {}", e)))?;

    Ok(memories)
}

/// Save memories to disk
fn save_memories(
    cwd: &Path,
    memories: &HashMap<String, KeyValueMemory>,
) -> Result<(), MemoryError> {
    let memory_path = cwd.join(MEMORY_FILE);

    // Create directory if it doesn't exist
    if let Some(parent) = memory_path.parent() {
        fs::create_dir_all(parent).map_err(|e| MemoryError::IoError(e.to_string()))?;
    }

    // Serialize and write to file
    let content = serde_json::to_string_pretty(memories)
        .map_err(|e| MemoryError::IoError(format!("Failed to serialize: {}", e)))?;
    fs::write(&memory_path, content)
        .map_err(|e| MemoryError::IoError(format!("Failed to write: {}", e)))?;

    Ok(())
}

/// Validate memory key format
/// Only allows alphanumeric characters, underscores, and hyphens
fn is_valid_key(key: &str) -> bool {
    if key.is_empty() || key.len() > MAX_KEY_LENGTH {
        return false;
    }

    key.chars()
        .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
}

/// Truncate string to maximum length with ellipsis
fn truncate_string(s: &str, max_len: usize) -> String {
    if s.chars().count() <= max_len {
        s.to_string()
    } else {
        let end = s
            .char_indices()
            .nth(max_len.saturating_sub(3))
            .map(|(i, _)| i)
            .unwrap_or(s.len());
        format!("{}...", &s[..end])
    }
}

/// Format creation time for display
fn format_created_time(timestamp: &str) -> String {
    match chrono::DateTime::parse_from_rfc3339(timestamp) {
        Ok(dt) => dt.format("%Y-%m-%d %H:%M:%S").to_string(),
        Err(_) => timestamp.to_string(),
    }
}

/// Handle /memory inject command
///
/// Controls automatic memory injection:
/// - /memory inject \[on\|off\] - Toggle auto-injection
/// - /memory inject threshold `<0.0-1.0>` - Set relevance threshold (default 0.7)
/// - /memory inject max `<n>` - Set max memories to inject (default 5)
/// - /memory inject show - Show what would be injected for current input
///
pub fn handle_inject_command(
    cwd: &Path,
    args: Option<String>,
    injection_config: &mut crate::memory_injection::InjectionConfig,
) -> Result<String> {
    use crate::memory::memory_auto::ThreadSafeAutoMemory;
    use crate::memory_injection::preview_injection;

    let args_default = args.as_deref().unwrap_or_default();
    let args_str = args_default.trim();

    if args_str.is_empty() {
        // Show current status
        return Ok(format!(
            "💭 Memory Injection Status:\n  Enabled: {}\n  Threshold: {:.1}%\n  Max memories: {}",
            injection_config.enabled,
            injection_config.threshold * 100.0,
            injection_config.max_memories
        ));
    }

    let parts: Vec<&str> = args_str.split_whitespace().collect();

    match parts.first().copied() {
            Some("on") => {
                injection_config.enabled = true;
                Ok("✓ Memory injection enabled".to_string())
            }
            Some("off") => {
                injection_config.enabled = false;
                Ok("✓ Memory injection disabled".to_string())
            }
            Some("threshold") => {
                if let Some(value_str) = parts.get(1) {
                    match value_str.parse::<f64>() {
                        Ok(value) if (0.0..=1.0).contains(&value) => {
                            injection_config.threshold = value;
                            Ok(format!("✓ Injection threshold set to {:.1}%", value * 100.0))
                        }
                        _ => Ok("❌ Invalid threshold. Use value between 0.0 and 1.0".to_string()),
                    }
                } else {
                    Ok(format!("✓ Current threshold: {:.1}%", injection_config.threshold * 100.0))
                }
            }
            Some("max") => {
                if let Some(value_str) = parts.get(1) {
                    match value_str.parse::<usize>() {
                        Ok(value) if (1..=10).contains(&value) => {
                            injection_config.max_memories = value;
                            Ok(format!("✓ Max memories to inject set to {}", value))
                        }
                        _ => Ok("❌ Invalid max. Use value between 1 and 10".to_string()),
                    }
                } else {
                    Ok(format!("✓ Current max memories: {}", injection_config.max_memories))
                }
            }
            Some("show") => {
                // Preview what would be injected for a sample query
                if let Some(query) = parts.get(1) {
                    // Load auto-memories
                    let auto_memory = ThreadSafeAutoMemory::new(cwd);
                    if let Ok(memory_manager) = auto_memory {
                        let recent_memories = memory_manager.recent(7);
                        let important_memories = memory_manager.important(0.6);

                        use std::collections::HashMap;
                        let mut memory_map: HashMap<String, _> = HashMap::new();
                        for memory in recent_memories.into_iter().chain(important_memories.into_iter()) {
                            memory_map.entry(memory.id.clone()).or_insert(memory);
                        }
                        let all_memories: Vec<_> = memory_map.into_values().collect();

                        let preview = preview_injection(query, &all_memories, injection_config);

                        if preview.is_empty() {
                            Ok(format!("ℹ️ No memories would be injected for query: '{}'", query))
                        } else {
                            let mut result = format!("💭 Memories that would be injected for '{}':\n", query);
                            for (key, value, confidence) in preview {
                                result.push_str(&format!(
                                    "  • {}: {} (confidence: {:.0}%)\n",
                                    key, value, confidence * 100.0
                                ));
                            }
                            Ok(result)
                        }
                    } else {
                        Ok("❌ Failed to load auto-memories".to_string())
                    }
                } else {
                    Ok("❌ Usage: /memory inject show <query>".to_string())
                }
            }
            _ => Ok("❌ Unknown inject command. Usage:\n  /memory inject [on|off]\n  /memory inject threshold <0.0-1.0>\n  /memory inject max <1-10>\n  /memory inject show <query>".to_string()),
        }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_is_valid_key() {
        assert!(is_valid_key("valid_key-123"));
        assert!(is_valid_key("test"));
        assert!(is_valid_key("test-key"));
        assert!(is_valid_key("test_key"));
        assert!(is_valid_key("test-key_123"));

        assert!(!is_valid_key(""));
        assert!(!is_valid_key("invalid key"));
        assert!(!is_valid_key("invalid.key"));
        assert!(!is_valid_key("invalid@key"));
        assert!(!is_valid_key("a".repeat(101).as_str()));
    }

    #[test]
    fn test_truncate_string() {
        assert_eq!(truncate_string("short", 10), "short");
        assert_eq!(truncate_string("exactlyten!!", 10), "exactly...");
        assert_eq!(truncate_string("this is way too long", 10), "this is...");
        assert_eq!(truncate_string("test", 3), "...");
    }

    #[tokio::test]
    async fn test_save_and_recall() {
        let temp_dir = TempDir::new().unwrap();
        let cwd = temp_dir.path();

        // Save a memory
        let result =
            handle_save_command(cwd, "test_key".to_string(), "test_value".to_string()).await;
        assert!(result.is_ok());
        assert!(result.unwrap().contains("✓"));

        // Recall the memory
        let result = handle_recall_command(cwd, "test_key".to_string()).await;
        assert!(result.is_ok());
        assert!(result.unwrap().contains("test_value"));

        // Try to recall non-existent memory
        let result = handle_recall_command(cwd, "nonexistent".to_string()).await;
        assert!(result.is_ok());
        assert!(result.unwrap().contains("❌"));
    }

    #[tokio::test]
    async fn test_duplicate_key() {
        let temp_dir = TempDir::new().unwrap();
        let cwd = temp_dir.path();

        // Save a memory
        let result = handle_save_command(cwd, "test_key".to_string(), "value1".to_string()).await;
        assert!(result.is_ok());

        // Try to save with duplicate key
        let result = handle_save_command(cwd, "test_key".to_string(), "value2".to_string()).await;
        assert!(result.is_err());
        match result {
            Err(MemoryError::KeyExists(key)) => assert_eq!(key, "test_key"),
            _ => panic!("Expected KeyExists error"),
        }
    }

    #[tokio::test]
    async fn test_search() {
        let temp_dir = TempDir::new().unwrap();
        let cwd = temp_dir.path();

        // Save some memories - convert to anyhow::Result for tests
        let _ = into_anyhow_result(
            handle_save_command(cwd, "project_name".to_string(), "RustyCode".to_string()).await,
        )
        .unwrap();
        let _ = into_anyhow_result(
            handle_save_command(
                cwd,
                "user_preference".to_string(),
                "Dark mode theme".to_string(),
            )
            .await,
        )
        .unwrap();

        // Search for matching memories
        let result = handle_search_command(cwd, "project".to_string()).await;
        assert!(result.is_ok());
        assert!(result.unwrap().contains("project_name"));

        // Search for non-existent query
        let result = handle_search_command(cwd, "nonexistent".to_string()).await;
        assert!(result.is_ok());
        assert!(result.unwrap().contains("No memories found"));
    }

    #[tokio::test]
    async fn test_list() {
        let temp_dir = TempDir::new().unwrap();
        let cwd = temp_dir.path();

        // List empty memories
        let result = handle_list_command(cwd).await;
        assert!(result.is_ok());
        assert!(result.unwrap().contains("No memories saved"));

        // Save some memories
        let _ = into_anyhow_result(
            handle_save_command(cwd, "key1".to_string(), "value1".to_string()).await,
        )
        .unwrap();
        let _ = into_anyhow_result(
            handle_save_command(cwd, "key2".to_string(), "value2".to_string()).await,
        )
        .unwrap();

        // List memories
        let result = handle_list_command(cwd).await;
        assert!(result.is_ok());
        let result_str = result.unwrap();
        assert!(result_str.contains("2 items"));
        assert!(result_str.contains("key1"));
        assert!(result_str.contains("key2"));
    }

    #[tokio::test]
    async fn test_delete() {
        let temp_dir = TempDir::new().unwrap();
        let cwd = temp_dir.path();

        // Save a memory
        let _ = into_anyhow_result(
            handle_save_command(cwd, "test_key".to_string(), "test_value".to_string()).await,
        )
        .unwrap();

        // Delete the memory
        let result = handle_delete_command(cwd, "test_key".to_string()).await;
        assert!(result.is_ok());
        assert!(result.unwrap().contains("✓"));

        // Verify it's deleted
        let result = handle_recall_command(cwd, "test_key".to_string()).await;
        assert!(result.is_ok());
        assert!(result.unwrap().contains("❌"));
    }

    #[tokio::test]
    async fn test_clear() {
        let temp_dir = TempDir::new().unwrap();
        let cwd = temp_dir.path();

        // Save some memories
        handle_save_command(cwd, "key1".to_string(), "value1".to_string())
            .await
            .unwrap();
        handle_save_command(cwd, "key2".to_string(), "value2".to_string())
            .await
            .unwrap();

        // Clear all memories
        let result = handle_clear_command(cwd).await;
        assert!(result.is_ok());
        assert!(result.unwrap().contains("Cleared 2 memories"));

        // Verify they're cleared
        let result = handle_list_command(cwd).await;
        assert!(result.is_ok());
        assert!(result.unwrap().contains("No memories saved"));
    }

    #[test]
    fn test_get_memory_count() {
        let temp_dir = TempDir::new().unwrap();
        let cwd = temp_dir.path();

        // Initially empty
        assert_eq!(memory_count(cwd), 0);

        // Add memories
        let mut memories = HashMap::new();
        memories.insert(
            "key1".to_string(),
            KeyValueMemory {
                key: "key1".to_string(),
                value: "value1".to_string(),
                created_at: Utc::now().to_rfc3339(),
                last_accessed: Utc::now().to_rfc3339(),
                access_count: 0,
                tags: vec![],
            },
        );
        save_memories(cwd, &memories).unwrap();

        assert_eq!(memory_count(cwd), 1);
    }
}
