// ── Auto-Compact: LLM-Backed Context Compression ───────────────────────────────
//
// Provides automatic context compaction when the context window exceeds a
// configurable usage threshold. Old items are summarized by the LLM into a
// single compact context item, preserving essential information while freeing
// token budget.

use crate::context_management::compression::{CompressionResult, CompressionStrategy};
use crate::context_management::window::ContextWindow;
use crate::context_prio::{ContextItem, Priority};
use crate::error::{CoreError, Result};
use chrono::Utc;
use std::time::Instant;
use tracing::{debug, info, warn};

// ── Constants ──────────────────────────────────────────────────────────────────

/// Default usage threshold (80%) above which auto-compaction triggers.
pub const DEFAULT_COMPACT_THRESHOLD: f64 = 0.80;

/// Target usage after compaction (60%), leaving headroom for new content.
const POST_COMPACT_TARGET: f64 = 0.60;

/// Maximum input characters sent to the LLM for summarization.
/// Caps the summarization prompt to avoid excessive token spend.
const MAX_SUMMARIZATION_INPUT_CHARS: usize = 30_000;

/// The system prompt instructing the LLM to summarize conversation history.
const SUMMARIZATION_SYSTEM_PROMPT: &str = "\
You are a context compaction assistant. Your job is to produce a concise, \
information-dense summary of the conversation history provided by the user. \
Preserve all factual details that would be needed to continue the task: \
file paths, function names, error messages, decisions made, partial progress, \
and any unresolved issues. Omit pleasantries and redundant explanations. \
Use bullet points. Keep the summary under 400 words.";

// ── Types ──────────────────────────────────────────────────────────────────────

/// Metrics for a single compaction event.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CompactionEvent {
    /// Timestamp of the compaction event.
    pub timestamp: chrono::DateTime<Utc>,
    /// Strategy used for compaction.
    pub strategy: CompressionStrategy,
    /// Tokens before compaction.
    pub tokens_before: usize,
    /// Tokens after compaction.
    pub tokens_after: usize,
    /// Tokens saved by compaction.
    pub tokens_saved: usize,
    /// Number of items removed.
    pub items_removed: usize,
    /// Number of items summarized into the summary.
    pub items_summarized: usize,
    /// Wall-clock duration of the compaction in milliseconds.
    pub duration_ms: u64,
    /// Whether LLM summarization was used (vs. fallback truncation).
    pub used_llm: bool,
}

/// Accumulated metrics across all compaction events in a session.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct CompactionMetrics {
    /// Total number of compaction events.
    pub total_compactions: usize,
    /// Total tokens saved across all events.
    pub total_tokens_saved: usize,
    /// Total items removed across all events.
    pub total_items_removed: usize,
    /// Total items summarized across all events.
    pub total_items_summarized: usize,
    /// Number of compactions that used LLM summarization.
    pub llm_compactions: usize,
    /// Number of compactions that fell back to truncation.
    pub fallback_compactions: usize,
    /// Individual events (capped to the most recent 50).
    pub events: Vec<CompactionEvent>,
}

impl CompactionMetrics {
    /// Maximum number of individual events to retain.
    const MAX_EVENTS: usize = 50;

    /// Record a new compaction event.
    pub fn record(&mut self, event: CompactionEvent) {
        self.total_compactions = self.total_compactions.saturating_add(1);
        self.total_tokens_saved = self.total_tokens_saved.saturating_add(event.tokens_saved);
        self.total_items_removed = self.total_items_removed.saturating_add(event.items_removed);
        self.total_items_summarized = self
            .total_items_summarized
            .saturating_add(event.items_summarized);

        if event.used_llm {
            self.llm_compactions = self.llm_compactions.saturating_add(1);
        } else {
            self.fallback_compactions = self.fallback_compactions.saturating_add(1);
        }

        self.events.push(event);
        if self.events.len() > Self::MAX_EVENTS {
            self.events.remove(0);
        }
    }

    /// Average tokens saved per compaction event.
    pub fn avg_tokens_saved(&self) -> f64 {
        if self.total_compactions == 0 {
            0.0
        } else {
            self.total_tokens_saved as f64 / self.total_compactions as f64
        }
    }
}

// ── Core Functions ─────────────────────────────────────────────────────────────

/// Determine whether the context window exceeds the compaction threshold.
///
/// Returns `true` when `(used + reserved) / max >= threshold`.
pub fn should_compact(window: &ContextWindow, threshold: f64) -> bool {
    let usage = window.usage_percentage();
    debug!(
        "Compact check: usage={:.1}%, threshold={:.0}%",
        usage * 100.0,
        threshold * 100.0
    );
    usage >= threshold
}

/// Check compaction need using the default threshold (80%).
pub fn should_compact_default(window: &ContextWindow) -> bool {
    should_compact(window, DEFAULT_COMPACT_THRESHOLD)
}

/// Run auto-compaction if the context window exceeds the threshold.
///
/// When triggered, old items are collected, sent to the LLM for summarization,
/// and replaced by a single summary context item. If the LLM call fails, the
/// function falls back to simple truncation of old items.
///
/// # Arguments
///
/// * `window` - Mutable reference to the context window.
/// * `provider` - LLM provider used for summarization.
/// * `model` - Model name to use for the summarization request.
/// * `metrics` - Optional metrics accumulator for tracking compaction events.
///
/// # Returns
///
/// `Ok(Some(CompressionResult))` if compaction was performed,
/// `Ok(None)` if compaction was not needed.
pub async fn auto_compact_if_needed(
    window: &mut ContextWindow,
    provider: &dyn rustycode_llm::provider::LLMProvider,
    model: &str,
    metrics: Option<&mut CompactionMetrics>,
) -> Result<Option<CompressionResult>> {
    auto_compact_with_threshold(window, provider, model, DEFAULT_COMPACT_THRESHOLD, metrics).await
}

/// Run auto-compaction with a custom threshold.
///
/// See [`auto_compact_if_needed`] for full documentation.
pub async fn auto_compact_with_threshold(
    window: &mut ContextWindow,
    provider: &dyn rustycode_llm::provider::LLMProvider,
    model: &str,
    threshold: f64,
    metrics: Option<&mut CompactionMetrics>,
) -> Result<Option<CompressionResult>> {
    if !should_compact(window, threshold) {
        return Ok(None);
    }

    let start = Instant::now();
    let tokens_before = window.used_tokens() + window.reserved_tokens();
    let item_count_before = window.len();

    info!(
        "Auto-compact triggered: usage={:.1}%, items={}, tokens={}",
        window.usage_percentage() * 100.0,
        item_count_before,
        tokens_before,
    );

    let target_tokens = (window.max_tokens() as f64 * POST_COMPACT_TARGET) as usize;
    let result = compact_with_llm(window, provider, model, target_tokens).await;

    let duration = start.elapsed();

    let (result, used_llm) = match result {
        Ok(r) => (r, true),
        Err(e) => {
            warn!(
                "LLM summarization failed ({}), falling back to truncation",
                e
            );
            let fallback_target = (window.max_tokens() as f64 * POST_COMPACT_TARGET) as usize;
            let r = crate::context_management::compression::compress_context(
                window,
                CompressionStrategy::OldestFirst,
                fallback_target,
            )?;
            (r, false)
        }
    };

    let tokens_after_final = window.used_tokens() + window.reserved_tokens();
    let tokens_saved = tokens_before.saturating_sub(tokens_after_final);
    let items_removed = item_count_before.saturating_sub(window.len());

    info!(
        "Auto-compact complete: strategy={:?}, llm={}, saved={} tokens, removed={} items, duration={}ms",
        result.strategy,
        used_llm,
        tokens_saved,
        items_removed,
        duration.as_millis(),
    );

    if let Some(metrics) = metrics {
        let event = CompactionEvent {
            timestamp: Utc::now(),
            strategy: result.strategy,
            tokens_before,
            tokens_after: tokens_after_final,
            tokens_saved,
            items_removed,
            items_summarized: result.items_summarized,
            duration_ms: duration.as_millis() as u64,
            used_llm,
        };
        metrics.record(event);
    }

    Ok(Some(result))
}

/// Perform LLM-backed summarization of old context items.
///
/// Collects items older than the cutoff, concatenates their content, sends it
/// to the LLM for summarization, and replaces the old items with a single
/// summary item.
async fn compact_with_llm(
    window: &mut ContextWindow,
    provider: &dyn rustycode_llm::provider::LLMProvider,
    model: &str,
    _target_tokens: usize,
) -> Result<CompressionResult> {
    let now = Utc::now();
    // Items older than 30 minutes are candidates for summarization.
    let cutoff = now - chrono::Duration::minutes(30);

    // Identify old items by index.
    let old_indices: Vec<usize> = window
        .content()
        .iter()
        .enumerate()
        .filter(|(_, item)| item.metadata.timestamp.is_some_and(|ts| ts < cutoff))
        .map(|(idx, _)| idx)
        .collect();

    if old_indices.is_empty() {
        debug!("No old items to summarize; nothing to compact");
        return Ok(CompressionResult {
            items_removed: 0,
            tokens_saved: 0,
            items_summarized: 0,
            strategy: CompressionStrategy::SummarizeOld,
        });
    }

    // Collect the text of old items for the summarization prompt.
    let mut combined_text = String::new();
    let mut tokens_in_old: usize = 0;
    for &idx in &old_indices {
        if idx < window.content().len() {
            let item = &window.content()[idx];
            combined_text.push_str("---\n");
            combined_text.push_str(&item.content);
            combined_text.push('\n');
            tokens_in_old = tokens_in_old.saturating_add(item.token_count);
        }
    }

    // Cap the input to avoid excessive LLM spend.
    let combined_text = if combined_text.len() > MAX_SUMMARIZATION_INPUT_CHARS {
        let mut truncated: String = combined_text
            .chars()
            .take(MAX_SUMMARIZATION_INPUT_CHARS)
            .collect();
        truncated.push_str("\n[... content truncated for summarization ...]");
        truncated
    } else {
        combined_text
    };

    debug!(
        "Summarizing {} old items ({} tokens, {} chars)",
        old_indices.len(),
        tokens_in_old,
        combined_text.len(),
    );

    // Build the LLM request.
    let request = rustycode_llm::provider::CompletionRequest::new(
        model.to_string(),
        vec![rustycode_llm::provider::ChatMessage::user(format!(
            "Summarize the following conversation history into a compact summary \
             that preserves all essential details:\n\n{}",
            combined_text
        ))],
    )
    .with_system_prompt(SUMMARIZATION_SYSTEM_PROMPT.to_string())
    .with_max_tokens(1024)
    .with_temperature(0.0);

    let response = provider
        .complete(request)
        .await
        .map_err(|e| CoreError::Internal(format!("LLM summarization failed: {}", e)))?;

    let summary_text = response.content.trim().to_string();

    if summary_text.is_empty() {
        return Err(CoreError::Internal(
            "LLM returned empty summarization response".to_string(),
        ));
    }

    // Remove old items in reverse order to preserve indices.
    for &idx in old_indices.iter().rev() {
        if idx < window.content().len() {
            window.remove(idx).ok();
        }
    }

    // Insert the summary as a single high-priority item at the beginning.
    let summary_content = format!("[Conversation Summary]\n{}\n[End Summary]", summary_text);
    let summary_item = ContextItem::new(summary_content, Priority::High)
        .with_timestamp(now)
        .with_id("compaction-summary");

    let summary_tokens = summary_item.token_count;
    window.add_item(summary_item)?;

    // Recalculate total used tokens.
    let new_used: usize = window.content().iter().map(|item| item.token_count).sum();
    window.set_used_tokens(new_used);

    let tokens_saved = tokens_in_old.saturating_sub(summary_tokens);

    Ok(CompressionResult {
        items_removed: old_indices.len().saturating_sub(1), // replaced by 1 summary
        tokens_saved,
        items_summarized: old_indices.len(),
        strategy: CompressionStrategy::SummarizeOld,
    })
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context_prio::Priority;
    use futures::Stream;
    use rustycode_llm::provider::{
        CompletionRequest, CompletionResponse, LLMProvider, ProviderConfig, Usage,
    };
    use std::pin::Pin;

    /// Mock LLM provider that returns a canned summary.
    struct MockSummarizingProvider {
        config: ProviderConfig,
        response_content: String,
    }

    impl MockSummarizingProvider {
        fn new(response: &str) -> Self {
            Self {
                config: ProviderConfig::default(),
                response_content: response.to_string(),
            }
        }
    }

    #[async_trait::async_trait]
    impl LLMProvider for MockSummarizingProvider {
        fn name(&self) -> &'static str {
            "mock-summarizer"
        }

        async fn is_available(&self) -> bool {
            true
        }

        async fn list_models(
            &self,
        ) -> std::result::Result<Vec<String>, rustycode_llm::provider::ProviderError> {
            Ok(vec!["mock-model".to_string()])
        }

        async fn complete(
            &self,
            request: CompletionRequest,
        ) -> std::result::Result<CompletionResponse, rustycode_llm::provider::ProviderError>
        {
            Ok(CompletionResponse {
                content: self.response_content.clone(),
                model: request.model,
                usage: Some(Usage::new(100, 50)),
                stop_reason: None,
                citations: Some(Vec::new()),
                thinking_blocks: None,
                structured_output: None,
            })
        }

        async fn complete_stream(
            &self,
            _request: CompletionRequest,
        ) -> std::result::Result<
            Pin<Box<dyn Stream<Item = rustycode_llm::provider::StreamChunk> + Send>>,
            rustycode_llm::provider::ProviderError,
        > {
            Err(rustycode_llm::provider::ProviderError::Configuration(
                "stream not implemented".to_string(),
            ))
        }

        fn config(&self) -> Option<&ProviderConfig> {
            Some(&self.config)
        }
    }

    /// Build a window with old timestamped items that are summarization candidates.
    fn build_window_with_old_items(max_tokens: usize, item_count: usize) -> ContextWindow {
        let mut window = ContextWindow::new(max_tokens);
        let old_time = Utc::now() - chrono::Duration::hours(2);

        for i in 0..item_count {
            let content = format!(
                "Old conversation turn {} with some content about file_{}.rs and function_{}().",
                i, i, i
            );
            let item = ContextItem::new(content, Priority::Medium)
                .with_timestamp(old_time)
                .with_id(format!("old-item-{}", i));
            window.push_item_unchecked(item);
        }

        // Add one recent item that should NOT be summarized.
        let recent_content = "Recent work that must be preserved intact.".to_string();
        let recent_item = ContextItem::new(recent_content, Priority::High)
            .with_timestamp(Utc::now())
            .with_id("recent-item");
        window.push_item_unchecked(recent_item);

        window
    }

    #[test]
    fn test_should_compact_below_threshold() {
        let mut window = ContextWindow::new(10_000);
        window.add_content("small content", None).unwrap();
        assert!(!should_compact_default(&window));
    }

    #[test]
    fn test_should_compact_above_threshold() {
        let mut window = ContextWindow::new(100);
        window.reserve(50).unwrap();
        // Simulate a window that grew past threshold.
        window.set_used_tokens(81);
        window.push_item_unchecked(ContextItem::new("x".repeat(400), Priority::Medium));
        assert!(should_compact_default(&window));
    }

    #[test]
    fn test_should_compact_custom_threshold() {
        let mut window = ContextWindow::new(100);
        window.reserve(50).unwrap();
        window.set_used_tokens(10);
        // 60% usage, below the default 80% threshold.
        assert!(!should_compact_default(&window));
        // But above a 50% threshold.
        assert!(should_compact(&window, 0.50));
    }

    #[test]
    fn test_compaction_metrics_record() {
        let mut metrics = CompactionMetrics::default();
        let event = CompactionEvent {
            timestamp: Utc::now(),
            strategy: CompressionStrategy::SummarizeOld,
            tokens_before: 1000,
            tokens_after: 400,
            tokens_saved: 600,
            items_removed: 5,
            items_summarized: 6,
            duration_ms: 250,
            used_llm: true,
        };
        metrics.record(event);

        assert_eq!(metrics.total_compactions, 1);
        assert_eq!(metrics.total_tokens_saved, 600);
        assert_eq!(metrics.total_items_removed, 5);
        assert_eq!(metrics.llm_compactions, 1);
        assert_eq!(metrics.fallback_compactions, 0);
        assert_eq!(metrics.events.len(), 1);
    }

    #[test]
    fn test_compaction_metrics_avg() {
        let mut metrics = CompactionMetrics::default();
        metrics.record(CompactionEvent {
            timestamp: Utc::now(),
            strategy: CompressionStrategy::SummarizeOld,
            tokens_before: 1000,
            tokens_after: 400,
            tokens_saved: 600,
            items_removed: 5,
            items_summarized: 5,
            duration_ms: 100,
            used_llm: true,
        });
        metrics.record(CompactionEvent {
            timestamp: Utc::now(),
            strategy: CompressionStrategy::SummarizeOld,
            tokens_before: 800,
            tokens_after: 300,
            tokens_saved: 500,
            items_removed: 3,
            items_summarized: 3,
            duration_ms: 150,
            used_llm: true,
        });
        // Average: (600 + 500) / 2 = 550
        assert!((metrics.avg_tokens_saved() - 550.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_compaction_metrics_event_cap() {
        let mut metrics = CompactionMetrics::default();
        for i in 0..60 {
            metrics.record(CompactionEvent {
                timestamp: Utc::now(),
                strategy: CompressionStrategy::SummarizeOld,
                tokens_before: 100,
                tokens_after: 50,
                tokens_saved: 50,
                items_removed: 1,
                items_summarized: 1,
                duration_ms: i as u64,
                used_llm: true,
            });
        }
        assert_eq!(metrics.events.len(), CompactionMetrics::MAX_EVENTS);
        assert_eq!(metrics.total_compactions, 60);
    }

    #[tokio::test]
    async fn test_auto_compact_not_needed() {
        let provider = MockSummarizingProvider::new("summary");
        let mut window = ContextWindow::new(10_000);
        window.add_content("small content", None).unwrap();

        let result = auto_compact_if_needed(&mut window, &provider, "mock-model", None)
            .await
            .unwrap();

        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_auto_compact_with_llm_summarization() {
        let provider = MockSummarizingProvider::new(
            "Summary of past work:\n- Explored files file_0.rs through file_4.rs\n- Key functions identified",
        );
        let mut window = build_window_with_old_items(50_000, 5);

        assert_eq!(window.content().len(), 6);

        // Force compaction by inflating usage past 80%.
        let extra = 40_000;
        window.set_used_tokens(window.used_tokens().saturating_add(extra));

        let mut metrics = CompactionMetrics::default();
        let result =
            auto_compact_if_needed(&mut window, &provider, "mock-model", Some(&mut metrics))
                .await
                .unwrap();

        assert!(result.is_some());
        let r = result.unwrap();
        assert!(r.items_summarized > 0, "should have summarized old items");
        assert_eq!(metrics.total_compactions, 1);
        assert_eq!(metrics.llm_compactions, 1);

        // Recent item should still be present.
        let has_recent = window
            .content()
            .iter()
            .any(|item| item.content.contains("Recent work"));
        assert!(has_recent, "recent item must survive compaction");
    }

    #[tokio::test]
    async fn test_auto_compact_fallback_on_llm_failure() {
        struct FailingProvider {
            config: ProviderConfig,
        }

        #[async_trait::async_trait]
        impl LLMProvider for FailingProvider {
            fn name(&self) -> &'static str {
                "failing"
            }
            async fn is_available(&self) -> bool {
                false
            }
            async fn list_models(
                &self,
            ) -> std::result::Result<Vec<String>, rustycode_llm::provider::ProviderError>
            {
                Ok(vec![])
            }
            async fn complete(
                &self,
                _request: CompletionRequest,
            ) -> std::result::Result<CompletionResponse, rustycode_llm::provider::ProviderError>
            {
                Err(rustycode_llm::provider::ProviderError::Api(
                    "intentional failure".to_string(),
                ))
            }
            async fn complete_stream(
                &self,
                _request: CompletionRequest,
            ) -> std::result::Result<
                Pin<Box<dyn Stream<Item = rustycode_llm::provider::StreamChunk> + Send>>,
                rustycode_llm::provider::ProviderError,
            > {
                Err(rustycode_llm::provider::ProviderError::Configuration(
                    "stream not implemented".to_string(),
                ))
            }
            fn config(&self) -> Option<&ProviderConfig> {
                Some(&self.config)
            }
        }

        let provider = FailingProvider {
            config: ProviderConfig::default(),
        };
        let mut window = ContextWindow::new(1000);
        let old_time = Utc::now() - chrono::Duration::hours(2);

        // Add several old items directly, bypassing capacity check.
        for i in 0..5 {
            let content = "x".repeat(200);
            let item = ContextItem::new(format!("{}-{}", content, i), Priority::Medium)
                .with_timestamp(old_time);
            window.push_item_unchecked(item);
        }

        // Add a recent item.
        let recent = ContextItem::new("recent content".to_string(), Priority::High)
            .with_timestamp(Utc::now());
        window.push_item_unchecked(recent);

        // Inflate usage to push past the 80% threshold so compaction triggers.
        window.set_used_tokens(900);

        let mut metrics = CompactionMetrics::default();
        let result =
            auto_compact_if_needed(&mut window, &provider, "mock-model", Some(&mut metrics))
                .await
                .unwrap();

        assert!(result.is_some());
        assert_eq!(metrics.fallback_compactions, 1);
        assert_eq!(metrics.llm_compactions, 0);
    }

    #[test]
    fn test_should_compact_empty_window() {
        let window = ContextWindow::new(10_000);
        assert!(!should_compact_default(&window));
    }

    #[test]
    fn test_should_compact_zero_max_tokens() {
        let window = ContextWindow::new(0);
        assert!(!should_compact_default(&window));
    }
}
