//! Token usage tracking for LLM API calls.
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Per-model usage statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModelUsage {
    pub model: String,
    pub total_tokens: u64,
    pub request_count: u64,
    pub total_cost_usd: f64,
    pub avg_tokens_per_request: f64,
}

/// Per-provider usage statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderUsage {
    pub provider_type: String,
    pub total_tokens: u64,
    pub request_count: u64,
    pub total_cost_usd: f64,
    pub by_model: HashMap<String, ModelUsage>,
}

/// Aggregated usage across all models and providers
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UsageSummary {
    pub by_model: HashMap<String, ModelUsage>,
    pub by_provider: HashMap<String, ProviderUsage>,
    pub total_tokens: u64,
    pub total_requests: u64,
    pub total_cost_usd: f64,
}

/// A single tracked request record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackedRequest {
    pub provider_type: String,
    pub model: String,
    pub tokens_used: u64,
    pub cost_usd: f64,
    pub timestamp_secs: u64,
    pub duration_ms: u64,
}

/// Cost per 1M input tokens by model (approximate, April 2026)
pub fn cost_per_million_tokens(model: &str) -> f64 {
    cost_per_million_tokens_io(model).0
}

/// Cost per 1M tokens (input, output) by model (approximate, April 2026)
///
/// Returns `(input_cost, output_cost)` per million tokens.
/// Inspired by Goose's `estimate_cost_usd` pattern.
pub fn cost_per_million_tokens_io(model: &str) -> (f64, f64) {
    // Claude 4.x models (latest)
    if model.contains("claude-opus-4") {
        (15.0, 75.0)
    } else if model.contains("claude-sonnet-4") {
        (3.0, 15.0)
    } else if model.contains("claude-haiku-4") {
        (0.80, 4.0)
    }
    // Claude 3.x models
    else if model.starts_with("claude-3-opus") {
        (15.0, 75.0)
    } else if model.starts_with("claude-3-7-sonnet") || model.starts_with("claude-3-5-sonnet") {
        (3.0, 15.0)
    } else if model.starts_with("claude-3") {
        (0.25, 1.25)
    }
    // GPT-4o series
    else if model.starts_with("gpt-4o") {
        (2.5, 10.0)
    }
    // o3/o1 series (reasoning models)
    else if model.starts_with("o3") {
        (10.0, 40.0)
    } else if model.starts_with("o1") {
        (15.0, 60.0)
    }
    // GPT-4.x legacy
    else if model.starts_with("gpt-4") {
        (30.0, 60.0)
    } else if model.starts_with("gpt-3.5") {
        (0.50, 1.50)
    }
    // Gemini models
    else if model.starts_with("gemini-2.5-pro") {
        (1.25, 10.0)
    } else if model.starts_with("gemini-2") || model.starts_with("gemini-1.5-pro") {
        (1.25, 5.0)
    } else if model.starts_with("gemini") {
        (0.075, 0.30)
    }
    // Local models (ollama, etc.) are free
    else {
        (0.0, 0.0)
    }
}

/// Estimate cost in USD for a given number of input/output tokens
pub fn estimate_cost(model: &str, input_tokens: usize, output_tokens: usize) -> f64 {
    let (input_cost, output_cost) = cost_per_million_tokens_io(model);
    (input_tokens as f64 / 1_000_000.0) * input_cost
        + (output_tokens as f64 / 1_000_000.0) * output_cost
}

#[derive(Debug, Default)]
struct TrackerInner {
    history: Vec<TrackedRequest>,
}

/// Thread-safe token usage tracker with lock-free counters
#[derive(Debug, Clone)]
pub struct TokenTracker {
    inner: Arc<Mutex<TrackerInner>>,
    // Lock-free atomic counters for hot metrics
    total_tokens: Arc<AtomicU64>,
    total_cost_cents: Arc<AtomicU64>, // Store as cents to avoid f64 atomic issues
    request_count: Arc<AtomicU64>,
}

impl TokenTracker {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(TrackerInner::default())),
            total_tokens: Arc::new(AtomicU64::new(0)),
            total_cost_cents: Arc::new(AtomicU64::new(0)),
            request_count: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Record a completed LLM request (lock-free for counters)
    pub fn record(&self, provider_type: &str, model: &str, tokens: u64, duration_ms: u64) {
        let cost = (tokens as f64 / 1_000_000.0) * cost_per_million_tokens(model);
        let cost_cents = (cost * 100.0) as u64;
        let timestamp_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_secs();

        // Lock-free counter updates (Ordering::Relaxed is OK for counters)
        self.total_tokens.fetch_add(tokens, Ordering::Relaxed);
        self.total_cost_cents
            .fetch_add(cost_cents, Ordering::Relaxed);
        self.request_count.fetch_add(1, Ordering::Relaxed);

        // Only lock for history append (rare operation)
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        inner.history.push(TrackedRequest {
            provider_type: provider_type.to_string(),
            model: model.to_string(),
            tokens_used: tokens,
            cost_usd: cost,
            timestamp_secs,
            duration_ms,
        });

        // Cap history to prevent unbounded memory growth
        const MAX_HISTORY: usize = 10_000;
        let len = inner.history.len();
        if len > MAX_HISTORY {
            inner.history.drain(0..len - MAX_HISTORY);
        }
    }

    /// Get aggregated usage summary
    pub fn summary(&self) -> UsageSummary {
        let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let mut by_model: HashMap<String, ModelUsage> = HashMap::new();
        let mut by_provider: HashMap<String, ProviderUsage> = HashMap::new();

        // Use lock-free counters for totals (no need to recalculate from history)
        let total_tokens = self.total_tokens.load(Ordering::Relaxed);
        let total_requests = self.request_count.load(Ordering::Relaxed);
        let total_cost = self.total_cost_cents.load(Ordering::Relaxed) as f64 / 100.0;

        // Only calculate per-model and per-provider breakdowns from history
        for req in &inner.history {
            // By model tracking
            let entry = by_model
                .entry(req.model.clone())
                .or_insert_with(|| ModelUsage {
                    model: req.model.clone(),
                    ..Default::default()
                });
            entry.total_tokens += req.tokens_used;
            entry.request_count += 1;
            entry.total_cost_usd += req.cost_usd;

            // By provider tracking
            let provider_entry =
                by_provider
                    .entry(req.provider_type.clone())
                    .or_insert_with(|| ProviderUsage {
                        provider_type: req.provider_type.clone(),
                        total_tokens: 0,
                        request_count: 0,
                        total_cost_usd: 0.0,
                        by_model: HashMap::new(),
                    });
            provider_entry.total_tokens += req.tokens_used;
            provider_entry.request_count += 1;
            provider_entry.total_cost_usd += req.cost_usd;

            let model_entry = provider_entry
                .by_model
                .entry(req.model.clone())
                .or_insert_with(|| ModelUsage {
                    model: req.model.clone(),
                    ..Default::default()
                });
            model_entry.total_tokens += req.tokens_used;
            model_entry.request_count += 1;
            model_entry.total_cost_usd += req.cost_usd;
        }

        for usage in by_model.values_mut() {
            if usage.request_count > 0 {
                usage.avg_tokens_per_request =
                    usage.total_tokens as f64 / usage.request_count as f64;
            }
        }

        for provider in by_provider.values_mut() {
            for model_usage in provider.by_model.values_mut() {
                if model_usage.request_count > 0 {
                    model_usage.avg_tokens_per_request =
                        model_usage.total_tokens as f64 / model_usage.request_count as f64;
                }
            }
        }

        UsageSummary {
            by_model,
            by_provider,
            total_tokens,
            total_requests,
            total_cost_usd: total_cost,
        }
    }

    /// Get the N most recent requests
    pub fn recent(&self, n: usize) -> Vec<TrackedRequest> {
        let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        inner.history.iter().rev().take(n).cloned().collect()
    }

    /// Reset all tracking data
    pub fn reset(&self) {
        // Reset atomic counters
        self.total_tokens.store(0, Ordering::Relaxed);
        self.total_cost_cents.store(0, Ordering::Relaxed);
        self.request_count.store(0, Ordering::Relaxed);

        // Clear history
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .history
            .clear();
    }

    /// Total tokens used (lock-free)
    pub fn total_tokens(&self) -> u64 {
        self.total_tokens.load(Ordering::Relaxed)
    }

    /// Total cost in USD (lock-free)
    pub fn total_cost_usd(&self) -> f64 {
        self.total_cost_cents.load(Ordering::Relaxed) as f64 / 100.0
    }
}

impl Default for TokenTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tracker_record_and_summary() {
        let tracker = TokenTracker::new();
        tracker.record("openai", "gpt-4", 1000, 500);
        tracker.record("openai", "gpt-4", 500, 300);
        tracker.record("ollama", "llama2", 200, 100);

        let summary = tracker.summary();
        assert_eq!(summary.total_tokens, 1700);
        assert_eq!(summary.total_requests, 3);
        assert!(summary.total_cost_usd > 0.0);
        assert_eq!(summary.by_model.len(), 2);
        assert_eq!(summary.by_model["gpt-4"].request_count, 2);
    }

    #[test]
    fn test_tracker_reset() {
        let tracker = TokenTracker::new();
        tracker.record("openai", "gpt-4", 1000, 500);
        assert_eq!(tracker.total_tokens(), 1000);
        tracker.reset();
        assert_eq!(tracker.total_tokens(), 0);
    }

    #[test]
    fn test_cost_calculation() {
        let tracker = TokenTracker::new();
        tracker.record("openai", "gpt-4", 1_000_000, 1000);
        let cost = tracker.total_cost_usd();
        // Input cost: 1M * $30/M = $30
        assert!((cost - 30.0).abs() < 0.01, "Expected ~$30, got ${}", cost);
    }

    #[test]
    fn test_free_model() {
        let tracker = TokenTracker::new();
        tracker.record("ollama", "llama2", 50_000, 2000);
        assert_eq!(tracker.total_cost_usd(), 0.0);
    }

    #[test]
    fn test_recent() {
        let tracker = TokenTracker::new();
        for i in 0..10 {
            tracker.record("openai", "gpt-4", i * 100, 100);
        }
        let recent = tracker.recent(3);
        assert_eq!(recent.len(), 3);
        // most recent first
        assert_eq!(recent[0].tokens_used, 900);
    }

    #[test]
    fn test_cost_per_million_tokens_io() {
        // Claude Sonnet 4: $3/M in, $15/M out
        let (inp, out) = cost_per_million_tokens_io("claude-sonnet-4-6");
        assert_eq!(inp, 3.0);
        assert_eq!(out, 15.0);

        // Claude Opus 4: $15/M in, $75/M out
        let (inp, out) = cost_per_million_tokens_io("claude-opus-4-6");
        assert_eq!(inp, 15.0);
        assert_eq!(out, 75.0);

        // Claude Haiku 4: $0.80/M in, $4/M out
        let (inp, out) = cost_per_million_tokens_io("claude-haiku-4-20250514");
        assert_eq!(inp, 0.80);
        assert_eq!(out, 4.0);

        // Free models
        let (inp, out) = cost_per_million_tokens_io("llama2");
        assert_eq!(inp, 0.0);
        assert_eq!(out, 0.0);
    }

    #[test]
    fn test_estimate_cost() {
        // Claude Sonnet 4: 1M input + 1M output = $3 + $15 = $18
        let cost = estimate_cost("claude-sonnet-4-6", 1_000_000, 1_000_000);
        assert!((cost - 18.0).abs() < 0.01, "Expected $18, got ${}", cost);

        // Free model
        let cost = estimate_cost("ollama/llama2", 100_000, 50_000);
        assert_eq!(cost, 0.0);

        // Small usage
        let cost = estimate_cost("claude-sonnet-4-6", 1000, 500);
        let expected = (1000.0 / 1_000_000.0) * 3.0 + (500.0 / 1_000_000.0) * 15.0;
        assert!((cost - expected).abs() < 0.0001);
    }
}
