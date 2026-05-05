//! Session-level cost tracking with budget enforcement
//!
//! Builds on top of token_tracker's per-request tracking to provide:
//! - Session-scoped cost accumulation
//! - Budget limits with warnings
//! - Cost breakdown by tool and model
//! - Session summaries

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::token_tracker;

/// Single API call record for a session
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ApiCall {
    pub model: String,
    pub input_tokens: usize,
    pub output_tokens: usize,
    pub cost_usd: f64,
    pub timestamp: DateTime<Utc>,
    pub tool_name: Option<String>,
    /// Tokens served from cache (billed at 0.1× base input price)
    #[serde(default)]
    pub cache_read_tokens: u32,
    /// Tokens written to cache (billed at 1.25× base input price)
    #[serde(default)]
    pub cache_creation_tokens: u32,
    /// Estimated cost savings from cache hits (0.9 × base price × cache_read_tokens)
    #[serde(default)]
    pub cache_savings_usd: f64,
}

/// Budget status for the current session
#[derive(Clone, Debug, Serialize)]
pub struct BudgetStatus {
    pub total_spent: f64,
    pub remaining: f64,
    pub limit: Option<f64>,
    pub percent_used: f64,
    pub is_exceeded: bool,
    pub is_warning: bool,
}

/// Session cost summary
#[derive(Clone, Debug, Serialize)]
pub struct CostSummary {
    pub total_cost: f64,
    pub total_input_tokens: usize,
    pub total_output_tokens: usize,
    pub calls_count: usize,
    pub average_cost_per_call: f64,
    pub by_model: HashMap<String, ModelCost>,
    pub by_tool: HashMap<String, f64>,
    /// Total cache read tokens (cost savings benefit)
    #[serde(default)]
    pub total_cache_read_tokens: u64,
    /// Total cache creation tokens (write cost)
    #[serde(default)]
    pub total_cache_creation_tokens: u64,
    /// Total estimated savings from cache hits
    #[serde(default)]
    pub total_cache_savings_usd: f64,
    /// Cache hit ratio: cache_read / (cache_read + cache_creation + input_tokens)
    #[serde(default)]
    pub cache_hit_rate: f64,
}

/// Cost breakdown per model
#[derive(Clone, Debug, Serialize)]
pub struct ModelCost {
    pub model: String,
    pub total_cost: f64,
    pub calls_count: usize,
    pub input_tokens: usize,
    pub output_tokens: usize,
    /// Total cache read tokens for this model
    #[serde(default)]
    pub cache_read_tokens: u64,
    /// Total cache creation tokens for this model
    #[serde(default)]
    pub cache_creation_tokens: u64,
    /// Total cache savings for this model
    #[serde(default)]
    pub cache_savings_usd: f64,
}

/// Budget warning level
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BudgetWarningLevel {
    /// Under 50%
    Green,
    /// 50-80%
    Yellow,
    /// 80-100%
    Red,
    /// Over budget
    Exceeded,
}

/// Session-scoped cost tracker with optional budget enforcement
pub struct CostTracker {
    calls: Vec<ApiCall>,
    budget_limit: Option<f64>,
    /// Percentage threshold for warnings (default: 0.8 = 80%)
    warning_threshold: f64,
}

impl CostTracker {
    pub fn new(budget_limit: Option<f64>) -> Self {
        Self {
            calls: Vec::new(),
            budget_limit,
            warning_threshold: 0.8,
        }
    }

    /// Create with a budget limit in USD
    pub fn with_budget(limit_usd: f64) -> Self {
        Self::new(Some(limit_usd))
    }

    /// Create without a budget limit (tracking only)
    pub fn unlimited() -> Self {
        Self::new(None)
    }

    /// Record an LLM API call
    pub fn record_call(&mut self, call: ApiCall) -> Result<(), BudgetExceeded> {
        if let Some(limit) = self.budget_limit {
            let new_total = self.total_cost() + call.cost_usd;
            if new_total > limit {
                self.calls.push(call);
                return Err(BudgetExceeded {
                    total_spent: new_total,
                    budget_limit: limit,
                });
            }
        }
        self.calls.push(call);
        Ok(())
    }

    /// Record a call using token counts (calculates cost automatically)
    pub fn record_tokens(
        &mut self,
        model: &str,
        input_tokens: usize,
        output_tokens: usize,
        tool_name: Option<String>,
    ) -> Result<(), BudgetExceeded> {
        let cost_usd = token_tracker::estimate_cost(model, input_tokens, output_tokens);
        let call = ApiCall {
            model: model.to_string(),
            input_tokens,
            output_tokens,
            cost_usd,
            timestamp: Utc::now(),
            tool_name,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
            cache_savings_usd: 0.0,
        };
        self.record_call(call)
    }

    /// Record a call using a Usage struct (includes cache metrics)
    pub fn record_call_with_usage(
        &mut self,
        model: &str,
        usage: &crate::provider::Usage,
        tool_name: Option<String>,
    ) -> Result<(), BudgetExceeded> {
        let (input_cost_per_m, output_cost_per_m) =
            token_tracker::cost_per_million_tokens_io(model);

        // Calculate base cost: non-cached input + all output
        let base_input_cost = (usage.input_tokens as f64 / 1_000_000.0) * input_cost_per_m;
        let output_cost = (usage.output_tokens as f64 / 1_000_000.0) * output_cost_per_m;

        // Cache hits are billed at 0.1× base input price
        let cache_read_cost =
            (usage.cache_read_input_tokens as f64 / 1_000_000.0) * input_cost_per_m * 0.1;

        // Cache creation is billed at 1.25× base input price (5min TTL)
        let cache_creation_cost =
            (usage.cache_creation_input_tokens as f64 / 1_000_000.0) * input_cost_per_m * 1.25;

        let total_cost = base_input_cost + output_cost + cache_read_cost + cache_creation_cost;

        // Savings: what would have been charged without cache
        let uncached_cost =
            (usage.total_input_tokens() as f64 / 1_000_000.0) * input_cost_per_m + output_cost;
        let cache_savings_usd = (uncached_cost - total_cost).max(0.0);

        let call = ApiCall {
            model: model.to_string(),
            input_tokens: usage.input_tokens as usize,
            output_tokens: usage.output_tokens as usize,
            cost_usd: total_cost,
            timestamp: Utc::now(),
            tool_name,
            cache_read_tokens: usage.cache_read_input_tokens,
            cache_creation_tokens: usage.cache_creation_input_tokens,
            cache_savings_usd,
        };
        self.record_call(call)
    }

    /// Check current budget status
    pub fn check_budget(&self) -> BudgetStatus {
        let total = self.total_cost();
        match self.budget_limit {
            Some(limit) => {
                let percent = if limit > 0.0 {
                    (total / limit) * 100.0
                } else {
                    0.0
                };
                BudgetStatus {
                    total_spent: total,
                    remaining: (limit - total).max(0.0),
                    limit: Some(limit),
                    percent_used: percent,
                    is_exceeded: total > limit,
                    is_warning: total >= limit * self.warning_threshold,
                }
            }
            None => BudgetStatus {
                total_spent: total,
                remaining: f64::INFINITY,
                limit: None,
                percent_used: 0.0,
                is_exceeded: false,
                is_warning: false,
            },
        }
    }

    /// Get the warning level
    pub fn warning_level(&self) -> BudgetWarningLevel {
        let status = self.check_budget();
        if status.is_exceeded {
            BudgetWarningLevel::Exceeded
        } else if status.is_warning {
            BudgetWarningLevel::Red
        } else if status.percent_used >= 50.0 {
            BudgetWarningLevel::Yellow
        } else {
            BudgetWarningLevel::Green
        }
    }

    /// Get session cost summary
    pub fn session_summary(&self) -> CostSummary {
        let total_cost = self.total_cost();
        let total_input: usize = self.calls.iter().map(|c| c.input_tokens).sum();
        let total_output: usize = self.calls.iter().map(|c| c.output_tokens).sum();
        let total_cache_read: u64 = self.calls.iter().map(|c| c.cache_read_tokens as u64).sum();
        let total_cache_creation: u64 = self
            .calls
            .iter()
            .map(|c| c.cache_creation_tokens as u64)
            .sum();
        let total_cache_savings: f64 = self.calls.iter().map(|c| c.cache_savings_usd).sum();
        let count = self.calls.len();

        // Calculate cache hit rate: cache_read / (cache_read + cache_creation + input_tokens)
        let total_all_input = total_cache_read
            .saturating_add(total_cache_creation)
            .saturating_add(total_input as u64);
        let cache_hit_rate = if total_all_input > 0 {
            (total_cache_read as f64 / total_all_input as f64) * 100.0
        } else {
            0.0
        };

        let by_model = self.costs_by_model();
        let by_tool = self.costs_by_tool();

        CostSummary {
            total_cost,
            total_input_tokens: total_input,
            total_output_tokens: total_output,
            calls_count: count,
            average_cost_per_call: if count > 0 {
                total_cost / count as f64
            } else {
                0.0
            },
            by_model,
            by_tool,
            total_cache_read_tokens: total_cache_read,
            total_cache_creation_tokens: total_cache_creation,
            total_cache_savings_usd: total_cache_savings,
            cache_hit_rate,
        }
    }

    /// Get costs broken down by model
    pub fn costs_by_model(&self) -> HashMap<String, ModelCost> {
        let mut map: HashMap<String, ModelCost> = HashMap::new();
        for call in &self.calls {
            let entry = map.entry(call.model.clone()).or_insert_with(|| ModelCost {
                model: call.model.clone(),
                total_cost: 0.0,
                calls_count: 0,
                input_tokens: 0,
                output_tokens: 0,
                cache_read_tokens: 0,
                cache_creation_tokens: 0,
                cache_savings_usd: 0.0,
            });
            entry.total_cost += call.cost_usd;
            entry.calls_count += 1;
            entry.input_tokens += call.input_tokens;
            entry.output_tokens += call.output_tokens;
            entry.cache_read_tokens = entry
                .cache_read_tokens
                .saturating_add(call.cache_read_tokens as u64);
            entry.cache_creation_tokens = entry
                .cache_creation_tokens
                .saturating_add(call.cache_creation_tokens as u64);
            entry.cache_savings_usd += call.cache_savings_usd;
        }
        map
    }

    /// Get costs broken down by tool
    pub fn costs_by_tool(&self) -> HashMap<String, f64> {
        let mut map: HashMap<String, f64> = HashMap::new();
        for call in &self.calls {
            let tool = call
                .tool_name
                .clone()
                .unwrap_or_else(|| "unknown".to_string());
            *map.entry(tool).or_insert(0.0) += call.cost_usd;
        }
        map
    }

    /// Total cost so far
    pub fn total_cost(&self) -> f64 {
        self.calls.iter().map(|c| c.cost_usd).sum()
    }

    /// Total number of calls
    pub fn calls_count(&self) -> usize {
        self.calls.len()
    }

    /// Get the budget limit
    pub fn budget_limit(&self) -> Option<f64> {
        self.budget_limit
    }

    /// Set the budget limit
    pub fn set_budget_limit(&mut self, limit: Option<f64>) {
        self.budget_limit = limit;
    }
}

/// Error when budget is exceeded
#[derive(Clone, Debug)]
pub struct BudgetExceeded {
    pub total_spent: f64,
    pub budget_limit: f64,
}

impl std::fmt::Display for BudgetExceeded {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Budget exceeded: ${:.4} / ${:.2}",
            self.total_spent, self.budget_limit
        )
    }
}

impl std::error::Error for BudgetExceeded {}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_call(cost: f64) -> ApiCall {
        ApiCall {
            model: "test-model".to_string(),
            input_tokens: 100,
            output_tokens: 50,
            cost_usd: cost,
            timestamp: Utc::now(),
            tool_name: None,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
            cache_savings_usd: 0.0,
        }
    }

    fn test_call_with(model: &str, cost: f64, tool: Option<&str>) -> ApiCall {
        ApiCall {
            model: model.to_string(),
            input_tokens: 100,
            output_tokens: 50,
            cost_usd: cost,
            timestamp: Utc::now(),
            tool_name: tool.map(|t| t.to_string()),
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
            cache_savings_usd: 0.0,
        }
    }

    fn test_call_with_cache(
        model: &str,
        cost: f64,
        tool: Option<&str>,
        cache_read: u32,
        cache_creation: u32,
        cache_savings: f64,
    ) -> ApiCall {
        ApiCall {
            model: model.to_string(),
            input_tokens: 100,
            output_tokens: 50,
            cost_usd: cost,
            timestamp: Utc::now(),
            tool_name: tool.map(|t| t.to_string()),
            cache_read_tokens: cache_read,
            cache_creation_tokens: cache_creation,
            cache_savings_usd: cache_savings,
        }
    }

    #[test]
    fn unlimited_tracker() {
        let mut tracker = CostTracker::unlimited();
        tracker.record_call(test_call(0.05)).unwrap();
        tracker.record_call(test_call(0.03)).unwrap();
        assert!((tracker.total_cost() - 0.08).abs() < 0.001);
        assert_eq!(tracker.calls_count(), 2);
    }

    #[test]
    fn budget_tracker_within_limit() {
        let mut tracker = CostTracker::with_budget(1.0);
        tracker.record_call(test_call(0.40)).unwrap();
        tracker.record_call(test_call(0.30)).unwrap();

        let status = tracker.check_budget();
        assert!(!status.is_exceeded);
        assert!(!status.is_warning);
        assert!((status.total_spent - 0.70).abs() < 0.001);
        assert!((status.remaining - 0.30).abs() < 0.001);
    }

    #[test]
    fn budget_tracker_exceeded() {
        let mut tracker = CostTracker::with_budget(0.10);
        tracker.record_call(test_call(0.05)).unwrap();
        let result = tracker.record_call(test_call(0.10));

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!((err.total_spent - 0.15).abs() < 0.001);
    }

    #[test]
    fn budget_warning_levels() {
        let mut tracker = CostTracker::with_budget(1.0);
        assert_eq!(tracker.warning_level(), BudgetWarningLevel::Green);

        tracker.record_call(test_call(0.50)).unwrap();
        assert_eq!(tracker.warning_level(), BudgetWarningLevel::Yellow);

        tracker.record_call(test_call(0.30)).unwrap();
        assert_eq!(tracker.warning_level(), BudgetWarningLevel::Red);
    }

    #[test]
    fn cost_summary() {
        let mut tracker = CostTracker::unlimited();
        tracker
            .record_call(test_call_with("claude-sonnet-4", 0.03, Some("read")))
            .unwrap();
        tracker
            .record_call(test_call_with("claude-sonnet-4", 0.02, Some("edit")))
            .unwrap();
        tracker
            .record_call(test_call_with("gpt-4o", 0.05, None))
            .unwrap();

        let summary = tracker.session_summary();
        assert_eq!(summary.calls_count, 3);
        assert!((summary.total_cost - 0.10).abs() < 0.001);
        assert!((summary.average_cost_per_call - 0.0333).abs() < 0.001);
        assert_eq!(summary.by_model.len(), 2);
        assert_eq!(summary.by_tool.len(), 3); // read, edit, unknown
    }

    #[test]
    fn costs_by_model() {
        let mut tracker = CostTracker::unlimited();
        tracker
            .record_call(test_call_with("model-a", 0.01, None))
            .unwrap();
        tracker
            .record_call(test_call_with("model-a", 0.02, None))
            .unwrap();
        tracker
            .record_call(test_call_with("model-b", 0.05, None))
            .unwrap();

        let by_model = tracker.costs_by_model();
        assert_eq!(by_model.len(), 2);
        assert_eq!(by_model["model-a"].calls_count, 2);
        assert!((by_model["model-a"].total_cost - 0.03).abs() < 0.001);
        assert_eq!(by_model["model-b"].calls_count, 1);
    }

    #[test]
    fn costs_by_tool() {
        let mut tracker = CostTracker::unlimited();
        tracker
            .record_call(test_call_with("m", 0.01, Some("grep")))
            .unwrap();
        tracker
            .record_call(test_call_with("m", 0.02, Some("grep")))
            .unwrap();
        tracker
            .record_call(test_call_with("m", 0.05, Some("edit")))
            .unwrap();

        let by_tool = tracker.costs_by_tool();
        assert!((by_tool["grep"] - 0.03).abs() < 0.001);
        assert!((by_tool["edit"] - 0.05).abs() < 0.001);
    }

    #[test]
    fn budget_status_no_limit() {
        let tracker = CostTracker::unlimited();
        let status = tracker.check_budget();
        assert!(!status.is_exceeded);
        assert!(!status.is_warning);
        assert!(status.remaining.is_infinite());
        assert!(status.limit.is_none());
    }

    #[test]
    fn record_tokens_auto_calculates_cost() {
        let mut tracker = CostTracker::unlimited();
        tracker
            .record_tokens("claude-sonnet-4", 1000, 500, Some("test".to_string()))
            .unwrap();

        assert_eq!(tracker.calls_count(), 1);
        let call = &tracker.calls[0];
        assert_eq!(call.model, "claude-sonnet-4");
        assert_eq!(call.input_tokens, 1000);
        assert_eq!(call.output_tokens, 500);
        assert_eq!(call.tool_name, Some("test".to_string()));
        assert!(call.cost_usd > 0.0);
    }

    #[test]
    fn set_budget_limit() {
        let mut tracker = CostTracker::unlimited();
        assert!(tracker.budget_limit().is_none());
        tracker.set_budget_limit(Some(5.0));
        assert_eq!(tracker.budget_limit(), Some(5.0));
    }

    #[test]
    fn budget_exceeded_display() {
        let err = BudgetExceeded {
            total_spent: 1.50,
            budget_limit: 1.00,
        };
        let msg = err.to_string();
        assert!(msg.contains("1.50"));
        assert!(msg.contains("1.00"));
    }

    #[test]
    fn empty_tracker() {
        let tracker = CostTracker::unlimited();
        assert_eq!(tracker.calls_count(), 0);
        assert!((tracker.total_cost() - 0.0).abs() < 0.001);
        let summary = tracker.session_summary();
        assert_eq!(summary.calls_count, 0);
    }

    #[test]
    fn cache_tokens_recorded() {
        let mut tracker = CostTracker::unlimited();
        let call = test_call_with_cache("model-a", 0.01, Some("edit"), 500, 100, 0.0045);
        tracker.record_call(call).unwrap();

        assert_eq!(tracker.calls_count(), 1);
        let recorded_call = &tracker.calls[0];
        assert_eq!(recorded_call.cache_read_tokens, 500);
        assert_eq!(recorded_call.cache_creation_tokens, 100);
        assert!((recorded_call.cache_savings_usd - 0.0045).abs() < 0.0001);
    }

    #[test]
    fn cache_stats_in_summary() {
        let mut tracker = CostTracker::unlimited();

        // Call 1: with cache hits
        tracker
            .record_call(test_call_with_cache(
                "model-a",
                0.010,
                Some("read"),
                1000,  // cache read
                0,     // cache creation
                0.009, // savings (0.9 * 0.01)
            ))
            .unwrap();

        // Call 2: with cache misses
        tracker
            .record_call(test_call_with_cache(
                "model-a",
                0.015,
                Some("write"),
                0,   // cache read
                500, // cache creation
                0.0, // no savings on writes
            ))
            .unwrap();

        let summary = tracker.session_summary();
        assert_eq!(summary.calls_count, 2);
        assert_eq!(summary.total_cache_read_tokens, 1000);
        assert_eq!(summary.total_cache_creation_tokens, 500);
        assert!((summary.total_cache_savings_usd - 0.009).abs() < 0.0001);

        // Cache hit rate: 1000 / (1000 + 500 + 200) ≈ 58.8%
        // (200 is combined input_tokens from both calls: 100 + 100)
        assert!(summary.cache_hit_rate > 50.0 && summary.cache_hit_rate < 65.0);
    }

    #[test]
    fn cache_stats_by_model() {
        let mut tracker = CostTracker::unlimited();

        tracker
            .record_call(test_call_with_cache(
                "model-a",
                0.010,
                Some("tool1"),
                500,
                100,
                0.0045,
            ))
            .unwrap();

        tracker
            .record_call(test_call_with_cache(
                "model-a",
                0.015,
                Some("tool2"),
                300,
                50,
                0.0027,
            ))
            .unwrap();

        let by_model = tracker.costs_by_model();
        let model_a = &by_model["model-a"];

        assert_eq!(model_a.cache_read_tokens, 800);
        assert_eq!(model_a.cache_creation_tokens, 150);
        assert!((model_a.cache_savings_usd - 0.0072).abs() < 0.0001);
    }

    #[test]
    #[allow(dead_code)] // Used to test record_call_with_usage
    fn record_call_with_usage_tracks_cache() {
        use crate::provider::Usage;

        let mut tracker = CostTracker::unlimited();

        // Simulate a response with cache hits
        let usage = Usage::with_cache(
            100,  // input tokens (non-cached)
            50,   // output tokens
            1000, // cache read tokens (savings benefit)
            0,    // cache creation tokens
        );

        // Call with Usage struct
        tracker
            .record_call_with_usage("claude-sonnet-4-6", &usage, Some("query".to_string()))
            .unwrap();

        let call = &tracker.calls[0];
        assert_eq!(call.cache_read_tokens, 1000);
        assert_eq!(call.cache_creation_tokens, 0);
        // Savings should be calculated: 0.9 * base_input_cost * cache_read_tokens
        // For claude-sonnet-4: input=$3/M, so 0.9 * (1000/1M) * 3 = 0.0027
        assert!(call.cache_savings_usd > 0.0);

        let summary = tracker.session_summary();
        assert!((summary.total_cache_savings_usd - call.cache_savings_usd).abs() < 0.00001);
    }

    #[test]
    fn cache_token_accumulation_no_overflow() {
        let mut tracker = CostTracker::unlimited();

        // Add many calls with large cache tokens to verify u64 accumulation
        for _ in 0..100 {
            tracker
                .record_call(test_call_with_cache(
                    "model-a",
                    0.01,
                    Some("test"),
                    u32::MAX / 100, // Each call near max u32
                    u32::MAX / 100,
                    0.0,
                ))
                .unwrap();
        }

        let summary = tracker.session_summary();
        // 100 * (u32::MAX / 100) ≈ u32::MAX, but accumulated in u64 — no overflow
        assert!(summary.total_cache_read_tokens > 0);
        assert!(summary.total_cache_creation_tokens > 0);
        assert!(summary.cache_hit_rate > 0.0);

        let by_model = tracker.costs_by_model();
        let model_a = &by_model["model-a"];
        assert!(model_a.cache_read_tokens > 0);
        assert!(model_a.cache_creation_tokens > 0);
    }
}
