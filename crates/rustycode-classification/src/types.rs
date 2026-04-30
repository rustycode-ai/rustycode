use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskComplexity {
    Mundane,
    Complex,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[non_exhaustive]
pub enum ComplexityTier {
    Light,
    #[default]
    Standard,
    Heavy,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClassificationReason {
    KeywordMatch(String),
    TaskLengthShort,
    TaskLengthLong,
    HistoricalPattern,
    MultipleTools,
    ReasoningKeywords,
    Unknown,
}

pub struct ClassificationResult {
    pub complexity: TaskComplexity,
    pub confidence: f64,
    pub reasons: Vec<ClassificationReason>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct ComplexitySignals {
    pub estimated_steps: u8,
    pub requires_context: bool,
    pub ambiguous: bool,
    pub multi_file: bool,
    pub debugging: bool,
    pub strategic: bool,
    pub risky: bool,
}

impl Default for ComplexitySignals {
    fn default() -> Self {
        Self {
            estimated_steps: 1,
            requires_context: false,
            ambiguous: false,
            multi_file: false,
            debugging: false,
            strategic: false,
            risky: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskClassification {
    pub complexity_score: u8,
    pub tier: ComplexityTier,
    pub signals: ComplexitySignals,
    pub agent_role: rustycode_protocol::agent_protocol::AgentRole,
    pub reasoning: String,
}

#[derive(Debug, Clone)]
pub struct StoredPattern {
    pub task_type: String,
    pub occurrence_count: u32,
}

pub trait PatternQuery: Send + Sync {
    fn query_patterns(&self, task_type: &str) -> anyhow::Result<Vec<StoredPattern>>;
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_task_complexity_variants() {
        assert_ne!(TaskComplexity::Mundane, TaskComplexity::Complex);
    }

    #[test]
    fn test_complexity_tier_default() {
        assert_eq!(ComplexityTier::default(), ComplexityTier::Standard);
    }

    #[test]
    fn test_complexity_signals_default() {
        let signals = ComplexitySignals::default();
        assert_eq!(signals.estimated_steps, 1);
        assert!(!signals.requires_context);
        assert!(!signals.ambiguous);
        assert!(!signals.multi_file);
        assert!(!signals.debugging);
        assert!(!signals.strategic);
        assert!(!signals.risky);
    }

    #[test]
    fn test_classification_reason_debug() {
        let reason = ClassificationReason::KeywordMatch("test".into());
        assert!(format!("{reason:?}").contains("KeywordMatch"));
    }

    #[test]
    fn test_stored_pattern() {
        let pattern = StoredPattern {
            task_type: "refactor".into(),
            occurrence_count: 5,
        };
        assert_eq!(pattern.task_type, "refactor");
        assert_eq!(pattern.occurrence_count, 5);
    }

    #[test]
    fn test_complexity_tier_serde() {
        let tier = ComplexityTier::Heavy;
        let json = serde_json::to_string(&tier).unwrap();
        assert_eq!(json, "\"Heavy\"");
        let back: ComplexityTier = serde_json::from_str(&json).unwrap();
        assert_eq!(back, tier);
    }
}
