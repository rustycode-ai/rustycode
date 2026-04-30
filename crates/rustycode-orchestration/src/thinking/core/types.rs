use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

pub type ThoughtId = Uuid;

/// Kinds of thoughts in the reasoning process
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThoughtKind {
    Initial,
    Refinement,
    Analysis,
    Synthesis,
    Critique,
    Resolution,
}

/// Types of relationships between thoughts
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EdgeKind {
    /// Derives from (direct consequence)
    DerivesFrom,
    /// Supports (provides evidence for)
    Supports,
    /// Contradicts (conflicts with)
    Contradicts,
    /// Refines (improves upon)
    Refines,
    /// Relates to (loosely connected)
    RelatesTo,
    /// Aggregates (combines multiple)
    Aggregates,
}

/// Metadata about a thought's quality and derivation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThoughtMetadata {
    /// Confidence score [0.0, 1.0]
    pub confidence: f64,
    /// Strategy that generated this thought
    pub strategy: String,
    /// Reasoning depth (distance from initial thought)
    pub depth: usize,
    /// Whether this thought has been pruned from active consideration
    pub pruned: bool,
    /// Number of times this thought has been analyzed
    pub analysis_count: usize,
    /// Supporting evidence references
    pub evidence: Vec<String>,
}

impl Default for ThoughtMetadata {
    fn default() -> Self {
        Self {
            confidence: 0.5,
            strategy: String::new(),
            depth: 0,
            pruned: false,
            analysis_count: 0,
            evidence: Vec::new(),
        }
    }
}

/// Represents a single thought in the reasoning graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Thought {
    pub id: ThoughtId,
    pub kind: ThoughtKind,
    pub content: String,
    pub metadata: ThoughtMetadata,
    /// Unix timestamp (seconds) when this thought was created
    pub created_at: i64,
}

impl Thought {
    #[must_use]
    #[allow(clippy::cast_possible_wrap)]
    pub fn new(kind: ThoughtKind, content: String) -> Self {
        let created_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        Self {
            id: Uuid::new_v4(),
            kind,
            content,
            metadata: ThoughtMetadata::default(),
            created_at,
        }
    }

    #[must_use]
    pub fn with_strategy(mut self, strategy: impl Into<String>) -> Self {
        self.metadata.strategy = strategy.into();
        self
    }

    #[must_use]
    pub const fn with_confidence(mut self, confidence: f64) -> Self {
        self.metadata.confidence = confidence.clamp(0.0, 1.0);
        self
    }
}

/// Directed edge between two thoughts
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    pub from: ThoughtId,
    pub to: ThoughtId,
    pub kind: EdgeKind,
    pub strength: f64, // [0.0, 1.0]
}

impl Edge {
    #[must_use]
    pub const fn new(from: ThoughtId, to: ThoughtId, kind: EdgeKind) -> Self {
        Self {
            from,
            to,
            kind,
            strength: 0.8,
        }
    }
}

/// Operations that can be performed on the reasoning graph
#[derive(Debug, Clone)]
pub enum Operation {
    /// Generate new thoughts from a source thought
    Generate {
        from: ThoughtId,
        count: usize,
        prompt_template: String,
    },
    /// Aggregate multiple thoughts into a single thought
    Aggregate {
        from_ids: Vec<ThoughtId>,
        aggregation_method: AggregationMethod,
        prompt_template: String,
    },
    /// Score thoughts based on criteria
    Score {
        thought_id: ThoughtId,
        criteria: Vec<String>,
    },
    /// Refine a thought with more detailed reasoning
    Refine {
        thought_id: ThoughtId,
        refinement_prompt: String,
    },
    /// Select a subset of thoughts based on strategy
    Select {
        from_ids: Vec<ThoughtId>,
        count: usize,
        strategy: SelectionStrategy,
    },
}

#[derive(Debug, Clone, Copy)]
pub enum AggregationMethod {
    /// Combine all contributions
    Combine,
    /// Synthesis of viewpoints
    Synthesize,
    /// Consensus across ideas
    Consensus,
}

#[derive(Debug, Clone, Copy)]
pub enum SelectionStrategy {
    /// Highest confidence thoughts
    TopConfidence,
    /// Most diverse thoughts
    Diversity,
    /// Most supported thoughts
    Support,
    /// Best coverage of different aspects
    Coverage,
}

/// Configuration for the thinking process
#[derive(Debug, Clone)]
pub struct ThinkingConfig {
    /// Maximum number of thoughts to maintain
    pub max_nodes: usize,
    /// Maximum reasoning depth
    pub max_depth: usize,
    /// Confidence threshold for keeping thoughts
    pub confidence_threshold: f64,
    /// Time limit in seconds
    pub time_limit_secs: u64,
    /// Target confidence for early stopping
    pub target_confidence: f64,
    /// Override LLM model (None = use provider default)
    pub model: Option<String>,
    /// Max tokens per LLM thought generation call
    pub max_tokens_per_thought: u32,
}

impl Default for ThinkingConfig {
    fn default() -> Self {
        Self {
            max_nodes: 200,
            max_depth: 8,
            confidence_threshold: 0.3,
            time_limit_secs: 120,
            target_confidence: 0.7,
            model: None,
            max_tokens_per_thought: 800,
        }
    }
}

impl ThinkingConfig {
    /// Set the LLM model override
    #[must_use]
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }
}

/// Execution parameters
#[derive(Debug, Clone)]
pub struct ExecutionParams {
    pub config: ThinkingConfig,
    pub initial_prompt: String,
    pub selected_strategy: Option<String>,
    pub metadata: HashMap<String, String>,
}

impl ExecutionParams {
    pub fn new(initial_prompt: impl Into<String>) -> Self {
        Self {
            config: ThinkingConfig::default(),
            initial_prompt: initial_prompt.into(),
            selected_strategy: None,
            metadata: HashMap::new(),
        }
    }

    #[must_use]
    pub fn with_config(mut self, config: ThinkingConfig) -> Self {
        self.config = config;
        self
    }

    #[must_use]
    pub fn with_strategy(mut self, strategy: impl Into<String>) -> Self {
        self.selected_strategy = Some(strategy.into());
        self
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_thought_new() {
        let t = Thought::new(ThoughtKind::Initial, "hello".into());
        assert_eq!(t.kind, ThoughtKind::Initial);
        assert_eq!(t.content, "hello");
        assert!(!t.metadata.pruned);
    }

    #[test]
    fn test_thought_with_strategy() {
        let t = Thought::new(ThoughtKind::Analysis, "test".into()).with_strategy("Sequential");
        assert_eq!(t.metadata.strategy, "Sequential");
    }

    #[test]
    fn test_thought_with_confidence_clamps() {
        let t = Thought::new(ThoughtKind::Synthesis, "test".into()).with_confidence(1.5);
        assert!((t.metadata.confidence - 1.0).abs() < f64::EPSILON);

        let t2 = Thought::new(ThoughtKind::Critique, "test".into()).with_confidence(-0.5);
        assert!((t2.metadata.confidence - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_thought_serialization_roundtrip() {
        let t = Thought::new(ThoughtKind::Resolution, "result".into());
        let json = serde_json::to_string(&t).unwrap();
        let back: Thought = serde_json::from_str(&json).unwrap();
        assert_eq!(t.id, back.id);
        assert_eq!(t.kind, back.kind);
        assert_eq!(t.content, back.content);
    }

    #[test]
    fn test_edge_new() {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let e = Edge::new(a, b, EdgeKind::Supports);
        assert_eq!(e.from, a);
        assert_eq!(e.to, b);
        assert_eq!(e.kind, EdgeKind::Supports);
        assert!((e.strength - 0.8).abs() < f64::EPSILON);
    }

    #[test]
    fn test_thought_metadata_default() {
        let m = ThoughtMetadata::default();
        assert!((m.confidence - 0.5).abs() < f64::EPSILON);
        assert!(m.strategy.is_empty());
        assert_eq!(m.depth, 0);
        assert!(!m.pruned);
    }

    #[test]
    fn test_thinking_config_default() {
        let c = ThinkingConfig::default();
        assert_eq!(c.max_nodes, 200);
        assert_eq!(c.max_depth, 8);
        assert!(c.model.is_none());
    }

    #[test]
    fn test_thinking_config_with_model() {
        let c = ThinkingConfig::default().with_model("gpt-4");
        assert_eq!(c.model, Some("gpt-4".into()));
    }

    #[test]
    fn test_execution_params_builder() {
        let params = ExecutionParams::new("solve X").with_strategy("Dialectic");
        assert_eq!(params.initial_prompt, "solve X");
        assert_eq!(params.selected_strategy, Some("Dialectic".into()));
    }

    #[test]
    fn test_execution_params_with_config() {
        let config = ThinkingConfig {
            max_nodes: 50,
            ..ThinkingConfig::default()
        };
        let params = ExecutionParams::new("test").with_config(config);
        assert_eq!(params.config.max_nodes, 50);
    }

    #[test]
    fn test_thought_kind_variants_serialize() {
        for kind in [
            ThoughtKind::Initial,
            ThoughtKind::Refinement,
            ThoughtKind::Analysis,
            ThoughtKind::Synthesis,
            ThoughtKind::Critique,
            ThoughtKind::Resolution,
        ] {
            let json = serde_json::to_string(&kind).unwrap();
            let back: ThoughtKind = serde_json::from_str(&json).unwrap();
            assert_eq!(kind, back);
        }
    }

    #[test]
    fn test_edge_kind_variants_serialize() {
        for kind in [
            EdgeKind::DerivesFrom,
            EdgeKind::Supports,
            EdgeKind::Contradicts,
            EdgeKind::Refines,
            EdgeKind::RelatesTo,
            EdgeKind::Aggregates,
        ] {
            let json = serde_json::to_string(&kind).unwrap();
            let back: EdgeKind = serde_json::from_str(&json).unwrap();
            assert_eq!(kind, back);
        }
    }

    #[test]
    fn test_thought_created_at_populated() {
        let t = Thought::new(ThoughtKind::Initial, "test".into());
        assert!(
            t.created_at > 0,
            "created_at should be set to current Unix time"
        );

        // Thoughts created close together should have close timestamps
        let t2 = Thought::new(ThoughtKind::Analysis, "second".into());
        assert!((t2.created_at - t.created_at).unsigned_abs() <= 2);
    }

    #[test]
    fn test_thought_created_at_preserved_in_serialization() {
        let t = Thought::new(ThoughtKind::Initial, "persist".into());
        let original_created_at = t.created_at;
        let json = serde_json::to_string(&t).unwrap();
        let back: Thought = serde_json::from_str(&json).unwrap();
        assert_eq!(back.created_at, original_created_at);
    }
}
