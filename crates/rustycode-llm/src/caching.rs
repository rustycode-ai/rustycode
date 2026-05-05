//! Prompt caching strategies for cost optimization
//!
//! This module provides utilities for implementing effective prompt caching
//! strategies with Claude's API to reduce token costs by 50-90% for repetitive content.

use crate::provider::{ChatMessage, MessageRole};
use rustycode_protocol::{CacheType, ContentBlock, MessageContent};

/// Strategy for automatically caching prompt content
///
/// Prompt caching reduces costs by marking certain content blocks as cacheable.
/// Cached content is billed at a 90% discount and can be reused across requests.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub enum CachingStrategy {
    /// No caching (default)
    #[default]
    None,

    /// Cache system prompts
    ///
    /// System prompts are excellent candidates for caching as they rarely change.
    SystemPrompts,

    /// Cache tool definitions
    ///
    /// Tool definitions are repetitive and can benefit significantly from caching.
    ToolDefinitions,

    /// Cache large context blocks
    ///
    /// Large code snippets, documentation, or reference material.
    LargeContext {
        /// Minimum size in characters to consider caching
        min_size: usize,
    },

    /// Cache everything except user messages
    ///
    /// Aggressive caching for maximum cost savings.
    Aggressive,

    /// Custom caching with specific rules
    Custom(CustomCachingRules),
}

/// Custom caching rules
#[derive(Debug, Clone)]
pub struct CustomCachingRules {
    /// Cache system messages
    pub cache_system: bool,

    /// Cache tool definitions
    pub cache_tools: bool,

    /// Cache messages over this size
    pub cache_large_threshold: Option<usize>,

    /// Specific patterns to cache
    pub cache_patterns: Vec<String>,
}

impl CachingStrategy {
    /// Apply caching strategy to a list of messages
    ///
    /// Returns a new list of messages with cache_control added where appropriate.
    pub fn apply_to_messages(&self, messages: Vec<ChatMessage>) -> Vec<ChatMessage> {
        match self {
            Self::None => messages,
            Self::SystemPrompts => Self::cache_system_prompts(messages),
            Self::ToolDefinitions => Self::cache_tool_definitions(messages),
            Self::LargeContext { min_size } => Self::cache_large_content(messages, *min_size),
            Self::Aggressive => Self::cache_aggressive(messages),
            Self::Custom(rules) => Self::apply_custom_rules(messages, rules),
        }
    }

    /// Cache system prompts (role = "system")
    fn cache_system_prompts(messages: Vec<ChatMessage>) -> Vec<ChatMessage> {
        messages
            .into_iter()
            .map(|msg| {
                // Cache system messages
                if matches!(msg.role, MessageRole::System) {
                    Self::cache_message_content(msg, CacheType::Ephemeral)
                } else {
                    msg
                }
            })
            .collect()
    }

    /// Cache tool definitions
    ///
    /// This identifies tool definitions in messages and marks them for caching.
    fn cache_tool_definitions(messages: Vec<ChatMessage>) -> Vec<ChatMessage> {
        messages
            .into_iter()
            .map(|msg| {
                // Heuristic: tool definitions are in assistant messages
                // and contain JSON schema descriptions
                if matches!(msg.role, MessageRole::Assistant) {
                    let content = msg.content.to_text();
                    // Check if this looks like a tool definition
                    if Self::is_tool_definition(&content) {
                        return Self::cache_message_content(msg, CacheType::Ephemeral);
                    }
                }
                msg
            })
            .collect()
    }

    /// Cache large content blocks
    fn cache_large_content(messages: Vec<ChatMessage>, min_size: usize) -> Vec<ChatMessage> {
        messages
            .into_iter()
            .map(|msg| {
                let content_size = msg.content.len();
                if content_size > min_size {
                    Self::cache_message_content(msg, CacheType::Ephemeral)
                } else {
                    msg
                }
            })
            .collect()
    }

    /// Cache everything except user messages
    fn cache_aggressive(messages: Vec<ChatMessage>) -> Vec<ChatMessage> {
        messages
            .into_iter()
            .map(|msg| {
                // Don't cache user messages (they're always different)
                if !matches!(msg.role, MessageRole::User) {
                    Self::cache_message_content(msg, CacheType::Ephemeral)
                } else {
                    msg
                }
            })
            .collect()
    }

    /// Apply custom caching rules
    fn apply_custom_rules(
        messages: Vec<ChatMessage>,
        rules: &CustomCachingRules,
    ) -> Vec<ChatMessage> {
        messages
            .into_iter()
            .map(|msg| {
                let mut should_cache = false;
                let cache_type = CacheType::Ephemeral;

                // Check system message caching
                if rules.cache_system && matches!(msg.role, MessageRole::System) {
                    should_cache = true;
                }

                // Check tool definition caching
                if rules.cache_tools && matches!(msg.role, MessageRole::Assistant) {
                    let content = msg.content.to_text();
                    if Self::is_tool_definition(&content) {
                        should_cache = true;
                    }
                }

                // Check size threshold
                if let Some(threshold) = rules.cache_large_threshold {
                    if msg.content.len() > threshold {
                        should_cache = true;
                    }
                }

                // Check custom patterns
                let content = msg.content.to_text();
                for pattern in &rules.cache_patterns {
                    if content.contains(pattern) {
                        should_cache = true;
                        break;
                    }
                }

                if should_cache {
                    Self::cache_message_content(msg, cache_type)
                } else {
                    msg
                }
            })
            .collect()
    }

    /// Add cache_control to a message's content
    fn cache_message_content(mut msg: ChatMessage, cache_type: CacheType) -> ChatMessage {
        match &msg.content {
            MessageContent::Simple(text) => {
                // Convert simple text to cached text block
                msg.content = MessageContent::cached_text_block(text.clone(), cache_type);
            }
            MessageContent::Blocks(blocks) => {
                // Add cache_control to all text blocks
                let cached_blocks = blocks
                    .iter()
                    .map(|block| match block {
                        ContentBlock::Text { text, .. } => {
                            ContentBlock::cached_text(text, cache_type)
                        }
                        ContentBlock::Image { source, .. } => {
                            // Images typically don't benefit from caching
                            ContentBlock::image(source.clone())
                        }
                        ContentBlock::ToolUse { id, name, input } => {
                            // Tool use blocks don't support caching, clone as-is
                            ContentBlock::tool_use(id.clone(), name.clone(), input.clone())
                        }
                        ContentBlock::Thinking {
                            thinking,
                            signature,
                        } => {
                            // Thinking blocks don't support caching, clone as-is
                            ContentBlock::thinking(thinking.clone(), signature.clone())
                        }
                        #[allow(unreachable_patterns)]
                        _ => block.clone(),
                    })
                    .collect();
                msg.content = MessageContent::Blocks(cached_blocks);
            }
            #[allow(unreachable_patterns)]
            _ => {}
        }
        msg
    }

    /// Check if content looks like a tool definition
    fn is_tool_definition(content: &str) -> bool {
        let content = content.trim();

        // Must be valid JSON to be a tool definition
        let Ok(json) = serde_json::from_str::<serde_json::Value>(content) else {
            return false;
        };

        // Tool definitions are typically arrays with objects containing specific fields
        let arr = match &json {
            serde_json::Value::Array(arr) => arr,
            serde_json::Value::Object(obj) => {
                // Single tool definition - check for required fields
                return obj.contains_key("name")
                    && obj.contains_key("parameters")
                    && obj.get("parameters").and_then(|p| p.get("type")).is_some();
            }
            _ => return false,
        };

        // Check if array contains valid tool definitions
        // Each should have name, description, and parameters
        arr.iter().all(|item| {
            item.get("name").is_some()
                && item.get("description").is_some()
                && (item.get("parameters").is_some() || item.get("input_schema").is_some())
        }) && !arr.is_empty()
    }
}

/// Builder for creating cached messages
pub struct CachedMessageBuilder {
    strategy: CachingStrategy,
}

impl CachedMessageBuilder {
    pub fn new(strategy: CachingStrategy) -> Self {
        Self { strategy }
    }

    /// Apply caching to messages
    pub fn apply(&self, messages: Vec<ChatMessage>) -> Vec<ChatMessage> {
        self.strategy.apply_to_messages(messages)
    }
}

impl Default for CachedMessageBuilder {
    fn default() -> Self {
        Self::new(CachingStrategy::default())
    }
}

/// Check if a ChatMessage has cached content
pub fn is_message_cached(msg: &ChatMessage) -> bool {
    match &msg.content {
        MessageContent::Simple(_) => false,
        MessageContent::Blocks(blocks) => blocks.iter().any(|block| block.is_cached()),
        #[allow(unreachable_patterns)]
        _ => false,
    }
}

/// Convenience functions for common caching patterns
pub mod helpers {
    use super::*;

    /// Cache system prompts only
    pub fn cache_system_prompts(messages: Vec<ChatMessage>) -> Vec<ChatMessage> {
        CachingStrategy::SystemPrompts.apply_to_messages(messages)
    }

    /// Cache tool definitions only
    pub fn cache_tool_definitions(messages: Vec<ChatMessage>) -> Vec<ChatMessage> {
        CachingStrategy::ToolDefinitions.apply_to_messages(messages)
    }

    /// Cache content larger than specified size
    pub fn cache_large_content(messages: Vec<ChatMessage>, min_size: usize) -> Vec<ChatMessage> {
        CachingStrategy::LargeContext { min_size }.apply_to_messages(messages)
    }

    /// Cache everything except user messages
    pub fn cache_aggressive(messages: Vec<ChatMessage>) -> Vec<ChatMessage> {
        CachingStrategy::Aggressive.apply_to_messages(messages)
    }

    /// Apply custom caching rules
    pub fn apply_custom_rules(
        messages: Vec<ChatMessage>,
        rules: &CustomCachingRules,
    ) -> Vec<ChatMessage> {
        CachingStrategy::Custom(rules.clone()).apply_to_messages(messages)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::MessageRole;

    #[test]
    fn test_cached_text_block() {
        use rustycode_protocol::ContentBlock;

        let block = ContentBlock::cached_text("Test message", CacheType::Ephemeral);
        assert!(block.is_cached());
        assert!(block.is_text());
    }

    #[test]
    fn test_cache_system_prompts() {
        let messages = vec![
            ChatMessage {
                role: MessageRole::System,
                content: MessageContent::simple("You are a helpful assistant."),
            },
            ChatMessage {
                role: MessageRole::User,
                content: MessageContent::simple("Hello!"),
            },
        ];

        let cached = helpers::cache_system_prompts(messages);

        // System message should be cached
        assert!(is_message_cached(&cached[0]));
        // User message should NOT be cached
        assert!(!is_message_cached(&cached[1]));
    }

    #[test]
    fn test_cache_large_content() {
        let messages = vec![
            ChatMessage {
                role: MessageRole::User,
                content: MessageContent::simple("Small message"),
            },
            ChatMessage {
                role: MessageRole::User,
                content: MessageContent::simple("A".repeat(2000)),
            },
        ];

        let cached = helpers::cache_large_content(messages, 1000);

        // Small message should NOT be cached
        assert!(!is_message_cached(&cached[0]));
        // Large message should be cached
        assert!(is_message_cached(&cached[1]));
    }

    #[test]
    fn test_aggressive_caching() {
        let messages = vec![
            ChatMessage {
                role: MessageRole::System,
                content: MessageContent::simple("System prompt"),
            },
            ChatMessage {
                role: MessageRole::Assistant,
                content: MessageContent::simple("Assistant response"),
            },
            ChatMessage {
                role: MessageRole::User,
                content: MessageContent::simple("User message"),
            },
        ];

        let cached = helpers::cache_aggressive(messages);

        // System and assistant should be cached
        assert!(is_message_cached(&cached[0]));
        assert!(is_message_cached(&cached[1]));
        // User should NOT be cached
        assert!(!is_message_cached(&cached[2]));
    }

    #[test]
    fn test_is_tool_definition() {
        // Valid tool definition array
        let valid_tools = r#"[
            {
                "name": "read_file",
                "description": "Read a file from the filesystem",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": {"type": "string"}
                    }
                }
            }
        ]"#;
        assert!(CachingStrategy::is_tool_definition(valid_tools));

        // Valid single tool definition
        let valid_single = r#"{
            "name": "bash",
            "description": "Run a bash command",
            "parameters": {"type": "object"}
        }"#;
        assert!(CachingStrategy::is_tool_definition(valid_single));

        // Invalid - just a regular text
        assert!(!CachingStrategy::is_tool_definition(
            "Hello, this is just a message"
        ));

        // Invalid - random JSON without tool fields
        assert!(!CachingStrategy::is_tool_definition(r#"{"foo": "bar"}"#));
    }
}
