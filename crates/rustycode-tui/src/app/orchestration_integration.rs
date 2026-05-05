//! Orchestration integration for the TUI message flow.
//!
//! Wraps [`QualityDetector`], [`StrategySelector`], and [`ReasoningStore`]
//! to provide a single integration point. The TUI's `send_message()` path
//! calls into this module to:
//!
//! 1. Analyze message complexity
//! 2. Select an execution strategy
//! 3. Inject the structured thinking tool when warranted
//! 4. Store and retrieve multi-phase reasoning context

use rustycode_orchestration::quality_detector::QualityDetector;
use rustycode_orchestration::reasoning_store::ReasoningStore;
use rustycode_orchestration::strategy_selector::StrategySelector;
use rustycode_orchestration::types::{QualityScore, ReasoningStrategy, StructuredThought};
use std::path::PathBuf;

/// Returns true if the model has built-in extended thinking/reasoning.
///
/// Models with native thinking don't need the structured_thinking tool —
/// they already reason internally. Injecting the tool adds ~1400 tokens
/// of overhead per request and can conflict with the model's own reasoning.
fn has_native_thinking(model_id: &str) -> bool {
    let lower = model_id.to_ascii_lowercase();
    lower.starts_with("glm-5")
        || lower.starts_with("claude-opus-4")
        || lower.starts_with("claude-sonnet-4")
        || lower.contains("deepseek-r1")
        || lower.contains("o1-")
        || lower.contains("o3-")
        || lower.contains("o4-")
}

/// Result of analyzing a user message for orchestration routing.
#[derive(Debug)]
pub struct AnalysisResult {
    /// Numeric complexity score (0.0–5.0).
    pub complexity: f64,
    /// Chosen execution strategy.
    pub strategy: ReasoningStrategy,
    /// Whether the structured thinking tool should be injected.
    pub enable_structured_thinking: bool,
    /// Clarity report from pre-pipeline scoring (only for complex tasks).
    pub clarity_report: Option<rustycode_orchestration::ast::ClarityReport>,
}

/// Unified orchestration integration for the TUI.
pub struct OrchestrationIntegration {
    quality_detector: QualityDetector,
    strategy_selector: StrategySelector,
    reasoning_store: Option<ReasoningStore>,
    current_task_id: Option<String>,
    current_phase: u32,
}

impl OrchestrationIntegration {
    /// Create a new integration instance.
    ///
    /// If `store_path` is `Some`, reasoning persistence is enabled.
    pub fn new(store_path: Option<PathBuf>) -> Self {
        let reasoning_store = store_path.map(ReasoningStore::new);
        Self {
            quality_detector: QualityDetector::new(),
            strategy_selector: StrategySelector::new(),
            reasoning_store,
            current_task_id: None,
            current_phase: 1,
        }
    }

    /// Analyze a user message and determine the execution strategy.
    ///
    /// When `model_id` refers to a model with native thinking (GLM-5.x,
    /// Claude Opus/Sonnet 4+, DeepSeek-R1, OpenAI o-series), the structured
    /// thinking tool is suppressed — these models reason internally.
    pub fn analyze_message(&self, content: &str, model_id: Option<&str>) -> AnalysisResult {
        let complexity = StrategySelector::detect_complexity(content);
        let quality = self.quality_detector.evaluate(content);
        let strategy = self.strategy_selector.select(complexity, &quality, 75);
        let native_thinking = model_id.is_some_and(has_native_thinking);
        let enable_structured_thinking =
            strategy.requires_structured_thinking() && !native_thinking;

        tracing::info!(
            complexity = %format!("{complexity:.2}"),
            strategy = ?strategy,
            enable_structured_thinking,
            "Orchestration analysis"
        );

        // Run clarity assessment for tasks that use structured thinking
        let clarity_report = if enable_structured_thinking {
            let report = rustycode_orchestration::ast::assess_clarity(content);
            tracing::info!(
                ambiguity = report.ambiguity,
                questions = report.questions.len(),
                "Clarity assessment"
            );
            Some(report)
        } else {
            None
        };

        AnalysisResult {
            complexity,
            strategy,
            enable_structured_thinking,
            clarity_report,
        }
    }

    /// Evaluate the quality of an LLM response.
    pub fn evaluate_quality(&self, response: &str) -> QualityScore {
        self.quality_detector.evaluate(response)
    }

    /// Select a strategy given complexity, quality, and confidence.
    pub fn select_strategy(
        &self,
        complexity: f64,
        quality: &QualityScore,
        confidence: u32,
    ) -> ReasoningStrategy {
        self.strategy_selector
            .select(complexity, quality, confidence)
    }

    /// Whether the structured thinking tool should be added to the tools schema.
    pub fn should_enable_structured_thinking(&self, strategy: ReasoningStrategy) -> bool {
        strategy.requires_structured_thinking()
    }

    /// Get the structured thinking tool schema for injection into the LLM request.
    pub fn structured_thinking_tool_schema() -> serde_json::Value {
        rustycode_orchestration::StructuredThinkingToolSchema::schema()
    }

    /// Get the system prompt guidance for structured thinking.
    pub fn structured_thinking_guidance() -> &'static str {
        rustycode_orchestration::StructuredThinkingToolSchema::system_prompt_guidance()
    }

    /// Start a new task for phase tracking.
    pub fn start_task(&mut self, task_id: String) {
        self.current_task_id = Some(task_id);
        self.current_phase = 1;
    }

    pub fn ensure_task(&mut self, task_id: String) {
        if self.current_task_id.is_none() {
            self.current_task_id = Some(task_id);
        }
    }

    /// Advance to the next phase.
    pub fn advance_phase(&mut self) -> u32 {
        self.current_phase = self.current_phase.saturating_add(1);
        self.current_phase
    }

    /// Set the phase to a specific value (used for restoring state).
    pub fn advance_to(&mut self, phase: u32) {
        self.current_phase = phase;
    }

    /// Get the current phase number.
    pub fn current_phase(&self) -> u32 {
        self.current_phase
    }

    /// Handle a structured_thinking tool call from the LLM.
    pub fn handle_structured_thought_tool_call(
        &mut self,
        args: &serde_json::Value,
    ) -> anyhow::Result<StructuredThought> {
        let thought_text = args["thought"].as_str().unwrap_or("").to_string();
        let phase = args["phase"].as_u64().unwrap_or(1) as u32;
        let confidence = args["confidence"].as_u64().unwrap_or(50) as u32;
        let next_thought_needed = args["next_thought_needed"].as_bool().unwrap_or(true);

        let thought_type_str = args["type"].as_str().unwrap_or("decision");
        let thought_type = match thought_type_str {
            "constraint" => rustycode_orchestration::types::ThoughtType::Constraint,
            "validation" => rustycode_orchestration::types::ThoughtType::Validation,
            "learning" => rustycode_orchestration::types::ThoughtType::Learning,
            "hypothesis" => rustycode_orchestration::types::ThoughtType::Hypothesis,
            _ => rustycode_orchestration::types::ThoughtType::Decision,
        };

        let mut thought = StructuredThought::new(thought_text, phase, thought_type);
        thought.confidence = confidence;
        thought.next_thought_needed = next_thought_needed;

        if let (Some(ref store), Some(ref task_id)) = (&self.reasoning_store, &self.current_task_id)
        {
            store.store_thought(task_id, phase, &thought)?;
        }

        Ok(thought)
    }

    /// Get phase context for multi-phase orchestration.
    pub fn get_phase_context(&self) -> Option<serde_json::Value> {
        let store = self.reasoning_store.as_ref()?;
        let task_id = self.current_task_id.as_ref()?;
        store
            .get_context_for_next_phase(task_id, self.current_phase)
            .ok()
    }

    /// Check if reasoning persistence is enabled.
    pub fn has_persistence(&self) -> bool {
        self.reasoning_store.is_some()
    }
}

impl Default for OrchestrationIntegration {
    fn default() -> Self {
        Self::new(None)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_analyze_simple_message() {
        let integration = OrchestrationIntegration::default();
        let result = integration.analyze_message("fix the typo", None);
        // "fix" keyword gives complexity 1.5, default confidence 75 → SequentialThinking
        assert!(result.complexity < 2.0);
    }

    #[test]
    fn test_analyze_complex_message() {
        let integration = OrchestrationIntegration::default();
        let result = integration.analyze_message(
            "Explore the maze solver algorithms and design an architecture",
            None,
        );
        assert!(result.complexity > 3.0);
        assert!(result.enable_structured_thinking);
    }

    #[test]
    fn test_should_enable_structured_thinking() {
        let integration = OrchestrationIntegration::default();
        assert!(
            integration.should_enable_structured_thinking(ReasoningStrategy::SequentialThinking)
        );
        assert!(
            integration.should_enable_structured_thinking(ReasoningStrategy::PhasedOrchestration)
        );
        assert!(!integration.should_enable_structured_thinking(ReasoningStrategy::DirectExecution));
        assert!(!integration.should_enable_structured_thinking(ReasoningStrategy::QuickSelfEval));
    }

    #[test]
    fn test_structured_thinking_tool_schema() {
        let schema = OrchestrationIntegration::structured_thinking_tool_schema();
        assert_eq!(schema["type"], "function");
        assert_eq!(schema["function"]["name"], "structured_thinking");
    }

    #[test]
    fn test_handle_tool_call() {
        let dir = tempfile::tempdir().unwrap();
        let mut integration = OrchestrationIntegration::new(Some(dir.path().to_path_buf()));
        integration.start_task("test-task".to_string());

        let args = serde_json::json!({
            "thought": "Use BFS for graph traversal",
            "phase": 1,
            "type": "decision",
            "confidence": 85,
            "next_thought_needed": true
        });

        let thought = integration
            .handle_structured_thought_tool_call(&args)
            .unwrap();
        assert_eq!(thought.thought, "Use BFS for graph traversal");
        assert_eq!(thought.confidence, 85);
    }

    #[test]
    fn test_phase_tracking() {
        let mut integration = OrchestrationIntegration::default();
        assert_eq!(integration.current_phase(), 1);
        integration.start_task("task-1".to_string());
        let next = integration.advance_phase();
        assert_eq!(next, 2);
    }

    #[test]
    fn test_no_persistence_by_default() {
        let integration = OrchestrationIntegration::default();
        assert!(!integration.has_persistence());
    }

    #[test]
    fn test_phase_context_with_persistence() {
        let dir = tempfile::tempdir().unwrap();
        let mut integration = OrchestrationIntegration::new(Some(dir.path().to_path_buf()));
        integration.start_task("ctx-test".to_string());

        let args = serde_json::json!({
            "thought": "Phase 1 conclusion",
            "phase": 1,
            "type": "decision",
            "confidence": 90,
            "next_thought_needed": false
        });
        integration
            .handle_structured_thought_tool_call(&args)
            .unwrap();
        integration.advance_phase();

        let ctx = integration.get_phase_context();
        assert!(ctx.is_some());
        assert_eq!(ctx.unwrap()["phase"], 2);
    }

    #[test]
    fn test_evaluate_quality_on_llm_response() {
        let integration = OrchestrationIntegration::default();
        let response = "Use Dijkstra with a binary heap as the algorithmic approach for O((V+E) log V) \
                        complexity because it guarantees shortest path. However, if edges have \
                        negative weights, use Bellman-Ford instead. Edge cases: empty graph, \
                        disconnected components, and verify the implementation against cycle-related failures.";
        let score = integration.evaluate_quality(response);
        assert!(
            score.specificity > 0.5,
            "Should detect algorithm keywords: {score:?}"
        );
        assert!(
            score.depth > 0.0,
            "Should detect reasoning keywords: {score:?}"
        );
        assert!(
            score.completeness > 0.0,
            "Should detect edge case keywords: {score:?}"
        );
    }

    #[test]
    fn test_select_strategy_direct_execution() {
        let integration = OrchestrationIntegration::default();
        let high_quality = QualityScore {
            specificity: 4.0,
            depth: 4.0,
            completeness: 3.5,
            uncertainty: 1.5,
            total: 6.5,
        };
        let strategy = integration.select_strategy(1.0, &high_quality, 90);
        assert_eq!(strategy, ReasoningStrategy::DirectExecution);
    }

    #[test]
    fn test_select_strategy_phased_orchestration() {
        let integration = OrchestrationIntegration::default();
        let low_quality = QualityScore {
            specificity: 0.5,
            depth: 0.5,
            completeness: 0.5,
            uncertainty: 0.0,
            total: 1.0,
        };
        let strategy = integration.select_strategy(4.5, &low_quality, 30);
        assert_eq!(strategy, ReasoningStrategy::PhasedOrchestration);
    }

    #[test]
    fn test_structured_thinking_guidance_returns_content() {
        let guidance = OrchestrationIntegration::structured_thinking_guidance();
        assert!(guidance.contains("structured_thinking"));
        assert!(guidance.contains("phase"));
    }

    #[test]
    fn test_handle_tool_call_with_missing_fields() {
        let dir = tempfile::tempdir().unwrap();
        let mut integration = OrchestrationIntegration::new(Some(dir.path().to_path_buf()));
        integration.start_task("sparse-args".to_string());

        let args = serde_json::json!({
            "thought": "Minimal thought"
        });

        let thought = integration
            .handle_structured_thought_tool_call(&args)
            .unwrap();
        assert_eq!(thought.thought, "Minimal thought");
        assert_eq!(thought.confidence, 50);
        assert!(thought.next_thought_needed);
    }

    #[test]
    fn test_phase_context_without_persistence_is_none() {
        let integration = OrchestrationIntegration::default();
        assert!(integration.get_phase_context().is_none());
    }

    #[test]
    fn test_phase_context_without_task_is_none() {
        let dir = tempfile::tempdir().unwrap();
        let integration = OrchestrationIntegration::new(Some(dir.path().to_path_buf()));
        assert!(integration.get_phase_context().is_none());
    }

    #[test]
    fn test_start_task_resets_phase() {
        let mut integration = OrchestrationIntegration::default();
        integration.start_task("first".to_string());
        integration.advance_phase();
        assert_eq!(integration.current_phase(), 2);
        integration.start_task("second".to_string());
        assert_eq!(integration.current_phase(), 1);
    }

    #[test]
    fn test_analyze_message_strategy_consistency() {
        let integration = OrchestrationIntegration::default();
        let result = integration.analyze_message("explore and investigate the algorithm", None);
        assert!(result.complexity > 4.0);
        assert_eq!(result.strategy, ReasoningStrategy::PhasedOrchestration);
        assert!(result.enable_structured_thinking);
    }

    #[test]
    fn test_multi_phase_context_accumulation() {
        let dir = tempfile::tempdir().unwrap();
        let mut integration = OrchestrationIntegration::new(Some(dir.path().to_path_buf()));
        integration.start_task("multi-phase".to_string());

        // Phase 1
        let args1 = serde_json::json!({
            "thought": "Phase 1: decide on DFS approach",
            "phase": 1,
            "type": "decision",
            "confidence": 75,
            "next_thought_needed": false
        });
        integration
            .handle_structured_thought_tool_call(&args1)
            .unwrap();
        integration.advance_phase();

        // Phase 2
        let args2 = serde_json::json!({
            "thought": "Phase 2: validate DFS correctness",
            "phase": 2,
            "type": "validation",
            "confidence": 85,
            "next_thought_needed": false
        });
        integration
            .handle_structured_thought_tool_call(&args2)
            .unwrap();
        integration.advance_phase();

        let ctx = integration.get_phase_context();
        assert!(ctx.is_some());
        let ctx = ctx.unwrap();
        assert_eq!(ctx["phase"], 3);
        assert!(ctx["previous_summary"]["decisions_made"]
            .as_array()
            .unwrap()
            .is_empty());
    }

    #[test]
    fn test_handle_tool_call_thought_types() {
        let dir = tempfile::tempdir().unwrap();
        let mut integration = OrchestrationIntegration::new(Some(dir.path().to_path_buf()));
        integration.start_task("types-test".to_string());

        for thought_type in &[
            "constraint",
            "validation",
            "learning",
            "hypothesis",
            "decision",
            "unknown",
        ] {
            let args = serde_json::json!({
                "thought": format!("A {thought_type} thought"),
                "phase": 1,
                "type": thought_type,
                "confidence": 80,
                "next_thought_needed": true
            });
            let thought = integration
                .handle_structured_thought_tool_call(&args)
                .unwrap();
            assert_eq!(thought.thought, format!("A {thought_type} thought"));
        }
    }
}
