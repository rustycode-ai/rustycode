//! Message type definitions
//!
//! This module contains all the core data structures for the message system,
//! including the Message struct, its role, tool execution details, and metadata.

use crate::ui::message_tags::Tag;
use chrono::{DateTime, Utc};
use ratatui::style::Color;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;

// PUBLIC EXPORTS

// MESSAGE ROLE

/// Message role in the conversation
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MessageRole {
    /// User message
    User,
    /// AI assistant message (may have tool_executions and thinking)
    Assistant,
    /// System notification (errors, info, no children)
    System,
}

impl fmt::Display for MessageRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MessageRole::User => write!(f, "you"),
            MessageRole::Assistant => write!(f, "ai"),
            MessageRole::System => write!(f, "system"),
        }
    }
}

// TOOL EXECUTION

/// Tool execution status
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToolStatus {
    /// Tool is currently running
    Running,
    /// Tool completed successfully
    Complete,
    /// Tool failed
    Failed,
    /// Tool was cancelled by user
    Cancelled,
}

impl ToolStatus {
    pub fn icon(&self) -> &str {
        match self {
            ToolStatus::Running => "◐",   // Half circle (running)
            ToolStatus::Complete => "●",  // Full circle (complete)
            ToolStatus::Failed => "✗",    // X mark (failed)
            ToolStatus::Cancelled => "⚠", // Warning icon (cancelled)
        }
    }

    pub fn color(&self) -> Color {
        match self {
            ToolStatus::Running => Color::Rgb(255, 200, 80), // Amber
            ToolStatus::Complete => Color::Rgb(80, 200, 120), // Green
            ToolStatus::Failed => Color::Rgb(255, 80, 80),   // Red
            ToolStatus::Cancelled => Color::Rgb(200, 150, 50), // Orange/Brown
        }
    }
}

/// Tool execution metadata
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ToolExecution {
    /// Unique tool execution ID (matches tool_use_id from LLM)
    pub tool_id: String,

    /// Tool name (e.g., "Read")
    pub name: String,

    /// Execution status
    pub status: ToolStatus,

    /// When tool started
    pub start_time: DateTime<Utc>,

    /// When tool completed (if finished)
    pub end_time: Option<DateTime<Utc>>,

    /// Duration in milliseconds (if finished)
    pub duration_ms: Option<u64>,

    /// One-line summary (e.g., "read_file: src/main.rs (145b)")
    pub result_summary: String,

    /// Detailed output (shown when expanded)
    pub detailed_output: Option<String>,

    /// Original tool input parameters as JSON (for conversation history reconstruction)
    #[serde(default)]
    pub input_json: Option<serde_json::Value>,

    /// Cached pretty-printed input JSON — computed once, reused for rendering.
    #[serde(skip)]
    pub input_json_pretty: Option<String>,

    /// Current progress step (for multi-step tools, e.g., "3/10 files processed")
    #[serde(default)]
    pub progress_current: Option<usize>,

    /// Total progress steps (for multi-step tools)
    #[serde(default)]
    pub progress_total: Option<usize>,

    /// Progress description (e.g., "Reading files...", "Compiling...")
    #[serde(default)]
    pub progress_description: Option<String>,
}

impl ToolExecution {
    /// Create a new tool execution (compatibility for tests)
    pub fn new_simple(name: String) -> Self {
        Self::new("simple".to_string(), name.clone(), format!("{}...", name))
    }

    pub fn new(tool_id: String, name: String, result_summary: String) -> Self {
        Self {
            tool_id,
            name,
            status: ToolStatus::Running,
            start_time: Utc::now(),
            end_time: None,
            duration_ms: None,
            result_summary,
            detailed_output: None,
            input_json: None,
            input_json_pretty: None,
            progress_current: None,
            progress_total: None,
            progress_description: None,
        }
    }

    /// Mark the tool as complete
    pub fn complete(&mut self, detailed_output: Option<String>) {
        self.status = ToolStatus::Complete;
        let end_time = Utc::now();
        self.end_time = Some(end_time);
        self.duration_ms = Some(
            end_time
                .signed_duration_since(self.start_time)
                .num_milliseconds()
                .max(0) as u64,
        );
        self.detailed_output = detailed_output;
    }

    /// Mark the tool as failed
    pub fn fail(&mut self, error: String) {
        self.status = ToolStatus::Failed;
        let end_time = Utc::now();
        self.end_time = Some(end_time);
        self.duration_ms = Some(
            end_time
                .signed_duration_since(self.start_time)
                .num_milliseconds()
                .max(0) as u64,
        );
        self.result_summary = format!("{}: Error", self.name);
        self.detailed_output = Some(error);
    }

    /// Mark the tool as cancelled
    pub fn cancel(&mut self) {
        self.status = ToolStatus::Cancelled;
        let end_time = Utc::now();
        self.end_time = Some(end_time);
        self.duration_ms = Some(
            end_time
                .signed_duration_since(self.start_time)
                .num_milliseconds()
                .max(0) as u64,
        );
        self.result_summary = format!("{}: Cancelled", self.name);
        self.detailed_output = Some("Tool execution was cancelled by user".to_string());
    }

    /// Get a human-readable duration string
    pub fn duration_string(&self) -> String {
        if let Some(ms) = self.duration_ms {
            if ms < 1000 {
                format!("{}ms", ms)
            } else if ms < 60000 {
                format!("{:.1}s", ms as f64 / 1000.0)
            } else {
                let secs = ms / 1000;
                let mins = secs / 60;
                let remaining_secs = secs % 60;
                format!("{}m{}s", mins, remaining_secs)
            }
        } else {
            "running".to_string()
        }
    }

    /// Update progress for multi-step tools
    pub fn update_progress(
        &mut self,
        current: usize,
        total: usize,
        description: impl Into<String>,
    ) {
        self.progress_current = Some(current);
        self.progress_total = Some(total);
        self.progress_description = Some(description.into());
    }

    /// Clear progress (call when tool completes)
    pub fn clear_progress(&mut self) {
        self.progress_current = None;
        self.progress_total = None;
        self.progress_description = None;
    }

    /// Get progress percentage
    pub fn progress_percent(&self) -> Option<f64> {
        match (self.progress_current, self.progress_total) {
            (Some(current), Some(total)) if total > 0 => {
                Some((current as f64 / total as f64) * 100.0)
            }
            _ => None,
        }
    }

    pub fn size_summary(&self) -> String {
        // Try to extract size from detailed_output
        if let Some(ref output) = self.detailed_output {
            let bytes = output.len();
            if bytes < 1024 {
                format!("{}b", bytes)
            } else if bytes < 1024 * 1024 {
                format!("{:.1}kb", bytes as f64 / 1024.0)
            } else {
                format!("{:.1}mb", bytes as f64 / (1024.0 * 1024.0))
            }
        } else if let Some(_ms) = self.duration_ms {
            // For fast tools, show duration
            self.duration_string().to_string()
        } else {
            "0b".to_string()
        }
    }
}

// IMAGE ATTACHMENT

/// Image attachment for messages and input state
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ImageAttachment {
    /// Unique identifier (ULID)
    pub id: String,
    /// Path to the image file (temp file)
    pub path: PathBuf,
    pub mime_type: String,
    /// Base64-encoded image data for API transmission
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data_base64: Option<String>,
    /// ASCII preview (24x6 chars)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preview: Option<String>,
    /// Image width in pixels
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    /// Image height in pixels
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
}

// MESSAGE METADATA

/// Message metadata (model, tokens, etc.)
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct MessageMetadata {
    /// Model used for this message (if applicable)
    pub model: Option<String>,

    /// Input tokens used
    pub input_tokens: Option<usize>,

    /// Output tokens generated
    pub output_tokens: Option<usize>,

    /// Cost in USD (if available)
    pub cost_usd: Option<f64>,

    /// Image attachments (for vision-capable models)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub images: Option<Vec<ImageAttachment>>,
}

// EXPANSION LEVEL

/// Expansion level for tools/thinking display
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ExpansionLevel {
    /// Collapsed - show summary only
    #[default]
    Collapsed,

    /// Expanded - show tool list
    Expanded,

    /// Deep - show detailed output for specific tool
    Deep,
}

// MESSAGE STRUCT

/// A message in the conversation
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Message {
    /// Unique message identifier
    pub id: String,

    /// Message role
    pub role: MessageRole,

    /// Message content
    pub content: String,

    /// Message timestamp
    pub timestamp: DateTime<Utc>,

    /// Tool executions associated with this message
    /// (Only for Assistant messages that used tools)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_executions: Option<Vec<ToolExecution>>,

    /// Thinking process (optional, for models that expose reasoning)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<String>,

    /// Metadata (model, tokens, etc.)
    #[serde(default)]
    pub metadata: MessageMetadata,

    /// Expansion state for tools
    #[serde(default)]
    pub tools_expansion: ExpansionLevel,

    /// Expansion state for thinking
    #[serde(default)]
    pub thinking_expansion: ExpansionLevel,

    /// Which tool is currently focused (for deep expansion)
    #[serde(default)]
    pub focused_tool_index: Option<usize>,

    #[serde(default)]
    pub collapsed: bool,

    /// Tags applied to this message
    #[serde(default)]
    pub tags: Vec<Tag>,
}

impl Message {
    pub fn new(role: MessageRole, content: String) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            role,
            content,
            timestamp: Utc::now(),
            tool_executions: None,
            thinking: None,
            metadata: MessageMetadata::default(),
            tools_expansion: ExpansionLevel::Collapsed,
            thinking_expansion: ExpansionLevel::Collapsed,
            focused_tool_index: None,
            collapsed: false,
            tags: Vec::new(),
        }
    }

    /// Create a user message
    pub fn user(content: String) -> Self {
        Self::new(MessageRole::User, content)
    }

    /// Create an assistant message
    pub fn assistant(content: String) -> Self {
        Self::new(MessageRole::Assistant, content)
    }

    /// Create a system message
    pub fn system(content: String) -> Self {
        Self::new(MessageRole::System, content)
    }

    /// Add tool executions to this message
    pub fn with_tools(mut self, tools: Vec<ToolExecution>) -> Self {
        self.tool_executions = Some(tools);
        self
    }

    /// Add thinking to this message
    pub fn with_thinking(mut self, thinking: String) -> Self {
        self.thinking = Some(thinking);
        self
    }

    /// Add metadata to this message
    pub fn with_metadata(mut self, metadata: MessageMetadata) -> Self {
        self.metadata = metadata;
        self
    }

    /// Add images to this message
    pub fn with_images(mut self, images: Vec<ImageAttachment>) -> Self {
        self.metadata.images = Some(images);
        self
    }

    pub fn has_images(&self) -> bool {
        self.metadata
            .images
            .as_ref()
            .map(|i| !i.is_empty())
            .unwrap_or(false)
    }

    pub fn image_count(&self) -> usize {
        self.metadata.images.as_ref().map(|i| i.len()).unwrap_or(0)
    }

    pub fn has_tools(&self) -> bool {
        self.tool_executions
            .as_ref()
            .map(|t| !t.is_empty())
            .unwrap_or(false)
    }

    pub fn has_thinking(&self) -> bool {
        self.thinking
            .as_ref()
            .map(|t| !t.is_empty())
            .unwrap_or(false)
    }

    pub fn tool_count(&self) -> usize {
        self.tool_executions.as_ref().map(|t| t.len()).unwrap_or(0)
    }

    pub fn completed_tool_count(&self) -> usize {
        self.tool_executions
            .as_ref()
            .map(|t| {
                t.iter()
                    .filter(|t| t.status == ToolStatus::Complete)
                    .count()
            })
            .unwrap_or(0)
    }

    pub fn failed_tool_count(&self) -> usize {
        self.tool_executions
            .as_ref()
            .map(|t| t.iter().filter(|t| t.status == ToolStatus::Failed).count())
            .unwrap_or(0)
    }

    /// Calculate total size of all tool outputs
    pub fn total_tool_output_size(&self) -> usize {
        self.tool_executions
            .as_ref()
            .map(|t| {
                t.iter()
                    .map(|tool| tool.detailed_output.as_ref().map(|o| o.len()).unwrap_or(0))
                    .sum()
            })
            .unwrap_or(0)
    }

    /// Toggle tools expansion
    pub fn toggle_tools_expansion(&mut self) {
        self.tools_expansion = match self.tools_expansion {
            ExpansionLevel::Collapsed => ExpansionLevel::Expanded,
            ExpansionLevel::Expanded => ExpansionLevel::Collapsed,
            ExpansionLevel::Deep => ExpansionLevel::Collapsed,
        };
        self.focused_tool_index = None;
    }

    /// Toggle thinking expansion
    pub fn toggle_thinking_expansion(&mut self) {
        self.thinking_expansion = match self.thinking_expansion {
            ExpansionLevel::Collapsed => ExpansionLevel::Expanded,
            _ => ExpansionLevel::Collapsed,
        };
    }

    /// Toggle message collapse (collapse entire message content)
    pub fn toggle_collapsed(&mut self) {
        self.collapsed = !self.collapsed;
    }

    /// Set deep expansion for a specific tool
    pub fn set_deep_tool_expansion(&mut self, tool_index: usize) {
        if self.has_tools() && tool_index < self.tool_count() {
            self.tools_expansion = ExpansionLevel::Deep;
            self.focused_tool_index = Some(tool_index);
        }
    }

    /// Get the pipe character and color for this message role.
    /// User/Assistant use '▌' (U+258C LEFT HALF BLACK RECTANGLE).
    /// System uses '│' (U+2502 BOX DRAWINGS LIGHT VERTICAL).
    pub fn pipe_style(&self) -> (char, Color) {
        match self.role {
            MessageRole::User => ('▌', Color::Rgb(255, 105, 180)), // Hot pink
            MessageRole::Assistant => ('▌', Color::Rgb(0, 255, 255)), // Cyan
            MessageRole::System => ('│', Color::Gray),
        }
    }

    pub fn role_color(&self) -> Color {
        match self.role {
            MessageRole::User => Color::Rgb(255, 105, 180), // Hot pink
            MessageRole::Assistant => Color::Rgb(0, 255, 255), // Cyan
            MessageRole::System => Color::Gray,
        }
    }

    /// Format timestamp for display
    pub fn formatted_time(&self) -> String {
        self.timestamp.format("%H:%M").to_string()
    }

    /// Add a tag to this message
    pub fn add_tag(&mut self, tag: Tag) -> bool {
        // Prevent duplicates
        if !self.tags.contains(&tag) {
            self.tags.push(tag);
            self.tags.sort();
            true
        } else {
            false
        }
    }

    /// Remove a tag from this message
    pub fn remove_tag(&mut self, tag: &Tag) -> bool {
        let original_len = self.tags.len();
        self.tags.retain(|t| t != tag);
        original_len > 0 && self.tags.len() < original_len
    }

    /// Remove all tags of a specific type
    pub fn remove_tag_type(&mut self, tag_type: &crate::ui::message_tags::TagType) -> bool {
        let original_len = self.tags.len();
        self.tags.retain(|t| &t.tag_type != tag_type);
        original_len > 0 && self.tags.len() < original_len
    }

    pub fn has_tag(&self, tag_type: &crate::ui::message_tags::TagType) -> bool {
        self.tags.iter().any(|t| &t.tag_type == tag_type)
    }

    /// Get all tags
    pub fn tags(&self) -> &[Tag] {
        &self.tags
    }

    /// Clear all tags
    pub fn clear_tags(&mut self) {
        self.tags.clear();
    }

    pub fn has_any_tags(&self) -> bool {
        !self.tags.is_empty()
    }
}

// TESTS

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_creation() {
        let msg = Message::user("Hello".to_string());
        assert_eq!(msg.role, MessageRole::User);
        assert_eq!(msg.content, "Hello");
        assert!(!msg.has_tools());
        assert!(!msg.has_thinking());
    }

    #[test]
    fn test_message_with_tools() {
        let tool1 = ToolExecution::new(
            "tool_1".to_string(),
            "Read".to_string(),
            "read_file: src/main.rs (145b)".to_string(),
        );
        let tool2 = ToolExecution::new(
            "tool_2".to_string(),
            "Write".to_string(),
            "write_file: src/tree.rs (122b)".to_string(),
        );

        let msg = Message::assistant("I'll help you".to_string()).with_tools(vec![tool1, tool2]);

        assert_eq!(msg.role, MessageRole::Assistant);
        assert!(msg.has_tools());
        assert_eq!(msg.tool_count(), 2);
    }

    #[test]
    fn test_tool_execution_complete() {
        let mut tool = ToolExecution::new(
            "tool_1".to_string(),
            "Bash".to_string(),
            "bash: cargo check".to_string(),
        );
        assert_eq!(tool.status, ToolStatus::Running);

        tool.complete(Some("Compiling... Done".to_string()));
        assert_eq!(tool.status, ToolStatus::Complete);
        assert!(tool.end_time.is_some());
        assert!(tool.duration_ms.is_some());
        assert!(tool.detailed_output.is_some());
    }

    #[test]
    fn test_tool_execution_fail() {
        let mut tool = ToolExecution::new(
            "tool_1".to_string(),
            "Bash".to_string(),
            "bash: cargo check".to_string(),
        );
        tool.fail("Compilation failed".to_string());

        assert_eq!(tool.status, ToolStatus::Failed);
        assert!(tool.end_time.is_some());
        assert_eq!(tool.result_summary, "Bash: Error");
    }

    #[test]
    fn test_expansion_toggle() {
        let mut msg = Message::assistant("Test".to_string());
        assert_eq!(msg.tools_expansion, ExpansionLevel::Collapsed);

        msg.toggle_tools_expansion();
        assert_eq!(msg.tools_expansion, ExpansionLevel::Expanded);

        msg.toggle_tools_expansion();
        assert_eq!(msg.tools_expansion, ExpansionLevel::Collapsed);
    }

    #[test]
    fn test_pipe_styles() {
        let user_msg = Message::user("Test".to_string());
        let (pipe, color) = user_msg.pipe_style();
        assert_eq!(pipe, '▌');
        assert_eq!(color, Color::Rgb(255, 105, 180)); // Pink

        let ai_msg = Message::assistant("Test".to_string());
        let (pipe, color) = ai_msg.pipe_style();
        assert_eq!(pipe, '▌');
        assert_eq!(color, Color::Rgb(0, 255, 255)); // Cyan
    }

    #[test]
    fn test_tool_status_icons() {
        assert_eq!(ToolStatus::Running.icon(), "◐");
        assert_eq!(ToolStatus::Complete.icon(), "●");
        assert_eq!(ToolStatus::Failed.icon(), "✗");
    }

    #[test]
    fn test_tool_count() {
        let tool1 = ToolExecution::new(
            "tool_1".to_string(),
            "read".to_string(),
            "read: file.txt".to_string(),
        );
        let mut tool2 = ToolExecution::new(
            "tool_2".to_string(),
            "write".to_string(),
            "write: file.txt".to_string(),
        );
        tool2.complete(Some("Done".to_string()));

        let msg = Message::assistant("Test".to_string()).with_tools(vec![tool1, tool2]);

        assert_eq!(msg.tool_count(), 2);
        assert_eq!(msg.completed_tool_count(), 1);
        assert_eq!(msg.failed_tool_count(), 0);
    }

    #[test]
    fn test_deep_tool_expansion() {
        let tool1 = ToolExecution::new(
            "tool_1".to_string(),
            "read".to_string(),
            "read: file.txt".to_string(),
        );
        let tool2 = ToolExecution::new(
            "tool_2".to_string(),
            "write".to_string(),
            "write: file.txt".to_string(),
        );

        let mut msg = Message::assistant("Test".to_string()).with_tools(vec![tool1, tool2]);

        msg.set_deep_tool_expansion(1);
        assert_eq!(msg.tools_expansion, ExpansionLevel::Deep);
        assert_eq!(msg.focused_tool_index, Some(1));
    }

    #[test]
    fn test_thinking_toggle() {
        let mut msg =
            Message::assistant("Test".to_string()).with_thinking("Let me think...".to_string());

        assert!(msg.has_thinking());
        assert_eq!(msg.thinking_expansion, ExpansionLevel::Collapsed);

        msg.toggle_thinking_expansion();
        assert_eq!(msg.thinking_expansion, ExpansionLevel::Expanded);
    }

    #[test]
    fn test_role_display() {
        assert_eq!(format!("{}", MessageRole::User), "you");
        assert_eq!(format!("{}", MessageRole::Assistant), "ai");
        assert_eq!(format!("{}", MessageRole::System), "system");
    }

    #[test]
    fn test_message_tags() {
        use crate::ui::message_tags::{Tag, TagType};

        let mut msg = Message::user("Test".to_string());
        assert!(!msg.has_any_tags());

        let tag = Tag::new(TagType::Important);
        assert!(msg.add_tag(tag.clone()));
        assert!(msg.has_tag(&TagType::Important));
        assert_eq!(msg.tags().len(), 1);

        assert!(msg.remove_tag(&tag));
        assert!(!msg.has_tag(&TagType::Important));
    }

    #[test]
    fn test_message_tag_duplicates() {
        use crate::ui::message_tags::{Tag, TagType};

        let mut msg = Message::user("Test".to_string());
        let tag = Tag::new(TagType::Important);

        assert!(msg.add_tag(tag.clone()));
        assert!(!msg.add_tag(tag)); // Duplicate should fail
        assert_eq!(msg.tags().len(), 1);
    }

    #[test]
    fn test_message_remove_tag_type() {
        use crate::ui::message_tags::{Tag, TagType};

        let mut msg = Message::user("Test".to_string());
        msg.add_tag(Tag::new(TagType::Important));
        msg.add_tag(Tag::new(TagType::Idea));

        assert!(msg.remove_tag_type(&TagType::Important));
        assert!(!msg.has_tag(&TagType::Important));
        assert!(msg.has_tag(&TagType::Idea));
    }

    #[test]
    fn test_message_clear_tags() {
        use crate::ui::message_tags::{Tag, TagType};

        let mut msg = Message::user("Test".to_string());
        msg.add_tag(Tag::new(TagType::Important));
        msg.add_tag(Tag::new(TagType::Idea));

        msg.clear_tags();
        assert!(!msg.has_any_tags());
        assert_eq!(msg.tags().len(), 0);
    }

    #[test]
    fn test_duration_string() {
        let mut tool = ToolExecution::new(
            "tool_1".to_string(),
            "Bash".to_string(),
            "bash: test".to_string(),
        );
        assert_eq!(tool.duration_string(), "running");

        tool.complete(None);
        assert!(tool.duration_string().contains("ms") || tool.duration_string().contains("s"));
    }

    #[test]
    fn test_size_summary() {
        let mut tool = ToolExecution::new(
            "tool_1".to_string(),
            "read".to_string(),
            "read: file.txt".to_string(),
        );
        tool.complete(Some("Hello, World!".to_string()));
        assert_eq!(tool.size_summary(), "13b");
    }
}
