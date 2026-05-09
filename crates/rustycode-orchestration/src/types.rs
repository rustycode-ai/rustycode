use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;

use crate::guard::RequiredResources;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructuredThought {
    pub thought: String,
    pub phase: u32,
    pub thought_type: ThoughtType,
    pub references: Vec<String>, // thought IDs this references
    pub confidence: u32,         // 0-100
    pub next_thought_needed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch_id: Option<String>, // for exploring alternatives
    pub metadata: ThoughtMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ThoughtType {
    Decision,
    Constraint,
    Validation,
    Learning,
    Hypothesis,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ThoughtMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub algorithm_choice: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rationale: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alternatives_rejected: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validation_points: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dependencies: Option<HashMap<String, String>>,
}

impl StructuredThought {
    pub fn new(thought: String, phase: u32, thought_type: ThoughtType) -> Self {
        Self {
            thought,
            phase,
            thought_type,
            references: vec![],
            confidence: 50,
            next_thought_needed: true,
            branch_id: None,
            metadata: ThoughtMetadata::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum Difficulty {
    Easy,
    Medium,
    Hard,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ExecutionTier {
    Musician = 2,
    Editor = 3,
    Composer = 4,
    Thinking = 5,
}

impl ExecutionTier {
    pub const fn as_u8(self) -> u8 {
        self as u8
    }

    pub const fn is_thinking(self) -> bool {
        matches!(self, Self::Thinking)
    }

    pub const fn from_u8(tier: u8) -> Option<Self> {
        match tier {
            2 => Some(Self::Musician),
            3 => Some(Self::Editor),
            4 => Some(Self::Composer),
            5 => Some(Self::Thinking),
            _ => None,
        }
    }
}

impl fmt::Display for ExecutionTier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Musician => write!(f, "musician"),
            Self::Editor => write!(f, "editor"),
            Self::Composer => write!(f, "composer"),
            Self::Thinking => write!(f, "thinking"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum OutputType {
    File,
    Command,
    Query,
    Code,
    Data,
    Verification,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Step {
    pub id: String,
    pub index: u8,
    pub description: String,
    pub expected_output_type: OutputType,
    pub suggested_tool: Option<String>,
    pub retry_on_failure: bool,
    #[serde(default)]
    pub required_resources: RequiredResources,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StepResult {
    pub output: String,
    pub exit_code: Option<i32>,
}

impl StepResult {
    pub const fn is_success(&self) -> bool {
        matches!(self.exit_code, Some(0) | None)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TaskOutcome {
    SuccessAtTier(u8),
    Abandoned { reason: String },
    BudgetExceeded,
    HallucinationLoop,
}

impl TaskOutcome {
    pub const fn is_success(&self) -> bool {
        matches!(self, Self::SuccessAtTier(_))
    }
}

impl fmt::Display for TaskOutcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SuccessAtTier(tier) => write!(f, "success_at_tier_{tier}"),
            Self::Abandoned { reason } => write!(f, "abandoned: {reason}"),
            Self::BudgetExceeded => write!(f, "budget_exceeded"),
            Self::HallucinationLoop => write!(f, "hallucination_loop"),
        }
    }
}

/// Heuristic quality score for an LLM response.
/// Scored on 4 axes, total 0-7.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct QualityScore {
    pub specificity: f64,  // 0-5: named algorithms, concrete details vs vague
    pub depth: f64,        // 0-5: explanations with reasoning vs bare assertions
    pub completeness: f64, // 0-5: edge cases, error handling, alternatives considered
    pub uncertainty: f64,  // 0-2: caveats stated, limitations acknowledged
    pub total: f64,        // 0-7: weighted combination
}

impl QualityScore {
    pub fn is_high_quality(&self) -> bool {
        self.total >= 5.0
    }
}

/// Reasoning strategy selected based on task complexity and response quality.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum ReasoningStrategy {
    /// Simple task, high quality → execute immediately
    DirectExecution,
    /// Moderate task, good quality → ask LLM to self-eval, then execute
    QuickSelfEval,
    /// Moderate complexity, medium confidence → step-by-step prompts
    SequentialThinking,
    /// High complexity, low confidence → phase 1-3 detailed, 4+ outlined
    PhasedOrchestration,
}

impl ReasoningStrategy {
    /// Returns `true` when the strategy requires the structured thinking tool.
    pub const fn requires_structured_thinking(&self) -> bool {
        matches!(self, Self::SequentialThinking | Self::PhasedOrchestration)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_step_result_is_success() {
        let ok = StepResult {
            output: "done".into(),
            exit_code: Some(0),
        };
        let no_code = StepResult {
            output: "done".into(),
            exit_code: None,
        };
        let fail = StepResult {
            output: "err".into(),
            exit_code: Some(1),
        };

        assert!(ok.is_success());
        assert!(no_code.is_success());
        assert!(!fail.is_success());
    }

    #[test]
    fn test_task_outcome_is_success() {
        assert!(TaskOutcome::SuccessAtTier(2).is_success());
        assert!(!TaskOutcome::Abandoned { reason: "x".into() }.is_success());
        assert!(!TaskOutcome::BudgetExceeded.is_success());
        assert!(!TaskOutcome::HallucinationLoop.is_success());
    }

    #[test]
    fn test_task_outcome_display() {
        assert_eq!(
            TaskOutcome::SuccessAtTier(2).to_string(),
            "success_at_tier_2"
        );
        assert_eq!(TaskOutcome::BudgetExceeded.to_string(), "budget_exceeded");
        assert_eq!(
            TaskOutcome::HallucinationLoop.to_string(),
            "hallucination_loop"
        );
        assert_eq!(
            TaskOutcome::Abandoned {
                reason: "timeout".into()
            }
            .to_string(),
            "abandoned: timeout"
        );
    }

    #[test]
    fn test_step_serialization_roundtrip() {
        let step = Step {
            id: "s1".into(),
            index: 0,
            description: "test".into(),
            expected_output_type: OutputType::Verification,
            suggested_tool: Some("Bash".into()),
            retry_on_failure: true,
            required_resources: RequiredResources::new(),
        };
        let json = serde_json::to_string(&step).unwrap();
        let deserialized: Step = serde_json::from_str(&json).unwrap();
        assert_eq!(step, deserialized);
    }

    #[test]
    fn test_execution_tier_values() {
        assert_eq!(ExecutionTier::Musician.as_u8(), 2);
        assert_eq!(ExecutionTier::Editor.as_u8(), 3);
        assert_eq!(ExecutionTier::Composer.as_u8(), 4);
        assert_eq!(ExecutionTier::Thinking.as_u8(), 5);
    }

    #[test]
    fn test_execution_tier_from_u8() {
        assert_eq!(ExecutionTier::from_u8(2), Some(ExecutionTier::Musician));
        assert_eq!(ExecutionTier::from_u8(3), Some(ExecutionTier::Editor));
        assert_eq!(ExecutionTier::from_u8(4), Some(ExecutionTier::Composer));
        assert_eq!(ExecutionTier::from_u8(5), Some(ExecutionTier::Thinking));
        assert_eq!(ExecutionTier::from_u8(0), None);
        assert_eq!(ExecutionTier::from_u8(6), None);
    }

    #[test]
    fn test_execution_tier_is_thinking() {
        assert!(ExecutionTier::Thinking.is_thinking());
        assert!(!ExecutionTier::Musician.is_thinking());
        assert!(!ExecutionTier::Composer.is_thinking());
    }

    #[test]
    fn test_execution_tier_display() {
        assert_eq!(ExecutionTier::Musician.to_string(), "musician");
        assert_eq!(ExecutionTier::Editor.to_string(), "editor");
        assert_eq!(ExecutionTier::Composer.to_string(), "composer");
        assert_eq!(ExecutionTier::Thinking.to_string(), "thinking");
    }

    #[test]
    fn test_step_deserialization_without_required_resources() {
        let json = r#"{"id":"s1","index":0,"description":"test","expected_output_type":"Verification","suggested_tool":null,"retry_on_failure":false}"#;
        let step: Step = serde_json::from_str(json).unwrap();
        assert!(step.required_resources.resources.is_empty());
    }
}
