//! Session-level cost tracking with budget enforcement
//!
//! Shared between LLM and tool crates to provide consistent cost tracking.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use rustycode_protocol::ApiCall;
use rustycode_protocol::llm::Usage;

/// Cost per 1M tokens (input, output) by model (approximate, April 2026)
///
/// Returns `(input_cost, output_cost)` per million tokens.
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
    else if model.starts_with("o4-mini") {
        (1.10, 4.40)
    } else if model.starts_with("o3") {
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
        let cost_usd = estimate_cost(model, input_tokens, output_tokens);
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
        usage: &Usage,
        tool_name: Option<String>,
    ) -> Result<(), BudgetExceeded> {
        let (input_cost_per_m, output_cost_per_m) =
            cost_per_million_tokens_io(model);

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
