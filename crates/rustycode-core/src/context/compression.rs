// ── Context Compression Strategies ──────────────────────────────────────────────

use crate::error::{CoreError, Result};
use tracing::{debug, info};

use super::window::ContextWindow;

/// Strategy for compressing context when it exceeds capacity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
#[non_exhaustive]
pub enum CompressionStrategy {
    /// No compression - fail if content exceeds capacity
    None,
    /// Remove oldest items first
    OldestFirst,
    /// Remove lowest priority items first
    #[default]
    LowestPriorityFirst,
    /// Summarize old items while keeping recent ones
    SummarizeOld,
    /// Truncate less important content
    TruncateLowValue,
    /// Hybrid: combine multiple strategies
    Hybrid,
}

/// Result of a compression operation.
#[derive(Debug, Clone)]
pub struct CompressionResult {
    /// Number of items removed
    pub items_removed: usize,
    /// Number of tokens saved
    pub tokens_saved: usize,
    /// Number of items summarized
    pub items_summarized: usize,
    /// Strategy used
    pub strategy: CompressionStrategy,
}

/// Compress context using the specified strategy.
///
pub fn compress_context(
    window: &mut ContextWindow,
    strategy: CompressionStrategy,
    target_tokens: usize,
) -> Result<CompressionResult> {
    if target_tokens >= window.max_tokens() {
        return Err(CoreError::Validation(format!(
            "Target tokens {} must be less than max capacity {}",
            target_tokens,
            window.max_tokens()
        )));
    }

    let current_usage = window.used_tokens() + window.reserved_tokens();
    if current_usage <= target_tokens {
        // No compression needed
        return Ok(CompressionResult {
            items_removed: 0,
            tokens_saved: 0,
            items_summarized: 0,
            strategy,
        });
    }

    let target_reduction = current_usage.saturating_sub(target_tokens);

    debug!(
        "Compressing context: {} -> {} tokens (reduce by {})",
        current_usage, target_tokens, target_reduction
    );

    let result = match strategy {
        CompressionStrategy::None => {
            return Err(CoreError::Validation(
                "Context exceeds capacity but compression is disabled".to_string(),
            ))
        }
        CompressionStrategy::OldestFirst => compress_oldest_first(window, target_reduction),
        CompressionStrategy::LowestPriorityFirst => {
            compress_lowest_priority(window, target_reduction)
        }
        CompressionStrategy::SummarizeOld => compress_summarize_old(window, target_reduction),
        CompressionStrategy::TruncateLowValue => {
            compress_truncate_low_value(window, target_reduction)
        }
        CompressionStrategy::Hybrid => compress_hybrid(window, target_reduction),
    };

    // Update metadata
    window.metadata.compression_count = window.metadata.compression_count.saturating_add(1);
    window.metadata.tokens_saved = window
        .metadata
        .tokens_saved
        .saturating_add(result.tokens_saved);
    window.touch();

    info!(
        "Compression complete: removed {} items, saved {} tokens",
        result.items_removed, result.tokens_saved
    );

    Ok(result)
}

/// Compress by removing oldest items first.
fn compress_oldest_first(window: &mut ContextWindow, target_reduction: usize) -> CompressionResult {
    // Sort by timestamp (oldest first)
    window.content_mut().sort_by_key(|a| a.metadata.timestamp);

    // Count how many items to remove
    let mut saved = 0;
    let count = window
        .content()
        .iter()
        .take_while(|item| {
            if saved >= target_reduction {
                false
            } else {
                saved += item.token_count;
                true
            }
        })
        .count();

    window.drain_front(count);

    CompressionResult {
        items_removed: count,
        tokens_saved: saved,
        items_summarized: 0,
        strategy: CompressionStrategy::OldestFirst,
    }
}

/// Compress by removing lowest priority items first.
fn compress_lowest_priority(
    window: &mut ContextWindow,
    target_reduction: usize,
) -> CompressionResult {
    // Sort by priority (lowest first)
    window.content_mut().sort_by_key(|a| a.priority);

    // Count how many items to remove
    let mut saved = 0;
    let count = window
        .content()
        .iter()
        .take_while(|item| {
            if saved >= target_reduction {
                false
            } else {
                saved += item.token_count;
                true
            }
        })
        .count();

    window.drain_front(count);

    CompressionResult {
        items_removed: count,
        tokens_saved: saved,
        items_summarized: 0,
        strategy: CompressionStrategy::LowestPriorityFirst,
    }
}

/// Compress by summarizing old items.
fn compress_summarize_old(
    window: &mut ContextWindow,
    _target_reduction: usize,
) -> CompressionResult {
    let mut summarized = 0;

    // Separate old and recent items (older than 1 hour)
    let now = chrono::Utc::now();
    let old_threshold = now - chrono::Duration::hours(1);

    // Collect old items and their indices
    let old_indices: Vec<_> = window
        .content()
        .iter()
        .enumerate()
        .filter(|(_, item)| item.metadata.timestamp.is_some_and(|ts| ts < old_threshold))
        .map(|(idx, _)| idx)
        .collect();

    let mut total_saved = 0;

    // Process old items (in reverse to avoid index shifting)
    for idx in old_indices.into_iter().rev() {
        if idx < window.content().len() {
            let item = &mut window.content_mut()[idx];

            // Simple summarization: truncate to 20% of original
            let original_len = item.content.len();
            let summary_len = (original_len / 5).max(50); // At least 50 chars
            let summary = format!(
                "[Summary] {}...",
                &item.content.chars().take(summary_len).collect::<String>()
            );

            let tokens_before = item.token_count;
            item.content = summary;
            item.token_count = crate::context::TokenCounter::estimate_tokens(&item.content);

            let tokens_saved_this = tokens_before.saturating_sub(item.token_count);
            total_saved += tokens_saved_this;
            summarized += 1;
        }
    }

    // Recalculate used tokens
    let new_used: usize = window.content().iter().map(|item| item.token_count).sum();
    window.set_used_tokens(new_used);

    CompressionResult {
        items_removed: 0,
        tokens_saved: total_saved,
        items_summarized: summarized,
        strategy: CompressionStrategy::SummarizeOld,
    }
}

/// Compress by truncating low-value content.
fn compress_truncate_low_value(
    window: &mut ContextWindow,
    target_reduction: usize,
) -> CompressionResult {
    let mut removed = 0;
    let mut saved = 0;

    // Collect indices and scores, then sort
    let mut indexed: Vec<_> = window
        .content()
        .iter()
        .enumerate()
        .map(|(idx, item)| (idx, item.score_per_token()))
        .collect();

    indexed.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

    // Remove low-efficiency items (in reverse to avoid index shifting)
    for (idx, _efficiency) in indexed.into_iter().rev() {
        if saved >= target_reduction {
            break;
        }

        if idx < window.content().len() {
            let tokens = window.content()[idx].token_count;
            window.remove(idx).ok();
            saved += tokens;
            removed += 1;
        }
    }

    CompressionResult {
        items_removed: removed,
        tokens_saved: saved,
        items_summarized: 0,
        strategy: CompressionStrategy::TruncateLowValue,
    }
}

/// Compress using a hybrid approach.
fn compress_hybrid(window: &mut ContextWindow, target_reduction: usize) -> CompressionResult {
    // Try strategies in order until we meet the target
    let strategies = [
        CompressionStrategy::LowestPriorityFirst,
        CompressionStrategy::OldestFirst,
        CompressionStrategy::TruncateLowValue,
    ];

    for strategy in strategies {
        if let Ok(r) = compress_context(window, strategy, target_reduction) {
            if r.tokens_saved >= target_reduction {
                return r;
            }
        }
    }

    // If all else fails, use summarize old as last resort
    compress_summarize_old(window, target_reduction)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::Priority;

    fn create_test_window(max_tokens: usize) -> ContextWindow {
        let mut window = ContextWindow::new(max_tokens);

        // Add substantial content
        let chunk = "x".repeat(500);
        let _ = window.add_content(&format!("critical {}", chunk), Some(Priority::Critical));
        let _ = window.add_content(&format!("high {}", chunk), Some(Priority::High));
        let _ = window.add_content(&format!("medium {}", chunk), Some(Priority::Medium));
        let _ = window.add_content(&format!("low {}", chunk), Some(Priority::Low));
        let _ = window.add_content(&format!("minimal {}", chunk), Some(Priority::Minimal));

        window
    }

    #[test]
    fn test_compress_oldest_first() {
        let mut window = create_test_window(1000);
        let result = compress_context(&mut window, CompressionStrategy::OldestFirst, 500).unwrap();

        assert!(result.tokens_saved > 0);
    }

    #[test]
    fn test_compress_lowest_priority() {
        let mut window = create_test_window(1000);
        let result =
            compress_context(&mut window, CompressionStrategy::LowestPriorityFirst, 500).unwrap();

        assert!(result.tokens_saved > 0);
    }

    #[test]
    fn test_compress_truncate_low_value() {
        let mut window = create_test_window(1000);
        let result =
            compress_context(&mut window, CompressionStrategy::TruncateLowValue, 500).unwrap();

        assert!(result.tokens_saved > 0);
    }

    #[test]
    fn test_compress_none_fails_when_full() {
        let mut window = create_test_window(1000);
        let result = compress_context(&mut window, CompressionStrategy::None, 500);

        assert!(result.is_err());
    }
}
