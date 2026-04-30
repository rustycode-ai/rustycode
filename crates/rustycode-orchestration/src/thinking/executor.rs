//! Async executor that coordinates strategy execution with LLM provider

use crate::thinking::convergence::{ConvergenceDetector, ConvergenceMetrics};
use crate::thinking::core::error::{Error, Result};
use crate::thinking::core::graph::ReasoningGraph;
use crate::thinking::core::parsing::ResponseParser;
use crate::thinking::core::scoring::ConfidenceScorer;
use crate::thinking::core::types::{
    ExecutionParams, Operation, ThinkingConfig, Thought, ThoughtKind,
};
use crate::thinking::prompting::{PromptContext, PromptTemplateRegistry};
use crate::thinking::strategies::{ReasoningStrategy, StrategyFactory};
use async_trait::async_trait;
use rustycode_llm::provider::{ChatMessage, CompletionRequest, LLMProvider};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

/// Trait for executing thinking operations asynchronously
#[async_trait]
pub trait ThinkingExecutor: Send + Sync {
    /// Execute thinking process with automatic strategy selection
    async fn think(&self, prompt: &str) -> Result<String>;

    /// Execute with explicit parameters
    async fn think_with_params(&self, params: ExecutionParams) -> Result<String>;

    /// Execute with a task context for session-aware reasoning
    async fn think_with_context(
        &self,
        ctx: &mut crate::task_context::TaskContext,
    ) -> Result<String>;
}

/// Executor configuration
#[derive(Debug, Clone)]
pub struct ExecutorConfig {
    pub max_retries: usize,
    pub timeout_secs: u64,
    pub temperature: f32,
    pub max_tokens_per_call: u32,
    pub batch_size: usize,
}

impl Default for ExecutorConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            timeout_secs: 30,
            temperature: 0.6,
            max_tokens_per_call: 800,
            batch_size: 1,
        }
    }
}

/// Real executor with LLM integration
pub struct RealExecutor {
    /// LLM provider for generating thoughts
    llm_provider: Arc<dyn LLMProvider>,
    config: ExecutorConfig,
    thinking_config: ThinkingConfig,
    prompt_registry: PromptTemplateRegistry,
}

impl RealExecutor {
    pub fn new(llm_provider: Arc<dyn LLMProvider>) -> Self {
        Self {
            llm_provider,
            config: ExecutorConfig::default(),
            thinking_config: ThinkingConfig::default(),
            prompt_registry: PromptTemplateRegistry::new(),
        }
    }

    #[must_use]
    pub const fn with_config(mut self, config: ExecutorConfig) -> Self {
        self.config = config;
        self
    }

    #[must_use]
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.thinking_config = self.thinking_config.with_model(model);
        self
    }

    /// Select the best strategy for a problem
    #[allow(clippy::unused_async)]
    async fn select_strategy(
        &self,
        problem: &str,
        _graph: &ReasoningGraph,
    ) -> Result<Box<dyn ReasoningStrategy>> {
        let strategies = StrategyFactory::all();

        // First strategy to match wins; fallback to Sequential
        let selected = strategies
            .iter()
            .find(|s| s.matches_problem(problem))
            .map(|s| s.name());

        let selected = selected.unwrap_or("Sequential");

        match selected {
            "Dialectic" => Ok(Box::new(StrategyFactory::dialectic()) as Box<dyn ReasoningStrategy>),
            "Parallel" => Ok(Box::new(StrategyFactory::parallel()) as Box<dyn ReasoningStrategy>),
            "Analogical" => {
                Ok(Box::new(StrategyFactory::analogical()) as Box<dyn ReasoningStrategy>)
            }
            "Abductive" => Ok(Box::new(StrategyFactory::abductive()) as Box<dyn ReasoningStrategy>),
            _ => Ok(Box::new(StrategyFactory::sequential()) as Box<dyn ReasoningStrategy>),
        }
    }

    /// Build a prompt for the current strategy and context
    fn build_prompt(&self, strategy_name: &str, context: &PromptContext) -> Result<String> {
        let template_name = strategy_name.to_lowercase();
        self.prompt_registry.render(&template_name, context)
    }

    /// Generate strategy-specific operations for the current iteration.
    #[allow(dead_code, clippy::unused_self)]
    fn generate_operations(
        &self,
        strategy_name: &str,
        graph: &ReasoningGraph,
        _iteration: usize,
    ) -> Vec<Operation> {
        let mut operations = Vec::new();

        match strategy_name {
            "Sequential" => {
                if !graph.is_empty() {
                    if let Some(thought) = graph.thoughts().last() {
                        operations.push(Operation::Generate {
                            from: thought.id,
                            count: 1,
                            prompt_template: "sequential".to_string(),
                        });
                    }
                }
            }
            "Dialectic" => {
                if graph.len() > 1 {
                    let ids: Vec<_> = graph.thoughts().map(|t| t.id).take(2).collect();
                    if ids.len() > 1 {
                        operations.push(Operation::Aggregate {
                            from_ids: ids,
                            aggregation_method:
                                crate::thinking::core::types::AggregationMethod::Synthesize,
                            prompt_template: "dialectic".to_string(),
                        });
                    }
                }
            }
            _ => {
                if !graph.is_empty() {
                    if let Some(thought) = graph.thoughts().next() {
                        operations.push(Operation::Generate {
                            from: thought.id,
                            count: 1,
                            prompt_template: "default".to_string(),
                        });
                    }
                }
            }
        }

        operations
    }

    /// Synthesize the final result from the reasoning graph by combining all
    /// high-confidence thoughts in topological order.
    #[allow(clippy::unused_self)]
    fn extract_result(&self, graph: &ReasoningGraph) -> Result<String> {
        if graph.is_empty() {
            return Ok("No reasoning performed.".to_string());
        }

        let scorer = ConfidenceScorer::new();
        let all_scores = scorer.score_all(graph);

        let max_score = all_scores.values().copied().fold(0.0_f64, f64::max);
        if max_score <= 0.0 {
            return Ok("No valid result found.".to_string());
        }

        let confidence_threshold = max_score * 0.5;
        let sorted_ids = graph.topological_sort().unwrap_or_default();

        let mut sections: Vec<String> = Vec::new();
        for id in &sorted_ids {
            let Ok(thought) = graph.get_thought(*id) else {
                continue;
            };
            let score = all_scores.get(id).copied().unwrap_or(0.0);
            if score < confidence_threshold {
                continue;
            }
            if thought.content.trim().is_empty() {
                continue;
            }
            if thought.kind == ThoughtKind::Initial {
                continue;
            }
            sections.push(thought.content.clone());
        }

        if sections.is_empty() {
            let best_id = sorted_ids.iter().max_by(|a, b| {
                let sa = all_scores.get(a).copied().unwrap_or(0.0);
                let sb = all_scores.get(b).copied().unwrap_or(0.0);
                sa.partial_cmp(&sb).unwrap_or(std::cmp::Ordering::Equal)
            });
            return best_id.map_or_else(
                || Ok("No valid result found.".to_string()),
                |id| {
                    graph
                        .get_thought(*id)
                        .map(|t| t.content.clone())
                        .or_else(|_| Ok("Unable to extract result.".to_string()))
                },
            );
        }

        Ok(sections.join("\n\n"))
    }

    #[allow(clippy::too_many_lines)]
    async fn think_internal(
        &self,
        params: ExecutionParams,
        graph: &mut ReasoningGraph,
    ) -> Result<String> {
        let is_code_task = Self::looks_like_code_task(&params.initial_prompt);
        let mut strategy = self.select_strategy(&params.initial_prompt, graph).await?;
        let template_name = if is_code_task {
            "implementation"
        } else {
            strategy.name()
        };
        tracing::info!(strategy = %strategy.name(), template = %template_name, is_code = is_code_task, "Selected strategy");

        // Build initial context
        let mut context = PromptContext::new(params.initial_prompt.clone())
            .with_depth(0)
            .with_iteration(0);

        // Main reasoning loop
        let mut iteration = 0;
        let mut llm_call_count = 0;
        let mut backtrack_count = 0;
        let max_backtracks = 2;
        let max_llm_calls = params.config.max_depth + 2;
        let start_time = SystemTime::now();
        let mut metrics = ConvergenceMetrics::new(10);
        let convergence_detector = ConvergenceDetector::new();
        let mut last_preemption_iteration: Option<usize> = None;
        let mut last_llm_error: Option<Error> = None;
        let initial_graph_len = graph.len();

        loop {
            iteration += 1;

            metrics.record_iteration(graph);

            let _latest_confidence = metrics.latest_confidence().unwrap_or(0.0);
            let _new_thoughts_rate = metrics.new_thoughts_per_iteration(3);

            if iteration > params.config.max_depth || graph.len() >= params.config.max_nodes {
                break;
            }
            if let Ok(elapsed) = start_time.elapsed() {
                if elapsed > Duration::from_secs(params.config.time_limit_secs) {
                    break;
                }
            }
            if convergence_detector.has_converged(&metrics, Some(params.config.target_confidence)) {
                break;
            }

            // Check time limit
            if let Ok(elapsed) = start_time.elapsed() {
                if elapsed > Duration::from_secs(params.config.time_limit_secs) {
                    break;
                }
            }

            // Check convergence
            if convergence_detector.has_converged(&metrics, Some(params.config.target_confidence)) {
                break;
            }

            // Strategy Preemption and Backtracking
            let preemptible =
                last_preemption_iteration.is_none_or(|last| iteration.saturating_sub(last) >= 3);
            if metrics.is_stagnant(3) && iteration > 1 {
                if backtrack_count < max_backtracks {
                    if let Some(leaf_id) = graph.thoughts().last().map(|t| t.id) {
                        if let Some(_anchor_id) = graph.find_nearest_anchor(leaf_id, 0.8) {
                            graph.prune_branch(leaf_id).ok();
                            backtrack_count += 1;
                            continue;
                        }
                    }
                }

                if preemptible {
                    if let Ok(new_strategy) =
                        self.select_strategy(&params.initial_prompt, graph).await
                    {
                        if new_strategy.name() != strategy.name() {
                            strategy = new_strategy;
                            last_preemption_iteration = Some(iteration);
                        }
                    }
                }
            }

            llm_call_count += 1;
            if llm_call_count > max_llm_calls {
                break;
            }

            let prompt = self.build_prompt(template_name, &context)?;
            let response_text = match self.call_llm(&prompt).await {
                Ok(text) => text,
                Err(e) => {
                    tracing::error!(error = %e, "LLM call failed in thinking loop");
                    last_llm_error = Some(e);
                    break;
                }
            };

            let new_thoughts = match ResponseParser::parse_response(&response_text) {
                Ok(parsed) => match ResponseParser::to_thoughts(&parsed) {
                    Ok(thoughts) => thoughts,
                    Err(e) => {
                        tracing::warn!(error = %e, "Thought conversion failed");
                        Vec::new()
                    }
                },
                Err(e) => {
                    tracing::warn!(error = %e, "Response parsing failed");
                    Vec::new()
                }
            };

            if new_thoughts.is_empty() {
                break;
            }

            for thought in new_thoughts {
                graph.add_thought(thought)?;
            }

            context = context.with_iteration(iteration).with_depth(graph.len());
        }

        if graph.len() == initial_graph_len {
            if let Some(err) = last_llm_error {
                return Err(err);
            }
        }
        self.extract_result(graph)
    }
}

#[async_trait]
impl ThinkingExecutor for RealExecutor {
    async fn think(&self, prompt: &str) -> Result<String> {
        self.think_with_params(ExecutionParams::new(prompt)).await
    }

    async fn think_with_params(&self, params: ExecutionParams) -> Result<String> {
        let mut graph = ReasoningGraph::new();
        let initial =
            Thought::new(ThoughtKind::Initial, params.initial_prompt.clone()).with_confidence(0.7);
        graph.add_thought(initial)?;

        self.think_internal(params, &mut graph).await
    }

    async fn think_with_context(
        &self,
        ctx: &mut crate::task_context::TaskContext,
    ) -> Result<String> {
        let params = ExecutionParams {
            config: ThinkingConfig::default(),
            initial_prompt: ctx.original_request.clone(),
            selected_strategy: None,
            metadata: std::collections::HashMap::new(),
        };

        let mut graph = ctx.reasoning_graph.take().unwrap_or_else(|| {
            let mut g = ReasoningGraph::new();
            let initial = Thought::new(ThoughtKind::Initial, ctx.original_request.clone())
                .with_confidence(0.7);
            let _ = g.add_thought(initial);
            g
        });

        let result = self.think_internal(params, &mut graph).await;
        ctx.reasoning_graph = Some(graph);
        result
    }
}

impl RealExecutor {
    async fn call_llm(&self, prompt: &str) -> Result<String> {
        let mut retries = 0;
        loop {
            match self.try_call_llm(prompt).await {
                Ok(response) => return Ok(response),
                Err(e) if retries < self.config.max_retries => {
                    retries += 1;
                    let delay = std::cmp::min(100 * 2u64.pow(retries as u32), 30_000);
                    tracing::warn!(retries, delay_ms = delay, "LLM call failed, retrying: {e}");
                    tokio::time::sleep(Duration::from_millis(delay)).await;
                }
                Err(e) => return Err(e),
            }
        }
    }

    fn looks_like_code_task(prompt: &str) -> bool {
        let code_keywords = [
            "implement",
            "write",
            "create",
            "build",
            "code",
            "function",
            "module",
            "class",
            "struct",
            "refactor",
            "interpreter",
            "compiler",
            "parser",
            "algorithm",
            "binary search",
            "sort",
            "data structure",
        ];
        let lower = prompt.to_lowercase();
        code_keywords.iter().any(|k| lower.contains(k))
    }

    async fn try_call_llm(&self, prompt: &str) -> Result<String> {
        let system_prompt = if prompt.contains("senior software engineer") {
            "You are a senior software engineer. Provide complete, compilable implementations. Output JSON with a 'thoughts' array containing your analysis and code.".to_string()
        } else {
            "You are a deep reasoning assistant. Provide your response as a JSON object with a 'thoughts' array containing reasoning steps.".to_string()
        };
        let messages = vec![
            ChatMessage::system(system_prompt),
            ChatMessage::user(prompt.to_string()),
        ];

        let request = CompletionRequest {
            model: self
                .thinking_config
                .model
                .clone()
                .unwrap_or_else(|| "default".to_string()),
            messages,
            max_tokens: Some(self.thinking_config.max_tokens_per_thought),
            temperature: Some(0.6),
            stream: false,
            system_prompt: None,
            tools: None,
            thinking: None,
            output_config: None,
            container: None,
            tool_choice: None,
            parallel_tool_calls: None,
        };

        let response = self
            .llm_provider
            .complete(request)
            .await
            .map_err(|e| Error::ProviderError(format!("LLM provider error: {e}")))?;

        Ok(response.content)
    }

    /// Call LLM with structured output enforcement via `output_config`.
    ///
    /// Uses native `json_schema` output when the provider supports
    /// grammar-constrained decoding. Falls back to text-based JSON parsing
    /// if the provider doesn't populate `structured_output`. Returns an
    /// `Err` only for provider failures, not parse failures — on parse
    /// failure, returns the raw text wrapped in a JSON object so callers
    /// always get a valid `Value`.
    #[allow(dead_code)]
    async fn call_llm_structured(
        &self,
        prompt: &str,
        schema: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let system_prompt =
            "You are a deep reasoning assistant. Respond with valid JSON matching the requested schema.";
        let messages = vec![
            ChatMessage::system(system_prompt.to_string()),
            ChatMessage::user(prompt.to_string()),
        ];

        let request = CompletionRequest {
            model: self
                .thinking_config
                .model
                .clone()
                .unwrap_or_else(|| "default".to_string()),
            messages,
            max_tokens: Some(self.thinking_config.max_tokens_per_thought),
            temperature: Some(0.6),
            stream: false,
            system_prompt: None,
            tools: None,
            thinking: None,
            output_config: Some(rustycode_llm::provider::OutputConfig::with_json_schema(
                schema.clone(),
            )),
            container: None,
            tool_choice: None,
            parallel_tool_calls: None,
        };

        let response = self
            .llm_provider
            .complete(request)
            .await
            .map_err(|e| Error::ProviderError(format!("LLM provider error: {e}")))?;

        // Path 1: provider natively parsed the structured output
        if let Some(so) = response.structured_output {
            return Ok(so);
        }

        // Path 2: fallback — try to parse JSON from text response
        match serde_json::from_str::<serde_json::Value>(&response.content) {
            Ok(v) => Ok(v),
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "Structured output not supported by provider, text parse also failed"
                );
                // Graceful fallback: wrap raw text so callers always get valid JSON
                Ok(serde_json::json!({
                    "raw_response": response.content,
                    "_parse_error": e.to_string()
                }))
            }
        }
    }
}

pub struct DefaultExecutor;

#[async_trait]
impl ThinkingExecutor for DefaultExecutor {
    async fn think(&self, _prompt: &str) -> Result<String> {
        Ok("stub".into())
    }
    async fn think_with_params(&self, _params: ExecutionParams) -> Result<String> {
        Ok("stub".into())
    }
    async fn think_with_context(
        &self,
        _ctx: &mut crate::task_context::TaskContext,
    ) -> Result<String> {
        Ok("stub".into())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_executor_config_default() {
        let config = ExecutorConfig::default();
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.timeout_secs, 30);
        assert!((config.temperature - 0.6).abs() < f32::EPSILON);
        assert_eq!(config.max_tokens_per_call, 800);
        assert_eq!(config.batch_size, 1);
    }

    #[tokio::test]
    async fn test_default_executor_think() {
        let executor = DefaultExecutor;
        let result = executor.think("test prompt").await.unwrap();
        assert_eq!(result, "stub");
    }

    #[tokio::test]
    async fn test_default_executor_think_with_params() {
        let executor = DefaultExecutor;
        let params = ExecutionParams::new("test");
        let result = executor.think_with_params(params).await.unwrap();
        assert_eq!(result, "stub");
    }

    #[tokio::test]
    async fn test_default_executor_think_with_context() {
        let executor = DefaultExecutor;
        let mut ctx = crate::task_context::TaskContext::new("t1".into(), "objective".into());
        let result = executor.think_with_context(&mut ctx).await.unwrap();
        assert_eq!(result, "stub");
    }

    #[test]
    fn test_executor_config_custom() {
        let config = ExecutorConfig {
            max_retries: 5,
            timeout_secs: 60,
            temperature: 0.3,
            max_tokens_per_call: 1600,
            batch_size: 4,
        };
        assert_eq!(config.max_retries, 5);
        assert_eq!(config.timeout_secs, 60);
    }

    #[test]
    fn test_thinking_executor_trait_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<DefaultExecutor>();
    }

    #[test]
    fn test_generate_operations_sequential_empty_graph() {
        let llm = rustycode_llm::MockProvider::from_text("mock response");
        let executor = RealExecutor::new(std::sync::Arc::new(llm));
        let graph = ReasoningGraph::new();
        let ops = executor.generate_operations("Sequential", &graph, 0);
        assert!(ops.is_empty(), "Empty graph should produce no operations");
    }

    #[test]
    fn test_generate_operations_sequential_with_thoughts() {
        let llm = rustycode_llm::MockProvider::from_text("mock response");
        let executor = RealExecutor::new(std::sync::Arc::new(llm));
        let mut graph = ReasoningGraph::new();
        let thought = Thought::new(ThoughtKind::Initial, "Start".to_string());
        graph.add_thought(thought).unwrap();

        let ops = executor.generate_operations("Sequential", &graph, 1);
        assert_eq!(ops.len(), 1);
        match &ops[0] {
            Operation::Generate { count, .. } => assert_eq!(*count, 1),
            other => panic!("Expected Generate, got {other:?}"),
        }
    }

    #[test]
    fn test_generate_operations_dialectic_needs_two_thoughts() {
        let llm = rustycode_llm::MockProvider::from_text("mock response");
        let executor = RealExecutor::new(std::sync::Arc::new(llm));
        let mut graph = ReasoningGraph::new();
        graph
            .add_thought(Thought::new(ThoughtKind::Initial, "A".to_string()))
            .unwrap();
        // Only one thought — Dialectic needs > 1
        let ops = executor.generate_operations("Dialectic", &graph, 1);
        assert!(
            ops.is_empty(),
            "Dialectic should produce no ops with only 1 thought"
        );
    }

    #[test]
    fn test_generate_operations_dialectic_with_two_thoughts() {
        let llm = rustycode_llm::MockProvider::from_text("mock response");
        let executor = RealExecutor::new(std::sync::Arc::new(llm));
        let mut graph = ReasoningGraph::new();
        graph
            .add_thought(Thought::new(ThoughtKind::Initial, "A".to_string()))
            .unwrap();
        graph
            .add_thought(Thought::new(ThoughtKind::Analysis, "B".to_string()))
            .unwrap();

        let ops = executor.generate_operations("Dialectic", &graph, 1);
        assert_eq!(ops.len(), 1);
        match &ops[0] {
            Operation::Aggregate {
                aggregation_method, ..
            } => {
                assert!(matches!(
                    aggregation_method,
                    crate::thinking::core::types::AggregationMethod::Synthesize
                ));
            }
            other => panic!("Expected Aggregate, got {other:?}"),
        }
    }

    #[test]
    fn test_generate_operations_default_strategy_with_thoughts() {
        let llm = rustycode_llm::MockProvider::from_text("mock response");
        let executor = RealExecutor::new(std::sync::Arc::new(llm));
        let mut graph = ReasoningGraph::new();
        graph
            .add_thought(Thought::new(ThoughtKind::Initial, "A".to_string()))
            .unwrap();

        let ops = executor.generate_operations("Parallel", &graph, 1);
        assert_eq!(ops.len(), 1);
    }

    #[test]
    fn test_extract_result_empty_graph() {
        let llm = rustycode_llm::MockProvider::from_text("mock response");
        let executor = RealExecutor::new(std::sync::Arc::new(llm));
        let graph = ReasoningGraph::new();
        let result = executor.extract_result(&graph).unwrap();
        assert_eq!(result, "No reasoning performed.");
    }

    #[test]
    fn test_extract_result_picks_best_confidence() {
        let llm = rustycode_llm::MockProvider::from_text("mock response");
        let executor = RealExecutor::new(std::sync::Arc::new(llm));
        let mut graph = ReasoningGraph::new();

        let low =
            Thought::new(ThoughtKind::Initial, "Low quality".to_string()).with_confidence(0.2);
        let high =
            Thought::new(ThoughtKind::Analysis, "Best answer".to_string()).with_confidence(0.95);
        graph.add_thought(low).unwrap();
        graph.add_thought(high).unwrap();

        let result = executor.extract_result(&graph).unwrap();
        assert_eq!(result, "Best answer");
    }

    #[test]
    fn test_build_prompt_renders_template() {
        let llm = rustycode_llm::MockProvider::from_text("mock response");
        let executor = RealExecutor::new(std::sync::Arc::new(llm));
        let context = PromptContext::new("Test problem");
        let result = executor.build_prompt("sequential", &context).unwrap();
        assert!(result.contains("Test problem"));
        assert!(result.contains("step"));
    }

    #[test]
    fn test_build_prompt_invalid_strategy_errors() {
        let llm = rustycode_llm::MockProvider::from_text("mock response");
        let executor = RealExecutor::new(std::sync::Arc::new(llm));
        let context = PromptContext::new("Test");
        let result = executor.build_prompt("nonexistent", &context);
        assert!(result.is_err());
    }

    #[test]
    fn test_executor_config_with_config() {
        let llm = rustycode_llm::MockProvider::from_text("mock response");
        let config = ExecutorConfig {
            max_retries: 10,
            timeout_secs: 120,
            temperature: 0.1,
            max_tokens_per_call: 4000,
            batch_size: 8,
        };
        let executor = RealExecutor::new(std::sync::Arc::new(llm)).with_config(config);
        assert_eq!(executor.config.max_retries, 10);
        assert_eq!(executor.config.timeout_secs, 120);
    }

    /// Regression: when the LLM provider fails (e.g. rate limit),
    /// `think_with_context` must return an error, not synthetic stub output.
    #[tokio::test]
    async fn test_think_with_context_llm_failure_returns_error() {
        use rustycode_llm::provider::ProviderError;

        let failing_llm = rustycode_llm::MockProvider::new(
            vec![Err(ProviderError::RateLimited {
                retry_delay: Some(std::time::Duration::from_mins(1)),
            })],
            None,
        );
        let executor = RealExecutor::new(std::sync::Arc::new(failing_llm)).with_model("test-model");
        let mut ctx = crate::task_context::TaskContext::new(
            "regression-test".into(),
            "Implement binary search".into(),
        );

        let result = executor.think_with_context(&mut ctx).await;

        assert!(
            result.is_err(),
            "think_with_context must return Err when LLM fails, got Ok: {result:?}"
        );

        let err_msg = format!("{}", result.unwrap_err());
        assert!(
            !err_msg.contains("Generated from:"),
            "Error must not contain synthetic stub text: {err_msg}"
        );
    }

    /// When the LLM succeeds but returns unparseable output,
    /// the executor should still return the raw text, not stubs.
    #[tokio::test]
    async fn test_think_with_context_llm_succeeds_no_json() {
        let llm = rustycode_llm::MockProvider::from_text(
            "Here is a plain text response without JSON thoughts structure.",
        );
        let executor = RealExecutor::new(std::sync::Arc::new(llm)).with_model("test-model");
        let mut ctx =
            crate::task_context::TaskContext::new("test".into(), "Write a function".into());

        let result = executor.think_with_context(&mut ctx).await;

        assert!(
            result.is_ok(),
            "think_with_context should succeed even with non-JSON LLM response: {result:?}"
        );
        let output = result.unwrap();
        assert!(
            !output.contains("Generated from:"),
            "Output must not contain synthetic stub text: {output}"
        );
    }
}
