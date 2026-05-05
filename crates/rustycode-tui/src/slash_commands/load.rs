use anyhow::Result;

/// Handle /load slash command
///
/// Lists available sessions or loads a specific session
///
pub async fn handle_load_command(session_id: Option<String>) -> Result<String> {
    use crate::session::{load_session, load_session_history_list};

    if let Some(id) = session_id {
        // Load specific session
        match load_session(&id) {
            Ok((name, messages, age)) => {
                let msg_count = messages.len();
                Ok(format!(
                    "✓ Loaded session '{}' (from {}) — {} messages",
                    name, age, msg_count
                ))
            }
            Err(e) => Err(anyhow::anyhow!("Failed to load session '{}': {}", id, e)),
        }
    } else {
        // List available sessions
        let sessions = load_session_history_list("Current Session", 0);
        if sessions.is_empty() {
            Ok("No saved sessions found".to_string())
        } else {
            let mut output = String::from("Available sessions:\n");
            for (i, session) in sessions.iter().enumerate() {
                output.push_str(&format!("  {}. {}\n", i + 1, session.title));
            }
            Ok(output)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_handle_load_command_list_sessions() {
        let result = handle_load_command(None).await;
        assert!(result.is_ok());
        let message = result.unwrap();
        // Should either list sessions or say none found
        assert!(message.contains("Available sessions") || message.contains("No saved sessions"));
    }

    #[tokio::test]
    async fn test_handle_load_command_with_id() {
        let result = handle_load_command(Some("nonexistent".to_string())).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Failed to load session"));
    }
}
