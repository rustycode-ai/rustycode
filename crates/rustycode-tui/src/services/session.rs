//! Session management for the TUI
#![allow(dead_code)]

use std::path::PathBuf;

/// Entry in session history
#[derive(Clone, Debug, PartialEq)]
pub struct SessionHistoryEntry {
    pub id: String,
    pub title: String,
    pub timestamp: std::time::SystemTime,
    pub message_count: usize,
    /// First user message preview (up to 60 chars)
    pub first_message: Option<String>,
}

/// Message type for serialization
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum SerializedMessageType {
    User,
    AI,
    System,
    Tool,
}

/// Serialized message for session storage
#[derive(Clone, Debug)]
pub struct SerializedMessage {
    pub role: SerializedMessageType,
    pub content: String,
}

thread_local! {
    static TEST_SESSIONS_DIR: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
}

/// Set a thread-local override for the sessions directory path (tests only)
#[cfg(test)]
pub fn set_test_sessions_dir(path: Option<PathBuf>) {
    TEST_SESSIONS_DIR.with(|p| *p.borrow_mut() = path);
}

pub fn sessions_dir() -> PathBuf {
    #[cfg(test)]
    {
        let override_path = TEST_SESSIONS_DIR.with(|p| p.borrow().clone());
        if let Some(path) = override_path {
            return path;
        }
    }
    if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home).join(".rustycode/sessions")
    } else {
        PathBuf::from(".rustycode/sessions")
    }
}

pub fn command_history_path() -> PathBuf {
    // Store in workspace directory for workspace-specific history
    PathBuf::from(".rustycode_command_history")
}

/// Save current session to disk
///
pub fn save_current_session(
    title: &str,
    messages: &[SerializedMessage],
) -> std::io::Result<PathBuf> {
    use std::fs;

    let sessions_dir = sessions_dir();
    fs::create_dir_all(&sessions_dir)?;

    // Create session file with timestamp as ID (millisecond precision for uniqueness)
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);

    let session_file = sessions_dir.join(format!("{}.json", timestamp));

    // Extract first user message for preview
    let first_message = messages
        .iter()
        .find(|m| matches!(m.role, SerializedMessageType::User))
        .map(|m| m.content.chars().take(60).collect::<String>());

    let session_data = serde_json::json!({
        "title": title,
        "message_count": messages.len(),
        "timestamp": timestamp,
        "first_message": first_message,
        "messages": messages.iter().map(|m| serde_json::json!({
            "role": match m.role {
                SerializedMessageType::User => "user",
                SerializedMessageType::AI => "assistant",
                SerializedMessageType::System => "system",
                SerializedMessageType::Tool => "tool",
            },
            "content": m.content,
        })).collect::<Vec<_>>()
    });

    fs::write(&session_file, session_data.to_string())?;
    Ok(session_file)
}

/// Load session from disk.
///
/// Tries recovery directories first (which have real messages),
/// then falls back to flat JSON files.
///
pub fn load_session(session_id: &str) -> std::io::Result<(String, Vec<SerializedMessage>, String)> {
    use std::fs;

    let sdir = sessions_dir();

    // Try recovery directory first: {session_id}/state.json
    let recovery_path = sdir.join(session_id).join("state.json");
    if recovery_path.exists() {
        let content = fs::read_to_string(&recovery_path)?;
        let value: serde_json::Value = serde_json::from_str(&content)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        let title = format!(
            "Session {}",
            session_id.split('-').next().unwrap_or(session_id)
        );

        let mut messages = Vec::new();
        if let Some(msgs) = value.get("messages").and_then(|v| v.as_array()) {
            for msg in msgs {
                if let (Some(role), Some(content)) = (
                    msg.get("role")
                        .or_else(|| msg.get("message_role"))
                        .and_then(|r| r.as_str()),
                    msg.get("content").and_then(|c| c.as_str()),
                ) {
                    let msg_type = match role {
                        "user" => SerializedMessageType::User,
                        "assistant" => SerializedMessageType::AI,
                        "system" => SerializedMessageType::System,
                        "tool" => SerializedMessageType::Tool,
                        _ => SerializedMessageType::System,
                    };
                    messages.push(SerializedMessage {
                        role: msg_type,
                        content: content.to_string(),
                    });
                }
            }
        }

        let age_description = value
            .get("last_saved")
            .and_then(|v| v.as_str())
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| {
                let millis = dt.timestamp_millis() as u64;
                let session_time = std::time::UNIX_EPOCH + std::time::Duration::from_millis(millis);
                if let Ok(elapsed) = std::time::SystemTime::now().duration_since(session_time) {
                    let secs = elapsed.as_secs();
                    if secs < 60 {
                        "just now".to_string()
                    } else if secs < 3600 {
                        format!("{} min ago", secs / 60)
                    } else if secs < 86400 {
                        format!("{}h {}m ago", secs / 3600, (secs % 3600) / 60)
                    } else {
                        format!("{}d ago", secs / 86400)
                    }
                } else {
                    "unknown".to_string()
                }
            })
            .unwrap_or_else(|| "unknown".to_string());

        return Ok((title, messages, age_description));
    }

    // Fallback: flat JSON file
    let session_path = sdir.join(format!("{}.json", session_id));
    let content = if session_path.exists() {
        fs::read_to_string(&session_path)?
    } else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!(
                "Session not found: {} (tried recovery dir and flat file)",
                session_id
            ),
        ));
    };

    let value = serde_json::from_str::<serde_json::Value>(&content)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

    let title = value
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("Untitled")
        .to_string();

    let mut messages = Vec::new();
    if let Some(msgs) = value.get("messages").and_then(|v| v.as_array()) {
        for msg in msgs {
            if let (Some(role), Some(content)) = (
                msg.get("role").and_then(|r| r.as_str()),
                msg.get("content").and_then(|c| c.as_str()),
            ) {
                let msg_type = match role {
                    "user" => SerializedMessageType::User,
                    "assistant" => SerializedMessageType::AI,
                    "system" => SerializedMessageType::System,
                    "tool" => SerializedMessageType::Tool,
                    _ => SerializedMessageType::System,
                };
                messages.push(SerializedMessage {
                    role: msg_type,
                    content: content.to_string(),
                });
            }
        }
    }

    let age_description = value
        .get("timestamp")
        .and_then(|v| v.as_u64())
        .map(|millis| {
            let session_time = std::time::UNIX_EPOCH + std::time::Duration::from_millis(millis);
            if let Ok(elapsed) = std::time::SystemTime::now().duration_since(session_time) {
                let secs = elapsed.as_secs();
                if secs < 60 {
                    "just now".to_string()
                } else if secs < 3600 {
                    format!("{} min ago", secs / 60)
                } else if secs < 86400 {
                    format!("{}h {}m ago", secs / 3600, (secs % 3600) / 60)
                } else {
                    format!("{}d ago", secs / 86400)
                }
            } else {
                "unknown".to_string()
            }
        })
        .unwrap_or_else(|| "unknown".to_string());

    Ok((title, messages, age_description))
}

/// Load list of available sessions from disk.
///
/// Reads from recovery directories (which contain full session state with messages)
/// and falls back to flat JSON files. Results are sorted newest first.
///
pub fn load_session_history_list(
    current_title: &str,
    current_message_count: usize,
) -> Vec<SessionHistoryEntry> {
    use std::fs;

    let mut entries = Vec::new();

    // Ensure sessions directory exists
    let sessions_dir = sessions_dir();
    if let Err(e) = fs::create_dir_all(&sessions_dir) {
        tracing::warn!("Failed to create sessions directory: {}", e);
        return entries;
    }

    // Add current session to the list
    let current_entry = SessionHistoryEntry {
        id: "current".to_string(),
        title: if current_title == "Current Session" {
            "Current Session".to_string()
        } else {
            current_title.to_string()
        },
        timestamp: std::time::SystemTime::now(),
        message_count: current_message_count,
        first_message: None,
    };
    entries.push(current_entry);

    // Track which session IDs we've already loaded (from recovery dirs)
    let mut loaded_ids = std::collections::HashSet::new();
    loaded_ids.insert("current".to_string());

    // Primary: read recovery directories (state.json with real messages)
    if let Ok(read_dir) = fs::read_dir(&sessions_dir) {
        for entry in read_dir.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let session_id = match path.file_name().and_then(|n| n.to_str()) {
                Some(id) => id.to_string(),
                None => continue,
            };
            let state_path = path.join("state.json");
            if !state_path.exists() {
                continue;
            }
            let content = match fs::read_to_string(&state_path) {
                Ok(c) => c,
                Err(_) => continue,
            };
            let value: serde_json::Value = match serde_json::from_str(&content) {
                Ok(v) => v,
                Err(_) => continue,
            };

            let message_count = value
                .get("messages")
                .and_then(|v| v.as_array())
                .map(|a| a.len())
                .unwrap_or(0);

            // Skip sessions with no messages
            if message_count == 0 {
                continue;
            }

            let first_message = value
                .get("messages")
                .and_then(|v| v.as_array())
                .and_then(|msgs| msgs.first())
                .and_then(|m| m.get("content"))
                .and_then(|c| c.as_str())
                .map(|s| s.chars().take(60).collect::<String>());

            let timestamp = value
                .get("last_saved")
                .and_then(|v| v.as_str())
                .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                .map(|dt| {
                    std::time::UNIX_EPOCH
                        + std::time::Duration::from_millis(dt.timestamp_millis() as u64)
                })
                .unwrap_or(std::time::SystemTime::now());

            loaded_ids.insert(session_id.clone());
            entries.push(SessionHistoryEntry {
                id: session_id,
                title: format!("Session ({} messages)", message_count),
                timestamp,
                message_count,
                first_message,
            });
        }
    }

    // Sort by timestamp (newest first)
    entries.sort_by_key(|a| std::cmp::Reverse(a.timestamp));
    entries
}

/// Delete a session by ID.
///
/// Removes both the recovery directory and the flat JSON file if they exist.
pub fn delete_session(session_id: &str) -> std::io::Result<()> {
    use std::fs;

    let sdir = sessions_dir();

    // Remove recovery directory
    let recovery_dir = sdir.join(session_id);
    if recovery_dir.is_dir() {
        fs::remove_dir_all(&recovery_dir)?;
    }

    // Remove flat JSON file
    let flat_file = sdir.join(format!("{}.json", session_id));
    if flat_file.is_file() {
        fs::remove_file(&flat_file)?;
    }

    Ok(())
}

/// Clean up old sessions, keeping only the most recent `keep` sessions.
///
/// Removes both recovery directories and flat JSON files for sessions
/// beyond the keep limit. Also removes sessions with 0 messages.
///
pub fn cleanup_old_sessions(keep: usize) -> std::io::Result<usize> {
    use std::fs;

    let sdir = sessions_dir();
    if !sdir.exists() {
        return Ok(0);
    }

    // Get current list (already sorted newest first, excludes empty)
    let entries = load_session_history_list("Current Session", 0);

    // Skip "current" entry and sessions we want to keep
    let to_delete: Vec<&str> = entries
        .iter()
        .filter(|e| e.id != "current")
        .skip(keep)
        .map(|e| e.id.as_str())
        .collect();

    let mut removed = 0;
    for id in &to_delete {
        if delete_session(id).is_ok() {
            removed += 1;
        }
    }

    // Also clean up flat JSON files with 0 messages (stubs)
    if let Ok(read_dir) = fs::read_dir(&sdir) {
        for entry in read_dir.flatten() {
            let path = entry.path();
            if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("json") {
                if let Ok(content) = fs::read_to_string(&path) {
                    if let Ok(value) = serde_json::from_str::<serde_json::Value>(&content) {
                        let msg_count = value
                            .get("message_count")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0);
                        if msg_count == 0 {
                            let _ = fs::remove_file(&path);
                        }
                    }
                }
            }
        }
    }

    Ok(removed)
}

/// Load command history from disk
///
pub fn load_command_history() -> Vec<String> {
    let path = command_history_path();
    if path.exists() {
        if let Ok(content) = std::fs::read_to_string(&path) {
            return content
                .lines()
                .filter(|line| !line.trim().is_empty())
                .map(|s| s.to_string())
                .collect();
        }
    }
    Vec::new()
}

/// Save command history to disk
///
pub fn save_command_history(history: &[String]) -> std::io::Result<()> {
    let path = command_history_path();
    std::fs::write(&path, history.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_command_history_path() {
        let path = command_history_path();
        assert!(path.ends_with(".rustycode_command_history"));
    }

    #[test]
    fn test_save_and_load_command_history() {
        // Create a temp directory to change into
        let temp_dir = TempDir::new().unwrap();
        let original_path = std::env::current_dir().unwrap();

        // Change to temp directory
        std::env::set_current_dir(temp_dir.path()).unwrap();

        let commands = vec![
            "first command".to_string(),
            "second command".to_string(),
            "third command".to_string(),
        ];

        // Save
        save_command_history(&commands).unwrap();

        // Load
        let loaded = load_command_history();
        assert_eq!(loaded, commands);

        // Restore original directory
        std::env::set_current_dir(original_path).unwrap();
    }

    #[test]
    fn test_save_and_load_session() {
        let temp_dir = TempDir::new().unwrap();
        let sessions_path = temp_dir.path().join("sessions");

        // Use thread-local override instead of env var (avoids race with parallel tests)
        set_test_sessions_dir(Some(sessions_path));

        let title = "Test Session";
        let messages = vec![
            SerializedMessage {
                role: SerializedMessageType::User,
                content: "Hello, world!".to_string(),
            },
            SerializedMessage {
                role: SerializedMessageType::AI,
                content: "Hi there!".to_string(),
            },
        ];

        // Save session
        let session_path = save_current_session(title, &messages).unwrap();
        assert!(session_path.exists());

        // Extract session ID from filename
        let session_id = session_path.file_stem().and_then(|s| s.to_str()).unwrap();

        // Load session
        let (loaded_title, loaded_messages, age) = load_session(session_id).unwrap();
        assert_eq!(loaded_title, title);
        assert_eq!(loaded_messages.len(), messages.len());
        assert_eq!(loaded_messages[0].content, "Hello, world!");
        assert_eq!(loaded_messages[1].content, "Hi there!");
        // Age should indicate "just now" since we just saved it
        assert!(!age.is_empty(), "Age description should not be empty");

        // Restore override
        set_test_sessions_dir(None);
    }

    #[test]
    fn test_load_session_history_list() {
        let temp_dir = TempDir::new().unwrap();
        let sessions_path = temp_dir.path().join("sessions");

        // Use thread-local override instead of env var (avoids race with parallel tests)
        set_test_sessions_dir(Some(sessions_path.clone()));

        // Create recovery directory structures (what the rewritten code reads)
        let now = chrono::Utc::now();
        for (i, (session_id, content)) in [("sess-older", "Message 1"), ("sess-newer", "Message 2")]
            .iter()
            .enumerate()
        {
            let dir = sessions_path.join(session_id);
            std::fs::create_dir_all(&dir).unwrap();
            let state = serde_json::json!({
                "messages": [{ "role": "user", "content": content }],
                "last_saved": (now - chrono::Duration::seconds(10 * (2 - i) as i64))
                    .to_rfc3339(),
            });
            std::fs::write(dir.join("state.json"), state.to_string()).unwrap();
        }

        let list = load_session_history_list("Current Session", 5);

        // current + 2 recovery sessions
        assert_eq!(
            list.len(),
            3,
            "Expected 3 sessions (current + 2 recovery), got {}",
            list.len()
        );

        // Current session should be present
        let current = list
            .iter()
            .find(|e| e.id == "current")
            .expect("current session missing");
        assert_eq!(current.title, "Current Session");
        assert_eq!(current.message_count, 5);

        // Saved sessions should be sorted newest first
        let saved: Vec<_> = list.iter().filter(|e| e.id != "current").collect();
        assert_eq!(saved.len(), 2, "Expected 2 saved sessions");
        assert_eq!(saved[0].id, "sess-newer");
        assert_eq!(saved[1].id, "sess-older");

        // Restore override
        set_test_sessions_dir(None);
    }
}
