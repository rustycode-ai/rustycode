//! [`StructuredThinkingTool`] — a proper [`Tool`] trait implementation for the AST pipeline.
//!
//! This is the shared tool that can be registered in any [`ToolRegistry`],
//! enabling structured thinking for TUI, headless, and bench agents alike.
//! Replaces the TUI-only `OrchestrationIntegration` bypass.

use std::path::PathBuf;
use std::sync::Mutex;

use anyhow::Result;
use serde_json::Value;

use crate::ask_user_tool::StuckDetector;
use crate::quality_detector::QualityDetector;
use crate::reasoning_store::ReasoningStore;
use crate::strategy_selector::StrategySelector;
use crate::structured_thinking_tool::StructuredThinkingToolSchema;
use crate::types::{QualityScore, ReasoningStrategy, StructuredThought, ThoughtType};

/// Result of analyzing a user message for orchestration routing.
#[derive(Debug)]
pub struct AnalysisResult {
    /// Numeric complexity score (0.0–5.0).
    pub complexity: f64,
    /// Chosen execution strategy.
    pub strategy: ReasoningStrategy,
    /// Whether the structured thinking tool should be injected.
    pub enable_structured_thinking: bool,
}

/// Internal mutable state for a thinking session.
struct ThinkingSessionState {
    task_id: Option<String>,
    current_phase: u32,
    reasoning_store: Option<ReasoningStore>,
    stuck_detector: StuckDetector,
}

/// A [`Tool`] implementation that records structured reasoning steps.
///
/// Register this in a [`ToolRegistry`] to enable structured thinking
/// in any `AgentSession` consumer (TUI, headless, bench).
pub struct StructuredThinkingTool {
    state: std::sync::Mutex<ThinkingSessionState>,
    quality_detector: QualityDetector,
    strategy_selector: StrategySelector,
}

impl StructuredThinkingTool {
    /// Create a new tool instance.
    ///
    /// If `store_path` is `Some`, reasoning persistence is enabled.
    pub fn new(store_path: Option<PathBuf>) -> Self {
        let reasoning_store = store_path.map(ReasoningStore::new);
        Self {
            state: Mutex::new(ThinkingSessionState {
                task_id: None,
                current_phase: 1,
                reasoning_store,
                stuck_detector: StuckDetector::with_default_config(),
            }),
            quality_detector: QualityDetector::new(),
            strategy_selector: StrategySelector::new(),
        }
    }

    /// Analyze a user message and determine the execution strategy.
    pub fn analyze_message(&self, content: &str) -> AnalysisResult {
        let complexity = StrategySelector::detect_complexity(content);
        let quality = self.quality_detector.evaluate(content);
        let strategy = self.strategy_selector.select(complexity, &quality, 75);
        let enable_structured_thinking = strategy.requires_structured_thinking();

        tracing::info!(
            complexity = %format!("{complexity:.2}"),
            strategy = ?strategy,
            enable_structured_thinking,
            "Orchestration analysis"
        );

        AnalysisResult {
            complexity,
            strategy,
            enable_structured_thinking,
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
    pub const fn should_enable_structured_thinking(strategy: ReasoningStrategy) -> bool {
        strategy.requires_structured_thinking()
    }

    /// Get the tool schema for injection into the LLM request.
    pub fn tool_schema() -> Value {
        StructuredThinkingToolSchema::schema()
    }

    /// Get the system prompt guidance for structured thinking.
    pub fn system_prompt_guidance() -> &'static str {
        StructuredThinkingToolSchema::system_prompt_guidance()
    }

    /// Start a new task for phase tracking.
    pub fn start_task(&self, task_id: String) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.task_id = Some(task_id);
        state.current_phase = 1;
    }

    /// Ensure a task ID is set (idempotent).
    pub fn ensure_task(&self, task_id: String) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.task_id.is_none() {
            state.task_id = Some(task_id);
        }
    }

    /// Advance to the next phase.
    pub fn advance_phase(&self) -> u32 {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.current_phase = state.current_phase.saturating_add(1);
        state.current_phase
    }

    /// Set the phase to a specific value.
    pub fn advance_to(&self, phase: u32) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.current_phase = phase;
    }

    /// Get the current phase number.
    pub fn current_phase(&self) -> u32 {
        let state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.current_phase
    }

    /// Get phase context for multi-phase orchestration.
    pub fn get_phase_context(&self) -> Option<Value> {
        let state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let store = state.reasoning_store.as_ref()?;
        let task_id = state.task_id.as_ref()?;
        store
            .get_context_for_next_phase(task_id, state.current_phase)
            .ok()
    }

    /// Check if reasoning persistence is enabled.
    pub fn has_persistence(&self) -> bool {
        let state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.reasoning_store.is_some()
    }

    /// Handle a `structured_thinking` tool call from the LLM.
    fn handle_thought_call(
        &self,
        args: &Value,
    ) -> Result<(StructuredThought, crate::ask_user_tool::StuckCheckResult)> {
        let thought_text = args["thought"].as_str().unwrap_or("").to_string();
        let phase = args["phase"].as_u64().unwrap_or(1) as u32;
        let confidence = args["confidence"].as_u64().unwrap_or(50) as u32;
        let next_thought_needed = args["next_thought_needed"].as_bool().unwrap_or(true);

        let thought_type_str = args["type"].as_str().unwrap_or("decision");
        let thought_type = match thought_type_str {
            "constraint" => ThoughtType::Constraint,
            "validation" => ThoughtType::Validation,
            "learning" => ThoughtType::Learning,
            "hypothesis" => ThoughtType::Hypothesis,
            _ => ThoughtType::Decision,
        };

        let mut thought = StructuredThought::new(thought_text, phase, thought_type);
        thought.confidence = confidence;
        thought.next_thought_needed = next_thought_needed;

        let stuck_result = {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);

            let stuck = state
                .stuck_detector
                .record_thought(&thought.thought, confidence, phase);

            if let (Some(ref store), Some(ref task_id)) = (&state.reasoning_store, &state.task_id) {
                store.store_thought(task_id, phase, &thought)?;
            }

            if !next_thought_needed {
                state.current_phase = state.current_phase.saturating_add(1);
            }

            stuck
        };

        Ok((thought, stuck_result))
    }
}

impl rustycode_tools_api::Tool for StructuredThinkingTool {
    fn name(&self) -> &'static str {
        "structured_thinking"
    }

    fn description(&self) -> &'static str {
        "Record structured reasoning steps during complex problem solving"
    }

    fn parameters_schema(&self) -> Value {
        StructuredThinkingToolSchema::schema()
            .get("function")
            .and_then(|f| f.get("parameters"))
            .cloned()
            .unwrap_or_else(|| serde_json::json!({"type": "object", "properties": {}}))
    }

    fn execute(
        &self,
        params: Value,
        _ctx: &rustycode_tools_api::ToolContext,
    ) -> Result<rustycode_tools_api::ToolOutput> {
        let task_id = format!("auto-task-{}", std::process::id());
        self.ensure_task(task_id);

        match self.handle_thought_call(&params) {
            Ok((thought, stuck_check)) => {
                tracing::info!(
                    thought_type = ?thought.thought_type,
                    confidence = thought.confidence,
                    phase = thought.phase,
                    next_thought_needed = thought.next_thought_needed,
                    is_stuck = stuck_check.is_stuck,
                    "Structured thinking tool executed"
                );

                let mut response = serde_json::json!({
                    "status": "recorded",
                    "thought_type": format!("{:?}", thought.thought_type),
                    "confidence": thought.confidence,
                    "phase": thought.phase,
                    "next_thought_needed": thought.next_thought_needed,
                });

                if stuck_check.is_stuck {
                    response["loop_warning"] = serde_json::json!({
                        "detected": true,
                        "signals": stuck_check.signals,
                        "suggestion": stuck_check.suggestion,
                    });
                }

                Ok(rustycode_tools_api::ToolOutput::text(response.to_string()))
            }
            Err(e) => {
                tracing::error!("Structured thinking tool call failed: {e}");
                Err(anyhow::anyhow!("Failed to record thought: {e}"))
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use rustycode_tools_api::Tool as _;

    #[test]
    fn tool_name_and_description() {
        let tool = StructuredThinkingTool::new(None);
        assert_eq!(tool.name(), "structured_thinking");
        assert!(!tool.description().is_empty());
    }

    #[test]
    fn schema_matches_structured_thinking_schema() {
        let schema = StructuredThinkingToolSchema::schema();
        assert_eq!(schema["type"], "function");
        assert_eq!(schema["function"]["name"], "structured_thinking");
    }

    #[test]
    fn execute_records_thought() {
        let tool = StructuredThinkingTool::new(None);
        let ctx = rustycode_tools_api::ToolContext::new(std::path::Path::new("/tmp"));

        let params = serde_json::json!({
            "thought": "I need to analyze the algorithm",
            "phase": 1,
            "type": "decision",
            "confidence": 85,
            "next_thought_needed": true
        });

        let result = tool.execute(params, &ctx).unwrap();
        assert!(result.text.contains("recorded"));
        assert!(result.text.contains("85"));
    }

    #[test]
    fn execute_advances_phase_on_final_thought() {
        let tool = StructuredThinkingTool::new(None);
        let ctx = rustycode_tools_api::ToolContext::new(std::path::Path::new("/tmp"));

        assert_eq!(tool.current_phase(), 1);

        let params = serde_json::json!({
            "thought": "Final conclusion",
            "phase": 1,
            "type": "validation",
            "confidence": 95,
            "next_thought_needed": false
        });

        tool.execute(params, &ctx).unwrap();
        assert_eq!(tool.current_phase(), 2);
    }

    #[test]
    fn analyze_simple_message() {
        let tool = StructuredThinkingTool::new(None);
        let result = tool.analyze_message("fix the typo");
        assert!(result.complexity < 2.0);
    }

    #[test]
    fn analyze_complex_message() {
        let tool = StructuredThinkingTool::new(None);
        let result = tool.analyze_message(
            "Implement a full authentication system with OAuth2, JWT tokens, and role-based access control",
        );
        assert!(result.complexity > 2.0);
        assert!(result.enable_structured_thinking);
    }

    #[test]
    fn start_and_advance_task() {
        let tool = StructuredThinkingTool::new(None);
        tool.start_task("test-task-1".to_string());
        assert_eq!(tool.current_phase(), 1);
        let next = tool.advance_phase();
        assert_eq!(next, 2);
        assert_eq!(tool.current_phase(), 2);
    }

    #[test]
    fn ensure_task_is_idempotent() {
        let tool = StructuredThinkingTool::new(None);
        tool.start_task("first".to_string());
        tool.ensure_task("second".to_string()); // should not overwrite
        assert_eq!(tool.current_phase(), 1);
    }

    #[test]
    fn tool_schema_static_method() {
        let schema = StructuredThinkingTool::tool_schema();
        assert_eq!(schema["function"]["name"], "structured_thinking");
    }

    #[test]
    fn system_prompt_guidance_is_not_empty() {
        let guidance = StructuredThinkingTool::system_prompt_guidance();
        assert!(!guidance.is_empty());
        assert!(guidance.contains("structured_thinking"));
    }

    #[test]
    fn no_persistence_by_default() {
        let tool = StructuredThinkingTool::new(None);
        assert!(!tool.has_persistence());
    }

    #[test]
    fn with_persistence() {
        let dir = tempfile::tempdir().unwrap();
        let tool = StructuredThinkingTool::new(Some(dir.path().to_path_buf()));
        assert!(tool.has_persistence());
    }

    #[test]
    fn loop_warning_appears_after_stagnation() {
        let tool = StructuredThinkingTool::new(None);
        let ctx = rustycode_tools_api::ToolContext::new(std::path::Path::new("/tmp"));

        // Feed 2 identical-confidence thoughts — should not trigger yet
        for i in 1..=2 {
            let params = serde_json::json!({
                "thought": format!("analyzing step {i}"),
                "phase": i,
                "type": "decision",
                "confidence": 60,
                "next_thought_needed": true
            });
            let result = tool.execute(params, &ctx).unwrap();
            assert!(
                !result.text.contains("loop_warning"),
                "should not warn at phase {i}: {}",
                result.text
            );
        }

        // 3rd thought with same confidence triggers stagnation (threshold=3)
        let params = serde_json::json!({
            "thought": "still analyzing",
            "phase": 3,
            "type": "decision",
            "confidence": 60,
            "next_thought_needed": true
        });
        let result = tool.execute(params, &ctx).unwrap();
        assert!(
            result.text.contains("loop_warning"),
            "should warn after 3 stagnant thoughts: {}",
            result.text
        );
        assert!(result.text.contains("ask_user"));
    }
}
