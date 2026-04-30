use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A learned pattern extracted from sessions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pattern {
    pub id: String,
    pub name: String,
    pub category: PatternCategory,
    pub description: String,
    pub examples: Vec<String>,
    pub confidence: f32,
    pub trigger_conditions: Vec<TriggerCondition>,
    pub metadata: HashMap<String, String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub usage_count: usize,
}

/// Category of pattern
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[non_exhaustive]
pub enum PatternCategory {
    Coding,
    Debugging,
    Refactoring,
    Testing,
    Documentation,
    Architecture,
    Optimization,
}

/// An instinct combines a pattern with triggers and actions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Instinct {
    pub id: String,
    pub pattern: Pattern,
    pub trigger: TriggerCondition,
    pub action: SuggestedAction,
    pub success_rate: f32,
    pub usage_count: usize,
    pub last_used: Option<chrono::DateTime<chrono::Utc>>,
}

/// Condition that triggers an instinct
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerCondition {
    pub id: String,
    pub trigger_type: TriggerType,
    pub pattern: String,
    pub context_requirements: HashMap<String, String>,
    pub confidence_threshold: f32,
}

/// Type of trigger
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[non_exhaustive]
pub enum TriggerType {
    /// Triggered by keywords in user message
    Keyword,
    /// Triggered by file type
    FileType,
    /// Triggered by error message
    ErrorPattern,
    /// Triggered by code pattern
    CodePattern,
    /// Triggered by language/framework context
    Context,
    /// Triggered by user intent
    Intent,
    /// Composite trigger combining multiple conditions
    Composite {
        operator: LogicalOperator,
        conditions: Vec<String>,
    },
}

/// Logical operator for composite triggers
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[non_exhaustive]
pub enum LogicalOperator {
    And,
    Or,
    Xor,
}

/// Suggested action to take when instinct is triggered
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuggestedAction {
    pub id: String,
    pub action_type: ActionType,
    pub description: String,
    pub template: String,
    pub parameters: HashMap<String, String>,
    pub auto_apply: bool,
}

/// Type of action to take
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[non_exhaustive]
pub enum ActionType {
    /// Suggest a command
    SuggestCommand,
    /// Apply code transformation
    Transform,
    /// Generate documentation
    GenerateDocs,
    /// Run tests
    RunTests,
    /// Apply refactoring
    Refactor,
    /// Debug strategy
    DebugStrategy,
    /// Optimization hint
    Optimization,
    /// Custom action
    Custom(String),
}

impl Pattern {
    pub fn new(id: String, name: String, category: PatternCategory, description: String) -> Self {
        Self {
            id,
            name,
            category,
            description,
            examples: Vec::new(),
            confidence: 0.5,
            trigger_conditions: Vec::new(),
            metadata: HashMap::new(),
            created_at: chrono::Utc::now(),
            usage_count: 0,
        }
    }

    pub fn with_example(mut self, example: String) -> Self {
        self.examples.push(example);
        self
    }

    pub const fn with_confidence(mut self, confidence: f32) -> Self {
        self.confidence = confidence.clamp(0.0, 1.0);
        self
    }

    pub fn with_trigger(mut self, trigger: TriggerCondition) -> Self {
        self.trigger_conditions.push(trigger);
        self
    }

    pub fn with_metadata(mut self, key: String, value: String) -> Self {
        self.metadata.insert(key, value);
        self
    }

    pub const fn increment_usage(&mut self) {
        self.usage_count = self.usage_count.saturating_add(1);
    }
}

impl Instinct {
    pub const fn new(
        id: String,
        pattern: Pattern,
        trigger: TriggerCondition,
        action: SuggestedAction,
    ) -> Self {
        Self {
            id,
            pattern,
            trigger,
            action,
            success_rate: 0.5,
            usage_count: 0,
            last_used: None,
        }
    }

    pub fn record_success(&mut self) {
        self.usage_count = self.usage_count.saturating_add(1);
        self.last_used = Some(chrono::Utc::now());
        self.success_rate = self.success_rate.mul_add(0.9, 1.0 * 0.1);
    }

    pub fn record_failure(&mut self) {
        self.usage_count = self.usage_count.saturating_add(1);
        self.last_used = Some(chrono::Utc::now());
        self.success_rate = self.success_rate.mul_add(0.9, 0.0 * 0.1);
    }
}

impl TriggerCondition {
    pub fn new(id: String, trigger_type: TriggerType, pattern: String) -> Self {
        Self {
            id,
            trigger_type,
            pattern,
            context_requirements: HashMap::new(),
            confidence_threshold: 0.5,
        }
    }

    pub fn with_context_requirement(mut self, key: String, value: String) -> Self {
        self.context_requirements.insert(key, value);
        self
    }

    pub const fn with_confidence_threshold(mut self, threshold: f32) -> Self {
        self.confidence_threshold = threshold.clamp(0.0, 1.0);
        self
    }
}

impl SuggestedAction {
    pub fn new(id: String, action_type: ActionType, description: String, template: String) -> Self {
        Self {
            id,
            action_type,
            description,
            template,
            parameters: HashMap::new(),
            auto_apply: false,
        }
    }

    pub fn with_parameter(mut self, key: String, value: String) -> Self {
        self.parameters.insert(key, value);
        self
    }

    pub const fn with_auto_apply(mut self, auto_apply: bool) -> Self {
        self.auto_apply = auto_apply;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pattern_new() {
        let pattern = Pattern::new(
            "p1".into(),
            "Test Pattern".into(),
            PatternCategory::Coding,
            "A test pattern".into(),
        );
        assert_eq!(pattern.id, "p1");
        assert_eq!(pattern.name, "Test Pattern");
        assert_eq!(pattern.category, PatternCategory::Coding);
        assert!(pattern.examples.is_empty());
        assert!((pattern.confidence - 0.5).abs() < 0.001);
        assert_eq!(pattern.usage_count, 0);
    }

    #[test]
    fn test_pattern_with_example() {
        let pattern = Pattern::new(
            "p2".into(),
            "Name".into(),
            PatternCategory::Testing,
            "desc".into(),
        )
        .with_example("example 1".into())
        .with_example("example 2".into());
        assert_eq!(pattern.examples.len(), 2);
    }

    #[test]
    fn test_pattern_with_confidence_clamped() {
        let pattern = Pattern::new(
            "p3".into(),
            "Name".into(),
            PatternCategory::Debugging,
            "desc".into(),
        )
        .with_confidence(2.0);
        assert!((pattern.confidence - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_pattern_with_confidence_below_zero() {
        let pattern = Pattern::new(
            "p4".into(),
            "Name".into(),
            PatternCategory::Architecture,
            "desc".into(),
        )
        .with_confidence(-1.0);
        assert!((pattern.confidence - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_pattern_with_trigger() {
        let trigger =
            TriggerCondition::new("t1".into(), TriggerType::Keyword, "test pattern".into());
        let pattern = Pattern::new(
            "p5".into(),
            "Name".into(),
            PatternCategory::Testing,
            "desc".into(),
        )
        .with_trigger(trigger);
        assert_eq!(pattern.trigger_conditions.len(), 1);
    }

    #[test]
    fn test_pattern_with_metadata() {
        let pattern = Pattern::new(
            "p6".into(),
            "Name".into(),
            PatternCategory::Optimization,
            "desc".into(),
        )
        .with_metadata("language".into(), "rust".into())
        .with_metadata("framework".into(), "tokio".into());
        assert_eq!(pattern.metadata.len(), 2);
        assert_eq!(pattern.metadata.get("language").unwrap(), "rust");
    }

    #[test]
    fn test_pattern_increment_usage() {
        let mut pattern = Pattern::new(
            "p7".into(),
            "Name".into(),
            PatternCategory::Coding,
            "desc".into(),
        );
        assert_eq!(pattern.usage_count, 0);
        pattern.increment_usage();
        assert_eq!(pattern.usage_count, 1);
        pattern.increment_usage();
        assert_eq!(pattern.usage_count, 2);
    }

    #[test]
    fn test_pattern_serialization_roundtrip() {
        let pattern = Pattern::new(
            "p8".into(),
            "Serialize Test".into(),
            PatternCategory::Refactoring,
            "desc".into(),
        )
        .with_example("ex1".into())
        .with_confidence(0.75);
        let json = serde_json::to_string(&pattern).unwrap();
        let decoded: Pattern = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.id, "p8");
        assert_eq!(decoded.name, "Serialize Test");
        assert_eq!(decoded.category, PatternCategory::Refactoring);
        assert_eq!(decoded.examples.len(), 1);
    }

    #[test]
    fn test_pattern_category_serde_roundtrip() {
        for cat in &[
            PatternCategory::Coding,
            PatternCategory::Debugging,
            PatternCategory::Refactoring,
            PatternCategory::Testing,
            PatternCategory::Documentation,
            PatternCategory::Architecture,
            PatternCategory::Optimization,
        ] {
            let json = serde_json::to_string(cat).unwrap();
            let decoded: PatternCategory = serde_json::from_str(&json).unwrap();
            assert_eq!(*cat, decoded);
        }
    }

    #[test]
    fn test_trigger_condition_new() {
        let trigger = TriggerCondition::new(
            "tc1".into(),
            TriggerType::ErrorPattern,
            r"error\[E\d+".into(),
        );
        assert_eq!(trigger.id, "tc1");
        assert_eq!(trigger.trigger_type, TriggerType::ErrorPattern);
        assert!(trigger.context_requirements.is_empty());
        assert!((trigger.confidence_threshold - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_trigger_condition_with_context() {
        let trigger = TriggerCondition::new("tc2".into(), TriggerType::FileType, "*.rs".into())
            .with_context_requirement("language".into(), "rust".into())
            .with_context_requirement("project".into(), "rustycode".into());
        assert_eq!(trigger.context_requirements.len(), 2);
    }

    #[test]
    fn test_trigger_condition_confidence_clamped() {
        let trigger = TriggerCondition::new("tc3".into(), TriggerType::Intent, "test".into())
            .with_confidence_threshold(5.0);
        assert!((trigger.confidence_threshold - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_trigger_type_serde_roundtrip() {
        let types = vec![
            TriggerType::Keyword,
            TriggerType::FileType,
            TriggerType::ErrorPattern,
            TriggerType::CodePattern,
            TriggerType::Context,
            TriggerType::Intent,
        ];
        for tt in &types {
            let json = serde_json::to_string(tt).unwrap();
            let decoded: TriggerType = serde_json::from_str(&json).unwrap();
            assert_eq!(*tt, decoded);
        }
    }

    #[test]
    fn test_trigger_type_composite() {
        let tt = TriggerType::Composite {
            operator: LogicalOperator::And,
            conditions: vec!["cond1".into(), "cond2".into()],
        };
        let json = serde_json::to_string(&tt).unwrap();
        let decoded: TriggerType = serde_json::from_str(&json).unwrap();
        match decoded {
            TriggerType::Composite {
                operator,
                conditions,
            } => {
                assert_eq!(operator, LogicalOperator::And);
                assert_eq!(conditions.len(), 2);
            }
            _ => panic!("Expected Composite"),
        }
    }

    #[test]
    fn test_suggested_action_new() {
        let action = SuggestedAction::new(
            "a1".into(),
            ActionType::SuggestCommand,
            "Run tests".into(),
            "cargo test".into(),
        );
        assert_eq!(action.id, "a1");
        assert_eq!(action.action_type, ActionType::SuggestCommand);
        assert!(!action.auto_apply);
        assert!(action.parameters.is_empty());
    }

    #[test]
    fn test_suggested_action_with_parameter() {
        let action = SuggestedAction::new(
            "a2".into(),
            ActionType::Transform,
            "Refactor".into(),
            "template".into(),
        )
        .with_parameter("file".into(), "main.rs".into())
        .with_parameter("method".into(), "run".into());
        assert_eq!(action.parameters.len(), 2);
        assert_eq!(action.parameters.get("file").unwrap(), "main.rs");
    }

    #[test]
    fn test_suggested_action_auto_apply() {
        let action = SuggestedAction::new(
            "a3".into(),
            ActionType::RunTests,
            "Run tests".into(),
            "cargo test".into(),
        )
        .with_auto_apply(true);
        assert!(action.auto_apply);
    }

    #[test]
    fn test_action_type_serde_roundtrip() {
        let types = vec![
            ActionType::SuggestCommand,
            ActionType::Transform,
            ActionType::GenerateDocs,
            ActionType::RunTests,
            ActionType::Refactor,
            ActionType::DebugStrategy,
            ActionType::Optimization,
            ActionType::Custom("my-action".into()),
        ];
        for at in &types {
            let json = serde_json::to_string(at).unwrap();
            let decoded: ActionType = serde_json::from_str(&json).unwrap();
            assert_eq!(*at, decoded);
        }
    }

    #[test]
    fn test_logical_operator_serde_roundtrip() {
        for op in &[
            LogicalOperator::And,
            LogicalOperator::Or,
            LogicalOperator::Xor,
        ] {
            let json = serde_json::to_string(op).unwrap();
            let decoded: LogicalOperator = serde_json::from_str(&json).unwrap();
            assert_eq!(*op, decoded);
        }
    }

    #[test]
    fn test_instinct_new() {
        let pattern = Pattern::new(
            "ip1".into(),
            "Inst Pattern".into(),
            PatternCategory::Coding,
            "desc".into(),
        );
        let trigger = TriggerCondition::new("it1".into(), TriggerType::Keyword, "trigger".into());
        let action = SuggestedAction::new(
            "ia1".into(),
            ActionType::SuggestCommand,
            "Do thing".into(),
            "cmd".into(),
        );

        let instinct = Instinct::new("i1".into(), pattern, trigger, action);
        assert_eq!(instinct.id, "i1");
        assert!((instinct.success_rate - 0.5).abs() < 0.001);
        assert_eq!(instinct.usage_count, 0);
        assert!(instinct.last_used.is_none());
    }

    #[test]
    fn test_instinct_record_success() {
        let pattern = Pattern::new(
            "ip2".into(),
            "Name".into(),
            PatternCategory::Testing,
            "desc".into(),
        );
        let trigger = TriggerCondition::new("it2".into(), TriggerType::Keyword, "t".into());
        let action = SuggestedAction::new(
            "ia2".into(),
            ActionType::RunTests,
            "Run".into(),
            "test".into(),
        );

        let mut instinct = Instinct::new("i2".into(), pattern, trigger, action);
        instinct.record_success();
        assert_eq!(instinct.usage_count, 1);
        assert!(instinct.last_used.is_some());
        // success_rate moves toward 1.0: 0.5 * 0.9 + 1.0 * 0.1 = 0.55
        assert!(instinct.success_rate > 0.5);
    }

    #[test]
    fn test_instinct_record_failure() {
        let pattern = Pattern::new(
            "ip3".into(),
            "Name".into(),
            PatternCategory::Debugging,
            "desc".into(),
        );
        let trigger = TriggerCondition::new("it3".into(), TriggerType::ErrorPattern, "err".into());
        let action = SuggestedAction::new(
            "ia3".into(),
            ActionType::DebugStrategy,
            "Debug".into(),
            "dbg".into(),
        );

        let mut instinct = Instinct::new("i3".into(), pattern, trigger, action);
        instinct.record_failure();
        assert_eq!(instinct.usage_count, 1);
        // success_rate moves toward 0.0: 0.5 * 0.9 + 0.0 * 0.1 = 0.45
        assert!(instinct.success_rate < 0.5);
    }

    #[test]
    fn test_instinct_multiple_records() {
        let pattern = Pattern::new(
            "ip4".into(),
            "Name".into(),
            PatternCategory::Coding,
            "desc".into(),
        );
        let trigger = TriggerCondition::new("it4".into(), TriggerType::CodePattern, "cp".into());
        let action = SuggestedAction::new(
            "ia4".into(),
            ActionType::Refactor,
            "Refactor".into(),
            "rf".into(),
        );

        let mut instinct = Instinct::new("i4".into(), pattern, trigger, action);
        for _ in 0..10 {
            instinct.record_success();
        }
        assert_eq!(instinct.usage_count, 10);
        // After many successes, rate should be close to 1.0
        assert!(instinct.success_rate > 0.8);
    }

    #[test]
    fn test_suggested_action_serialization() {
        let action = SuggestedAction::new(
            "sa1".into(),
            ActionType::GenerateDocs,
            "Gen docs".into(),
            "template".into(),
        )
        .with_parameter("lang".into(), "rust".into())
        .with_auto_apply(true);
        let json = serde_json::to_string(&action).unwrap();
        let decoded: SuggestedAction = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.id, "sa1");
        assert!(decoded.auto_apply);
        assert_eq!(decoded.parameters.len(), 1);
    }

    #[test]
    fn test_trigger_condition_serialization() {
        let trigger =
            TriggerCondition::new("tc-ser".into(), TriggerType::CodePattern, r"\bfn\b".into())
                .with_confidence_threshold(0.8)
                .with_context_requirement("lang".into(), "rust".into());
        let json = serde_json::to_string(&trigger).unwrap();
        let decoded: TriggerCondition = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.id, "tc-ser");
        assert!((decoded.confidence_threshold - 0.8).abs() < 0.001);
    }
}
