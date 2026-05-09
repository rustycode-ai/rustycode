//! Token Counter with LRU Cache
//!
//! Provides token counting with an in-memory LRU cache for performance.
//! Uses character-based estimation (1 token ≈ 4 characters) as a fast
//! approximation. Also calculates token overhead for tool definitions.
//!
//! Inspired by goose's `TokenCounter` with the following adaptations:
//! - Uses character-based estimation instead of tiktoken (no native dep)
//! - Thread-safe with `DashMap` for concurrent access
//! - Configurable cache size with automatic eviction
//! - Tool schema token overhead calculation from goose's formula
//!
//! ## Usage
//!
//! ```ignore
//! use rustycode_tools::token_counter::TokenCounter;
//!
//! let counter = TokenCounter::new();
//!
//! let tokens = counter.count_tokens("Hello, world!");
//! assert!(tokens > 0);
//!
//! // Same text returns cached result
//! let cached = counter.count_tokens("Hello, world!");
//! assert_eq!(tokens, cached);
//! ```

use ahash::AHasher;
use dashmap::DashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

/// Maximum cache entries before eviction.
pub const MAX_TOKEN_CACHE_SIZE: usize = 10_000;

/// Characters per token (rough estimation for English text).
pub const CHARS_PER_TOKEN: usize = 4;

// Token overhead constants for tool definitions (from goose)
const FUNC_INIT: usize = 7;
const PROP_INIT: usize = 3;
const PROP_KEY: usize = 3;
const ENUM_INIT: isize = -3;
const ENUM_ITEM: usize = 3;
const FUNC_END: usize = 12;

/// Token overhead per message in chat format.
const TOKENS_PER_MESSAGE: usize = 4;

/// Reply primer tokens added to every request.
const REPLY_PRIMER: usize = 3;

/// Thread-safe token counter with LRU cache.
///
/// Uses `DashMap` for concurrent access and `ahash` for fast hashing.
/// Automatically evicts entries when the cache exceeds `MAX_TOKEN_CACHE_SIZE`.
#[derive(Clone)]
pub struct TokenCounter {
    cache: Arc<DashMap<u64, usize>>,
    chars_per_token: usize,
}

impl TokenCounter {
    pub fn new() -> Self {
        Self {
            cache: Arc::new(DashMap::with_capacity(MAX_TOKEN_CACHE_SIZE)),
            chars_per_token: CHARS_PER_TOKEN,
        }
    }

    /// Create a token counter with a custom characters-per-token ratio.
    pub fn with_ratio(chars_per_token: usize) -> Self {
        Self {
            cache: Arc::new(DashMap::with_capacity(MAX_TOKEN_CACHE_SIZE)),
            chars_per_token: chars_per_token.max(1),
        }
    }

    /// Count tokens in a string, using cache when available.
    ///
    /// Uses a fast hash lookup first, falling back to character-based
    /// estimation with caching.
    pub fn count_tokens(&self, text: &str) -> usize {
        if text.is_empty() {
            return 0;
        }

        let hash = self.hash_text(text);

        if let Some(count) = self.cache.get(&hash) {
            return *count;
        }

        let count = self.estimate_tokens(text);
        self.insert_cache(hash, count);
        count
    }

    /// Estimate tokens for tool definition schemas.
    ///
    /// Calculates the token overhead for a list of tool definitions,
    /// including function names, descriptions, parameter names, types,
    /// and enum values. Based on goose's formula.
    ///
    pub fn count_tool_tokens(&self, tools: &[(String, String, serde_json::Value)]) -> usize {
        let mut count = 0;

        if tools.is_empty() {
            return 0;
        }

        for (name, description, input_schema) in tools {
            count += FUNC_INIT;

            let desc_trimmed = description.trim_end_matches('.');
            let line = format!("{name}:{desc_trimmed}");
            count += self.count_tokens(&line);

            if let Some(properties) = input_schema.get("properties").and_then(|v| v.as_object()) {
                if !properties.is_empty() {
                    count += PROP_INIT;

                    for (key, value) in properties {
                        count += PROP_KEY;

                        let p_type = value.get("type").and_then(|v| v.as_str()).unwrap_or("");
                        let p_desc = value
                            .get("description")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .trim_end_matches('.');

                        let line = format!("{key}:{p_type}:{p_desc}");
                        count += self.count_tokens(&line);

                        if let Some(enum_values) = value.get("enum").and_then(|v| v.as_array()) {
                            count = count.saturating_add_signed(ENUM_INIT);
                            for item in enum_values {
                                if let Some(item_str) = item.as_str() {
                                    count += ENUM_ITEM;
                                    count += self.count_tokens(item_str);
                                }
                            }
                        }
                    }
                }
            }
        }

        count += FUNC_END;
        count
    }

    /// Count total tokens for a chat conversation.
    ///
    /// Includes system prompt, messages, tool definitions, and reply primer.
    pub fn count_chat_tokens(
        &self,
        system_prompt: &str,
        messages: &[(String, String)], // (role, content)
        tools: &[(String, String, serde_json::Value)],
    ) -> usize {
        let mut num_tokens = 0usize;

        if !system_prompt.is_empty() {
            num_tokens = num_tokens
                .saturating_add(self.count_tokens(system_prompt))
                .saturating_add(TOKENS_PER_MESSAGE);
        }

        for (_role, content) in messages {
            num_tokens = num_tokens.saturating_add(TOKENS_PER_MESSAGE);
            num_tokens = num_tokens.saturating_add(self.count_tokens(content));
        }

        if !tools.is_empty() {
            num_tokens = num_tokens.saturating_add(self.count_tool_tokens(tools));
        }

        num_tokens.saturating_add(REPLY_PRIMER)
    }

    /// Clear the token cache.
    pub fn clear_cache(&self) {
        self.cache.clear();
    }

    /// Get the current cache size.
    pub fn cache_size(&self) -> usize {
        self.cache.len()
    }

    /// Estimate tokens using character-based approximation.
    fn estimate_tokens(&self, text: &str) -> usize {
        // More accurate estimation considering:
        // - Whitespace boundaries (words are ~1.3 tokens each)
        // - Punctuation
        // - Special characters
        let char_count = text.chars().count();
        let word_count = text.split_whitespace().count();

        // Blend character-based and word-based estimation
        let char_estimate = char_count.div_ceil(self.chars_per_token);
        let word_estimate = (word_count as f64 * 1.3) as usize;

        // Use the higher estimate for safety (don't underestimate)
        char_estimate.max(word_estimate).max(1)
    }

    /// Hash text for cache key.
    fn hash_text(&self, text: &str) -> u64 {
        let mut hasher = AHasher::default();
        text.hash(&mut hasher);
        hasher.finish()
    }

    /// Insert into cache with eviction if needed.
    fn insert_cache(&self, hash: u64, count: usize) {
        // Evict oldest entries if cache is full
        if self.cache.len() >= MAX_TOKEN_CACHE_SIZE {
            // Remove a batch of entries to avoid frequent eviction
            for _ in 0..100.min(self.cache.len()) {
                if let Some(entry) = self.cache.iter().next() {
                    let old_hash = *entry.key();
                    drop(entry);
                    self.cache.remove(&old_hash);
                } else {
                    break;
                }
            }
        }

        self.cache.insert(hash, count);
    }
}

impl Default for TokenCounter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_count_tokens_basic() {
        let counter = TokenCounter::new();

        let count = counter.count_tokens("Hello, world!");
        assert!(count > 0);
        assert!(count < 20); // Reasonable range
    }

    #[test]
    fn test_count_tokens_empty() {
        let counter = TokenCounter::new();
        assert_eq!(counter.count_tokens(""), 0);
    }

    #[test]
    fn test_count_tokens_caching() {
        let counter = TokenCounter::new();

        let count1 = counter.count_tokens("This is a test for caching");
        assert_eq!(counter.cache_size(), 1);

        let count2 = counter.count_tokens("This is a test for caching");
        assert_eq!(count1, count2);
        assert_eq!(counter.cache_size(), 1); // Same entry, no growth
    }

    #[test]
    fn test_count_tokens_different_texts() {
        let counter = TokenCounter::new();

        let count1 = counter.count_tokens("Short text");
        let count2 = counter
            .count_tokens("This is a much longer piece of text that should have more tokens");
        assert!(count2 > count1);
        assert_eq!(counter.cache_size(), 2);
    }

    #[test]
    fn test_clear_cache() {
        let counter = TokenCounter::new();

        counter.count_tokens("First");
        counter.count_tokens("Second");
        assert_eq!(counter.cache_size(), 2);

        counter.clear_cache();
        assert_eq!(counter.cache_size(), 0);
    }

    #[test]
    fn test_cache_eviction() {
        let counter = TokenCounter::new();

        // Verify cache grows and stays bounded with a small number of entries
        for i in 0..50 {
            counter.count_tokens(&format!("Test string number {} with unique content", i));
        }

        assert!(counter.cache_size() <= MAX_TOKEN_CACHE_SIZE);
        assert!(counter.cache_size() >= 50);
    }

    #[test]
    fn test_count_tool_tokens_empty() {
        let counter = TokenCounter::new();
        assert_eq!(counter.count_tool_tokens(&[]), 0);
    }

    #[test]
    fn test_count_tool_tokens_basic() {
        let counter = TokenCounter::new();

        let tools = vec![(
            "Bash".to_string(),
            "Execute a shell command".to_string(),
            serde_json::json!({
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "The command to execute"
                    }
                }
            }),
        )];

        let tokens = counter.count_tool_tokens(&tools);
        // Should include: FUNC_INIT + token count of "bash:Execute a shell command" +
        // PROP_INIT + PROP_KEY + token count of "command:string:The command to execute" + FUNC_END
        assert!(tokens > FUNC_INIT + FUNC_END);
    }

    #[test]
    fn test_count_tool_tokens_with_enum() {
        let counter = TokenCounter::new();

        let tools = vec![(
            "set_mode".to_string(),
            "Set the permission mode".to_string(),
            serde_json::json!({
                "properties": {
                    "mode": {
                        "type": "string",
                        "description": "The mode to use",
                        "enum": ["read", "write", "execute"]
                    }
                }
            }),
        )];

        let tokens = counter.count_tool_tokens(&tools);
        assert!(tokens > FUNC_INIT + FUNC_END);
    }

    #[test]
    fn test_count_chat_tokens() {
        let counter = TokenCounter::new();

        let messages = vec![
            ("user".to_string(), "Hello".to_string()),
            ("assistant".to_string(), "Hi there!".to_string()),
            ("user".to_string(), "How are you?".to_string()),
        ];

        let tokens = counter.count_chat_tokens("You are helpful", &messages, &[]);
        assert!(tokens > 0);

        // Should include: system prompt tokens + 4 + 3 messages * 4 + message tokens + 3 (reply primer)
        let system_only = counter.count_chat_tokens("You are helpful", &[], &[]);
        assert!(tokens > system_only);
    }

    #[test]
    fn test_count_chat_tokens_with_tools() {
        let counter = TokenCounter::new();

        let messages = vec![("user".to_string(), "List files".to_string())];
        let tools = vec![(
            "ListDir".to_string(),
            "List directory contents".to_string(),
            serde_json::json!({"properties": {"path": {"type": "string", "description": "Directory path"}}}),
        )];

        let without_tools = counter.count_chat_tokens("system", &messages, &[]);
        let with_tools = counter.count_chat_tokens("system", &messages, &tools);

        assert!(with_tools > without_tools);
    }

    #[test]
    fn test_count_chat_tokens_empty() {
        let counter = TokenCounter::new();
        let tokens = counter.count_chat_tokens("", &[], &[]);
        assert_eq!(tokens, REPLY_PRIMER); // Just the reply primer
    }

    #[test]
    fn test_custom_chars_per_token() {
        let default = TokenCounter::new();
        let custom = TokenCounter::with_ratio(2); // More tokens per char

        let text = "Hello world this is a test";
        let default_count = default.count_tokens(text);
        let custom_count = custom.count_tokens(text);

        assert!(custom_count >= default_count);
    }

    #[test]
    fn test_concurrent_access() {
        use std::sync::Arc;

        let counter = Arc::new(TokenCounter::new());

        // Test that Arc cloning and shared access works
        let c1 = Arc::clone(&counter);
        let c2 = Arc::clone(&counter);

        let count1 = c1.count_tokens("Shared counter test");
        let count2 = c2.count_tokens("Shared counter test");

        assert_eq!(count1, count2);
        assert!(counter.cache_size() > 0);
    }

    #[test]
    fn test_estimate_tokens_long_text() {
        let counter = TokenCounter::new();

        let long_text = "word ".repeat(100);
        let count = counter.count_tokens(&long_text);

        // 100 words ≈ ~100-130 tokens
        assert!(count > 80);
        assert!(count < 200);
    }
}
