use anyhow::Result;

/// Handle /save slash command
///
/// Saves the current session state to disk with a timestamp
///
/// # Arguments
/// * `name` - Optional session name. If not provided, uses timestamp
/// * `messages` - Optional pre-serialized messages to save
///
/// # Returns
/// Result with success message or error
pub async fn handle_save_command(
    name: Option<String>,
    messages: &[crate::session::SerializedMessage],
) -> Result<String> {
    use crate::session::save_current_session;
    use chrono::Utc;

    let session_name =
        name.unwrap_or_else(|| format!("session-{}", Utc::now().format("%Y%m%d-%H%M%S")));

    match save_current_session(&session_name, messages) {
        Ok(_) => Ok(format!("✓ Session saved as: {}", session_name)),
        Err(e) => Err(anyhow::anyhow!("Failed to save session: {}", e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_handle_save_command_with_name() {
        let messages: Vec<crate::session::SerializedMessage> = vec![];
        let result = handle_save_command(Some("test-session".to_string()), &messages).await;
        assert!(result.is_ok());
        let message = result.unwrap();
        assert!(message.contains("test-session"));
    }

    #[tokio::test]
    async fn test_handle_save_command_without_name() {
        let messages: Vec<crate::session::SerializedMessage> = vec![];
        let result = handle_save_command(None, &messages).await;
        assert!(result.is_ok());
        let message = result.unwrap();
        assert!(message.contains("session-"));
    }
}
