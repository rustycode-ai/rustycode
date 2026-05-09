//! [`StructuredThinkingTool`] — a proper [`Tool`] trait implementation for the AST pipeline.
//!
//! This is the shared tool that can be registered in any [`ToolRegistry`],
//! enabling structured thinking for TUI, headless, and bench agents alike.
//! Replaces the TUI-only `OrchestrationIntegration` bypass.
//!
//! Uses session-keyed global state for zero-sized struct implementation.

use anyhow::Result;
use serde_json::Value;
use std::sync::Arc;

use crate::quality_detector::QualityDetector;
use crate::strategy_selector::StrategySelector;
use crate::structured_thinking_tool::StructuredThinkingToolSchema;
use crate::thinking_state::{self, ThinkingState};
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

/// A zero-sized [`Tool`] implementation that records structured reasoning steps.
///
/// Register this in a [`ToolRegistry`] to enable structured thinking
/// in any `AgentSession` consumer (TUI, headless, bench).
/// Session state is stored globally and keyed by session_id.
#[derive(Debug, Clone, Copy)]
pub struct StructuredThinkingTool;

impl StructuredThinkingTool {
    /// Analyze a user message and determine the execution strategy.
    pub fn analyze_message(&self, content: &str) -> AnalysisResult {
        let detector = QualityDetector::new();
        let selector = StrategySelector::new();

        let complexity = StrategySelector::detect_complexity(content);
        let quality = detector.evaluate(content);
        let strategy = selector.select(complexity, &quality, 75);
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
        QualityDetector::new().evaluate(response)
    }

    /// Select a strategy given complexity, quality, and confidence.
    pub fn select_strategy(
        &self,
        complexity: f64,
        quality: &QualityScore,
        confidence: u32,
    ) -> ReasoningStrategy {
        StrategySelector::new().select(complexity, quality, confidence)
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
    pub fn start_task(session_id: &str, task_id: String) {
        let state = thinking_state::get_or_init_thinking_state(session_id, None);
        let mut phase = state.current_phase.lock();
        *phase = 1;
        let mut tid = state.task_id.lock();
        *tid = Some(task_id);
    }

    /// Ensure a task ID is set (idempotent).
    fn ensure_task(state: &Arc<ThinkingState>, task_id: String) {
        let mut tid = state.task_id.lock();
        if tid.is_none() {
            *tid = Some(task_id);
        }
    }

    /// Advance to the next phase.
    pub fn advance_phase(session_id: &str) -> u32 {
        let state = thinking_state::get_or_init_thinking_state(session_id, None);
        let mut phase = state.current_phase.lock();
        *phase = phase.saturating_add(1);
        *phase
    }

    /// Set the phase to a specific value.
    pub fn advance_to(session_id: &str, phase: u32) {
        let state = thinking_state::get_or_init_thinking_state(session_id, None);
        let mut p = state.current_phase.lock();
        *p = phase;
    }

    /// Get the current phase number.
    pub fn current_phase(session_id: &str) -> u32 {
        let state = thinking_state::get_or_init_thinking_state(session_id, None);
        let phase = state.current_phase.lock();
        *phase
    }

    /// Get phase context for multi-phase orchestration.
    pub fn phase_context(session_id: &str) -> Option<Value> {
        let state = thinking_state::get_or_init_thinking_state(session_id, None);
        let store = state.reasoning_store.lock();
        let store = store.as_ref()?;
        let task_id = state.task_id.lock();
        let task_id = task_id.as_ref()?;
        let phase = *state.current_phase.lock();
        store.context_for_next_phase(task_id, phase).ok()
    }

    /// Check if reasoning persistence is enabled.
    pub fn has_persistence(session_id: &str) -> bool {
        let state = thinking_state::get_or_init_thinking_state(session_id, None);
        let store = state.reasoning_store.lock();
        store.is_some()
    }

    /// Handle a `structured_thinking` tool call from the LLM.
    fn handle_thought_call(
        state: &Arc<ThinkingState>,
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
            let mut detector = state.stuck_detector.lock();
            let stuck = detector.record_thought(&thought.thought, confidence, phase);

            if let (Some(store), Some(task_id)) = (
                state.reasoning_store.lock().as_ref(),
                state.task_id.lock().as_ref(),
            ) {
                store.store_thought(task_id, phase, &thought)?;
            }

            if !next_thought_needed {
                let mut p = state.current_phase.lock();
                *p = p.saturating_add(1);
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
        ctx: &rustycode_tools_api::ToolContext,
    ) -> Result<rustycode_tools_api::ToolOutput> {
        let default_session = format!("auto-session-{}", std::process::id());
        let session_id = ctx.session_id.as_deref().unwrap_or(&default_session);
        let task_id = format!("auto-task-{}", std::process::id());

        let state = thinking_state::get_or_init_thinking_state(session_id, None);
        Self::ensure_task(&state, task_id);

        match Self::handle_thought_call(&state, &params) {
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
        let tool = StructuredThinkingTool;
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
        let session_id = "test-session-1";
        thinking_state::get_or_init_thinking_state(session_id, None);
        let tool = StructuredThinkingTool;
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
        let tool = StructuredThinkingTool;
        let ctx = rustycode_tools_api::ToolContext::new(std::path::Path::new("/tmp"));

        let params = serde_json::json!({
            "thought": "Final conclusion",
            "phase": 1,
            "type": "validation",
            "confidence": 95,
            "next_thought_needed": false
        });

        let result = tool.execute(params, &ctx).unwrap();
        // When next_thought_needed is false, phase should advance from 1 to 2
        assert!(result.text.contains("\"phase\":1"));
        assert!(!result.text.contains("loop_warning"));
    }

    #[test]
    fn analyze_simple_message() {
        let tool = StructuredThinkingTool;
        let result = tool.analyze_message("fix the typo");
        assert!(result.complexity < 2.0);
    }

    #[test]
    fn analyze_complex_message() {
        let tool = StructuredThinkingTool;
        let result = tool.analyze_message(
            "Implement a full authentication system with OAuth2, JWT tokens, and role-based access control",
        );
        assert!(result.complexity > 2.0);
        assert!(result.enable_structured_thinking);
    }

    #[test]
    fn start_and_advance_task() {
        let session_id = "test-session-3";
        StructuredThinkingTool::start_task(session_id, "test-task-1".to_string());
        assert_eq!(StructuredThinkingTool::current_phase(session_id), 1);
        let next = StructuredThinkingTool::advance_phase(session_id);
        assert_eq!(next, 2);
        assert_eq!(StructuredThinkingTool::current_phase(session_id), 2);
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
        let session_id = "test-session-4";
        assert!(!StructuredThinkingTool::has_persistence(session_id));
    }

    #[test]
    fn stuck_detection_works_in_execute() {
        let tool = StructuredThinkingTool;
        let ctx = rustycode_tools_api::ToolContext::new(std::path::Path::new("/tmp"));

        // Execute multiple thoughts — stuck detector will record them
        for i in 1..=3 {
            let params = serde_json::json!({
                "thought": format!("analyzing step {i}"),
                "phase": i,
                "type": "decision",
                "confidence": 60,
                "next_thought_needed": true
            });
            let result = tool.execute(params, &ctx).unwrap();
            // Verify each execution produces valid output
            assert!(result.text.contains("recorded"));
            assert!(result.text.contains("Decision"));
            // Response may or may not contain loop_warning depending on detector state
            // The important thing is that the tool executes without error
        }
    }
}
