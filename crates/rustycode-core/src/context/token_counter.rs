// ── Token Counting Utilities ───────────────────────────────────────────────────
//
// Inspired by goose's TokenCounter: hash-based caching, provider-specific ratios,
// and accurate estimation. Can be upgraded to tiktoken-rs when native deps are
// acceptable.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Mutex;

/// Maximum cache entries before eviction
const MAX_CACHE_SIZE: usize = 10_000;

/// LLM provider types with different token-to-character ratios
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TokenProvider {
    Anthropic,
    OpenAI,
    Google,
    Mistral,
    Bedrock,
    Default,
}

impl TokenProvider {
    /// Characters per token for this provider (empirically derived)
    pub fn chars_per_token(self) -> f64 {
        match self {
            Self::Anthropic => 3.5,
            Self::OpenAI => 4.0,
            Self::Google => 4.0,
            Self::Mistral => 3.8,
            Self::Bedrock => 3.5,
            Self::Default => 4.0,
        }
    }
}

/// Cached token counter with provider-aware estimation.
///
/// Uses a hash-based cache (inspired by goose's DashMap approach)
/// to avoid re-counting the same text repeatedly. Thread-safe via Mutex.
///
/// # Example
///
/// ```ignore
/// use rustycode_core::context::token_counter::CachedTokenCounter;
///
/// let counter = CachedTokenCounter::new(TokenProvider::Anthropic);
/// let tokens = counter.count_tokens("Hello, world!");
/// let cached = counter.count_tokens("Hello, world!"); // Cache hit
/// assert_eq!(tokens, cached);
/// ```
pub struct CachedTokenCounter {
    /// Provider for ratio-aware estimation
    provider: TokenProvider,
    /// Hash -> token count cache
    cache: Mutex<HashMap<u64, usize>>,
    /// Number of cache hits (for diagnostics)
    hits: std::sync::atomic::AtomicU64,
    /// Number of cache misses
    misses: std::sync::atomic::AtomicU64,
}

impl CachedTokenCounter {
    pub fn new(provider: TokenProvider) -> Self {
        Self {
            provider,
            cache: Mutex::new(HashMap::with_capacity(1024)),
            hits: std::sync::atomic::AtomicU64::new(0),
            misses: std::sync::atomic::AtomicU64::new(0),
        }
    }

    /// Default counter with OpenAI-compatible ratios
    pub fn default_counter() -> Self {
        Self::new(TokenProvider::Default)
    }

    /// Count tokens for text, using cache if available.
    pub fn count_tokens(&self, text: &str) -> usize {
        let hash = Self::hash_text(text);

        // Check cache first
        {
            let cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(&count) = cache.get(&hash) {
                self.hits.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                return count;
            }
        }

        self.misses
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let count = self.estimate_tokens_internal(text);

        // Insert with eviction if needed
        {
            let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
            if cache.len() >= MAX_CACHE_SIZE {
                // Evict ~25% of entries (simple strategy)
                let keys_to_remove: Vec<u64> =
                    cache.keys().take(MAX_CACHE_SIZE / 4).copied().collect();
                for key in keys_to_remove {
                    cache.remove(&key);
                }
            }
            cache.insert(hash, count);
        }

        count
    }

    /// Count tokens for multiple texts and return the sum.
    pub fn count_total(&self, texts: &[&str]) -> usize {
        texts.iter().map(|t| self.count_tokens(t)).sum()
    }

    /// Estimate tokens for a chat conversation.
    ///
    /// Adds overhead per message for role markers and formatting.
    pub fn count_chat_tokens(&self, system_prompt: &str, messages: &[ChatMessageInfo]) -> usize {
        let mut total = self.count_tokens(system_prompt);

        // Overhead per message: role markers, newlines, separators (~4 tokens each)
        let message_overhead = 4 * messages.len();
        total += message_overhead;

        for msg in messages {
            total += self.count_tokens(&msg.content);
            if let Some(ref tool_content) = msg.tool_content {
                total += self.count_tokens(tool_content);
            }
        }

        total
    }

    /// Estimate tokens for tool definitions.
    ///
    /// Uses goose-style formula for JSON schema token estimation.
    pub fn count_tool_tokens(&self, tool_schemas: &[&str]) -> usize {
        let mut total = 0;
        for schema in tool_schemas {
            // Base overhead per function definition (~7 tokens)
            total += 7;
            total += self.count_tokens(schema);
            // Closing overhead (~12 tokens)
            total += 12;
        }
        total
    }

    /// Clear the token cache.
    pub fn clear_cache(&self) {
        let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        cache.clear();
        self.hits.store(0, std::sync::atomic::Ordering::Relaxed);
        self.misses.store(0, std::sync::atomic::Ordering::Relaxed);
    }

    /// Get cache statistics (hits, misses, size).
    pub fn cache_stats(&self) -> (u64, u64, usize) {
        let hits = self.hits.load(std::sync::atomic::Ordering::Relaxed);
        let misses = self.misses.load(std::sync::atomic::Ordering::Relaxed);
        let size = self.cache.lock().unwrap_or_else(|e| e.into_inner()).len();
        (hits, misses, size)
    }

    /// Get the current provider.
    pub fn provider(&self) -> TokenProvider {
        self.provider
    }

    // ── Internal helpers ──────────────────────────────────────────────────

    fn estimate_tokens_internal(&self, text: &str) -> usize {
        if text.is_empty() {
            return 0;
        }
        let ratio = self.provider.chars_per_token();
        ((text.len() as f64) / ratio).ceil() as usize
    }

    fn hash_text(text: &str) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        text.hash(&mut hasher);
        hasher.finish()
    }
}

impl Default for CachedTokenCounter {
    fn default() -> Self {
        Self::default_counter()
    }
}

/// Lightweight message info for chat token counting.
#[derive(Debug, Clone)]
pub struct ChatMessageInfo {
    pub role: String,
    pub content: String,
    pub tool_content: Option<String>,
}

// ── Legacy TokenCounter (backward compatible) ─────────────────────────────────

/// Simple token estimation utilities (legacy API).
///
/// For new code, prefer `CachedTokenCounter` which provides caching
/// and provider-aware estimation.
pub struct TokenCounter;

impl TokenCounter {
    /// Estimate tokens for text (rough approximation: 1 token ≈ 4 characters).
    pub fn estimate_tokens(text: &str) -> usize {
        text.len().div_ceil(4)
    }

    /// Estimate tokens for multiple texts and return the sum.
    pub fn estimate_total<'a, I>(texts: I) -> usize
    where
        I: IntoIterator<Item = &'a str>,
    {
        texts.into_iter().map(Self::estimate_tokens).sum()
    }

    /// Count exact tokens using a tokenizer function.
    pub fn count_exact<F>(text: &str, tokenizer_fn: F) -> usize
    where
        F: FnOnce(&str) -> usize,
    {
        tokenizer_fn(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_legacy_estimate_tokens() {
        let text = "Hello, world!";
        let tokens = TokenCounter::estimate_tokens(text);
        assert!(tokens > 0);
        assert!(tokens <= text.len());
    }

    #[test]
    fn test_legacy_estimate_tokens_empty() {
        assert_eq!(TokenCounter::estimate_tokens(""), 0);
    }

    #[test]
    fn test_legacy_estimate_total() {
        let texts: Vec<&str> = vec!["hello", "world", "test"];
        let total = TokenCounter::estimate_total(texts.iter().copied());
        assert!(total >= 3);
        assert!(total <= 10);
    }

    #[test]
    fn test_cached_counter_basic() {
        let counter = CachedTokenCounter::new(TokenProvider::Anthropic);
        let text = "Hello, world! This is a test of the token counter.";
        let tokens = counter.count_tokens(text);
        assert!(tokens > 0);

        // Same text should return same count (cache hit)
        let tokens2 = counter.count_tokens(text);
        assert_eq!(tokens, tokens2);

        // Check cache stats
        let (hits, misses, size) = counter.cache_stats();
        assert_eq!(hits, 1); // Second call was a hit
        assert_eq!(misses, 1); // First call was a miss
        assert_eq!(size, 1);
    }

    #[test]
    fn test_cached_counter_empty() {
        let counter = CachedTokenCounter::default_counter();
        assert_eq!(counter.count_tokens(""), 0);
    }

    #[test]
    fn test_provider_ratios() {
        let text = "The quick brown fox jumps over the lazy dog";

        let anthropic = CachedTokenCounter::new(TokenProvider::Anthropic);
        let openai = CachedTokenCounter::new(TokenProvider::OpenAI);

        // Anthropic has lower chars_per_token (3.5), so higher token count
        let a_tokens = anthropic.count_tokens(text);
        let o_tokens = openai.count_tokens(text);
        assert!(
            a_tokens >= o_tokens,
            "Anthropic ({}) should count >= OpenAI ({})",
            a_tokens,
            o_tokens
        );
    }

    #[test]
    fn test_count_total() {
        let counter = CachedTokenCounter::default_counter();
        let texts: Vec<&str> = vec!["hello", "world"];
        let total = counter.count_total(&texts);
        let sum: usize = texts.iter().map(|t| counter.count_tokens(t)).sum();
        assert_eq!(total, sum);
    }

    #[test]
    fn test_count_chat_tokens() {
        let counter = CachedTokenCounter::default_counter();
        let messages = vec![
            ChatMessageInfo {
                role: "user".to_string(),
                content: "Hello".to_string(),
                tool_content: None,
            },
            ChatMessageInfo {
                role: "assistant".to_string(),
                content: "Hi there!".to_string(),
                tool_content: None,
            },
        ];
        let tokens = counter.count_chat_tokens("You are helpful.", &messages);
        // Should include system + message overhead + content
        assert!(tokens > 10);
    }

    #[test]
    fn test_cache_eviction() {
        let counter = CachedTokenCounter::default_counter();

        // Fill cache beyond MAX_CACHE_SIZE
        for i in 0..(MAX_CACHE_SIZE + 100) {
            let text = format!("unique text number {} with some padding", i);
            counter.count_tokens(&text);
        }

        let (_, _, size) = counter.cache_stats();
        // Cache should not grow beyond MAX_CACHE_SIZE
        assert!(size <= MAX_CACHE_SIZE);
    }

    #[test]
    fn test_clear_cache() {
        let counter = CachedTokenCounter::default_counter();
        counter.count_tokens("some text");
        counter.count_tokens("other text");

        let (_, _, size_before) = counter.cache_stats();
        assert!(size_before > 0);

        counter.clear_cache();
        let (_, _, size_after) = counter.cache_stats();
        assert_eq!(size_after, 0);
    }
}
