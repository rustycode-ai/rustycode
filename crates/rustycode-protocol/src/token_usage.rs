//! Token usage tracking for LLM consumption across agent hierarchy.
//! Supports aggregation across sub-agents and teams via saturating arithmetic.

use serde::{Deserialize, Serialize};

/// Cumulative token usage from LLM API calls.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct TokenUsage {
    /// Total input tokens sent.
    pub input_tokens: u64,
    /// Total output tokens received.
    pub output_tokens: u64,
    /// Tokens read from prompt cache.
    pub cache_read_tokens: u64,
    /// Tokens written to prompt cache.
    pub cache_creation_tokens: u64,
}

impl TokenUsage {
    /// Zero usage.
    #[must_use]
    pub const fn zero() -> Self {
        Self {
            input_tokens: 0,
            output_tokens: 0,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
        }
    }

    /// Total tokens consumed (input + output).
    #[must_use]
    pub const fn total(&self) -> u64 {
        self.input_tokens.saturating_add(self.output_tokens)
    }

    /// Effective tokens (input - cache_read, since cached tokens are cheaper).
    #[must_use]
    pub const fn effective_input(&self) -> u64 {
        self.input_tokens.saturating_sub(self.cache_read_tokens)
    }

    /// Add usage from another source (saturating, never overflows).
    #[must_use]
    pub const fn saturating_add(self, other: Self) -> Self {
        Self {
            input_tokens: self.input_tokens.saturating_add(other.input_tokens),
            output_tokens: self.output_tokens.saturating_add(other.output_tokens),
            cache_read_tokens: self
                .cache_read_tokens
                .saturating_add(other.cache_read_tokens),
            cache_creation_tokens: self
                .cache_creation_tokens
                .saturating_add(other.cache_creation_tokens),
        }
    }
}

impl Default for TokenUsage {
    fn default() -> Self {
        Self::zero()
    }
}

impl std::iter::Sum for TokenUsage {
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        iter.fold(Self::zero(), |acc, item| acc.saturating_add(item))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_usage() {
        let u = TokenUsage::zero();
        assert_eq!(u.input_tokens, 0);
        assert_eq!(u.output_tokens, 0);
        assert_eq!(u.total(), 0);
    }

    #[test]
    fn saturating_add_no_overflow() {
        let a = TokenUsage {
            input_tokens: u64::MAX,
            output_tokens: 100,
            cache_read_tokens: u64::MAX,
            cache_creation_tokens: 50,
        };
        let b = TokenUsage {
            input_tokens: 1,
            output_tokens: 200,
            cache_read_tokens: 1,
            cache_creation_tokens: 100,
        };
        let result = a.saturating_add(b);
        assert_eq!(result.input_tokens, u64::MAX); // saturated
        assert_eq!(result.output_tokens, 300);
        assert_eq!(result.cache_read_tokens, u64::MAX); // saturated
        assert_eq!(result.cache_creation_tokens, 150);
    }

    #[test]
    fn total_is_input_plus_output() {
        let u = TokenUsage {
            input_tokens: 1000,
            output_tokens: 500,
            cache_read_tokens: 200,
            cache_creation_tokens: 100,
        };
        assert_eq!(u.total(), 1500);
    }

    #[test]
    fn effective_input_subtracts_cache() {
        let u = TokenUsage {
            input_tokens: 1000,
            output_tokens: 500,
            cache_read_tokens: 300,
            cache_creation_tokens: 100,
        };
        assert_eq!(u.effective_input(), 700);
    }

    #[test]
    fn sum_iterator() {
        let usages = vec![
            TokenUsage {
                input_tokens: 100,
                output_tokens: 50,
                cache_read_tokens: 10,
                cache_creation_tokens: 5,
            },
            TokenUsage {
                input_tokens: 200,
                output_tokens: 80,
                cache_read_tokens: 20,
                cache_creation_tokens: 10,
            },
        ];
        let total: TokenUsage = usages.into_iter().sum();
        assert_eq!(total.input_tokens, 300);
        assert_eq!(total.output_tokens, 130);
    }

    #[test]
    fn serialization_round_trip() {
        let u = TokenUsage {
            input_tokens: 1234,
            output_tokens: 567,
            cache_read_tokens: 89,
            cache_creation_tokens: 12,
        };
        let json = serde_json::to_string(&u).unwrap();
        let back: TokenUsage = serde_json::from_str(&json).unwrap();
        assert_eq!(back.input_tokens, u.input_tokens);
        assert_eq!(back.output_tokens, u.output_tokens);
    }
}
