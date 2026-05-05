//! Context compaction for long-running conversations
//!
//! This module implements automatic context compaction to manage conversation
//! history and prevent token limits from being exceeded. Based on patterns from
//! Anthropic's Claude Cookbooks.
//!
//! ## Features
//!
//! - Token tracking per conversation turn
//! - Automatic compaction when threshold exceeded
//! - Structured summary generation with `<summary></summary>` tags
//! - Configurable compaction thresholds
//! - Support for different summarization models
//!
//! ## Example
//!
//! ```ignore
//! use rustycode_llm::compaction::{CompactionControl, CompactionEngine};
//!
//! let control = CompactionControl {
//!     enabled: true,
//!     context_token_threshold: 5000,
//!     summary_model: Some("claude-haiku-4-5".to_string()),
//!     summary_prompt: None, // Use default
//! };
//!
//! let engine = CompactionEngine::new(control);
//!
//! // After each turn, check if compaction is needed
//! if engine.should_compact(&conversation) {
//!     let compacted = engine.compact(&conversation).await?;
//! }
//! ```

use crate::provider::{ChatMessage, CompletionRequest};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Configuration for automatic context compaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionControl {
    /// Enable/disable automatic compaction
    pub enabled: bool,

    /// Token count threshold that triggers compaction
    ///
    /// Recommended values:
    /// - Low (5k-20k): Iterative task processing with clear boundaries
    /// - Medium (50k-100k): Multi-phase workflows
    /// - High (100k-150k): Tasks requiring substantial historical context
    pub context_token_threshold: usize,

    /// Model to use for summarization (optional, defaults to main model)
    ///
    /// Using a cheaper model like "claude-haiku-4-5" for summaries can reduce costs
    pub summary_model: Option<String>,

    /// Custom prompt for generating summaries (optional)
    ///
    /// If not provided, uses the default structured summary prompt
    pub summary_prompt: Option<String>,
}

impl Default for CompactionControl {
    fn default() -> Self {
        Self {
            enabled: false,
            context_token_threshold: 100_000,
            summary_model: None,
            summary_prompt: None,
        }
    }
}

impl CompactionControl {
    pub fn new() -> Self {
        Self::default()
    }

    /// Enable compaction with a specific threshold
    pub fn with_threshold(mut self, threshold: usize) -> Self {
        self.context_token_threshold = threshold;
        self.enabled = true;
        self
    }

    /// Use a specific model for summarization
    pub fn with_summary_model(mut self, model: String) -> Self {
        self.summary_model = Some(model);
        self
    }

    /// Use a custom summary prompt
    pub fn with_summary_prompt(mut self, prompt: String) -> Self {
        self.summary_prompt = Some(prompt);
        self
    }

    /// Enable compaction
    pub fn enable(mut self) -> Self {
        self.enabled = true;
        self
    }

    /// Disable compaction
    pub fn disable(mut self) -> Self {
        self.enabled = false;
        self
    }
}

/// Statistics about conversation token usage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
    /// Total input tokens (including cache)
    pub input_tokens: u64,

    /// Total output tokens
    pub output_tokens: u64,

    pub cache_creation_tokens: u64,

    pub cache_read_tokens: u64,

    /// Total tokens (input + output)
    pub total_tokens: u64,
}

impl TokenUsage {
    /// Calculate total input tokens including cache
    pub fn total_input(&self) -> u64 {
        self.input_tokens + self.cache_creation_tokens + self.cache_read_tokens
    }

    /// Calculate overall total
    pub fn total(&self) -> u64 {
        self.total_input() + self.output_tokens
    }
}

/// Conversation state with metadata for compaction
#[derive(Debug, Clone)]
pub struct Conversation {
    /// Messages in the conversation
    pub messages: Vec<ChatMessage>,

    /// Token usage tracking per turn
    pub token_usage: Vec<TokenUsage>,

    /// Number of turns (complete request-response cycles)
    pub turn_count: usize,

    /// Metadata about the conversation
    pub metadata: ConversationMetadata,
}

impl Conversation {
    pub fn new() -> Self {
        Self {
            messages: Vec::new(),
            token_usage: Vec::new(),
            turn_count: 0,
            metadata: ConversationMetadata::default(),
        }
    }

    /// Add a message to the conversation
    pub fn add_message(&mut self, message: ChatMessage) {
        self.messages.push(message);
    }

    /// Record token usage for a turn
    pub fn record_usage(&mut self, usage: TokenUsage) {
        self.token_usage.push(usage);
        self.turn_count = self.turn_count.saturating_add(1);
    }

    /// Get total tokens across all turns
    pub fn total_tokens(&self) -> u64 {
        self.token_usage.iter().map(|u| u.total()).sum()
    }

    /// Get current input tokens (last turn or sum of all)
    pub fn current_input_tokens(&self) -> u64 {
        self.token_usage
            .last()
            .map(|u| u.total_input())
            .unwrap_or(0)
    }

    /// Estimate token count of conversation (rough approximation)
    ///
    /// This is a fallback when we don't have actual token counts
    pub fn estimate_tokens(&self) -> usize {
        // Rough estimate: ~4 characters per token
        let text_length: usize = self
            .messages
            .iter()
            .map(|m| {
                // Extract content length based on message content type
                match &m.content {
                    rustycode_protocol::MessageContent::Simple(s) => s.len(),
                    rustycode_protocol::MessageContent::Blocks(blocks) => {
                        // For blocks, count the total content size
                        blocks
                            .iter()
                            .map(|b| match b {
                                rustycode_protocol::ContentBlock::Text { text, .. } => text.len(),
                                rustycode_protocol::ContentBlock::Image { .. } => 85, // ~85 tokens per image
                                rustycode_protocol::ContentBlock::ToolUse { id, name, input } => {
                                    // Estimate token count for tool use: id + name + JSON input
                                    id.len() + name.len() + input.to_string().len() / 4
                                }
                                rustycode_protocol::ContentBlock::Thinking {
                                    thinking,
                                    signature,
                                } => {
                                    // Estimate token count for thinking: content + signature
                                    thinking.len() + signature.len()
                                }
                                #[allow(unreachable_patterns)]
                                _ => 0,
                            })
                            .sum()
                    }
                    #[allow(unreachable_patterns)]
                    _ => 0,
                }
            })
            .sum();

        text_length / 4
    }
}

impl Default for Conversation {
    fn default() -> Self {
        Self::new()
    }
}

/// Metadata about a conversation
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ConversationMetadata {
    /// Task being worked on
    pub task: Option<String>,

    pub success_criteria: Option<String>,

    /// Files that have been created or modified
    pub files_touched: Vec<String>,

    /// Key decisions made
    pub decisions: Vec<String>,

    /// Errors encountered and their resolutions
    pub errors_resolved: Vec<String>,

    /// Current step/phase
    pub current_phase: Option<String>,
}

impl ConversationMetadata {
    /// Record that a file was touched
    pub fn touch_file(&mut self, path: String) {
        if !self.files_touched.contains(&path) {
            self.files_touched.push(path);
        }
    }

    /// Record a decision
    pub fn record_decision(&mut self, decision: String) {
        self.decisions.push(decision);
    }

    /// Record an error resolution
    pub fn record_error_resolution(&mut self, error: String, resolution: String) {
        self.errors_resolved
            .push(format!("{} → {}", error, resolution));
    }
}

/// Engine for managing context compaction
#[derive(Debug, Clone)]
pub struct CompactionEngine {
    config: CompactionControl,
}

impl CompactionEngine {
    pub fn new(config: CompactionControl) -> Self {
        Self { config }
    }

    /// Create a compaction engine with default settings
    pub fn with_defaults() -> Self {
        Self::new(CompactionControl::default())
    }

    /// Create a compaction engine optimized for iterative processing
    ///
    /// Low threshold (5k) for tasks with clear per-item boundaries
    pub fn for_iterative_processing() -> Self {
        Self::new(CompactionControl::default().enable().with_threshold(5_000))
    }

    /// Create a compaction engine optimized for multi-phase workflows
    ///
    /// Medium threshold (50k) for workflows with fewer checkpoints
    pub fn for_multi_phase() -> Self {
        Self::new(CompactionControl::default().enable().with_threshold(50_000))
    }

    /// Check if compaction should be triggered
    pub fn should_compact(&self, conversation: &Conversation) -> bool {
        if !self.config.enabled {
            return false;
        }

        // Use actual token counts if available, otherwise estimate
        let current_tokens = if !conversation.token_usage.is_empty() {
            conversation.current_input_tokens() as usize
        } else {
            conversation.estimate_tokens()
        };

        current_tokens >= self.config.context_token_threshold
    }

    /// Get the default structured summary prompt
    fn default_summary_prompt(&self) -> &str {
        r#"You have been working on the task described above but have not yet completed it. Write a continuation summary that will allow you (or another instance of yourself) to resume work efficiently in a future context window where the conversation history will be replaced with this summary. Your summary should be structured, concise, and actionable. Include:

1. **Task Overview**
   - The user's core request and success criteria
   - Any clarifications or constraints they specified

2. **Current State**
   - What has been completed so far
   - Files created, modified, or analyzed (with paths if relevant)
   - Key outputs or artifacts produced

3. **Important Discoveries**
   - Technical constraints or requirements uncovered
   - Decisions made and their rationale
   - Errors encountered and how they were resolved
   - What approaches were tried that didn't work (and why)

4. **Next Steps**
   - Specific actions needed to complete the task
   - Any blockers or open questions to resolve
   - Priority order if multiple steps remain

5. **Context to Preserve**
   - User preferences or style requirements
   - Domain-specific details that aren't obvious
   - Any promises made to the user

Be concise but complete—err on the side of including information that would prevent duplicate work or repeated mistakes. Write in a way that enables immediate resumption of the task.

Wrap your summary in <summary></summary> tags."#
    }

    /// Get the summary prompt to use
    fn get_summary_prompt(&self) -> &str {
        self.config
            .summary_prompt
            .as_deref()
            .unwrap_or_else(|| self.default_summary_prompt())
    }

    /// Prune large tool outputs from older messages.
    ///
    /// Walks messages in reverse, estimating token cost per tool result.
    /// Once the cumulative cost exceeds `protect_bytes`, old tool outputs
    /// are replaced with a placeholder like "[Output pruned — X bytes]".
    /// Recent tool outputs are always kept.
    ///
    /// This is a fast, no-LLM-call alternative to full compaction.
    /// Returns the number of bytes pruned.
    pub fn prune_tool_outputs(conversation: &mut Conversation, protect_bytes: usize) -> usize {
        use crate::provider::MessageRole;

        let mut total_bytes: usize = 0;
        let mut pruned_bytes: usize = 0;
        let mut pruned_count: usize = 0;

        // Walk messages in reverse (newest first)
        let msg_count = conversation.messages.len();
        for i in (0..msg_count).rev() {
            let msg = &mut conversation.messages[i];

            // Match tool results: Tool role, structured ContentBlock::ToolResult,
            // or legacy user messages wrapping tool_result JSON
            let is_tool_output = matches!(msg.role, MessageRole::Tool(_))
                || (matches!(msg.role, MessageRole::User)
                    && msg.content.as_text().contains("\"type\":\"tool_result\""))
                || matches!(
                    &msg.content,
                    rustycode_protocol::MessageContent::Blocks(blocks)
                        if blocks.iter().any(|b| matches!(b, rustycode_protocol::ContentBlock::ToolResult { .. }))
                );

            if !is_tool_output {
                continue;
            }

            let content_len = msg.content.as_text().len();
            total_bytes += content_len;

            // Protect recent messages (keep those within protect_bytes budget)
            if total_bytes <= protect_bytes {
                continue;
            }

            // Skip very small outputs — not worth pruning
            if content_len < 500 {
                continue;
            }

            // Prune: replace content with placeholder
            let placeholder = format!(
                "[Output pruned — {} bytes. Use read_file to view the result if needed.]",
                content_len
            );
            pruned_bytes += content_len.saturating_sub(placeholder.len());
            pruned_count += 1;

            // Replace content
            msg.content = rustycode_protocol::MessageContent::simple(&placeholder);
        }

        if pruned_count > 0 {
            tracing::info!(
                "Pruned {} tool outputs, freed ~{} bytes",
                pruned_count,
                pruned_bytes
            );
        }

        pruned_bytes
    }

    /// Generate a summary of the conversation
    ///
    /// This creates a structured summary that preserves critical information
    /// while discarding detailed tool results and intermediate outputs.
    pub async fn generate_summary(
        &self,
        conversation: &Conversation,
        provider: Arc<dyn crate::provider::LLMProvider>,
    ) -> Result<String> {
        use crate::provider::LLMProvider;

        // Build the summary prompt with conversation context
        let summary_prompt = self.get_summary_prompt();

        // Create a request to generate a summary
        // Include the conversation as context
        let mut messages = vec![ChatMessage::system(
            "You are an expert at summarizing conversations and technical work. Create clear, structured summaries that enable efficient resumption of work.".to_string(),
        )];

        // Add the conversation messages (limited to avoid overwhelming)
        let recent_messages: Vec<_> = conversation
            .messages
            .iter()
            .rev()
            .take(50) // Limit to recent 50 messages
            .collect();

        // Add in reverse order (newest first) then reverse back
        for msg in recent_messages.into_iter().rev() {
            messages.push(msg.clone());
        }

        // Add the summary request
        messages.push(ChatMessage::user(summary_prompt.to_string()));

        let model = self
            .config
            .summary_model
            .clone()
            .unwrap_or_else(|| "claude-sonnet-4-6".to_string());

        let request = CompletionRequest::new(model, messages)
            .with_max_tokens(4096)
            .with_temperature(0.1);

        let response = LLMProvider::complete(&*provider, request).await?;

        Ok(response.content)
    }

    /// Compact the conversation by replacing it with a summary
    ///
    /// Returns a new conversation with the summary as the sole message
    pub async fn compact(
        &self,
        conversation: &Conversation,
        provider: Arc<dyn crate::provider::LLMProvider>,
    ) -> Result<Conversation> {
        if !self.config.enabled {
            return Ok(conversation.clone());
        }

        // Generate the summary
        let summary = self.generate_summary(conversation, provider).await?;

        // Create a new conversation with just the summary
        let mut compacted = Conversation::new();
        compacted.metadata = conversation.metadata.clone();
        compacted.messages = vec![ChatMessage::user(summary)];
        compacted.turn_count = conversation.turn_count; // Preserve turn count

        Ok(compacted)
    }

    /// Compact only if threshold is exceeded
    ///
    /// Returns the original conversation if no compaction needed
    pub async fn compact_if_needed(
        &self,
        conversation: &Conversation,
        provider: Arc<dyn crate::provider::LLMProvider>,
    ) -> Result<Conversation> {
        if self.should_compact(conversation) {
            self.compact(conversation, provider).await
        } else {
            Ok(conversation.clone())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compaction_control_builder() {
        let control = CompactionControl::new()
            .enable()
            .with_threshold(5000)
            .with_summary_model("claude-haiku-4-5".to_string());

        assert!(control.enabled);
        assert_eq!(control.context_token_threshold, 5000);
        assert_eq!(control.summary_model, Some("claude-haiku-4-5".to_string()));
    }

    #[test]
    fn test_for_iterative_processing() {
        let engine = CompactionEngine::for_iterative_processing();
        assert!(engine.config.enabled);
        assert_eq!(engine.config.context_token_threshold, 5000);
    }

    #[test]
    fn test_for_multi_phase() {
        let engine = CompactionEngine::for_multi_phase();
        assert!(engine.config.enabled);
        assert_eq!(engine.config.context_token_threshold, 50_000);
    }

    #[test]
    fn test_should_compact_disabled() {
        let engine = CompactionEngine::new(CompactionControl::default());
        let mut conv = Conversation::new();
        conv.messages.push(ChatMessage::user("test".to_string()));

        assert!(!engine.should_compact(&conv));
    }

    #[test]
    fn test_conversation_estimate_tokens() {
        let mut conv = Conversation::new();
        conv.messages
            .push(ChatMessage::user("Hello world!".to_string()));
        conv.messages
            .push(ChatMessage::assistant("Hi there!".to_string()));

        // "Hello world!" + "Hi there!" = 24 chars, ~6 tokens
        let estimate = conv.estimate_tokens();
        assert!(estimate > 0 && estimate < 20);
    }

    #[test]
    fn test_conversation_metadata() {
        let mut meta = ConversationMetadata::default();
        meta.touch_file("src/main.rs".to_string());
        meta.record_decision("Use async/await pattern".to_string());
        meta.record_error_resolution(
            "Parse error".to_string(),
            "Fixed missing semicolon".to_string(),
        );

        assert_eq!(meta.files_touched.len(), 1);
        assert_eq!(meta.decisions.len(), 1);
        assert_eq!(meta.errors_resolved.len(), 1);
        assert!(meta.files_touched.contains(&"src/main.rs".to_string()));
    }

    #[test]
    fn test_token_usage_calculation() {
        let usage = TokenUsage {
            input_tokens: 1000,
            output_tokens: 500,
            cache_creation_tokens: 200,
            cache_read_tokens: 300,
            total_tokens: 0,
        };

        assert_eq!(usage.total_input(), 1500);
        assert_eq!(usage.total(), 2000);
    }

    #[test]
    fn test_prune_tool_outputs_no_tool_messages() {
        let mut conv = Conversation::new();
        conv.messages.push(ChatMessage::user("hello".to_string()));
        conv.messages
            .push(ChatMessage::assistant("world".to_string()));

        let pruned = CompactionEngine::prune_tool_outputs(&mut conv, 1000);
        assert_eq!(pruned, 0);
    }

    #[test]
    fn test_prune_tool_outputs_protects_recent() {
        let mut conv = Conversation::new();

        // Old large tool output
        let old_output = "x".repeat(5000);
        conv.messages.push(ChatMessage::tool_result(
            old_output.clone(),
            "tool-1".to_string(),
        ));

        // Recent tool output
        let recent_output = "y".repeat(1000);
        conv.messages.push(ChatMessage::tool_result(
            recent_output.clone(),
            "tool-2".to_string(),
        ));

        // Protect 2000 bytes from the end — recent is kept, old is pruned
        let pruned = CompactionEngine::prune_tool_outputs(&mut conv, 2000);
        assert!(pruned > 0);

        // Recent output should be unchanged
        let recent_msg = &conv.messages[1];
        let text = recent_msg.content.as_text();
        assert!(text.contains(&recent_output));

        // Old output should be pruned
        let old_msg = &conv.messages[0];
        let text = old_msg.content.as_text();
        assert!(text.contains("pruned"));
    }

    #[test]
    fn test_prune_tool_outputs_skips_small() {
        let mut conv = Conversation::new();

        // Small tool outputs should not be pruned even if old
        let small = "x".repeat(100);
        conv.messages.push(ChatMessage::tool_result(
            small.clone(),
            "tool-1".to_string(),
        ));
        conv.messages.push(ChatMessage::tool_result(
            small.clone(),
            "tool-2".to_string(),
        ));
        conv.messages.push(ChatMessage::tool_result(
            small.clone(),
            "tool-3".to_string(),
        ));

        let pruned = CompactionEngine::prune_tool_outputs(&mut conv, 0);
        assert_eq!(pruned, 0); // All under 500 byte threshold
    }
}
