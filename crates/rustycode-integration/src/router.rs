//! Routing types for integration metrics.
//!
//! These types represent routing decisions and execution results used by the
//! metrics collector. The actual routing logic lives in `rustycode-orchestration`.

use serde::{Deserialize, Serialize};

/// Decision about how to route a task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RoutingDecision {
    /// Route to orchestration pipeline.
    Orchestration {
        starting_tier: u32,
        reasoning: String,
    },
    /// Route to legacy execution path.
    Legacy { reasoning: String },
    /// Reject the task (cannot handle).
    Reject { reasoning: String },
}

/// Which execution path was taken.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExecutionPath {
    /// Orchestration pipeline.
    Orchestration,
    /// Legacy path.
    Legacy,
    /// Not yet determined.
    Unknown,
}

/// Outcome of a task execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExecutionOutcome {
    /// Task succeeded with optional output.
    Success(Option<String>),
    /// Task failed with error message.
    Failure(String),
}

/// Metadata about an execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionMetadata {
    /// Wall-clock time in seconds.
    pub wall_time_secs: f64,
    /// Token usage if applicable.
    pub tokens_used: Option<u64>,
    /// Cost in dollars if applicable.
    pub cost_usd: Option<f64>,
}

/// Result of executing a task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskExecutionResult {
    /// Which path was taken.
    pub execution_path: ExecutionPath,
    /// The outcome.
    pub result: ExecutionOutcome,
    /// Optional execution metadata.
    pub execution_metadata: Option<ExecutionMetadata>,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::float_cmp)]
mod tests {
    use super::*;

    #[test]
    fn routing_decision_orchestration_serde() {
        let decision = RoutingDecision::Orchestration {
            starting_tier: 3,
            reasoning: "complex".to_string(),
        };
        let json = serde_json::to_string(&decision).unwrap();
        let back: RoutingDecision = serde_json::from_str(&json).unwrap();
        assert!(matches!(
            back,
            RoutingDecision::Orchestration {
                starting_tier: 3,
                ..
            }
        ));
    }

    #[test]
    fn routing_decision_legacy_serde() {
        let decision = RoutingDecision::Legacy {
            reasoning: "simple".to_string(),
        };
        let json = serde_json::to_string(&decision).unwrap();
        let back: RoutingDecision = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, RoutingDecision::Legacy { .. }));
    }

    #[test]
    fn routing_decision_reject_serde() {
        let decision = RoutingDecision::Reject {
            reasoning: "unsupported".to_string(),
        };
        let json = serde_json::to_string(&decision).unwrap();
        let back: RoutingDecision = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, RoutingDecision::Reject { .. }));
    }

    #[test]
    fn execution_path_equality() {
        assert_eq!(ExecutionPath::Orchestration, ExecutionPath::Orchestration);
        assert_ne!(ExecutionPath::Orchestration, ExecutionPath::Legacy);
        assert_ne!(ExecutionPath::Legacy, ExecutionPath::Unknown);
    }

    #[test]
    fn execution_outcome_serde() {
        let success = ExecutionOutcome::Success(Some("result".to_string()));
        let json = serde_json::to_string(&success).unwrap();
        let back: ExecutionOutcome = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, ExecutionOutcome::Success(Some(_))));

        let failure = ExecutionOutcome::Failure("error".to_string());
        let json = serde_json::to_string(&failure).unwrap();
        let back: ExecutionOutcome = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, ExecutionOutcome::Failure(_)));
    }

    #[test]
    fn execution_metadata_full() {
        let meta = ExecutionMetadata {
            wall_time_secs: 42.5,
            tokens_used: Some(1000),
            cost_usd: Some(0.05),
        };
        let json = serde_json::to_string(&meta).unwrap();
        let back: ExecutionMetadata = serde_json::from_str(&json).unwrap();
        assert_eq!(back.wall_time_secs, 42.5);
        assert_eq!(back.tokens_used, Some(1000));
        assert_eq!(back.cost_usd, Some(0.05));
    }

    #[test]
    fn execution_metadata_minimal() {
        let meta = ExecutionMetadata {
            wall_time_secs: 1.0,
            tokens_used: None,
            cost_usd: None,
        };
        let json = serde_json::to_string(&meta).unwrap();
        let back: ExecutionMetadata = serde_json::from_str(&json).unwrap();
        assert_eq!(back.wall_time_secs, 1.0);
        assert_eq!(back.tokens_used, None);
    }

    #[test]
    fn task_execution_result_full_roundtrip() {
        let result = TaskExecutionResult {
            execution_path: ExecutionPath::Orchestration,
            result: ExecutionOutcome::Success(Some("done".to_string())),
            execution_metadata: Some(ExecutionMetadata {
                wall_time_secs: 10.0,
                tokens_used: Some(500),
                cost_usd: None,
            }),
        };
        let json = serde_json::to_string(&result).unwrap();
        let back: TaskExecutionResult = serde_json::from_str(&json).unwrap();
        assert_eq!(back.execution_path, ExecutionPath::Orchestration);
        assert!(matches!(back.result, ExecutionOutcome::Success(Some(_))));
        assert!(back.execution_metadata.is_some());
    }

    // ── Additional router tests ─────────────

    #[test]
    fn execution_outcome_success_none_serde() {
        let outcome = ExecutionOutcome::Success(None);
        let json = serde_json::to_string(&outcome).unwrap();
        let back: ExecutionOutcome = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, ExecutionOutcome::Success(None)));
    }

    #[test]
    fn execution_outcome_failure_with_message() {
        let outcome = ExecutionOutcome::Failure("timeout exceeded".to_string());
        let msg = format!("{outcome:?}");
        assert!(msg.contains("timeout exceeded"));
    }

    #[test]
    fn execution_path_all_variants_equality() {
        assert_eq!(ExecutionPath::Orchestration, ExecutionPath::Orchestration);
        assert_eq!(ExecutionPath::Legacy, ExecutionPath::Legacy);
        assert_eq!(ExecutionPath::Unknown, ExecutionPath::Unknown);
        assert_ne!(ExecutionPath::Orchestration, ExecutionPath::Legacy);
        assert_ne!(ExecutionPath::Orchestration, ExecutionPath::Unknown);
        assert_ne!(ExecutionPath::Legacy, ExecutionPath::Unknown);
    }

    #[test]
    fn execution_path_serde_all_variants() {
        for path in [
            ExecutionPath::Orchestration,
            ExecutionPath::Legacy,
            ExecutionPath::Unknown,
        ] {
            let json = serde_json::to_string(&path).unwrap();
            let back: ExecutionPath = serde_json::from_str(&json).unwrap();
            assert_eq!(back, path);
        }
    }

    #[test]
    fn routing_decision_orchestration_fields() {
        let decision = RoutingDecision::Orchestration {
            starting_tier: 5,
            reasoning: "complex multi-step task".to_string(),
        };
        if let RoutingDecision::Orchestration {
            starting_tier,
            reasoning,
        } = decision
        {
            assert_eq!(starting_tier, 5);
            assert_eq!(reasoning, "complex multi-step task");
        } else {
            panic!("Expected Orchestration variant");
        }
    }

    #[test]
    fn routing_decision_legacy_fields() {
        let decision = RoutingDecision::Legacy {
            reasoning: "simple task".to_string(),
        };
        if let RoutingDecision::Legacy { reasoning } = &decision {
            assert_eq!(reasoning, "simple task");
        } else {
            panic!("Expected Legacy variant");
        }
    }

    #[test]
    fn routing_decision_reject_fields() {
        let decision = RoutingDecision::Reject {
            reasoning: "unsupported operation".to_string(),
        };
        if let RoutingDecision::Reject { reasoning } = &decision {
            assert_eq!(reasoning, "unsupported operation");
        } else {
            panic!("Expected Reject variant");
        }
    }

    #[test]
    fn execution_metadata_zero_values() {
        let meta = ExecutionMetadata {
            wall_time_secs: 0.0,
            tokens_used: Some(0),
            cost_usd: Some(0.0),
        };
        let json = serde_json::to_string(&meta).unwrap();
        let back: ExecutionMetadata = serde_json::from_str(&json).unwrap();
        assert_eq!(back.wall_time_secs, 0.0);
        assert_eq!(back.tokens_used, Some(0));
        assert_eq!(back.cost_usd, Some(0.0));
    }

    #[test]
    fn task_execution_result_no_metadata() {
        let result = TaskExecutionResult {
            execution_path: ExecutionPath::Legacy,
            result: ExecutionOutcome::Failure("crashed".to_string()),
            execution_metadata: None,
        };
        let json = serde_json::to_string(&result).unwrap();
        let back: TaskExecutionResult = serde_json::from_str(&json).unwrap();
        assert_eq!(back.execution_path, ExecutionPath::Legacy);
        assert!(back.execution_metadata.is_none());
    }

    #[test]
    fn task_execution_result_unknown_path_serde() {
        let result = TaskExecutionResult {
            execution_path: ExecutionPath::Unknown,
            result: ExecutionOutcome::Success(Some("done".to_string())),
            execution_metadata: Some(ExecutionMetadata {
                wall_time_secs: 1.0,
                tokens_used: None,
                cost_usd: None,
            }),
        };
        let json = serde_json::to_string(&result).unwrap();
        let back: TaskExecutionResult = serde_json::from_str(&json).unwrap();
        assert_eq!(back.execution_path, ExecutionPath::Unknown);
    }

    #[test]
    fn routing_decision_debug_format() {
        let d1 = RoutingDecision::Orchestration {
            starting_tier: 1,
            reasoning: "test".to_string(),
        };
        let debug = format!("{d1:?}");
        assert!(debug.contains("Orchestration"));

        let d2 = RoutingDecision::Legacy {
            reasoning: "simple".to_string(),
        };
        let debug = format!("{d2:?}");
        assert!(debug.contains("Legacy"));

        let d3 = RoutingDecision::Reject {
            reasoning: "bad".to_string(),
        };
        let debug = format!("{d3:?}");
        assert!(debug.contains("Reject"));
    }

    #[test]
    fn execution_outcome_debug_format() {
        let success = ExecutionOutcome::Success(Some("ok".to_string()));
        assert!(format!("{success:?}").contains("Success"));

        let failure = ExecutionOutcome::Failure("timeout".to_string());
        assert!(format!("{failure:?}").contains("Failure"));
    }
}
