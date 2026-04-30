//! Token counter shared between LLM and tool crates.
//!
//! This mirrors the existing `RustyCode` heuristics so usage estimation can live
//! outside `rustycode-tools` without changing behavior.

use ahash::AHasher;
use dashmap::DashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

pub const MAX_TOKEN_CACHE_SIZE: usize = 10_000;
pub const CHARS_PER_TOKEN: usize = 4;

const FUNC_INIT: usize = 7;
const PROP_INIT: usize = 3;
const PROP_KEY: usize = 3;
const ENUM_INIT: isize = -3;
const ENUM_ITEM: usize = 3;
const FUNC_END: usize = 12;
const TOKENS_PER_MESSAGE: usize = 4;
const REPLY_PRIMER: usize = 3;

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

    pub fn with_ratio(chars_per_token: usize) -> Self {
        Self {
            cache: Arc::new(DashMap::with_capacity(MAX_TOKEN_CACHE_SIZE)),
            chars_per_token: chars_per_token.max(1),
        }
    }

    pub fn count_tokens(&self, text: &str) -> usize {
        if text.is_empty() {
            return 0;
        }

        let hash = Self::hash_text(text);
        if let Some(count) = self.cache.get(&hash) {
            return *count;
        }

        let count = self.estimate_tokens(text);
        self.insert_cache(hash, count);
        count
    }

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

        count + FUNC_END
    }

    pub fn count_chat_tokens(
        &self,
        system_prompt: &str,
        messages: &[(String, String)],
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

    pub fn clear_cache(&self) {
        self.cache.clear();
    }

    pub fn cache_size(&self) -> usize {
        self.cache.len()
    }

    fn estimate_tokens(&self, text: &str) -> usize {
        let char_count = text.chars().count();
        let word_count = text.split_whitespace().count();

        let char_estimate = char_count.div_ceil(self.chars_per_token);
        // Cast is safe: word_count comes from split_whitespace() which never exceeds usize
        #[allow(clippy::cast_precision_loss)]
        let word_estimate = (word_count as f64 * 1.3) as usize;

        char_estimate.max(word_estimate).max(1)
    }

    fn hash_text(text: &str) -> u64 {
        let mut hasher = AHasher::default();
        text.hash(&mut hasher);
        hasher.finish()
    }

    fn insert_cache(&self, hash: u64, count: usize) {
        if self.cache.len() >= MAX_TOKEN_CACHE_SIZE {
            for _ in 0..100.min(self.cache.len()) {
                let value = self.cache.iter().next();
                if let Some(entry) = value {
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
        assert!(count < 20);
    }

    #[test]
    fn test_count_tokens_empty() {
        let counter = TokenCounter::new();
        assert_eq!(counter.count_tokens(""), 0);
    }

    #[test]
    fn test_count_chat_tokens() {
        let counter = TokenCounter::new();
        let messages = vec![
            ("user".to_string(), "Hello".to_string()),
            ("assistant".to_string(), "Hi".to_string()),
        ];
        let tokens = counter.count_chat_tokens("You are helpful", &messages, &[]);
        assert!(tokens > 0);
    }

    #[test]
    fn test_count_chat_tokens_with_tools() {
        let counter = TokenCounter::new();
        let messages = vec![("user".to_string(), "Hello".to_string())];
        let tools = vec![(
            "bash".to_string(),
            "Execute a shell command".to_string(),
            serde_json::json!({
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "The command"
                    }
                }
            }),
        )];
        assert!(counter.count_chat_tokens("system", &messages, &tools) > 0);
    }

    // --- Cache behavior ---

    #[test]
    fn test_cache_hit_returns_same_value() {
        let counter = TokenCounter::new();
        let text = "cache consistency check";
        let first = counter.count_tokens(text);
        let second = counter.count_tokens(text);
        assert_eq!(first, second);
        assert_eq!(counter.cache_size(), 1);
    }

    #[test]
    fn test_clear_cache_empties() {
        let counter = TokenCounter::new();
        counter.count_tokens("something");
        assert_ne!(counter.cache_size(), 0);
        counter.clear_cache();
        assert_eq!(counter.cache_size(), 0);
    }

    #[test]
    fn test_hash_deterministic() {
        let text = "deterministic hash test";
        let h1 = TokenCounter::hash_text(text);
        let h2 = TokenCounter::hash_text(text);
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_hash_different_texts_differ() {
        let h1 = TokenCounter::hash_text("first text");
        let h2 = TokenCounter::hash_text("second text");
        assert_ne!(h1, h2);
    }

    #[test]
    fn test_estimate_tokens_minimum_one() {
        let counter = TokenCounter::new();
        // Single character should estimate at least 1 token
        assert!(counter.count_tokens("a") >= 1);
    }

    // --- Custom ratio ---

    #[test]
    fn test_with_ratio_overrides_default() {
        let default = TokenCounter::new();
        let wider = TokenCounter::with_ratio(8);
        let text = "this is a test of token estimation accuracy";
        let d = default.count_tokens(text);
        let w = wider.count_tokens(text);
        // Higher chars_per_token should produce fewer tokens
        assert!(w < d, "wider ratio ({w}) should be < default ({d})");
    }

    #[test]
    fn test_with_ratio_minimum_one() {
        let counter = TokenCounter::with_ratio(0);
        let count = counter.count_tokens("hello world");
        assert!(count > 0, "ratio=0 should clamp to 1 and still estimate");
    }

    // --- Tool token counting ---

    #[test]
    fn test_count_tool_tokens_empty() {
        let counter = TokenCounter::new();
        assert_eq!(counter.count_tool_tokens(&[]), 0);
    }

    #[test]
    fn test_count_tool_tokens_with_enum() {
        let counter = TokenCounter::new();
        let tools = vec![(
            "search".to_string(),
            "Search for files".to_string(),
            serde_json::json!({
                "type": "object",
                "properties": {
                    "mode": {
                        "type": "string",
                        "description": "Search mode",
                        "enum": ["exact", "fuzzy", "regex"]
                    }
                }
            }),
        )];
        let with_enum = counter.count_tool_tokens(&tools);

        let tools_no_enum = vec![(
            "search".to_string(),
            "Search for files".to_string(),
            serde_json::json!({
                "type": "object",
                "properties": {
                    "mode": {
                        "type": "string",
                        "description": "Search mode"
                    }
                }
            }),
        )];
        counter.clear_cache();
        let without_enum = counter.count_tool_tokens(&tools_no_enum);

        assert!(
            with_enum > without_enum,
            "enum values should add tokens: {with_enum} vs {without_enum}"
        );
    }

    #[test]
    fn test_count_tool_tokens_multiple_tools() {
        let counter = TokenCounter::new();
        let tools = vec![
            (
                "bash".to_string(),
                "Run a command".to_string(),
                serde_json::json!({"type": "object", "properties": {}}),
            ),
            (
                "read".to_string(),
                "Read a file".to_string(),
                serde_json::json!({"type": "object", "properties": {}}),
            ),
        ];
        let count = counter.count_tool_tokens(&tools);
        assert!(count > 0, "multiple tools should have positive token count");
    }

    #[test]
    fn test_count_tool_tokens_no_properties() {
        let counter = TokenCounter::new();
        let tools = vec![(
            "ping".to_string(),
            "Simple tool".to_string(),
            serde_json::json!({"type": "object"}),
        )];
        let count = counter.count_tool_tokens(&tools);
        assert!(count > 0, "tool with no properties still has base tokens");
    }

    // --- Chat token counting edge cases ---

    #[test]
    fn test_count_chat_tokens_empty_messages() {
        let counter = TokenCounter::new();
        let tokens = counter.count_chat_tokens("system prompt", &[], &[]);
        assert!(tokens > 0, "system prompt alone should have tokens");
    }

    #[test]
    fn test_count_chat_tokens_no_system() {
        let counter = TokenCounter::new();
        let messages = vec![("user".to_string(), "Hello".to_string())];
        let tokens = counter.count_chat_tokens("", &messages, &[]);
        assert!(tokens > 0, "messages alone should have tokens");
    }

    #[test]
    fn test_count_chat_tokens_more_messages_more_tokens() {
        let counter = TokenCounter::new();
        let few = vec![("user".to_string(), "Hello".to_string())];
        let many = vec![
            ("user".to_string(), "Hello".to_string()),
            ("assistant".to_string(), "Hi there!".to_string()),
            ("user".to_string(), "How are you?".to_string()),
        ];
        counter.clear_cache();
        let few_tokens = counter.count_chat_tokens("", &few, &[]);
        counter.clear_cache();
        let many_tokens = counter.count_chat_tokens("", &many, &[]);
        assert!(many_tokens > few_tokens, "more messages = more tokens");
    }

    // --- Cache eviction ---

    #[test]
    fn test_cache_eviction_under_pressure() {
        let counter = TokenCounter::new();
        // Fill cache beyond MAX_TOKEN_CACHE_SIZE
        for i in 0..(MAX_TOKEN_CACHE_SIZE + 200) {
            counter.count_tokens(&format!("unique text number {i}"));
        }
        // Cache should not grow unbounded — it must be ≤ MAX + 100 (eviction batch)
        assert!(
            counter.cache_size() <= MAX_TOKEN_CACHE_SIZE + 100,
            "cache should evict entries: got {}",
            counter.cache_size()
        );
    }

    // --- Default trait ---

    #[test]
    fn test_default_equals_new() {
        let via_new = TokenCounter::new();
        let via_default = TokenCounter::default();
        let text = "default comparison";
        assert_eq!(via_new.count_tokens(text), via_default.count_tokens(text));
    }
}
