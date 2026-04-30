//! Message export functionality
//!
//! Provides export of conversations to various formats:
//! - Markdown: Clean, readable format with syntax highlighting
//! - JSON: Structured format with all metadata
//! - Plain Text: Simple text export

use crate::ui::message_types::{Message, MessageRole, ToolExecution, ToolStatus};
use anyhow::Result;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;

// ============================================================================
// EXPORT OPTIONS
// ============================================================================

/// Options for conversation export
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExportOptions {
    /// Include tool execution outputs
    pub include_tools: bool,
    /// Include thinking spans
    pub include_thinking: bool,
    /// Include message timestamps
    pub include_timestamps: bool,
    /// Include message metadata (model, tokens, cost)
    pub include_metadata: bool,
}

impl Default for ExportOptions {
    fn default() -> Self {
        Self {
            include_tools: true,
            include_thinking: false,
            include_timestamps: false,
            include_metadata: false,
        }
    }
}

// ============================================================================
// EXPORT FORMAT
// ============================================================================

/// Export format
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ExportFormat {
    /// Markdown format
    Markdown,
    /// JSON format
    Json,
    /// Plain text format
    PlainText,
}

impl ExportFormat {
    /// Get file extension for this format
    pub fn extension(&self) -> &str {
        match self {
            ExportFormat::Markdown => "md",
            ExportFormat::Json => "json",
            ExportFormat::PlainText => "txt",
        }
    }
}

// ============================================================================
// EXPORTER
// ============================================================================

/// Conversation exporter
pub struct ConversationExporter {
    export_dir: PathBuf,
}

impl ConversationExporter {
    /// Create a new exporter with the given export directory
    pub fn new(export_dir: PathBuf) -> Result<Self> {
        // Create directory if it doesn't exist
        fs::create_dir_all(&export_dir)?;
        Ok(Self { export_dir })
    }

    /// Export messages to file
    pub fn export(
        &self,
        messages: &[Message],
        format: ExportFormat,
        options: ExportOptions,
    ) -> Result<PathBuf> {
        // Generate filename with timestamp
        let timestamp = Utc::now().format("%Y%m%d_%H%M%S").to_string();
        let filename = format!("conversation_{}.{}", timestamp, format.extension());
        let filepath = self.export_dir.join(&filename);

        // Format content based on format type
        let content = match format {
            ExportFormat::Markdown => self.format_markdown(messages, &options)?,
            ExportFormat::Json => self.format_json(messages, &options)?,
            ExportFormat::PlainText => self.format_plaintext(messages, &options)?,
        };

        // Write to temporary file first
        let temp_path = self.export_dir.join(format!("{}.tmp", filename));
        let mut file = File::create(&temp_path)?;
        file.write_all(content.as_bytes())?;
        file.sync_all()?;
        drop(file);

        // Atomic rename
        fs::rename(&temp_path, &filepath)?;

        Ok(filepath)
    }

    /// Format messages as Markdown
    fn format_markdown(&self, messages: &[Message], options: &ExportOptions) -> Result<String> {
        let mut output = String::new();

        // Header
        output.push_str("# Conversation Export\n\n");
        output.push_str(&format!(
            "*Exported on {}*\n\n",
            Utc::now().format("%Y-%m-%d %H:%M:%S UTC")
        ));

        // Messages
        for message in messages {
            self.format_markdown_message(&mut output, message, options)?;
        }

        Ok(output)
    }

    /// Format a single message as Markdown
    fn format_markdown_message(
        &self,
        output: &mut String,
        message: &Message,
        options: &ExportOptions,
    ) -> Result<()> {
        // Role header
        let role_label = match message.role {
            MessageRole::User => "User",
            MessageRole::Assistant => "Assistant",
            MessageRole::System => "System",
        };

        output.push_str(&format!("## {}\n\n", role_label));

        // Timestamp (if requested)
        if options.include_timestamps {
            output.push_str(&format!("*{}*\n\n", message.timestamp.format("%H:%M:%S")));
        }

        // Content
        output.push_str(&message.content);
        output.push_str("\n\n");

        // Thinking (if present and requested)
        if options.include_thinking {
            if let Some(thinking) = &message.thinking {
                output.push_str("### Thinking\n\n");
                output.push_str("> ");
                output.push_str(&thinking.replace("\n", "\n> "));
                output.push_str("\n\n");
            }
        }

        // Metadata (if requested)
        if options.include_metadata {
            if let Some(model) = &message.metadata.model {
                output.push_str(&format!("**Model:** {}\n\n", model));
            }
            if let Some(tokens) = message.metadata.input_tokens {
                output.push_str(&format!("**Input tokens:** {}\n\n", tokens));
            }
            if let Some(tokens) = message.metadata.output_tokens {
                output.push_str(&format!("**Output tokens:** {}\n\n", tokens));
            }
        }

        // Tool executions (if present and requested)
        if options.include_tools {
            if let Some(tools) = &message.tool_executions {
                if !tools.is_empty() {
                    output.push_str("### Tools\n\n");
                    for tool in tools {
                        self.format_markdown_tool(output, tool)?;
                    }
                    output.push('\n');
                }
            }
        }

        Ok(())
    }

    /// Format a single tool execution as Markdown
    fn format_markdown_tool(&self, output: &mut String, tool: &ToolExecution) -> Result<()> {
        let status_icon = tool.status.icon();
        let duration_str = tool.duration_string();

        output.push_str(&format!(
            "#### {} `{}` ({})\n\n",
            status_icon, tool.name, duration_str
        ));

        output.push_str(&format!("**Result:** {}\n\n", tool.result_summary));

        if let Some(detailed_output) = &tool.detailed_output {
            output.push_str("**Output:**\n\n");
            output.push_str("```\n");
            output.push_str(detailed_output);
            output.push_str("\n```\n\n");
        }

        Ok(())
    }

    /// Format messages as JSON
    fn format_json(&self, messages: &[Message], options: &ExportOptions) -> Result<String> {
        let mut filtered_messages = Vec::new();

        for message in messages {
            let mut filtered = message.clone();

            // Filter tools if not requested
            if !options.include_tools {
                filtered.tool_executions = None;
            }

            // Filter thinking if not requested
            if !options.include_thinking {
                filtered.thinking = None;
            }

            // Filter metadata if not requested
            if !options.include_metadata {
                filtered.metadata.model = None;
                filtered.metadata.input_tokens = None;
                filtered.metadata.output_tokens = None;
                filtered.metadata.cost_usd = None;
            }

            filtered_messages.push(filtered);
        }

        let export_data = serde_json::json!({
            "format": "conversation_export",
            "version": "1.0",
            "exported_at": Utc::now().to_rfc3339(),
            "message_count": filtered_messages.len(),
            "options": {
                "include_tools": options.include_tools,
                "include_thinking": options.include_thinking,
                "include_timestamps": options.include_timestamps,
                "include_metadata": options.include_metadata,
            },
            "messages": filtered_messages,
        });

        Ok(serde_json::to_string_pretty(&export_data)?)
    }

    /// Format messages as plain text
    fn format_plaintext(&self, messages: &[Message], options: &ExportOptions) -> Result<String> {
        let mut output = String::new();

        // Header
        output.push_str("CONVERSATION EXPORT\n");
        output.push_str(&format!(
            "Exported: {}\n",
            Utc::now().format("%Y-%m-%d %H:%M:%S UTC")
        ));
        output.push_str("=".repeat(80).as_str());
        output.push_str("\n\n");

        // Messages
        for message in messages {
            self.format_plaintext_message(&mut output, message, options)?;
        }

        Ok(output)
    }

    /// Format a single message as plain text
    fn format_plaintext_message(
        &self,
        output: &mut String,
        message: &Message,
        options: &ExportOptions,
    ) -> Result<()> {
        // Role label
        let role_label = match message.role {
            MessageRole::User => "[YOU]",
            MessageRole::Assistant => "[AI]",
            MessageRole::System => "[SYSTEM]",
        };

        output.push_str(role_label);

        // Timestamp (if requested)
        if options.include_timestamps {
            output.push_str(&format!(" ({})", message.timestamp.format("%H:%M:%S")));
        }

        output.push('\n');

        // Content
        output.push_str(&message.content);
        output.push('\n');

        // Thinking (if present and requested)
        if options.include_thinking {
            if let Some(thinking) = &message.thinking {
                output.push_str("\n[THINKING]\n");
                output.push_str(thinking);
                output.push('\n');
            }
        }

        // Metadata (if requested)
        if options.include_metadata {
            if let Some(model) = &message.metadata.model {
                output.push_str(&format!("\nModel: {}\n", model));
            }
            if let Some(tokens) = message.metadata.input_tokens {
                output.push_str(&format!("Input tokens: {}\n", tokens));
            }
            if let Some(tokens) = message.metadata.output_tokens {
                output.push_str(&format!("Output tokens: {}\n", tokens));
            }
        }

        // Tool executions (if present and requested)
        if options.include_tools {
            if let Some(tools) = &message.tool_executions {
                if !tools.is_empty() {
                    output.push_str("\n[TOOLS]\n");
                    for tool in tools {
                        self.format_plaintext_tool(output, tool)?;
                    }
                }
            }
        }

        output.push('\n');
        output.push_str("-".repeat(80).as_str());
        output.push_str("\n\n");

        Ok(())
    }

    /// Format a single tool execution as plain text
    fn format_plaintext_tool(&self, output: &mut String, tool: &ToolExecution) -> Result<()> {
        let status_str = match tool.status {
            ToolStatus::Running => "RUNNING",
            ToolStatus::Complete => "COMPLETE",
            ToolStatus::Failed => "FAILED",
            ToolStatus::Cancelled => "CANCELLED",
        };

        output.push_str(&format!(
            "  {} - {} ({}ms)\n",
            status_str,
            tool.name,
            tool.duration_ms.unwrap_or(0)
        ));
        output.push_str(&format!("    Result: {}\n", tool.result_summary));

        if let Some(detailed_output) = &tool.detailed_output {
            output.push_str("    Output:\n");
            for line in detailed_output.lines() {
                output.push_str(&format!("      {}\n", line));
            }
        }

        Ok(())
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    #[allow(unused_imports)]
    use tempfile::TempDir;

    fn create_test_message_user(content: &str) -> Message {
        Message::user(content.to_string())
    }

    fn create_test_message_assistant(content: &str) -> Message {
        Message::assistant(content.to_string())
    }

    fn create_test_message_with_thinking(content: &str, thinking: &str) -> Message {
        let mut msg = Message::assistant(content.to_string());
        msg.thinking = Some(thinking.to_string());
        msg
    }

    fn create_test_message_with_tool(content: &str) -> Message {
        let mut msg = Message::assistant(content.to_string());
        let mut tool = ToolExecution::new(
            "tool_1".to_string(),
            "read_file".to_string(),
            "read_file: src/main.rs (145 bytes)".to_string(),
        );
        tool.complete(Some("fn main() { println!(\"Hello!\"); }".to_string()));
        msg.tool_executions = Some(vec![tool]);
        msg
    }

    fn create_test_message_with_metadata(content: &str) -> Message {
        let mut msg = Message::assistant(content.to_string());
        msg.metadata.model = Some("claude-3-sonnet".to_string());
        msg.metadata.input_tokens = Some(100);
        msg.metadata.output_tokens = Some(50);
        msg.metadata.cost_usd = Some(0.001);
        msg
    }

    // ─── Export Format Tests ───────────────────────────────────────────

    #[test]
    fn test_export_format_extension() {
        assert_eq!(ExportFormat::Markdown.extension(), "md");
        assert_eq!(ExportFormat::Json.extension(), "json");
        assert_eq!(ExportFormat::PlainText.extension(), "txt");
    }

    // ─── Export Options Tests ──────────────────────────────────────────

    #[test]
    fn test_export_options_default() {
        let options = ExportOptions::default();
        assert!(options.include_tools);
        assert!(!options.include_thinking);
        assert!(!options.include_timestamps);
        assert!(!options.include_metadata);
    }

    #[test]
    fn test_export_options_custom() {
        let options = ExportOptions {
            include_tools: false,
            include_thinking: true,
            include_timestamps: true,
            include_metadata: true,
        };
        assert!(!options.include_tools);
        assert!(options.include_thinking);
        assert!(options.include_timestamps);
        assert!(options.include_metadata);
    }

    // ─── Exporter Creation Tests ───────────────────────────────────────

    #[test]
    fn test_exporter_creation() {
        let temp_dir = TempDir::new().unwrap();
        let exporter = ConversationExporter::new(temp_dir.path().to_path_buf());
        assert!(exporter.is_ok());
    }

    #[test]
    fn test_exporter_creates_directory() {
        let temp_dir = TempDir::new().unwrap();
        let export_dir = temp_dir.path().join("exports");
        assert!(!export_dir.exists());

        let _exporter = ConversationExporter::new(export_dir.clone()).unwrap();
        assert!(export_dir.exists());
    }

    // ─── Markdown Export Tests ────────────────────────────────────────────

    #[test]
    fn test_markdown_export_simple() {
        let temp_dir = TempDir::new().unwrap();
        let exporter = ConversationExporter::new(temp_dir.path().to_path_buf()).unwrap();

        let messages = vec![
            create_test_message_user("Hello!"),
            create_test_message_assistant("Hi there!"),
        ];

        let result = exporter.export(&messages, ExportFormat::Markdown, ExportOptions::default());
        assert!(result.is_ok());

        let path = result.unwrap();
        assert!(path.exists());
        assert!(path.to_string_lossy().ends_with(".md"));

        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("# Conversation Export"));
        assert!(content.contains("## User"));
        assert!(content.contains("## Assistant"));
        assert!(content.contains("Hello!"));
        assert!(content.contains("Hi there!"));
    }

    #[test]
    fn test_markdown_export_with_thinking() {
        let temp_dir = TempDir::new().unwrap();
        let exporter = ConversationExporter::new(temp_dir.path().to_path_buf()).unwrap();

        let messages = vec![create_test_message_with_thinking(
            "The answer is 42",
            "Let me think about this carefully",
        )];

        let options = ExportOptions {
            include_thinking: true,
            ..Default::default()
        };

        let result = exporter.export(&messages, ExportFormat::Markdown, options);
        assert!(result.is_ok());

        let path = result.unwrap();
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("### Thinking"));
        assert!(content.contains("Let me think about this carefully"));
    }

    #[test]
    fn test_markdown_export_without_thinking() {
        let temp_dir = TempDir::new().unwrap();
        let exporter = ConversationExporter::new(temp_dir.path().to_path_buf()).unwrap();

        let messages = vec![create_test_message_with_thinking(
            "The answer is 42",
            "Let me think about this carefully",
        )];

        let options = ExportOptions::default(); // include_thinking = false

        let result = exporter.export(&messages, ExportFormat::Markdown, options);
        assert!(result.is_ok());

        let path = result.unwrap();
        let content = fs::read_to_string(&path).unwrap();
        assert!(!content.contains("### Thinking"));
        assert!(!content.contains("Let me think about this carefully"));
    }

    #[test]
    fn test_markdown_export_with_tools() {
        let temp_dir = TempDir::new().unwrap();
        let exporter = ConversationExporter::new(temp_dir.path().to_path_buf()).unwrap();

        let messages = vec![create_test_message_with_tool("I read the file")];

        let options = ExportOptions {
            include_tools: true,
            ..Default::default()
        };

        let result = exporter.export(&messages, ExportFormat::Markdown, options);
        assert!(result.is_ok());

        let path = result.unwrap();
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("### Tools"));
        assert!(content.contains("read_file"));
        assert!(content.contains("fn main()"));
    }

    #[test]
    fn test_markdown_export_without_tools() {
        let temp_dir = TempDir::new().unwrap();
        let exporter = ConversationExporter::new(temp_dir.path().to_path_buf()).unwrap();

        let messages = vec![create_test_message_with_tool("I read the file")];

        let options = ExportOptions {
            include_tools: false,
            ..Default::default()
        };

        let result = exporter.export(&messages, ExportFormat::Markdown, options);
        assert!(result.is_ok());

        let path = result.unwrap();
        let content = fs::read_to_string(&path).unwrap();
        assert!(!content.contains("### Tools"));
        assert!(!content.contains("read_file"));
    }

    #[test]
    fn test_markdown_export_with_metadata() {
        let temp_dir = TempDir::new().unwrap();
        let exporter = ConversationExporter::new(temp_dir.path().to_path_buf()).unwrap();

        let messages = vec![create_test_message_with_metadata("Here is my response")];

        let options = ExportOptions {
            include_metadata: true,
            ..Default::default()
        };

        let result = exporter.export(&messages, ExportFormat::Markdown, options);
        assert!(result.is_ok());

        let path = result.unwrap();
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("**Model:**"));
        assert!(content.contains("claude-3-sonnet"));
        assert!(content.contains("**Input tokens:**"));
        assert!(content.contains("**Output tokens:**"));
    }

    #[test]
    fn test_markdown_export_with_timestamps() {
        let temp_dir = TempDir::new().unwrap();
        let exporter = ConversationExporter::new(temp_dir.path().to_path_buf()).unwrap();

        let messages = vec![create_test_message_user("Hello")];

        let options = ExportOptions {
            include_timestamps: true,
            ..Default::default()
        };

        let result = exporter.export(&messages, ExportFormat::Markdown, options);
        assert!(result.is_ok());

        let path = result.unwrap();
        let content = fs::read_to_string(&path).unwrap();
        // Should contain a time pattern
        assert!(content.contains(":")); // At least a colon for time
    }

    // ─── JSON Export Tests ────────────────────────────────────────────────

    #[test]
    fn test_json_export_simple() {
        let temp_dir = TempDir::new().unwrap();
        let exporter = ConversationExporter::new(temp_dir.path().to_path_buf()).unwrap();

        let messages = vec![
            create_test_message_user("Hello!"),
            create_test_message_assistant("Hi there!"),
        ];

        let result = exporter.export(&messages, ExportFormat::Json, ExportOptions::default());
        assert!(result.is_ok());

        let path = result.unwrap();
        assert!(path.exists());
        assert!(path.to_string_lossy().ends_with(".json"));

        let content = fs::read_to_string(&path).unwrap();
        let json: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(json["format"], "conversation_export");
        assert_eq!(json["message_count"], 2);
        assert!(json["messages"].is_array());
    }

    #[test]
    fn test_json_export_with_tools_included() {
        let temp_dir = TempDir::new().unwrap();
        let exporter = ConversationExporter::new(temp_dir.path().to_path_buf()).unwrap();

        let messages = vec![create_test_message_with_tool("I read the file")];

        let options = ExportOptions {
            include_tools: true,
            ..Default::default()
        };

        let result = exporter.export(&messages, ExportFormat::Json, options);
        assert!(result.is_ok());

        let path = result.unwrap();
        let content = fs::read_to_string(&path).unwrap();
        let json: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert!(json["messages"][0]["tool_executions"].is_array());
        assert!(json["messages"][0]["tool_executions"][0]["name"].is_string());
    }

    #[test]
    fn test_json_export_with_tools_excluded() {
        let temp_dir = TempDir::new().unwrap();
        let exporter = ConversationExporter::new(temp_dir.path().to_path_buf()).unwrap();

        let messages = vec![create_test_message_with_tool("I read the file")];

        let options = ExportOptions {
            include_tools: false,
            ..Default::default()
        };

        let result = exporter.export(&messages, ExportFormat::Json, options);
        assert!(result.is_ok());

        let path = result.unwrap();
        let content = fs::read_to_string(&path).unwrap();
        let json: serde_json::Value = serde_json::from_str(&content).unwrap();
        // tool_executions should be None/null after filtering
        assert!(json["messages"][0]["tool_executions"].is_null());
    }

    #[test]
    fn test_json_export_metadata() {
        let temp_dir = TempDir::new().unwrap();
        let exporter = ConversationExporter::new(temp_dir.path().to_path_buf()).unwrap();

        let messages = vec![create_test_message_user("Test")];

        let options = ExportOptions::default();

        let result = exporter.export(&messages, ExportFormat::Json, options);
        assert!(result.is_ok());

        let path = result.unwrap();
        let content = fs::read_to_string(&path).unwrap();
        let json: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert!(json["options"]["include_tools"].is_boolean());
        assert!(json["options"]["include_thinking"].is_boolean());
    }

    // ─── Plain Text Export Tests ────────────────────────────────────────

    #[test]
    fn test_plaintext_export_simple() {
        let temp_dir = TempDir::new().unwrap();
        let exporter = ConversationExporter::new(temp_dir.path().to_path_buf()).unwrap();

        let messages = vec![
            create_test_message_user("Hello!"),
            create_test_message_assistant("Hi there!"),
        ];

        let result = exporter.export(&messages, ExportFormat::PlainText, ExportOptions::default());
        assert!(result.is_ok());

        let path = result.unwrap();
        assert!(path.exists());
        assert!(path.to_string_lossy().ends_with(".txt"));

        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("CONVERSATION EXPORT"));
        assert!(content.contains("[YOU]"));
        assert!(content.contains("[AI]"));
        assert!(content.contains("Hello!"));
        assert!(content.contains("Hi there!"));
    }

    #[test]
    fn test_plaintext_export_with_thinking() {
        let temp_dir = TempDir::new().unwrap();
        let exporter = ConversationExporter::new(temp_dir.path().to_path_buf()).unwrap();

        let messages = vec![create_test_message_with_thinking(
            "The answer is 42",
            "Let me think about this carefully",
        )];

        let options = ExportOptions {
            include_thinking: true,
            ..Default::default()
        };

        let result = exporter.export(&messages, ExportFormat::PlainText, options);
        assert!(result.is_ok());

        let path = result.unwrap();
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("[THINKING]"));
        assert!(content.contains("Let me think about this carefully"));
    }

    #[test]
    fn test_plaintext_export_without_thinking() {
        let temp_dir = TempDir::new().unwrap();
        let exporter = ConversationExporter::new(temp_dir.path().to_path_buf()).unwrap();

        let messages = vec![create_test_message_with_thinking(
            "The answer is 42",
            "Let me think about this carefully",
        )];

        let options = ExportOptions::default(); // include_thinking = false

        let result = exporter.export(&messages, ExportFormat::PlainText, options);
        assert!(result.is_ok());

        let path = result.unwrap();
        let content = fs::read_to_string(&path).unwrap();
        assert!(!content.contains("[THINKING]"));
        assert!(!content.contains("Let me think about this carefully"));
    }

    #[test]
    fn test_plaintext_export_with_tools() {
        let temp_dir = TempDir::new().unwrap();
        let exporter = ConversationExporter::new(temp_dir.path().to_path_buf()).unwrap();

        let messages = vec![create_test_message_with_tool("I read the file")];

        let options = ExportOptions {
            include_tools: true,
            ..Default::default()
        };

        let result = exporter.export(&messages, ExportFormat::PlainText, options);
        assert!(result.is_ok());

        let path = result.unwrap();
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("[TOOLS]"));
        assert!(content.contains("read_file"));
        assert!(content.contains("fn main()"));
    }

    #[test]
    fn test_plaintext_export_without_tools() {
        let temp_dir = TempDir::new().unwrap();
        let exporter = ConversationExporter::new(temp_dir.path().to_path_buf()).unwrap();

        let messages = vec![create_test_message_with_tool("I read the file")];

        let options = ExportOptions {
            include_tools: false,
            ..Default::default()
        };

        let result = exporter.export(&messages, ExportFormat::PlainText, options);
        assert!(result.is_ok());

        let path = result.unwrap();
        let content = fs::read_to_string(&path).unwrap();
        assert!(!content.contains("[TOOLS]"));
        assert!(!content.contains("read_file"));
    }

    // ─── Filename Tests ───────────────────────────────────────────────────

    #[test]
    fn test_export_generates_unique_filenames() {
        let temp_dir = TempDir::new().unwrap();
        let exporter = ConversationExporter::new(temp_dir.path().to_path_buf()).unwrap();

        let messages = vec![create_test_message_user("Hello")];
        let options = ExportOptions::default();

        let result1 = exporter
            .export(&messages, ExportFormat::Markdown, options.clone())
            .unwrap();

        // Sleep to ensure different timestamp
        std::thread::sleep(std::time::Duration::from_secs(1));

        let result2 = exporter
            .export(&messages, ExportFormat::Markdown, options)
            .unwrap();

        // Filenames should be different (different timestamps)
        assert_ne!(result1, result2);
        assert!(result1.exists());
        assert!(result2.exists());

        // Both should be readable
        let content1 = fs::read_to_string(&result1).unwrap();
        let content2 = fs::read_to_string(&result2).unwrap();
        assert!(!content1.is_empty());
        assert!(!content2.is_empty());
    }

    // ─── Edge Cases ───────────────────────────────────────────────────────

    #[test]
    fn test_export_empty_conversation() {
        let temp_dir = TempDir::new().unwrap();
        let exporter = ConversationExporter::new(temp_dir.path().to_path_buf()).unwrap();

        let messages = vec![];

        let result = exporter.export(&messages, ExportFormat::Markdown, ExportOptions::default());
        assert!(result.is_ok());

        let path = result.unwrap();
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("# Conversation Export"));
    }

    #[test]
    fn test_export_special_characters() {
        let temp_dir = TempDir::new().unwrap();
        let exporter = ConversationExporter::new(temp_dir.path().to_path_buf()).unwrap();

        let messages = vec![create_test_message_user(
            "Special chars: <>&\"'\n\tand\ttabs",
        )];

        let result = exporter.export(&messages, ExportFormat::Markdown, ExportOptions::default());
        assert!(result.is_ok());

        let path = result.unwrap();
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("<>&"));
    }

    #[test]
    fn test_export_unicode() {
        let temp_dir = TempDir::new().unwrap();
        let exporter = ConversationExporter::new(temp_dir.path().to_path_buf()).unwrap();

        let messages = vec![create_test_message_user("Unicode: 你好世界 🌍 Ñoño")];

        let result = exporter.export(&messages, ExportFormat::PlainText, ExportOptions::default());
        assert!(result.is_ok());

        let path = result.unwrap();
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("你好世界"));
        assert!(content.contains("🌍"));
        assert!(content.contains("Ñoño"));
    }

    #[test]
    fn test_export_very_long_content() {
        let temp_dir = TempDir::new().unwrap();
        let exporter = ConversationExporter::new(temp_dir.path().to_path_buf()).unwrap();

        let long_content = "a".repeat(10000);
        let messages = vec![create_test_message_user(&long_content)];

        let result = exporter.export(&messages, ExportFormat::Markdown, ExportOptions::default());
        assert!(result.is_ok());

        let path = result.unwrap();
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.len() > 10000);
    }

    #[test]
    fn test_export_system_message() {
        let temp_dir = TempDir::new().unwrap();
        let exporter = ConversationExporter::new(temp_dir.path().to_path_buf()).unwrap();

        let msg = Message::system("System error".to_string());
        let messages = vec![msg];

        let result = exporter.export(&messages, ExportFormat::Markdown, ExportOptions::default());
        assert!(result.is_ok());

        let path = result.unwrap();
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("## System"));
        assert!(content.contains("System error"));
    }

    #[test]
    fn test_json_export_with_thinking_excluded() {
        let temp_dir = TempDir::new().unwrap();
        let exporter = ConversationExporter::new(temp_dir.path().to_path_buf()).unwrap();

        let messages = vec![create_test_message_with_thinking(
            "The answer",
            "Internal thinking",
        )];

        let options = ExportOptions {
            include_thinking: false,
            ..Default::default()
        };

        let result = exporter.export(&messages, ExportFormat::Json, options);
        assert!(result.is_ok());

        let path = result.unwrap();
        let content = fs::read_to_string(&path).unwrap();
        let json: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert!(json["messages"][0]["thinking"].is_null());
    }

    #[test]
    fn test_markdown_export_multiple_tools() {
        let temp_dir = TempDir::new().unwrap();
        let exporter = ConversationExporter::new(temp_dir.path().to_path_buf()).unwrap();

        let mut msg = Message::assistant("Multiple tools".to_string());
        let mut tool1 = ToolExecution::new(
            "tool_1".to_string(),
            "read".to_string(),
            "read: file.txt".to_string(),
        );
        tool1.complete(Some("content".to_string()));
        let mut tool2 = ToolExecution::new(
            "tool_2".to_string(),
            "write".to_string(),
            "write: file.txt".to_string(),
        );
        tool2.complete(Some("done".to_string()));
        msg.tool_executions = Some(vec![tool1, tool2]);

        let messages = vec![msg];
        let options = ExportOptions {
            include_tools: true,
            ..Default::default()
        };

        let result = exporter.export(&messages, ExportFormat::Markdown, options);
        assert!(result.is_ok());

        let path = result.unwrap();
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("read"));
        assert!(content.contains("write"));
    }
}
