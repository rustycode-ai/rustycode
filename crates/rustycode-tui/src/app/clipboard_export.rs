//! Clipboard operations and conversation export
//!
//! Handles copying messages to clipboard and exporting conversations to files.

use crate::ui::message::{Message, MessageRole};
use anyhow::Result;

/// Copy a single message to clipboard
pub fn copy_message_to_clipboard(message: &Message) -> Result<(usize, String)> {
    use crate::clipboard::copy_text_to_clipboard_both;

    let content = message.content.clone();
    let chars = content.chars().count();

    copy_text_to_clipboard_both(&content)
        .map_err(|e| anyhow::anyhow!("Failed to copy to clipboard: {}", e))?;

    Ok((chars, content))
}

/// Copy the last assistant (AI) response to clipboard
pub fn copy_last_ai_response(messages: &[Message]) -> Result<String> {
    use crate::clipboard::copy_text_to_clipboard_both;

    // Find the last assistant message
    let last_ai_idx = messages
        .iter()
        .rposition(|msg| matches!(msg.role, MessageRole::Assistant))
        .ok_or_else(|| anyhow::anyhow!("No AI response to copy yet"))?;

    let content = messages[last_ai_idx].content.clone();
    let chars = content.chars().count();

    copy_text_to_clipboard_both(&content)
        .map_err(|e| anyhow::anyhow!("Failed to copy to clipboard: {}", e))?;

    Ok(format!("Copied last AI response ({} chars)", chars))
}

/// Copy entire conversation to clipboard (excludes system messages)
pub fn copy_conversation_to_clipboard(messages: &[Message]) -> Result<String> {
    use crate::clipboard::copy_text_to_clipboard_both;

    // Build conversation text with just user/assistant messages
    let mut conversation = Vec::new();

    for msg in messages {
        match msg.role {
            MessageRole::User => {
                conversation.push(format!("User: {}", msg.content));
            }
            MessageRole::Assistant => {
                conversation.push(format!("Assistant: {}", msg.content));
            }
            MessageRole::System => {
                // Skip system messages (help, status, etc.)
                continue;
            }
        }
    }

    let content = conversation.join("\n\n");
    let msg_count = messages
        .iter()
        .filter(|m| matches!(m.role, MessageRole::User | MessageRole::Assistant))
        .count();
    let chars = content.chars().count();

    copy_text_to_clipboard_both(&content)
        .map_err(|e| anyhow::anyhow!("Failed to copy conversation: {}", e))?;

    Ok(format!("Copied {} messages ({} chars)", msg_count, chars))
}

/// Export conversation to file
pub fn export_conversation_to_file(
    messages: &[Message],
    export_dir: &std::path::Path,
) -> Result<std::path::PathBuf> {
    use crate::ui::message_export::{ConversationExporter, ExportFormat, ExportOptions};

    // Create exporter
    let exporter = ConversationExporter::new(export_dir.to_path_buf())?;

    // Use default export options (include tools, exclude thinking/metadata/timestamps)
    let options = ExportOptions::default();

    // Export as markdown
    let path = exporter.export(messages, ExportFormat::Markdown, options)?;

    Ok(path)
}

/// Get the default export directory
pub fn get_default_export_dir() -> std::path::PathBuf {
    use dirs::home_dir;

    if let Some(home) = home_dir() {
        home.join(".rustycode").join("exports")
    } else {
        std::path::PathBuf::from("./exports")
    }
}

/// Count user/assistant messages (excludes system messages)
pub fn count_conversation_messages(messages: &[Message]) -> usize {
    messages
        .iter()
        .filter(|m| matches!(m.role, MessageRole::User | MessageRole::Assistant))
        .count()
}
