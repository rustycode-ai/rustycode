//! Shared types for the Active Reasoning Engine tools.
//!
//! These types define the data structures used by the four reasoning tools:
//! `reasoning_decompose`, `reasoning_research`, `reasoning_validate`, `reasoning_integrate`.
//!
//! v1 design: Tools are stateless pure functions. They accept parameters from the LLM
//! and return structured JSON guidance. Graph state is managed at the TUI layer.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// A module identified during problem decomposition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReasoningModule {
    pub name: String,
    pub description: String,
    pub questions: Vec<String>,
    pub dependencies: Vec<String>,
    pub confidence: f32,
}

/// A prioritized research target for a specific module.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchTarget {
    pub target: String,
    pub why: String,
    pub expected_findings: String,
    pub priority: u8,
}

/// A before/after clarification of a requirement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequirementClarification {
    pub before: String,
    pub after: String,
    pub assumption_confirmed: bool,
}

/// An integration risk between modules.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrationRisk {
    pub description: String,
    /// "low", "medium", or "high"
    pub severity: String,
    pub mitigation: String,
}

/// Budget tracking for reasoning tool calls.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BudgetState {
    pub exploration_calls: usize,
    pub code_calls: usize,
    pub nodes_created: usize,
    /// Internal: set when `exploration_calls` >= MAX and no code written
    pub force_stop: bool,
    /// External: set by TUI layer to block reasoning tools after enforcement
    pub stop_and_code_active: bool,
}

/// Maximum exploration calls before enforcement kicks in.
pub const MAX_EXPLORATION_CALLS: usize = 10;

/// Maximum thinking nodes per conversation.
pub const MAX_THINKING_NODES: usize = 25;

impl BudgetState {
    /// Check if either budget limit has been hit.
    /// Decision 1C: EITHER triggers `STOP_AND_CODE`.
    pub fn is_exhausted(&self) -> bool {
        self.force_stop
            || self.exploration_calls >= MAX_EXPLORATION_CALLS
            || self.nodes_created >= MAX_THINKING_NODES
    }

    /// Record a reasoning tool call. Returns true if this call triggered enforcement.
    pub fn record_exploration(&mut self) -> bool {
        self.exploration_calls += 1;
        self.nodes_created += 1;
        let triggered = (self.exploration_calls >= MAX_EXPLORATION_CALLS && self.code_calls == 0)
            || self.nodes_created >= MAX_THINKING_NODES;
        if triggered {
            self.force_stop = true;
            self.stop_and_code_active = true;
        }
        triggered
    }

    /// Record a code-producing call (write, edit, bash with code output).
    pub fn record_code(&mut self) {
        self.code_calls += 1;
        self.force_stop = false;
    }

    /// Format a budget warning for inclusion in tool output.
    pub fn warning_text(&self) -> Option<String> {
        if self.exploration_calls > 0 {
            Some(format!(
                "You have used {}/{} exploration calls. {}",
                self.exploration_calls,
                MAX_EXPLORATION_CALLS,
                if self.force_stop {
                    "STOP_AND_CODE triggered — you must now produce code or implementation output."
                } else if self.exploration_calls >= MAX_EXPLORATION_CALLS - 3 {
                    "Approaching limit. Start producing code soon."
                } else {
                    "Continue researching if needed."
                }
            ))
        } else {
            None
        }
    }
}

/// The current phase in the active reasoning workflow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReasoningPhase {
    Decompose,
    Research,
    Clarify,
    Integrate,
}

impl ReasoningPhase {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Decompose => "decompose",
            Self::Research => "research",
            Self::Clarify => "clarify",
            Self::Integrate => "integrate",
        }
    }

    /// Get the recommended next tool for this phase.
    pub const fn recommended_next_tool(&self) -> &'static str {
        match self {
            Self::Decompose => "ReasoningResearch",
            Self::Research => "ReasoningValidate",
            Self::Clarify => "ReasoningIntegrate",
            Self::Integrate => "implement_now",
        }
    }
}

/// Output from a reasoning phase.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhaseOutput {
    pub phase: ReasoningPhase,
    pub summary: String,
    pub modules: Vec<ReasoningModule>,
    pub readiness: f32,
    pub next_action: String,
}

/// Build a tool output JSON value from a phase output.
pub fn build_tool_output(phase: &PhaseOutput, budget: &BudgetState) -> Value {
    let mut output = json!({
        "phase": phase.phase.as_str(),
        "summary": phase.summary,
        "modules": phase.modules,
        "readiness": phase.readiness,
        "next_action": phase.next_action,
    });

    if let Some(warning) = budget.warning_text() {
        output["budget_warning"] = json!(warning);
    }

    output
}

/// Format tool output as human-readable text for the LLM.
pub fn format_output_text(output: &Value) -> String {
    let phase = output["phase"].as_str().unwrap_or("unknown");
    let summary = output["summary"].as_str().unwrap_or("");
    let readiness = output["readiness"].as_f64().unwrap_or(0.0);
    let next_action = output["next_action"].as_str().unwrap_or("");

    let mut text = format!("## Phase: {phase}\n\n{summary}\n\n**Readiness: {readiness:.2}**\n\n**Next: {next_action}**");

    if let Some(warning) = output["budget_warning"].as_str() {
        text.push_str(&format!("\n\n⚠️ **{warning}**"));
    }

    if let Some(modules) = output["modules"].as_array() {
        if !modules.is_empty() {
            text.push_str("\n\n### Modules\n");
            for module in modules {
                let name = module["name"].as_str().unwrap_or("?");
                let confidence = module["confidence"].as_f64().unwrap_or(0.0);
                text.push_str(&format!("- **{name}** (confidence: {confidence:.2})\n"));
            }
        }
    }

    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn budget_state_default_is_not_exhausted() {
        let budget = BudgetState::default();
        assert!(!budget.is_exhausted());
        assert!(!budget.force_stop);
        assert!(!budget.stop_and_code_active);
    }

    #[test]
    fn budget_state_records_exploration() {
        let mut budget = BudgetState::default();
        assert!(!budget.record_exploration());
        assert_eq!(budget.exploration_calls, 1);
        assert_eq!(budget.nodes_created, 1);
        assert!(!budget.force_stop);
    }

    #[test]
    fn budget_state_triggers_force_stop_at_max_exploration() {
        let mut budget = BudgetState::default();
        for _ in 0..MAX_EXPLORATION_CALLS {
            budget.record_exploration();
        }
        assert!(budget.force_stop);
        assert!(budget.stop_and_code_active);
        assert!(budget.is_exhausted());
    }

    #[test]
    fn budget_state_triggers_force_stop_at_max_nodes() {
        let mut budget = BudgetState::default();
        // Add some code calls so exploration limit alone won't trigger
        for _ in 0..5 {
            budget.record_code();
        }
        for _ in 0..MAX_THINKING_NODES {
            budget.record_exploration();
        }
        assert!(budget.force_stop);
        assert!(budget.is_exhausted());
    }

    #[test]
    fn budget_state_record_code_clears_force_stop() {
        let mut budget = BudgetState::default();
        for _ in 0..MAX_EXPLORATION_CALLS {
            budget.record_exploration();
        }
        assert!(budget.force_stop);
        budget.record_code();
        assert!(!budget.force_stop);
        assert_eq!(budget.code_calls, 1);
        // stop_and_code_active remains true even after code
        assert!(budget.stop_and_code_active);
    }

    #[test]
    fn budget_state_no_warning_initially() {
        let budget = BudgetState::default();
        assert!(budget.warning_text().is_none());
    }

    #[test]
    fn budget_state_warning_after_exploration() {
        let mut budget = BudgetState::default();
        budget.record_exploration();
        let warning = budget.warning_text().unwrap();
        assert!(warning.contains("1/10"));
        assert!(warning.contains("Continue researching"));
    }

    #[test]
    fn budget_state_warning_near_limit() {
        let mut budget = BudgetState::default();
        for _ in 0..(MAX_EXPLORATION_CALLS - 2) {
            budget.record_exploration();
        }
        let warning = budget.warning_text().unwrap();
        assert!(warning.contains("Approaching limit"));
    }

    #[test]
    fn budget_state_warning_at_limit() {
        let mut budget = BudgetState::default();
        for _ in 0..MAX_EXPLORATION_CALLS {
            budget.record_exploration();
        }
        let warning = budget.warning_text().unwrap();
        assert!(warning.contains("STOP_AND_CODE"));
    }

    #[test]
    fn reasoning_phase_as_str() {
        assert_eq!(ReasoningPhase::Decompose.as_str(), "decompose");
        assert_eq!(ReasoningPhase::Research.as_str(), "research");
        assert_eq!(ReasoningPhase::Clarify.as_str(), "clarify");
        assert_eq!(ReasoningPhase::Integrate.as_str(), "integrate");
    }

    #[test]
    fn reasoning_phase_recommended_next_tool() {
        assert_eq!(
            ReasoningPhase::Decompose.recommended_next_tool(),
            "ReasoningResearch"
        );
        assert_eq!(
            ReasoningPhase::Research.recommended_next_tool(),
            "ReasoningValidate"
        );
        assert_eq!(
            ReasoningPhase::Clarify.recommended_next_tool(),
            "ReasoningIntegrate"
        );
        assert_eq!(
            ReasoningPhase::Integrate.recommended_next_tool(),
            "implement_now"
        );
    }

    #[test]
    fn phase_output_json_structure() {
        let output = PhaseOutput {
            phase: ReasoningPhase::Decompose,
            summary: "Test summary".into(),
            modules: vec![ReasoningModule {
                name: "mod1".into(),
                description: "desc".into(),
                questions: vec!["q1".into()],
                dependencies: vec![],
                confidence: 0.8,
            }],
            readiness: 0.75,
            next_action: "Next step".into(),
        };
        let json = build_tool_output(&output, &BudgetState::default());
        assert_eq!(json["phase"], "decompose");
        assert_eq!(json["summary"], "Test summary");
        assert_eq!(json["readiness"], 0.75);
        assert!(json["budget_warning"].is_null());
    }

    #[test]
    fn phase_output_includes_budget_warning() {
        let output = PhaseOutput {
            phase: ReasoningPhase::Research,
            summary: "Researching".into(),
            modules: vec![],
            readiness: 0.5,
            next_action: "Continue".into(),
        };
        let mut budget = BudgetState::default();
        budget.record_exploration();
        let json = build_tool_output(&output, &budget);
        assert!(json["budget_warning"].is_string());
    }

    #[test]
    fn format_output_text_basic() {
        let json = serde_json::json!({
            "phase": "decompose",
            "summary": "Breaking down the task",
            "readiness": 0.6,
            "next_action": "Research module A"
        });
        let text = format_output_text(&json);
        assert!(text.contains("decompose"));
        assert!(text.contains("Breaking down"));
        assert!(text.contains("0.60"));
        assert!(text.contains("Research module A"));
    }

    #[test]
    fn format_output_text_with_modules() {
        let json = serde_json::json!({
            "phase": "research",
            "summary": "Summary",
            "readiness": 0.8,
            "next_action": "Next",
            "modules": [
                {"name": "auth", "confidence": 0.9},
                {"name": "db", "confidence": 0.5}
            ]
        });
        let text = format_output_text(&json);
        assert!(text.contains("auth"));
        assert!(text.contains("db"));
        assert!(text.contains("0.90"));
        assert!(text.contains("0.50"));
    }
}
