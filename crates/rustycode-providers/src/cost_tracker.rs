//! Cost tracking and token usage accumulation
//!
//! This module provides utilities for tracking API costs across providers and models.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Accumulated costs and usage for a provider or model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostAccumulator {
    /// Total cost in USD
    pub total_cost: f64,

    /// Total input tokens
    pub input_tokens: u64,

    /// Total output tokens
    pub output_tokens: u64,

    /// Number of requests
    pub request_count: usize,
}

impl CostAccumulator {
    /// Create a new cost accumulator
    pub fn new() -> Self {
        Self {
            total_cost: 0.0,
            input_tokens: 0,
            output_tokens: 0,
            request_count: 0,
        }
    }

    /// Add a request to the accumulator
    pub fn add_request(&mut self, input_tokens: u64, output_tokens: u64, cost: f64) {
        self.input_tokens = self.input_tokens.saturating_add(input_tokens);
        self.output_tokens = self.output_tokens.saturating_add(output_tokens);
        self.total_cost += cost;
        self.request_count = self.request_count.saturating_add(1);
    }

    /// Get total tokens
    pub fn total_tokens(&self) -> u64 {
        self.input_tokens + self.output_tokens
    }

    /// Get average cost per request
    pub fn avg_cost_per_request(&self) -> f64 {
        if self.request_count == 0 {
            0.0
        } else {
            self.total_cost / self.request_count as f64
        }
    }

    /// Get average cost per 1k tokens
    pub fn avg_cost_per_1k(&self) -> f64 {
        let total = self.total_tokens();
        if total == 0 {
            0.0
        } else {
            (self.total_cost / total as f64) * 1000.0
        }
    }
}

impl Default for CostAccumulator {
    fn default() -> Self {
        Self::new()
    }
}

/// Summary of costs across all providers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostSummary {
    /// Total cost across all providers
    pub total_cost: f64,

    /// Total input tokens
    pub total_input_tokens: u64,

    /// Total output tokens
    pub total_output_tokens: u64,

    /// Total requests
    pub total_requests: usize,

    /// Cost breakdown by provider
    pub by_provider: HashMap<String, ProviderCostSummary>,
}

/// Cost summary for a single provider
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderCostSummary {
    /// Provider ID
    pub provider_id: String,

    /// Total cost for this provider
    pub total_cost: f64,

    /// Total input tokens
    pub input_tokens: u64,

    /// Total output tokens
    pub output_tokens: u64,

    /// Number of requests
    pub request_count: usize,

    /// Cost breakdown by model
    pub by_model: HashMap<String, f64>,
}

/// Thread-safe cost tracker
#[derive(Debug, Clone)]
pub struct CostTracker {
    /// Cost accumulators keyed by provider/model
    accumulators: Arc<RwLock<HashMap<String, CostAccumulator>>>,
}

impl CostTracker {
    /// Create a new cost tracker
    pub fn new() -> Self {
        Self {
            accumulators: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Track a request
    ///
    /// # Arguments
    ///
    /// * `key` - Unique key (e.g., "anthropic/claude-3-5-sonnet" or "anthropic")
    /// * `input_tokens` - Number of input tokens
    /// * `output_tokens` - Number of output tokens
    /// * `cost` - Cost in USD
    pub async fn track(&self, key: &str, input_tokens: u64, output_tokens: u64, cost: f64) {
        let mut accumulators = self.accumulators.write().await;
        let accumulator = accumulators.entry(key.to_string()).or_default();
        accumulator.add_request(input_tokens, output_tokens, cost);
    }

    /// Get accumulator for a specific key
    pub async fn get(&self, key: &str) -> Option<CostAccumulator> {
        let accumulators = self.accumulators.read().await;
        accumulators.get(key).cloned()
    }

    /// Get cost summary for all tracked requests
    pub async fn summary(&self) -> CostSummary {
        let accumulators = self.accumulators.read().await;

        let mut total_cost = 0.0;
        let mut total_input_tokens = 0;
        let mut total_output_tokens = 0;
        let mut total_requests = 0;
        let mut by_provider: HashMap<String, ProviderCostSummary> = HashMap::new();

        for (key, acc) in accumulators.iter() {
            total_cost += acc.total_cost;
            total_input_tokens += acc.input_tokens;
            total_output_tokens += acc.output_tokens;
            total_requests += acc.request_count;

            // Parse key (format: "provider/model" or "provider")
            let parts: Vec<&str> = key.split('/').collect();
            let provider_id = parts.first().copied().unwrap_or(key.as_str()).to_string();

            by_provider
                .entry(provider_id.clone())
                .and_modify(|summary| {
                    summary.total_cost += acc.total_cost;
                    summary.input_tokens += acc.input_tokens;
                    summary.output_tokens += acc.output_tokens;
                    summary.request_count += acc.request_count;
                    summary.by_model.insert(key.clone(), acc.total_cost);
                })
                .or_insert_with(|| {
                    let mut by_model = HashMap::new();
                    by_model.insert(key.clone(), acc.total_cost);

                    ProviderCostSummary {
                        provider_id,
                        total_cost: acc.total_cost,
                        input_tokens: acc.input_tokens,
                        output_tokens: acc.output_tokens,
                        request_count: acc.request_count,
                        by_model,
                    }
                });
        }

        CostSummary {
            total_cost,
            total_input_tokens,
            total_output_tokens,
            total_requests,
            by_provider,
        }
    }

    /// Reset all tracking
    pub async fn reset(&self) {
        self.accumulators.write().await.clear();
    }

    /// Get total cost across all keys
    pub async fn total_cost(&self) -> f64 {
        let accumulators = self.accumulators.read().await;
        accumulators.values().map(|acc| acc.total_cost).sum()
    }
}

impl Default for CostTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_cost_accumulator() {
        let mut acc = CostAccumulator::new();

        acc.add_request(1000, 500, 0.0105);
        assert_eq!(acc.input_tokens, 1000);
        assert_eq!(acc.output_tokens, 500);
        assert_eq!(acc.total_cost, 0.0105);
        assert_eq!(acc.request_count, 1);
        assert_eq!(acc.total_tokens(), 1500);

        acc.add_request(2000, 1000, 0.021);
        assert_eq!(acc.input_tokens, 3000);
        assert_eq!(acc.output_tokens, 1500);
        assert_eq!(acc.total_cost, 0.0315);
        assert_eq!(acc.request_count, 2);

        assert!((acc.avg_cost_per_request() - 0.01575).abs() < 0.0001);
    }

    #[tokio::test]
    async fn test_cost_tracker() {
        let tracker = CostTracker::new();

        tracker
            .track("anthropic/claude-3-5-sonnet", 1000, 500, 0.0105)
            .await;
        tracker.track("openai/gpt-4o", 2000, 1000, 0.025).await;

        let anthropic = tracker.get("anthropic/claude-3-5-sonnet").await;
        assert!(anthropic.is_some());
        assert_eq!(anthropic.unwrap().total_cost, 0.0105);

        let openai = tracker.get("openai/gpt-4o").await;
        assert!(openai.is_some());
        assert_eq!(openai.unwrap().total_cost, 0.025);

        assert!((tracker.total_cost().await - 0.0355).abs() < 0.0001);
    }

    #[tokio::test]
    async fn test_cost_summary() {
        let tracker = CostTracker::new();

        tracker
            .track("anthropic/claude-3-5-sonnet", 1000, 500, 0.0105)
            .await;
        tracker
            .track("anthropic/claude-3-opus", 2000, 1000, 0.035)
            .await;
        tracker.track("openai/gpt-4o", 2000, 1000, 0.025).await;

        let summary = tracker.summary().await;
        assert_eq!(summary.total_requests, 3);
        assert!((summary.total_cost - 0.0705).abs() < 0.0001);
        assert_eq!(summary.by_provider.len(), 2);

        let anthropic = summary.by_provider.get("anthropic").unwrap();
        assert!((anthropic.total_cost - 0.0455).abs() < 0.0001);
        assert_eq!(anthropic.request_count, 2);

        let openai = summary.by_provider.get("openai").unwrap();
        assert!((openai.total_cost - 0.025).abs() < 0.0001);
        assert_eq!(openai.request_count, 1);
    }

    #[tokio::test]
    async fn test_reset() {
        let tracker = CostTracker::new();

        tracker.track("test/model", 1000, 500, 0.01).await;
        assert_eq!(tracker.total_cost().await, 0.01);

        tracker.reset().await;
        assert_eq!(tracker.total_cost().await, 0.0);
    }

    #[test]
    fn test_cost_accumulator_default() {
        let acc = CostAccumulator::default();
        assert_eq!(acc.total_cost, 0.0);
        assert_eq!(acc.input_tokens, 0);
        assert_eq!(acc.output_tokens, 0);
        assert_eq!(acc.request_count, 0);
    }

    #[test]
    fn test_cost_accumulator_avg_cost_no_requests() {
        let acc = CostAccumulator::new();
        assert_eq!(acc.avg_cost_per_request(), 0.0);
    }

    #[test]
    fn test_cost_accumulator_avg_per_1k_no_tokens() {
        let acc = CostAccumulator::new();
        assert_eq!(acc.avg_cost_per_1k(), 0.0);
    }

    #[test]
    fn test_cost_accumulator_total_tokens() {
        let mut acc = CostAccumulator::new();
        acc.add_request(3000, 2000, 0.05);
        assert_eq!(acc.total_tokens(), 5000);
    }

    #[test]
    fn test_cost_accumulator_serialization() {
        let mut acc = CostAccumulator::new();
        acc.add_request(1000, 500, 0.015);
        let json = serde_json::to_string(&acc).unwrap();
        let decoded: CostAccumulator = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.request_count, 1);
        assert_eq!(decoded.input_tokens, 1000);
    }

    #[test]
    fn test_cost_summary_serialization() {
        let summary = CostSummary {
            total_cost: 1.5,
            total_input_tokens: 10000,
            total_output_tokens: 5000,
            total_requests: 5,
            by_provider: HashMap::new(),
        };
        let json = serde_json::to_string(&summary).unwrap();
        let decoded: CostSummary = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.total_requests, 5);
    }

    #[test]
    fn test_provider_cost_summary_serialization() {
        let summary = ProviderCostSummary {
            provider_id: "anthropic".to_string(),
            total_cost: 0.5,
            input_tokens: 5000,
            output_tokens: 2000,
            request_count: 3,
            by_model: HashMap::new(),
        };
        let json = serde_json::to_string(&summary).unwrap();
        let decoded: ProviderCostSummary = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.provider_id, "anthropic");
    }

    #[tokio::test]
    async fn test_tracker_get_nonexistent_key() {
        let tracker = CostTracker::new();
        assert!(tracker.get("nonexistent").await.is_none());
    }

    #[tokio::test]
    async fn test_tracker_track_accumulates() {
        let tracker = CostTracker::new();
        tracker.track("test/model", 1000, 500, 0.01).await;
        tracker.track("test/model", 2000, 1000, 0.02).await;
        let acc = tracker.get("test/model").await.unwrap();
        assert_eq!(acc.request_count, 2);
        assert_eq!(acc.input_tokens, 3000);
        assert_eq!(acc.output_tokens, 1500);
    }

    #[test]
    fn test_cost_accumulator_avg_per_1k_with_data() {
        let mut acc = CostAccumulator::new();
        acc.add_request(5000, 5000, 0.10);
        // total_tokens = 10000, cost = 0.10, per 1k = 0.01
        assert!((acc.avg_cost_per_1k() - 0.01).abs() < 0.0001);
    }

    #[test]
    fn test_accumulator_multiple_requests_average() {
        let mut acc = CostAccumulator::new();
        acc.add_request(1000, 500, 0.01);
        acc.add_request(3000, 1500, 0.03);
        // total = 6000 tokens, total_cost = 0.04, per 1k = 0.04/6 ≈ 0.006667
        assert!((acc.avg_cost_per_1k() - 0.006667).abs() < 0.001);
    }

    #[tokio::test]
    async fn test_tracker_summary_single_provider_multiple_models() {
        let tracker = CostTracker::new();
        tracker.track("openai/gpt-4o", 1000, 500, 0.02).await;
        tracker.track("openai/gpt-4o-mini", 2000, 1000, 0.005).await;

        let summary = tracker.summary().await;
        assert_eq!(summary.by_provider.len(), 1);
        let openai = summary.by_provider.get("openai").unwrap();
        assert_eq!(openai.request_count, 2);
        assert_eq!(openai.by_model.len(), 2);
    }

    #[tokio::test]
    async fn test_tracker_summary_no_slash_key() {
        let tracker = CostTracker::new();
        tracker.track("local-model", 100, 50, 0.001).await;

        let summary = tracker.summary().await;
        assert!(summary.by_provider.contains_key("local-model"));
    }

    #[tokio::test]
    async fn test_tracker_default_matches_new() {
        let t1 = CostTracker::new();
        let t2 = CostTracker::default();
        assert_eq!(t1.total_cost().await, t2.total_cost().await);
    }

    #[test]
    fn test_provider_cost_summary_fields() {
        let mut by_model = HashMap::new();
        by_model.insert("claude-3-opus".to_string(), 0.5);
        let summary = ProviderCostSummary {
            provider_id: "anthropic".to_string(),
            total_cost: 0.5,
            input_tokens: 5000,
            output_tokens: 2000,
            request_count: 3,
            by_model,
        };
        assert_eq!(summary.by_model.len(), 1);
        assert_eq!(summary.by_model.get("claude-3-opus"), Some(&0.5));
    }

    #[tokio::test]
    async fn test_tracker_concurrent_tracking() {
        let tracker = CostTracker::new();
        let handles: Vec<_> = (0..10)
            .map(|i| {
                let t = tracker.clone();
                tokio::spawn(async move {
                    t.track("test/model", 100, 50, 0.01).await;
                    i
                })
            })
            .collect();

        for h in handles {
            h.await.unwrap();
        }

        let acc = tracker.get("test/model").await.unwrap();
        assert_eq!(acc.request_count, 10);
        assert_eq!(acc.input_tokens, 1000);
    }
}
