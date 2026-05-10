use crate::execution_trace::ExecutionTrace;
use crate::shared_workspace::SharedWorkspace;
use chrono::{DateTime, Utc};
use rustycode_protocol::agent_protocol::AgentRole;
use rustycode_protocol::{ExecutionPhase, Message, PhaseSkipConfig, PhaseTransitionError};
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
}
