use crate::execution_limits::{ExecutionLimitError, ExecutionLimits, ExecutionLimitsConfig};
use crate::execution_trace::ExecutionTrace;
use crate::shared_workspace::SharedWorkspace;
use chrono::{DateTime, Utc};
use rustycode_protocol::agent_protocol::AgentRole;
use rustycode_protocol::{ExecutionPhase, Message, PhaseSkipConfig, PhaseTransitionError};
use rustycode_tools::doom_loop::DoomLoopDetector;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskPhase {
    Planning,
    Tier2Execution,
    Tier3Review,
    Tier4Recomposition,
    Tier5Thinking,
    Refining,
    Completed,
    Failed,
    Cancelled,
    Killed,
}

impl TaskPhase {
    pub const fn tier(&self) -> u8 {
        match self {
            Self::Tier2Execution => 2,
            Self::Tier3Review => 3,
            Self::Tier4Recomposition => 4,
            Self::Tier5Thinking => 5,
            _ => 0,
        }
    }

    pub const fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Failed | Self::Cancelled | Self::Killed
        )
    }

    pub const fn is_active(&self) -> bool {
        !self.is_terminal()
    }
}

impl fmt::Display for TaskPhase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Planning => write!(f, "planning"),
            Self::Tier2Execution => write!(f, "tier2_execution"),
            Self::Tier3Review => write!(f, "tier3_review"),
            Self::Tier4Recomposition => write!(f, "tier4_recomposition"),
            Self::Tier5Thinking => write!(f, "tier5_thinking"),
            Self::Refining => write!(f, "refining"),
            Self::Completed => write!(f, "completed"),
            Self::Failed => write!(f, "failed"),
            Self::Cancelled => write!(f, "cancelled"),
            Self::Killed => write!(f, "killed"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TaskComplexity {
    Simple,
    #[default]
    Moderate,
    Complex,
    Expert,
}

impl TaskComplexity {
    pub const fn complexity_description(&self) -> &'static str {
        match self {
            Self::Simple => "simple",
            Self::Moderate => "moderate",
            Self::Complex => "complex",
            Self::Expert => "expert",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskConstraints {
    pub complexity: TaskComplexity,
    pub max_retries: u8,
    pub timeout_seconds: u64,
}

impl Default for TaskConstraints {
    fn default() -> Self {
        Self {
            complexity: TaskComplexity::default(),
            max_retries: 3,
            timeout_seconds: 120,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskContext {
    pub task_id: String,
    pub original_request: String,
    pub current_phase: TaskPhase,
    pub current_tier: u8,
    pub attempt_count: u8,
    pub cost_used: f64,
    pub budget_limit: f64,
    pub token_count: u64,
    pub execution_trace: ExecutionTrace,
    pub constraints: TaskConstraints,
    pub agent_role: AgentRole,
    pub classification_tier: crate::types::ExecutionTier,
    #[serde(default)]
    pub execution_phase: ExecutionPhase,
    #[serde(default)]
    pub phase_skip: PhaseSkipConfig,
    pub created_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    #[serde(skip)]
    pub workspace: Option<Arc<SharedWorkspace>>,
    #[serde(skip)]
    pub reasoning_graph: Option<crate::thinking::ReasoningGraph>,
    #[serde(default)]
    pub conversation_history: Vec<Message>,
    #[serde(skip)]
    pub execution_limits: Option<ExecutionLimits>,
    #[serde(skip)]
    pub doom_loop_detector: Option<DoomLoopDetector>,
}

impl TaskContext {
    pub fn new(task_id: String, original_request: String) -> Self {
        Self {
            task_id: task_id.clone(),
            original_request,
            current_phase: TaskPhase::Planning,
            current_tier: 2,
            attempt_count: 0,
            cost_used: 0.0,
            budget_limit: 10.0,
            token_count: 0,
            execution_trace: ExecutionTrace::new(task_id),
            constraints: TaskConstraints::default(),
            agent_role: AgentRole::Worker,
            classification_tier: crate::types::ExecutionTier::Musician,
            execution_phase: ExecutionPhase::default(),
            phase_skip: PhaseSkipConfig::default(),
            created_at: Utc::now(),
            completed_at: None,
            workspace: None,
            reasoning_graph: None,
            conversation_history: Vec::new(),
            execution_limits: None,
            doom_loop_detector: None,
        }
    }

    pub fn with_workspace(
        task_id: String,
        original_request: String,
        workspace: Arc<SharedWorkspace>,
    ) -> Self {
        Self {
            workspace: Some(workspace),
            ..Self::new(task_id, original_request)
        }
    }

    pub const fn advance_phase(&mut self, new_phase: TaskPhase) {
        self.current_phase = new_phase;
        self.current_tier = new_phase.tier();
        self.attempt_count = 0;
    }

    pub fn complete(&mut self, phase: TaskPhase) {
        self.current_phase = phase;
        self.completed_at = Some(Utc::now());
    }

    pub fn kill(&mut self) {
        self.current_phase = TaskPhase::Killed;
        self.completed_at = Some(Utc::now());
    }

    pub fn duration_ms(&self) -> Option<i64> {
        self.completed_at
            .map(|end| (end - self.created_at).num_milliseconds())
    }

    pub fn budget_remaining(&self) -> f64 {
        (self.budget_limit - self.cost_used).max(0.0)
    }

    pub fn add_cost(&mut self, cost_usd: f64) {
        self.cost_used += cost_usd;
    }

    #[allow(clippy::missing_const_for_fn)]
    pub fn add_tokens(&mut self, tokens: u64) {
        self.token_count = self.token_count.saturating_add(tokens);
    }

    pub const fn transition_to(&mut self, phase: TaskPhase) {
        self.current_phase = phase;
    }

    /// Set the execution lifecycle phase.
    pub fn transition_execution_phase(
        &mut self,
        phase: ExecutionPhase,
    ) -> Result<(), PhaseTransitionError> {
        self.execution_phase.transition_to(phase)?;
        self.execution_phase = phase;
        Ok(())
    }

    /// Reset the execution phase from skip flags.
    pub fn reset_execution_phase(&mut self) {
        self.execution_phase = self.phase_skip.starting_phase();
    }

    pub fn escalate(&mut self) {
        self.current_tier = self.current_tier.saturating_add(1).min(5);
        self.attempt_count = 0;
    }

    /// Initialize execution limits from a config.
    pub fn init_execution_limits(&mut self, config: ExecutionLimitsConfig) {
        self.execution_limits = Some(ExecutionLimits::new(config));
    }

    /// Check whether a tool call is within budget.
    /// Returns `Ok(())` if no limits are configured (limits disabled).
    pub fn check_tool_limit(&mut self) -> Result<(), ExecutionLimitError> {
        if let Some(ref mut limits) = self.execution_limits {
            return limits.check_all_before_tool();
        }
        Ok(())
    }

    /// Check whether a model call is within budget.
    pub fn check_model_limit(&mut self) -> Result<(), ExecutionLimitError> {
        if let Some(ref mut limits) = self.execution_limits {
            return limits.check_all_before_model();
        }
        Ok(())
    }

    /// Record token consumption against the budget.
    pub fn check_token_limit(&mut self, tokens: u32) -> Result<(), ExecutionLimitError> {
        if let Some(ref mut limits) = self.execution_limits {
            return limits.check_tokens(tokens);
        }
        Ok(())
    }

    /// Check whether execution time has exceeded the wall-clock limit.
    pub fn check_time_limit(&self) -> Result<(), ExecutionLimitError> {
        if let Some(ref limits) = self.execution_limits {
            return limits.check_time();
        }
        Ok(())
    }

    /// Whether any execution limit is at or above its warning threshold.
    pub fn has_limit_warnings(&self) -> bool {
        self.execution_limits
            .as_ref()
            .is_some_and(|l| l.has_warnings())
    }

    /// Get a snapshot of current execution limit usage, if configured.
    pub fn execution_snapshot(&self) -> Option<crate::execution_limits::ExecutionSnapshot> {
        self.execution_limits.as_ref().map(|l| l.snapshot())
    }

    /// Enable doom loop detection for this task.
    pub fn enable_doom_loop_detection(&mut self) {
        self.doom_loop_detector = Some(DoomLoopDetector::new());
    }

    /// Record a tool call with the doom loop detector and enforce abort.
    /// Returns `Ok(())` if clean or warning, `Err` if abort threshold reached.
    pub fn check_doom_loop(
        &mut self,
        tool_name: &str,
        args: &str,
    ) -> Result<(), ExecutionLimitError> {
        if let Some(ref mut detector) = self.doom_loop_detector {
            let status = detector.record(tool_name, args);
            match status {
                rustycode_tools::doom_loop::DoomLoopStatus::Abort {
                    tool_name,
                    repeat_count,
                    ..
                } => {
                    return Err(ExecutionLimitError::DoomLoop {
                        tool_name,
                        repeat_count,
                    });
                }
                rustycode_tools::doom_loop::DoomLoopStatus::Warning { .. } => {
                    // Log but allow — the caller can check separately
                }
                rustycode_tools::doom_loop::DoomLoopStatus::Clean => {}
                // non-exhaustive: future variants are treated as clean
                _ => {}
            }
        }
        Ok(())
    }

    /// Combined pre-tool-call guard: checks execution limits AND doom loop.
    pub fn check_before_tool_call(
        &mut self,
        tool_name: &str,
        args: &str,
    ) -> Result<(), ExecutionLimitError> {
        self.check_tool_limit()?;
        self.check_doom_loop(tool_name, args)?;
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod execution_limit_tests {
    use super::*;
    use crate::autonomy::AutonomyLevel;

    fn ctx_with_limits(level: AutonomyLevel) -> TaskContext {
        let mut ctx = TaskContext::new("test-task".into(), "do thing".into());
        ctx.init_execution_limits(ExecutionLimitsConfig::for_autonomy(level));
        ctx
    }

    // --- Limit initialization ---

    #[test]
    fn no_limits_by_default() {
        let mut ctx = TaskContext::new("t".into(), "req".into());
        // Without init, all checks should pass
        assert!(ctx.check_tool_limit().is_ok());
        assert!(ctx.check_model_limit().is_ok());
        assert!(ctx.check_token_limit(999_999).is_ok());
        assert!(ctx.check_time_limit().is_ok());
    }

    #[test]
    fn init_limits_enforces_tool_cap() {
        let mut ctx = ctx_with_limits(AutonomyLevel::L1);
        // L1: 10 tool calls
        for _ in 0..10 {
            assert!(ctx.check_tool_limit().is_ok());
        }
        assert!(ctx.check_tool_limit().is_err());
    }

    #[test]
    fn init_limits_enforces_model_cap() {
        let mut ctx = ctx_with_limits(AutonomyLevel::L1);
        // L1: 15 model calls
        for _ in 0..15 {
            assert!(ctx.check_model_limit().is_ok());
        }
        assert!(ctx.check_model_limit().is_err());
    }

    #[test]
    fn init_limits_enforces_token_cap() {
        let mut ctx = ctx_with_limits(AutonomyLevel::L1);
        // L1: 50K tokens
        assert!(ctx.check_token_limit(50_000).is_ok());
        assert!(ctx.check_token_limit(1).is_err());
    }

    // --- Doom loop integration ---

    #[test]
    fn doom_loop_clean_when_disabled() {
        let mut ctx = TaskContext::new("t".into(), "req".into());
        // No detector → always clean
        for _ in 0..20 {
            assert!(ctx.check_doom_loop("Read", r#"{"path":"/foo"}"#).is_ok());
        }
    }

    #[test]
    fn doom_loop_allows_diverse_calls() {
        let mut ctx = TaskContext::new("t".into(), "req".into());
        ctx.enable_doom_loop_detection();
        for i in 0..20 {
            let args = format!(r#"{{"path": "/foo/{i}"}}"#);
            assert!(ctx.check_doom_loop("Read", &args).is_ok());
        }
    }

    #[test]
    fn doom_loop_blocks_repeated_calls() {
        let mut ctx = TaskContext::new("t".into(), "req".into());
        ctx.enable_doom_loop_detection();
        let args = r#"{"path": "/same/file"}"#;
        // Calls 1-4: clean or warning (under abort threshold of 5)
        for _ in 0..4 {
            assert!(ctx.check_doom_loop("Read", args).is_ok());
        }
        // 5th call: abort threshold (count = 4 in history + 1 current = 5)
        let result = ctx.check_doom_loop("Read", args);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("doom loop"),
            "should mention doom loop: {err}"
        );
        assert!(
            err.to_string().contains("Read"),
            "should name the tool: {err}"
        );
    }

    // --- Combined check_before_tool_call ---

    #[test]
    fn combined_check_limits_and_doom_loop() {
        let mut ctx = ctx_with_limits(AutonomyLevel::L2);
        ctx.enable_doom_loop_detection();
        // L2: 25 tool calls, should pass diverse calls
        for i in 0..25 {
            let args = format!(r#"{{"path": "/foo/{i}"}}"#);
            assert!(ctx.check_before_tool_call("Read", &args).is_ok());
        }
        // 26th diverse call should fail on tool limit, not doom loop
        let result = ctx.check_before_tool_call("Read", r#"{"path": "/other"}"#);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("tool_calls"));
    }

    #[test]
    fn combined_check_doom_loop_blocks_before_limit() {
        let mut ctx = ctx_with_limits(AutonomyLevel::L4);
        ctx.enable_doom_loop_detection();
        let args = r#"{"path": "/stuck"}"#;
        // L4 allows 100 tool calls, but doom loop aborts at 5th repeat
        for _ in 0..4 {
            assert!(ctx.check_before_tool_call("Read", args).is_ok());
        }
        let result = ctx.check_before_tool_call("Read", args);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("doom loop"));
    }

    // --- Warning detection ---

    #[test]
    fn warning_at_80_percent() {
        let mut ctx = ctx_with_limits(AutonomyLevel::L2);
        // L2: 25 tool calls, warn at 80% = 20
        assert!(!ctx.has_limit_warnings());
        for _ in 0..20 {
            ctx.check_tool_limit().ok();
        }
        assert!(ctx.has_limit_warnings());
    }

    // --- Snapshot ---

    #[test]
    fn snapshot_none_without_limits() {
        let ctx = TaskContext::new("t".into(), "req".into());
        assert!(ctx.execution_snapshot().is_none());
    }

    #[test]
    fn snapshot_tracks_usage() {
        let mut ctx = ctx_with_limits(AutonomyLevel::L2);
        ctx.check_tool_limit().ok();
        ctx.check_model_limit().ok();
        ctx.check_token_limit(1000).ok();

        let snap = ctx.execution_snapshot().unwrap();
        assert_eq!(snap.tool_calls, 1);
        assert_eq!(snap.model_calls, 1);
        assert_eq!(snap.tokens, 1000);
        assert_eq!(snap.tool_call_limit, 25);
    }

    // --- L0 blocks everything ---

    #[test]
    fn l0_blocks_immediately() {
        let mut ctx = ctx_with_limits(AutonomyLevel::L0);
        assert!(ctx.check_tool_limit().is_err());
        assert!(ctx.check_model_limit().is_err());
        assert!(ctx.check_token_limit(1).is_err());
        assert!(ctx.check_time_limit().is_err());
    }
}
