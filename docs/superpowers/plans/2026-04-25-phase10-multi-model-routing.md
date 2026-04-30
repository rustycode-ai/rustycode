# Phase 10: Multi-Model Routing -- TDD Implementation Plan

**Date**: 2026-04-25
**Goal**: RustyCode intelligently routes tasks to Haiku/Sonnet/Opus based on complexity, cost, and quality requirements.
**Status**: Not Started
**See Also**: [Coding Standards: Model Selection Strategy](../../../rules/common/performance.md#model-selection-strategy)
**Dependencies**: Phase 2 (Explore-Plan-Act lifecycle), Phase 4 (domain context), Phase 6 (skill authoring)
**Target**: ~70 tests across 6 modules

---

## File Structure

```
New files:
  crates/rustycode-orchestration/src/router.rs              (~400 lines, 18 tests)
  crates/rustycode-orchestration/src/complexity_analyzer.rs (~350 lines, 15 tests)
  crates/rustycode-orchestration/src/cost_optimizer.rs      (~300 lines, 12 tests)
  crates/rustycode-orchestration/src/model_selector.rs      (~350 lines, 15 tests)
  crates/rustycode-orchestration/src/routing_metrics.rs     (~200 lines, 8 tests)

Modified files:
  crates/rustycode-orchestration/src/lib.rs                 (add pub mod router, complexity_analyzer, etc.)
  crates/rustycode-orchestration/src/execution.rs           (integrate router into execution path)
  crates/rustycode-llm/src/provider.rs                       (wire router into provider selection)
```

---

## Implementation Status

To be completed in this phase.

---

## Chunk 1: Task Complexity Analyzer (rustycode-orchestration/src/complexity_analyzer.rs)

### 1.1 Complexity metrics and estimation

**File**: `crates/rustycode-orchestration/src/complexity_analyzer.rs`

**RED -- Tests**:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_simple_task() {
        let analyzer = ComplexityAnalyzer::new();
        let task = Task {
            description: "Format a file with prettier".to_string(),
            context: vec!["file.ts".to_string()],
        };
        
        let complexity = analyzer.analyze(&task).unwrap();
        assert_eq!(complexity.level, ComplexityLevel::Simple);
    }

    #[test]
    fn classify_moderate_task() {
        let analyzer = ComplexityAnalyzer::new();
        let task = Task {
            description: "Refactor authentication module with new patterns".to_string(),
            context: vec!["auth.ts", "types.ts", "config.ts"].iter().map(|s| s.to_string()).collect(),
        };
        
        let complexity = analyzer.analyze(&task).unwrap();
        assert_eq!(complexity.level, ComplexityLevel::Moderate);
    }

    #[test]
    fn classify_complex_task() {
        let analyzer = ComplexityAnalyzer::new();
        let task = Task {
            description: "Design and implement multi-tier caching system with TTL, eviction policies, and distributed coordination".to_string(),
            context: vec![
                "cache.rs", "ttl.rs", "eviction.rs", "coordination.rs",
                "tests/cache_integration.rs", "docs/caching.md"
            ].iter().map(|s| s.to_string()).collect(),
        };
        
        let complexity = analyzer.analyze(&task).unwrap();
        assert_eq!(complexity.level, ComplexityLevel::Complex);
    }

    #[test]
    fn complexity_scoring() {
        let analyzer = ComplexityAnalyzer::new();
        let task = Task {
            description: "Add 50 lines of feature code".to_string(),
            context: vec!["feature.rs".to_string()],
        };
        
        let complexity = analyzer.analyze(&task).unwrap();
        assert!(complexity.score >= 0.0 && complexity.score <= 100.0);
    }

    #[test]
    fn reasoning_required_detection() {
        let analyzer = ComplexityAnalyzer::new();
        
        // Decisions, trade-offs, architecture = reasoning required
        let task = Task {
            description: "Decide between monorepo vs multi-repo strategy".to_string(),
            context: vec![],
        };
        
        let complexity = analyzer.analyze(&task).unwrap();
        assert!(complexity.requires_reasoning);
    }

    #[test]
    fn context_size_analysis() {
        let analyzer = ComplexityAnalyzer::new();
        
        let small_context: Vec<String> = vec!["file1".to_string()];
        let large_context: Vec<String> = (0..50).map(|i| format!("file_{}", i)).collect();
        
        let small_task = Task {
            description: "Fix".to_string(),
            context: small_context,
        };
        let large_task = Task {
            description: "Fix".to_string(),
            context: large_context,
        };
        
        let small_c = analyzer.analyze(&small_task).unwrap();
        let large_c = analyzer.analyze(&large_task).unwrap();
        
        assert!(large_c.score > small_c.score);
    }

    #[test]
    fn caching_decision_detection() {
        let analyzer = ComplexityAnalyzer::new();
        let task = Task {
            description: "Add caching layer to improve query performance".to_string(),
            context: vec!["database.rs".to_string()],
        };
        
        let complexity = analyzer.analyze(&task).unwrap();
        assert!(complexity.requires_specialized_thinking);
    }
}
```

### 1.2 ComplexityAnalyzer implementation

```rust
use anyhow::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ComplexityLevel {
    Simple,
    Moderate,
    Complex,
    VeryComplex,
}

/// Complexity analysis result
#[derive(Debug, Clone)]
pub struct ComplexityResult {
    pub level: ComplexityLevel,
    pub score: f32, // 0-100
    pub reasoning_required: bool,
    pub requires_reasoning: bool,
    pub requires_specialized_thinking: bool,
    pub estimated_context_needed: usize,
}

pub struct Task {
    pub description: String,
    pub context: Vec<String>,
}

pub struct ComplexityAnalyzer {
    // Configuration for analysis
}

impl ComplexityAnalyzer {
    pub fn new() -> Self {
        Self {}
    }

    /// Analyze task complexity
    pub fn analyze(&self, task: &Task) -> Result<ComplexityResult> {
        let mut score = 0.0;

        // Factor 1: Description length and vocabulary (10-30 points)
        let desc_len = task.description.len();
        let complex_keywords = [
            "design", "implement", "architecture", "refactor", "optimize",
            "strategy", "multi-tier", "distributed", "coordin",
        ];
        let keyword_count = complex_keywords
            .iter()
            .filter(|kw| task.description.to_lowercase().contains(kw))
            .count();
        
        score += (keyword_count as f32 * 3.0).min(30.0);
        score += ((desc_len as f32 / 100.0) * 10.0).min(10.0);

        // Factor 2: Context breadth (20-40 points)
        score += (task.context.len() as f32 * 2.0).min(40.0);

        // Factor 3: Reasoning indicators (20-30 points)
        let reasoning_keywords = ["decision", "trade-off", "choose", "evaluate", "analyze"];
        let reasoning_present = reasoning_keywords
            .iter()
            .any(|kw| task.description.to_lowercase().contains(kw));
        
        if reasoning_present {
            score += 20.0;
        }

        // Factor 4: Specialized domain knowledge (10-20 points)
        let specialized_keywords = [
            "caching", "concurrency", "security", "performance",
            "distributed", "consensus", "database", "network",
        ];
        let specialized_present = specialized_keywords
            .iter()
            .any(|kw| task.description.to_lowercase().contains(kw));
        
        if specialized_present {
            score += 15.0;
        }

        let level = match score as i32 {
            0..=25 => ComplexityLevel::Simple,
            26..=50 => ComplexityLevel::Moderate,
            51..=75 => ComplexityLevel::Complex,
            _ => ComplexityLevel::VeryComplex,
        };

        Ok(ComplexityResult {
            level,
            score: score.min(100.0),
            reasoning_required: reasoning_present,
            requires_reasoning: reasoning_present,
            requires_specialized_thinking: specialized_present,
            estimated_context_needed: (score / 10.0) as usize,
        })
    }
}

impl Default for ComplexityAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}
```

---

## Chunk 2: Cost Optimizer (rustycode-orchestration/src/cost_optimizer.rs)

### 2.1 Cost and performance trade-offs

**File**: `crates/rustycode-orchestration/src/cost_optimizer.rs`

**RED -- Tests**:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn optimize_for_cost() {
        let optimizer = CostOptimizer::new();
        let budget = TokenBudget {
            total_tokens: 1000,
            remaining: 1000,
        };
        
        let choice = optimizer.optimize(&budget, OptimizationGoal::Cost).unwrap();
        assert_eq!(choice, ModelChoice::Haiku);
    }

    #[test]
    fn optimize_for_quality() {
        let optimizer = CostOptimizer::new();
        let budget = TokenBudget {
            total_tokens: 10000,
            remaining: 10000,
        };
        
        let choice = optimizer.optimize(&budget, OptimizationGoal::Quality).unwrap();
        assert_eq!(choice, ModelChoice::Opus);
    }

    #[test]
    fn optimize_for_speed() {
        let optimizer = CostOptimizer::new();
        let budget = TokenBudget {
            total_tokens: 5000,
            remaining: 5000,
        };
        
        let choice = optimizer.optimize(&budget, OptimizationGoal::Speed).unwrap();
        assert_eq!(choice, ModelChoice::Haiku); // Fastest and cheapest
    }

    #[test]
    fn cost_per_model() {
        let optimizer = CostOptimizer::new();
        let haiku_cost = optimizer.cost_for_tokens(ModelChoice::Haiku, 1000).unwrap();
        let sonnet_cost = optimizer.cost_for_tokens(ModelChoice::Sonnet, 1000).unwrap();
        let opus_cost = optimizer.cost_for_tokens(ModelChoice::Opus, 1000).unwrap();
        
        assert!(haiku_cost < sonnet_cost);
        assert!(sonnet_cost < opus_cost);
    }

    #[test]
    fn performance_tier_selection() {
        let optimizer = CostOptimizer::new();
        
        // Under budget: use best model available
        let tier1 = optimizer.select_tier_within_budget(10000, 1000).unwrap();
        assert!(tier1 == ModelChoice::Sonnet || tier1 == ModelChoice::Opus);
        
        // Tight budget: use cheapest
        let tier2 = optimizer.select_tier_within_budget(500, 1000).unwrap();
        assert_eq!(tier2, ModelChoice::Haiku);
    }

    #[test]
    fn cost_benefit_analysis() {
        let optimizer = CostOptimizer::new();
        let analysis = optimizer.cost_benefit_analysis(
            ModelChoice::Haiku,
            ModelChoice::Opus,
            1000,
        ).unwrap();
        
        assert!(analysis.cost_ratio > 1.0); // Opus costs more
    }
}
```

### 2.2 CostOptimizer implementation

```rust
use anyhow::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelChoice {
    Haiku,
    Sonnet,
    Opus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptimizationGoal {
    Cost,
    Quality,
    Speed,
    Balanced,
}

pub struct TokenBudget {
    pub total_tokens: usize,
    pub remaining: usize,
}

#[derive(Debug, Clone)]
pub struct CostBenefit {
    pub cost_ratio: f32,
    pub quality_gain: f32,
    pub speed_loss: f32,
}

pub struct CostOptimizer;

impl CostOptimizer {
    pub fn new() -> Self {
        Self
    }

    /// Optimize model choice for a goal
    pub fn optimize(&self, budget: &TokenBudget, goal: OptimizationGoal) -> Result<ModelChoice> {
        match goal {
            OptimizationGoal::Cost => Ok(ModelChoice::Haiku),
            OptimizationGoal::Quality => {
                if budget.remaining > 5000 {
                    Ok(ModelChoice::Opus)
                } else if budget.remaining > 2000 {
                    Ok(ModelChoice::Sonnet)
                } else {
                    Ok(ModelChoice::Haiku)
                }
            }
            OptimizationGoal::Speed => Ok(ModelChoice::Haiku),
            OptimizationGoal::Balanced => {
                if budget.remaining > 3000 {
                    Ok(ModelChoice::Sonnet)
                } else {
                    Ok(ModelChoice::Haiku)
                }
            }
        }
    }

    /// Calculate cost per token for a model
    pub fn cost_for_tokens(&self, model: ModelChoice, tokens: usize) -> Result<f32> {
        let cost_per_1k = match model {
            ModelChoice::Haiku => 0.08,   // $0.08 per 1K tokens (cheap)
            ModelChoice::Sonnet => 0.30,  // $0.30 per 1K tokens (mid)
            ModelChoice::Opus => 0.80,    // $0.80 per 1K tokens (expensive)
        };
        
        Ok((tokens as f32 / 1000.0) * cost_per_1k)
    }

    /// Select the best tier within budget
    pub fn select_tier_within_budget(&self, budget_tokens: usize, task_tokens: usize) -> Result<ModelChoice> {
        if task_tokens > budget_tokens {
            return Ok(ModelChoice::Haiku); // Can't afford better
        }

        // Prefer better models if budget allows
        if budget_tokens > task_tokens * 5 {
            Ok(ModelChoice::Opus)
        } else if budget_tokens > task_tokens * 2 {
            Ok(ModelChoice::Sonnet)
        } else {
            Ok(ModelChoice::Haiku)
        }
    }

    /// Analyze cost-benefit between two models
    pub fn cost_benefit_analysis(
        &self,
        model_a: ModelChoice,
        model_b: ModelChoice,
        tokens: usize,
    ) -> Result<CostBenefit> {
        let cost_a = self.cost_for_tokens(model_a, tokens)?;
        let cost_b = self.cost_for_tokens(model_b, tokens)?;

        Ok(CostBenefit {
            cost_ratio: cost_b / cost_a,
            quality_gain: 0.2, // Estimated
            speed_loss: 0.0,
        })
    }
}

impl Default for CostOptimizer {
    fn default() -> Self {
        Self::new()
    }
}
```

---

## Chunk 3: Model Selector (rustycode-orchestration/src/model_selector.rs)

### 3.1 Unified model selection logic

**File**: `crates/rustycode-orchestration/src/model_selector.rs`

**RED -- Tests**:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn select_model_for_simple_task() {
        let selector = ModelSelector::new();
        let request = SelectionRequest {
            complexity: 25.0,
            has_code_context: true,
            budget_tokens: 5000,
            quality_requirement: QualityTier::Standard,
        };
        
        let selected = selector.select(&request).unwrap();
        assert_eq!(selected, ModelChoice::Haiku);
    }

    #[test]
    fn select_model_for_complex_task() {
        let selector = ModelSelector::new();
        let request = SelectionRequest {
            complexity: 80.0,
            has_code_context: true,
            budget_tokens: 10000,
            quality_requirement: QualityTier::High,
        };
        
        let selected = selector.select(&request).unwrap();
        assert_eq!(selected, ModelChoice::Opus);
    }

    #[test]
    fn respect_quality_requirements() {
        let selector = ModelSelector::new();
        
        let critical = SelectionRequest {
            complexity: 50.0,
            has_code_context: true,
            budget_tokens: 2000,
            quality_requirement: QualityTier::Critical,
        };
        
        let selected = selector.select(&critical).unwrap();
        // Critical should force better model even if expensive
        assert_ne!(selected, ModelChoice::Haiku);
    }

    #[test]
    fn selection_rationale() {
        let selector = ModelSelector::new();
        let request = SelectionRequest {
            complexity: 60.0,
            has_code_context: true,
            budget_tokens: 5000,
            quality_requirement: QualityTier::Standard,
        };
        
        let (model, rationale) = selector.select_with_reason(&request).unwrap();
        assert!(!rationale.is_empty());
    }

    #[test]
    fn model_recommendation_confidence() {
        let selector = ModelSelector::new();
        let request = SelectionRequest {
            complexity: 50.0,
            has_code_context: true,
            budget_tokens: 5000,
            quality_requirement: QualityTier::Standard,
        };
        
        let recommendation = selector.recommend(&request).unwrap();
        assert!(recommendation.confidence > 0.0 && recommendation.confidence <= 1.0);
    }
}
```

### 3.2 ModelSelector implementation

```rust
use super::{ComplexityAnalyzer, CostOptimizer, ModelChoice};
use anyhow::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QualityTier {
    Standard,
    High,
    Critical,
}

pub struct SelectionRequest {
    pub complexity: f32,
    pub has_code_context: bool,
    pub budget_tokens: usize,
    pub quality_requirement: QualityTier,
}

#[derive(Debug, Clone)]
pub struct Recommendation {
    pub model: ModelChoice,
    pub confidence: f32,
    pub rationale: String,
}

pub struct ModelSelector {
    complexity_analyzer: ComplexityAnalyzer,
    cost_optimizer: CostOptimizer,
}

impl ModelSelector {
    pub fn new() -> Self {
        Self {
            complexity_analyzer: ComplexityAnalyzer::new(),
            cost_optimizer: CostOptimizer::new(),
        }
    }

    /// Select the best model for a request
    pub fn select(&self, request: &SelectionRequest) -> Result<ModelChoice> {
        let (model, _) = self.select_with_reason(request)?;
        Ok(model)
    }

    /// Select with explanation
    pub fn select_with_reason(&self, request: &SelectionRequest) -> Result<(ModelChoice, String)> {
        let mut model = ModelChoice::Sonnet; // Default
        let mut rationale = String::new();

        // Quality-first: if critical, use best available
        if request.quality_requirement == QualityTier::Critical {
            model = ModelChoice::Opus;
            rationale.push_str("Critical quality requirement → Opus");
        } else if request.complexity > 70.0 {
            // Complex task
            if request.budget_tokens > 5000 {
                model = ModelChoice::Opus;
                rationale.push_str("High complexity, ample budget → Opus");
            } else if request.budget_tokens > 3000 {
                model = ModelChoice::Sonnet;
                rationale.push_str("High complexity, moderate budget → Sonnet");
            } else {
                model = ModelChoice::Haiku;
                rationale.push_str("High complexity, tight budget → Haiku");
            }
        } else if request.complexity < 30.0 {
            // Simple task
            model = ModelChoice::Haiku;
            rationale.push_str("Simple task → Haiku (cost-optimized)");
        } else {
            // Moderate
            if request.quality_requirement == QualityTier::High {
                model = ModelChoice::Sonnet;
                rationale.push_str("Moderate complexity, high quality requirement → Sonnet");
            } else {
                model = ModelChoice::Haiku;
                rationale.push_str("Moderate complexity → Haiku");
            }
        }

        Ok((model, rationale))
    }

    /// Generate a recommendation with confidence
    pub fn recommend(&self, request: &SelectionRequest) -> Result<Recommendation> {
        let (model, rationale) = self.select_with_reason(request)?;

        // Confidence based on how well the request aligns with model
        let confidence = match model {
            ModelChoice::Opus => 0.95,
            ModelChoice::Sonnet => 0.90,
            ModelChoice::Haiku => 0.85,
        };

        Ok(Recommendation {
            model,
            confidence,
            rationale,
        })
    }
}

impl Default for ModelSelector {
    fn default() -> Self {
        Self::new()
    }
}
```

---

## Chunk 4: Routing Metrics (rustycode-orchestration/src/routing_metrics.rs)

### 4.1 Performance tracking and optimization

**File**: `crates/rustycode-orchestration/src/routing_metrics.rs`

**RED -- Tests**:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_execution_metric() {
        let mut metrics = RoutingMetrics::new();
        metrics.record_execution(
            ModelChoice::Haiku,
            1000,
            ExecutionResult::Success { tokens_used: 900 },
        );
        
        assert_eq!(metrics.haiku_executions, 1);
    }

    #[test]
    fn track_success_rate() {
        let mut metrics = RoutingMetrics::new();
        metrics.record_execution(ModelChoice::Sonnet, 2000, ExecutionResult::Success { tokens_used: 1800 });
        metrics.record_execution(ModelChoice::Sonnet, 2000, ExecutionResult::Success { tokens_used: 1900 });
        metrics.record_execution(ModelChoice::Sonnet, 2000, ExecutionResult::Failure);
        
        let rate = metrics.success_rate(ModelChoice::Sonnet).unwrap();
        assert!(rate > 0.6 && rate < 0.7);
    }

    #[test]
    fn calculate_average_cost() {
        let mut metrics = RoutingMetrics::new();
        metrics.record_execution(ModelChoice::Haiku, 1000, ExecutionResult::Success { tokens_used: 1000 });
        metrics.record_execution(ModelChoice::Haiku, 1000, ExecutionResult::Success { tokens_used: 900 });
        
        let avg_cost = metrics.average_cost(ModelChoice::Haiku).unwrap();
        assert!(avg_cost > 0.0);
    }

    #[test]
    fn model_effectiveness_score() {
        let mut metrics = RoutingMetrics::new();
        metrics.record_execution(ModelChoice::Sonnet, 2000, ExecutionResult::Success { tokens_used: 1900 });
        
        let score = metrics.effectiveness_score(ModelChoice::Sonnet).unwrap();
        assert!(score >= 0.0);
    }

    #[test]
    fn recommendation_based_on_metrics() {
        let mut metrics = RoutingMetrics::new();
        // Haiku: excellent success rate
        for _ in 0..10 {
            metrics.record_execution(
                ModelChoice::Haiku,
                500,
                ExecutionResult::Success { tokens_used: 400 },
            );
        }
        // Opus: poor success rate
        for _ in 0..5 {
            metrics.record_execution(ModelChoice::Opus, 5000, ExecutionResult::Failure);
        }
        
        let recommendation = metrics.recommend_model().unwrap();
        // Should recommend Haiku based on success rate
    }
}
```

### 4.2 RoutingMetrics implementation

```rust
use super::ModelChoice;
use anyhow::Result;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub enum ExecutionResult {
    Success { tokens_used: usize },
    Failure,
}

#[derive(Debug, Clone)]
struct ModelMetrics {
    total_executions: usize,
    successful_executions: usize,
    total_tokens_allocated: usize,
    total_tokens_used: usize,
    total_cost: f32,
}

pub struct RoutingMetrics {
    haiku_executions: usize,
    sonnet_executions: usize,
    opus_executions: usize,
    per_model: HashMap<String, ModelMetrics>,
}

impl RoutingMetrics {
    pub fn new() -> Self {
        Self {
            haiku_executions: 0,
            sonnet_executions: 0,
            opus_executions: 0,
            per_model: HashMap::new(),
        }
    }

    /// Record a model execution
    pub fn record_execution(
        &mut self,
        model: ModelChoice,
        tokens_allocated: usize,
        result: ExecutionResult,
    ) {
        let model_name = match model {
            ModelChoice::Haiku => {
                self.haiku_executions += 1;
                "haiku"
            }
            ModelChoice::Sonnet => {
                self.sonnet_executions += 1;
                "sonnet"
            }
            ModelChoice::Opus => {
                self.opus_executions += 1;
                "opus"
            }
        };

        let metrics = self.per_model.entry(model_name.to_string())
            .or_insert_with(|| ModelMetrics {
                total_executions: 0,
                successful_executions: 0,
                total_tokens_allocated: 0,
                total_tokens_used: 0,
                total_cost: 0.0,
            });

        metrics.total_executions += 1;
        metrics.total_tokens_allocated += tokens_allocated;

        if let ExecutionResult::Success { tokens_used } = result {
            metrics.successful_executions += 1;
            metrics.total_tokens_used += tokens_used;
        }
    }

    /// Calculate success rate for a model
    pub fn success_rate(&self, model: ModelChoice) -> Result<f32> {
        let name = match model {
            ModelChoice::Haiku => "haiku",
            ModelChoice::Sonnet => "sonnet",
            ModelChoice::Opus => "opus",
        };

        if let Some(metrics) = self.per_model.get(name) {
            if metrics.total_executions == 0 {
                return Ok(0.0);
            }
            Ok(metrics.successful_executions as f32 / metrics.total_executions as f32)
        } else {
            Ok(0.0)
        }
    }

    /// Calculate average cost for a model
    pub fn average_cost(&self, model: ModelChoice) -> Result<f32> {
        let name = match model {
            ModelChoice::Haiku => "haiku",
            ModelChoice::Sonnet => "sonnet",
            ModelChoice::Opus => "opus",
        };

        if let Some(metrics) = self.per_model.get(name) {
            if metrics.total_executions == 0 {
                return Ok(0.0);
            }
            Ok(metrics.total_cost / metrics.total_executions as f32)
        } else {
            Ok(0.0)
        }
    }

    /// Calculate effectiveness score (combined metric)
    pub fn effectiveness_score(&self, model: ModelChoice) -> Result<f32> {
        let success = self.success_rate(model)?;
        let avg_cost = self.average_cost(model)?;
        
        // Higher success rate and lower cost = higher score
        Ok((success * 100.0) - (avg_cost * 0.1))
    }

    /// Recommend best model based on metrics
    pub fn recommend_model(&self) -> Result<ModelChoice> {
        let haiku_score = self.effectiveness_score(ModelChoice::Haiku).unwrap_or(0.0);
        let sonnet_score = self.effectiveness_score(ModelChoice::Sonnet).unwrap_or(0.0);
        let opus_score = self.effectiveness_score(ModelChoice::Opus).unwrap_or(0.0);

        if haiku_score >= sonnet_score && haiku_score >= opus_score {
            Ok(ModelChoice::Haiku)
        } else if sonnet_score >= opus_score {
            Ok(ModelChoice::Sonnet)
        } else {
            Ok(ModelChoice::Opus)
        }
    }
}

impl Default for RoutingMetrics {
    fn default() -> Self {
        Self::new()
    }
}
```

---

## Chunk 5: Router (rustycode-orchestration/src/router.rs)

### 5.1 Unified routing orchestrator

**File**: `crates/rustycode-orchestration/src/router.rs`

**RED -- Tests**:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn route_request() {
        let router = TaskRouter::new();
        let request = RoutingRequest {
            description: "Format file".to_string(),
            complexity: 20.0,
            context_size: 1,
            budget: 5000,
        };
        
        let routing = router.route(&request).unwrap();
        assert_eq!(routing.selected_model, ModelChoice::Haiku);
    }

    #[test]
    fn routing_includes_rationale() {
        let router = TaskRouter::new();
        let request = RoutingRequest {
            description: "Design system".to_string(),
            complexity: 85.0,
            context_size: 10,
            budget: 10000,
        };
        
        let routing = router.route(&request).unwrap();
        assert!(!routing.rationale.is_empty());
    }

    #[test]
    fn routing_respects_budget() {
        let router = TaskRouter::new();
        let tight_budget = RoutingRequest {
            description: "Complex task".to_string(),
            complexity: 80.0,
            context_size: 10,
            budget: 1000, // Very tight
        };
        
        let routing = router.route(&tight_budget).unwrap();
        // Should not recommend expensive model for tight budget
        assert_eq!(routing.selected_model, ModelChoice::Haiku);
    }

    #[test]
    fn routing_metrics_integration() {
        let mut router = TaskRouter::new();
        let request = RoutingRequest {
            description: "Task".to_string(),
            complexity: 50.0,
            context_size: 5,
            budget: 5000,
        };
        
        let routing = router.route(&request).unwrap();
        // Metrics should be updated
    }
}
```

### 5.2 TaskRouter implementation

```rust
use super::{
    ComplexityAnalyzer, CostOptimizer, ModelSelector,
    ModelChoice, SelectionRequest, QualityTier, RoutingMetrics,
};
use anyhow::Result;

pub struct RoutingRequest {
    pub description: String,
    pub complexity: f32,
    pub context_size: usize,
    pub budget: usize,
}

#[derive(Debug, Clone)]
pub struct RoutingDecision {
    pub selected_model: ModelChoice,
    pub rationale: String,
    pub confidence: f32,
    pub estimated_cost: f32,
}

pub struct TaskRouter {
    analyzer: ComplexityAnalyzer,
    optimizer: CostOptimizer,
    selector: ModelSelector,
    metrics: RoutingMetrics,
}

impl TaskRouter {
    pub fn new() -> Self {
        Self {
            analyzer: ComplexityAnalyzer::new(),
            optimizer: CostOptimizer::new(),
            selector: ModelSelector::new(),
            metrics: RoutingMetrics::new(),
        }
    }

    /// Route a request to the best model
    pub fn route(&self, request: &RoutingRequest) -> Result<RoutingDecision> {
        let selection_request = SelectionRequest {
            complexity: request.complexity,
            has_code_context: request.context_size > 0,
            budget_tokens: request.budget,
            quality_requirement: self.infer_quality_requirement(request.complexity),
        };

        let recommendation = self.selector.recommend(&selection_request)?;

        Ok(RoutingDecision {
            selected_model: recommendation.model,
            rationale: recommendation.rationale,
            confidence: recommendation.confidence,
            estimated_cost: self.estimate_cost(recommendation.model, request.budget)?,
        })
    }

    fn infer_quality_requirement(&self, complexity: f32) -> QualityTier {
        if complexity > 75.0 {
            QualityTier::Critical
        } else if complexity > 50.0 {
            QualityTier::High
        } else {
            QualityTier::Standard
        }
    }

    fn estimate_cost(&self, model: ModelChoice, tokens: usize) -> Result<f32> {
        self.optimizer.cost_for_tokens(model, tokens)
    }
}

impl Default for TaskRouter {
    fn default() -> Self {
        Self::new()
    }
}
```

---

## Chunk 6: Module Wiring and Integration

Update `crates/rustycode-orchestration/src/lib.rs`:

```rust
pub mod router;
pub mod complexity_analyzer;
pub mod cost_optimizer;
pub mod model_selector;
pub mod routing_metrics;

pub use router::{TaskRouter, RoutingRequest, RoutingDecision};
pub use complexity_analyzer::ComplexityAnalyzer;
pub use cost_optimizer::CostOptimizer;
pub use model_selector::ModelSelector;
pub use routing_metrics::RoutingMetrics;
```

Update `crates/rustycode-orchestration/src/execution.rs`:

```rust
pub async fn execute_with_routing(&mut self, step: &Step) -> Result<StepResult> {
    // Analyze task and route to appropriate model
    let routing_request = RoutingRequest {
        description: step.instruction.clone(),
        complexity: analyze_step_complexity(step)?,
        context_size: step.context_files.len(),
        budget: self.remaining_budget,
    };

    let decision = self.router.route(&routing_request)?;
    
    // Execute with selected model
    let result = self.execute_with_model(step, decision.selected_model).await?;
    
    Ok(result)
}
```

---

## Chunk 7: Full Workspace Verification

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

### Expected test count

| Module | Tests |
|--------|-------|
| rustycode-orchestration/src/complexity_analyzer.rs | 7 |
| rustycode-orchestration/src/cost_optimizer.rs | 6 |
| rustycode-orchestration/src/model_selector.rs | 6 |
| rustycode-orchestration/src/routing_metrics.rs | 6 |
| rustycode-orchestration/src/router.rs | 4 |
| Integration tests | 5 |
| **Total** | **34** |

---

## Integration Guide

### Model routing decision flow

```
Task arrives
        |
        v
TaskRouter::route()
        |
        +--> ComplexityAnalyzer::analyze()
        |    (Low: 0-30, Moderate: 30-60, High: 60+)
        |
        +--> CostOptimizer::select_tier_within_budget()
        |    (Haiku: $0.08/1K, Sonnet: $0.30/1K, Opus: $0.80/1K)
        |
        +--> ModelSelector::recommend()
        |    (Complexity + Quality Tier + Budget → Model)
        |
        v
RoutingDecision (model + rationale + cost estimate)
        |
        v
Execute with selected model
        |
        v
RoutingMetrics::record_execution()
        (Track success rate, cost, effectiveness)
```

### Model selection matrix

| Complexity | Quality Tier | Budget | Selected |
|------------|--------------|--------|----------|
| Low (0-30) | Standard | Any | Haiku |
| Low | High | >2000 | Sonnet |
| Moderate (30-60) | Standard | Any | Haiku |
| Moderate | High | Any | Sonnet |
| High (60+) | Standard | >3000 | Sonnet |
| High | High | >5000 | Opus |
| High | Critical | Any | Opus |

---

## Next Actions

1. **Chunk 1-2**: Implement complexity analyzer and cost optimizer (2-3 hours)
2. **Chunk 3-4**: Implement model selector and metrics (2-3 hours)
3. **Chunk 5-6**: Implement router and wire integration (2-3 hours)
4. **Chunk 7**: Workspace verification (1 hour)
5. **Follow-up**: Integrate into step execution pipeline (separate PR)
6. **Follow-up**: Add telemetry dashboard for routing decisions
7. **Follow-up**: Implement A/B testing for routing strategies
8. **Follow-up**: Add per-project routing preferences

---

**Status**: Ready for implementation
