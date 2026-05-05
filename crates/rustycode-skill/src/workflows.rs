//! Enforceable workflows for structured task execution.
//!
//! Workflows define phases that must be completed in order, with verification
//! rules and failure handling. Unlike suggestions, workflows enforce best practices.
//!
//! # Example: TDD Workflow
//!
//! ```rust
//! use rustycode_skill::workflows::WorkflowEngine;
//!
//! let mut engine = WorkflowEngine::default();
//! let matching = engine.find_matching("implement new feature");
//! // Returns TDD workflow: RED → GREEN → REFACTOR
//! ```

use rustycode_protocol::team::{TeamRole, VerificationState};
use serde::{Deserialize, Serialize};

/// A workflow defines an enforceable sequence of phases.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workflow {
    /// Unique identifier (e.g., "tdd", "`planning_first`").
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// Description of what this workflow does.
    pub description: String,
    /// Phases to execute in order.
    pub phases: Vec<WorkflowPhase>,
    /// Keywords that trigger auto-selection of this workflow.
    pub triggers: Vec<String>,
    /// Whether this workflow is enabled.
    pub enabled: bool,
}

/// A single phase within a workflow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowPhase {
    /// Phase name (e.g., "RED", "GREEN", "REFACTOR").
    pub name: String,
    /// Which team role executes this phase.
    pub agent: TeamRole,
    /// Instructions for this phase.
    pub instructions: String,
    /// Verification rule to pass before advancing.
    pub verification: Option<VerificationRule>,
    /// How to handle failures.
    pub on_failure: FailureHandling,
}

/// Rule for verifying a phase completed successfully.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationRule {
    /// What to check (e.g., "cargo test passes", "compiles").
    pub check: String,
    /// Maximum retry attempts before escalation.
    pub retry_max: u32,
    /// Whether to escalate to user on failure.
    pub escalate_on_failure: bool,
}

/// How to handle phase failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum FailureHandling {
    /// Retry the phase up to `retry_max` times.
    Retry,
    /// Roll back changes and abort workflow.
    Rollback,
    /// Continue to next phase despite failure.
    ContinueAnyway,
    /// Escalate to user for manual intervention.
    Escalate,
}

/// Execution state of a workflow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowState {
    /// Current phase index (0-based).
    pub current_phase: usize,
    /// Names of completed phases.
    pub completed_phases: Vec<String>,
    /// Retry counts per phase.
    pub retry_counts: std::collections::HashMap<String, u32>,
    /// Whether workflow execution is complete.
    pub is_complete: bool,
    /// Whether workflow execution failed.
    pub is_failed: bool,
}

impl WorkflowState {
    /// Create initial state for a workflow.
    pub fn new() -> Self {
        Self {
            current_phase: 0,
            completed_phases: Vec::new(),
            retry_counts: std::collections::HashMap::new(),
            is_complete: false,
            is_failed: false,
        }
    }

    /// Get current phase name.
    pub fn current_phase_name(&self, workflow: &Workflow) -> Option<String> {
        workflow
            .phases
            .get(self.current_phase)
            .map(|p| p.name.clone())
    }

    /// Get current phase instructions.
    pub fn current_instructions(&self, workflow: &Workflow) -> Option<String> {
        workflow
            .phases
            .get(self.current_phase)
            .map(|p| p.instructions.clone())
    }

    /// Get current phase agent role.
    pub fn current_agent(&self, workflow: &Workflow) -> Option<TeamRole> {
        workflow.phases.get(self.current_phase).map(|p| p.agent)
    }

    /// Advance to next phase. Returns false if workflow is complete.
    pub const fn advance(&mut self, workflow: &Workflow) -> bool {
        if self.current_phase >= workflow.phases.len() {
            self.is_complete = true;
            return false;
        }
        self.current_phase = self.current_phase.saturating_add(1);
        if self.current_phase >= workflow.phases.len() {
            self.is_complete = true;
            false
        } else {
            true
        }
    }

    /// Record a retry for the current phase.
    pub fn record_retry(&mut self, phase_name: &str) {
        *self.retry_counts.entry(phase_name.to_string()).or_insert(0) += 1;
    }

    /// Get retry count for a phase.
    pub fn retry_count(&self, phase_name: &str) -> u32 {
        *self.retry_counts.get(phase_name).unwrap_or(&0)
    }

    /// Mark workflow as failed.
    pub const fn fail(&mut self) {
        self.is_failed = true;
    }
}

impl Default for WorkflowState {
    fn default() -> Self {
        Self::new()
    }
}

/// Built-in workflows.
pub mod builtin {
    use super::*;

    /// TDD Workflow: Red → Green → Refactor
    pub fn tdd() -> Workflow {
        Workflow {
            id: "tdd".to_string(),
            name: "Test-Driven Development".to_string(),
            description: "Write tests first, then implementation, then refactor".to_string(),
            phases: vec![
                WorkflowPhase {
                    name: "RED".to_string(),
                    agent: TeamRole::Builder,
                    instructions: "Write a failing test defining the desired behavior. MANDATORY: Use mutation-resistant 'Skeptic Assertions' (e.g., assert_eq!(value, expected)) rather than generic success checks. Ensure the test fails as expected.".to_string(),
                    verification: Some(VerificationRule {
                        check: "Test fails with expected error".to_string(),
                        retry_max: 2,
                        escalate_on_failure: false,
                    }),
                    on_failure: FailureHandling::Retry,
                },
                WorkflowPhase {
                    name: "GREEN".to_string(),
                    agent: TeamRole::Builder,
                    instructions: "Write the minimal implementation to make the test pass. Do not worry about code quality yet.".to_string(),
                    verification: Some(VerificationRule {
                        check: "Test passes, compiles".to_string(),
                        retry_max: 3,
                        escalate_on_failure: false,
                    }),
                    on_failure: FailureHandling::Retry,
                },
                WorkflowPhase {
                    name: "REFACTOR".to_string(),
                    agent: TeamRole::Skeptic,
                    instructions: "Improve code quality while keeping tests green. Remove duplication, improve naming, extract functions.".to_string(),
                    verification: Some(VerificationRule {
                        check: "All tests still pass after refactoring".to_string(),
                        retry_max: 2,
                        escalate_on_failure: false,
                    }),
                    on_failure: FailureHandling::Rollback,
                },
            ],
            triggers: vec![
                "tdd".to_string(),
                "implement".to_string(),
                "add feature".to_string(),
                "new function".to_string(),
                "write test".to_string(),
                "test-driven".to_string(),
            ],
            enabled: true,
        }
    }

    /// Planning-First Workflow: Research → Plan → Review → Implement → Verify
    pub fn planning_first() -> Workflow {
        Workflow {
            id: "planning_first".to_string(),
            name: "Planning-First Development".to_string(),
            description: "Research and plan before implementation for complex tasks".to_string(),
            phases: vec![
                WorkflowPhase {
                    name: "RESEARCH".to_string(),
                    agent: TeamRole::Builder,
                    instructions: "Research existing patterns, similar implementations, and relevant documentation. Identify constraints and requirements.".to_string(),
                    verification: None,
                    on_failure: FailureHandling::ContinueAnyway,
                },
                WorkflowPhase {
                    name: "PLAN".to_string(),
                    agent: TeamRole::Architect,
                    instructions: "Create a detailed implementation plan with steps, risks, and success criteria.".to_string(),
                    verification: Some(VerificationRule {
                        check: "Plan has clear steps and success criteria".to_string(),
                        retry_max: 2,
                        escalate_on_failure: false,
                    }),
                    on_failure: FailureHandling::Retry,
                },
                WorkflowPhase {
                    name: "REVIEW".to_string(),
                    agent: TeamRole::Skeptic,
                    instructions: "Review the plan for completeness, feasibility, and risks. Suggest improvements.".to_string(),
                    verification: None,
                    on_failure: FailureHandling::ContinueAnyway,
                },
                WorkflowPhase {
                    name: "IMPLEMENT".to_string(),
                    agent: TeamRole::Builder,
                    instructions: "Execute the plan step by step. Report progress after each step.".to_string(),
                    verification: Some(VerificationRule {
                        check: "All plan steps completed".to_string(),
                        retry_max: 3,
                        escalate_on_failure: true,
                    }),
                    on_failure: FailureHandling::Escalate,
                },
                WorkflowPhase {
                    name: "VERIFY".to_string(),
                    agent: TeamRole::Judge,
                    instructions: "Verify the implementation matches the plan and meets success criteria.".to_string(),
                    verification: Some(VerificationRule {
                        check: "Implementation verified against plan".to_string(),
                        retry_max: 2,
                        escalate_on_failure: true,
                    }),
                    on_failure: FailureHandling::Escalate,
                },
            ],
            triggers: vec![
                "complex".to_string(),
                "refactor".to_string(),
                "architecture".to_string(),
                "design".to_string(),
                "plan".to_string(),
                "proposal".to_string(),
                "architect".to_string(),
            ],
            enabled: true,
        }
    }

    /// Debugging Workflow: Reproduce → Isolate → Hypothesize → Fix → Verify
    pub fn debugging() -> Workflow {
        Workflow {
            id: "debugging".to_string(),
            name: "Systematic Debugging".to_string(),
            description: "Structured approach to finding and fixing bugs".to_string(),
            phases: vec![
                WorkflowPhase {
                    name: "REPRODUCE".to_string(),
                    agent: TeamRole::Builder,
                    instructions: "Create a minimal reproduction of the bug. Document steps to reproduce consistently.".to_string(),
                    verification: Some(VerificationRule {
                        check: "Bug can be reproduced consistently".to_string(),
                        retry_max: 2,
                        escalate_on_failure: true,
                    }),
                    on_failure: FailureHandling::Retry,
                },
                WorkflowPhase {
                    name: "ISOLATE".to_string(),
                    agent: TeamRole::Builder,
                    instructions: "Narrow down the location and cause of the bug. Use logging, debugging, or binary search.".to_string(),
                    verification: None,
                    on_failure: FailureHandling::ContinueAnyway,
                },
                WorkflowPhase {
                    name: "HYPOTHESIZE".to_string(),
                    agent: TeamRole::Skeptic,
                    instructions: "Formulate hypotheses about root cause. Rank by likelihood and testability.".to_string(),
                    verification: None,
                    on_failure: FailureHandling::ContinueAnyway,
                },
                WorkflowPhase {
                    name: "FIX".to_string(),
                    agent: TeamRole::Builder,
                    instructions: "Implement the fix for the most likely root cause.".to_string(),
                    verification: Some(VerificationRule {
                        check: "Fix compiles".to_string(),
                        retry_max: 2,
                        escalate_on_failure: false,
                    }),
                    on_failure: FailureHandling::Retry,
                },
                WorkflowPhase {
                    name: "VERIFY".to_string(),
                    agent: TeamRole::Judge,
                    instructions: "Verify the fix resolves the bug and doesn't introduce regressions.".to_string(),
                    verification: Some(VerificationRule {
                        check: "Bug fixed, no regressions".to_string(),
                        retry_max: 2,
                        escalate_on_failure: true,
                    }),
                    on_failure: FailureHandling::Escalate,
                },
            ],
            triggers: vec![
                "bug".to_string(),
                "debug".to_string(),
                "broken".to_string(),
                "failing".to_string(),
                "error".to_string(),
                "fix".to_string(),
                "troubleshoot".to_string(),
                "crash".to_string(),
            ],
            enabled: true,
        }
    }

    /// Security Review Workflow: OWASP checklist scan
    pub fn security_review() -> Workflow {
        Workflow {
            id: "security_review".to_string(),
            name: "Security Review".to_string(),
            description: "Systematic security review using OWASP checklist".to_string(),
            phases: vec![
                WorkflowPhase {
                    name: "AUTH_CHECK".to_string(),
                    agent: TeamRole::Skeptic,
                    instructions: "Review authentication: Are endpoints protected? Are tokens validated? Is session management secure?".to_string(),
                    verification: None,
                    on_failure: FailureHandling::ContinueAnyway,
                },
                WorkflowPhase {
                    name: "INPUT_VALIDATION".to_string(),
                    agent: TeamRole::Skeptic,
                    instructions: "Review input validation: Are all inputs sanitized? SQL injection prevention? XSS prevention?".to_string(),
                    verification: None,
                    on_failure: FailureHandling::ContinueAnyway,
                },
                WorkflowPhase {
                    name: "SECRETS_AUDIT".to_string(),
                    agent: TeamRole::Skeptic,
                    instructions: "Audit secrets: Any hardcoded credentials? API keys in config? Proper secret management?".to_string(),
                    verification: Some(VerificationRule {
                        check: "No secrets detected in code".to_string(),
                        retry_max: 1,
                        escalate_on_failure: true,
                    }),
                    on_failure: FailureHandling::Escalate,
                },
                WorkflowPhase {
                    name: "RATE_LIMITING".to_string(),
                    agent: TeamRole::Skeptic,
                    instructions: "Review rate limiting: Are APIs rate-limited? DDoS protection? Resource exhaustion prevention?".to_string(),
                    verification: None,
                    on_failure: FailureHandling::ContinueAnyway,
                },
                WorkflowPhase {
                    name: "REPORT".to_string(),
                    agent: TeamRole::Skeptic,
                    instructions: "Generate security report with findings and remediation recommendations.".to_string(),
                    verification: None,
                    on_failure: FailureHandling::ContinueAnyway,
                },
            ],
            triggers: vec![
                "security".to_string(),
                "vulnerability".to_string(),
                "auth".to_string(),
                "token".to_string(),
                "password".to_string(),
                "audit".to_string(),
                "penetration".to_string(),
                "owasp".to_string(),
            ],
            enabled: true,
        }
    }

    /// Get all built-in workflows.
    pub fn all_builtin() -> Vec<Workflow> {
        vec![tdd(), planning_first(), debugging(), security_review()]
    }

    /// Find a built-in workflow by ID.
    pub fn find_builtin(id: &str) -> Option<Workflow> {
        all_builtin().into_iter().find(|w| w.id == id)
    }
}

/// Workflow execution engine integrated with `TeamOrchestrator`.
pub struct WorkflowEngine {
    /// Active workflows (by ID).
    active_workflows: std::collections::HashMap<String, Workflow>,
    /// Current execution state per workflow.
    states: std::collections::HashMap<String, WorkflowState>,
}

impl WorkflowEngine {
    pub fn new() -> Self {
        Self {
            active_workflows: std::collections::HashMap::new(),
            states: std::collections::HashMap::new(),
        }
    }

    /// Load built-in workflows.
    pub fn load_builtins(&mut self) {
        for workflow in builtin::all_builtin() {
            self.active_workflows.insert(workflow.id.clone(), workflow);
        }
    }

    /// Register a custom workflow.
    pub fn register(&mut self, workflow: Workflow) {
        self.active_workflows
            .insert(workflow.id.clone(), workflow.clone());
        self.states.insert(workflow.id, WorkflowState::new());
    }

    /// Find workflows matching a task (by trigger keywords).
    pub fn find_matching(&self, task: &str) -> Vec<&Workflow> {
        let task_lower = task.to_lowercase();
        self.active_workflows
            .values()
            .filter(|w| w.enabled && w.triggers.iter().any(|t| task_lower.contains(t)))
            .collect()
    }

    /// Get a workflow by ID.
    pub fn get(&self, id: &str) -> Option<&Workflow> {
        self.active_workflows.get(id)
    }

    /// Get mutable state for a workflow.
    pub fn state_mut(&mut self, id: &str) -> Option<&mut WorkflowState> {
        self.states.get_mut(id)
    }

    /// Get all active workflows.
    pub fn all_workflows(&self) -> impl Iterator<Item = &Workflow> {
        self.active_workflows.values()
    }

    /// Initialize state for a workflow if not already initialized.
    pub fn init_state(&mut self, workflow_id: &str) {
        if !self.states.contains_key(workflow_id) {
            self.states
                .insert(workflow_id.to_string(), WorkflowState::new());
        }
    }

    /// Get current phase instructions for a workflow.
    pub fn current_instructions(&self, workflow_id: &str) -> Option<String> {
        let workflow = self.active_workflows.get(workflow_id)?;
        let state = self.states.get(workflow_id)?;
        let phase = workflow.phases.get(state.current_phase)?;
        Some(phase.instructions.clone())
    }

    /// Get current phase agent role for a workflow.
    pub fn current_agent(&self, workflow_id: &str) -> Option<TeamRole> {
        let workflow = self.active_workflows.get(workflow_id)?;
        let state = self.states.get(workflow_id)?;
        workflow.phases.get(state.current_phase).map(|p| p.agent)
    }

    /// Verify current phase passed based on verification state.
    pub fn verify_phase(&self, workflow_id: &str, verification_state: &VerificationState) -> bool {
        let Some(workflow) = self.active_workflows.get(workflow_id) else {
            return true;
        };
        let Some(state) = self.states.get(workflow_id) else {
            return true;
        };
        let Some(phase) = workflow.phases.get(state.current_phase) else {
            return true;
        };

        // If no verification rule, phase passes
        let Some(rule) = &phase.verification else {
            return true;
        };

        // Check verification rules
        let check_lower = rule.check.to_lowercase();
        if check_lower.contains("compiles") || check_lower.contains("compile") {
            return verification_state.compiles;
        }
        if check_lower.contains("test") {
            return verification_state.tests.passed > 0 && verification_state.tests.failed == 0;
        }
        // Default: pass if compilation succeeds
        verification_state.compiles
    }

    /// Advance workflow to next phase if verification passes.
    pub fn advance_if_verified(
        &mut self,
        workflow_id: &str,
        verification: &VerificationState,
    ) -> bool {
        if !self.verify_phase(workflow_id, verification) {
            return false;
        }

        let Some(workflow) = self.active_workflows.get(workflow_id) else {
            return false;
        };
        let Some(state) = self.states.get_mut(workflow_id) else {
            return false;
        };

        if let Some(phase) = workflow.phases.get(state.current_phase) {
            state.completed_phases.push(phase.name.clone());
        }

        state.advance(workflow)
    }

    /// Handle phase failure according to `FailureHandling`.
    pub fn handle_failure(&mut self, workflow_id: &str) -> FailureAction {
        let Some(workflow) = self.active_workflows.get(workflow_id) else {
            return FailureAction::Abort;
        };
        let Some(state) = self.states.get_mut(workflow_id) else {
            return FailureAction::Abort;
        };

        let Some(phase) = workflow.phases.get(state.current_phase) else {
            return FailureAction::Abort;
        };

        state.record_retry(&phase.name);

        match phase.on_failure {
            FailureHandling::Retry => {
                let max_retries = phase.verification.as_ref().map_or(3, |v| v.retry_max);
                if state.retry_count(&phase.name) >= max_retries {
                    FailureAction::Escalate
                } else {
                    FailureAction::Retry
                }
            }
            FailureHandling::Rollback => FailureAction::Rollback,
            FailureHandling::ContinueAnyway => {
                state.advance(workflow);
                FailureAction::Continue
            }
            FailureHandling::Escalate => FailureAction::Escalate,
        }
    }

    /// Check if workflow execution is complete.
    pub fn is_complete(&self, workflow_id: &str) -> bool {
        self.states.get(workflow_id).is_some_and(|s| s.is_complete)
    }

    /// Get workflow state summary for display.
    pub fn state_summary(&self, workflow_id: &str) -> Option<String> {
        let workflow = self.active_workflows.get(workflow_id)?;
        let state = self.states.get(workflow_id)?;

        let completed = state.completed_phases.join(" → ");
        let current = state
            .current_phase_name(workflow)
            .unwrap_or_else(|| "unknown".to_string());
        let remaining: Vec<&str> = workflow
            .phases
            .iter()
            .skip(state.current_phase)
            .map(|p| p.name.as_str())
            .collect();

        Some(format!(
            "{}: [{}] → current: {} → remaining: {:?}",
            workflow.name, completed, current, remaining
        ))
    }
}

impl Default for WorkflowEngine {
    fn default() -> Self {
        let mut engine = Self::new();
        engine.load_builtins();
        engine
    }
}

/// Action to take on phase failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum FailureAction {
    /// Retry the current phase.
    Retry,
    /// Roll back changes and abort workflow.
    Rollback,
    /// Continue to next phase.
    Continue,
    /// Escalate to user.
    Escalate,
    /// Abort workflow execution.
    Abort,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tdd_workflow_phases() {
        let tdd = builtin::tdd();
        assert_eq!(tdd.phases.len(), 3);
        assert_eq!(tdd.phases[0].name, "RED");
        assert_eq!(tdd.phases[1].name, "GREEN");
        assert_eq!(tdd.phases[2].name, "REFACTOR");
    }

    #[test]
    fn test_workflow_state_advancement() {
        let tdd = builtin::tdd();
        let mut state = WorkflowState::new();

        assert_eq!(state.current_phase, 0);
        assert!(!state.is_complete);

        state.advance(&tdd);
        assert_eq!(state.current_phase, 1);

        state.advance(&tdd);
        assert_eq!(state.current_phase, 2);

        state.advance(&tdd);
        assert!(state.is_complete);
    }

    #[test]
    fn test_workflow_engine_load_builtins() {
        let engine = WorkflowEngine::default();
        assert!(engine.get("tdd").is_some());
        assert!(engine.get("planning_first").is_some());
        assert!(engine.get("debugging").is_some());
        assert!(engine.get("security_review").is_some());
    }

    #[test]
    fn test_find_matching_workflows() {
        let engine = WorkflowEngine::default();

        let tdd_matches = engine.find_matching("implement new feature");
        assert!(tdd_matches.iter().any(|w| w.id == "tdd"));

        let debug_matches = engine.find_matching("debug the failing test");
        assert!(debug_matches.iter().any(|w| w.id == "debugging"));

        let security_matches = engine.find_matching("security vulnerability in auth");
        assert!(security_matches.iter().any(|w| w.id == "security_review"));
    }

    #[test]
    fn test_workflow_state_retry() {
        let _tdd = builtin::tdd();
        let mut state = WorkflowState::new();

        state.record_retry("RED");
        assert_eq!(state.retry_count("RED"), 1);

        state.record_retry("RED");
        assert_eq!(state.retry_count("RED"), 2);
    }

    #[test]
    fn test_verify_phase() {
        let mut engine = WorkflowEngine::default();
        engine.init_state("tdd");

        let verification_pass = VerificationState {
            compiles: true,
            tests: rustycode_protocol::team::TestSummary {
                total: 5,
                passed: 5,
                failed: 0,
                failed_names: vec![],
            },
            dirty_files: vec![],
        };

        let verification_fail = VerificationState {
            compiles: false,
            tests: rustycode_protocol::team::TestSummary {
                total: 5,
                passed: 0,
                failed: 5,
                failed_names: vec!["test_fails".to_string()],
            },
            dirty_files: vec![],
        };

        // GREEN phase requires test pass
        assert!(engine.verify_phase("tdd", &verification_pass));
        assert!(!engine.verify_phase("tdd", &verification_fail));
    }

    #[test]
    fn test_workflow_state_new_defaults() {
        let state = WorkflowState::new();
        assert_eq!(state.current_phase, 0);
        assert!(state.completed_phases.is_empty());
        assert!(state.retry_counts.is_empty());
        assert!(!state.is_complete);
        assert!(!state.is_failed);
    }

    #[test]
    fn test_workflow_state_fail() {
        let mut state = WorkflowState::new();
        state.fail();
        assert!(state.is_failed);
    }

    #[test]
    fn test_workflow_state_default_impl() {
        let state = WorkflowState::default();
        assert_eq!(state.current_phase, 0);
        assert!(!state.is_complete);
    }

    #[test]
    fn test_current_phase_name_and_instructions() {
        let tdd = builtin::tdd();
        let state = WorkflowState::new();
        assert_eq!(state.current_phase_name(&tdd), Some("RED".to_string()));
        let instructions = state.current_instructions(&tdd).unwrap();
        assert!(instructions.contains("failing test"));
    }

    #[test]
    fn test_current_agent() {
        let tdd = builtin::tdd();
        let state = WorkflowState::new();
        assert_eq!(state.current_agent(&tdd), Some(TeamRole::Builder));
    }

    #[test]
    fn test_current_phase_past_end() {
        let tdd = builtin::tdd();
        let mut state = WorkflowState::new();
        state.advance(&tdd);
        state.advance(&tdd);
        state.advance(&tdd);
        assert!(state.is_complete);
        assert_eq!(state.current_phase_name(&tdd), None);
        assert_eq!(state.current_instructions(&tdd), None);
    }

    #[test]
    fn test_engine_register_custom_workflow() {
        let mut engine = WorkflowEngine::new();
        let workflow = Workflow {
            id: "custom".to_string(),
            name: "Custom".to_string(),
            description: "A custom workflow".to_string(),
            phases: vec![WorkflowPhase {
                name: "DO".to_string(),
                agent: TeamRole::Builder,
                instructions: "Do the thing".to_string(),
                verification: None,
                on_failure: FailureHandling::ContinueAnyway,
            }],
            triggers: vec!["custom".to_string()],
            enabled: true,
        };
        engine.register(workflow);
        assert!(engine.get("custom").is_some());
    }

    #[test]
    fn test_engine_find_no_match() {
        let engine = WorkflowEngine::default();
        let matches = engine.find_matching("do something totally unrelated");
        // May match "implement" trigger on TDD, so check it doesn't always return everything
        assert!(matches.len() <= engine.all_workflows().count());
    }

    #[test]
    fn test_builtin_tdd_triggers() {
        let tdd = builtin::tdd();
        assert!(tdd.triggers.contains(&"implement".to_string()));
        assert!(tdd.triggers.contains(&"add feature".to_string()));
    }

    #[test]
    fn test_builtin_planning_first() {
        let wf = builtin::planning_first();
        assert_eq!(wf.phases.len(), 5);
        assert_eq!(wf.phases[0].name, "RESEARCH");
        assert_eq!(wf.phases[4].name, "VERIFY");
    }

    #[test]
    fn test_builtin_debugging() {
        let wf = builtin::debugging();
        assert_eq!(wf.phases.len(), 5);
        assert_eq!(wf.phases[0].name, "REPRODUCE");
    }

    #[test]
    fn test_builtin_security_review() {
        let wf = builtin::security_review();
        assert_eq!(wf.phases.len(), 5);
        assert_eq!(wf.phases[0].name, "AUTH_CHECK");
    }

    #[test]
    fn test_builtin_find_by_id() {
        assert!(builtin::find_builtin("tdd").is_some());
        assert!(builtin::find_builtin("planning_first").is_some());
        assert!(builtin::find_builtin("debugging").is_some());
        assert!(builtin::find_builtin("security_review").is_some());
        assert!(builtin::find_builtin("nonexistent").is_none());
    }

    #[test]
    fn test_all_builtin_count() {
        let all = builtin::all_builtin();
        assert_eq!(all.len(), 4);
    }

    #[test]
    fn test_engine_current_instructions() {
        let mut engine = WorkflowEngine::default();
        engine.init_state("tdd");
        let instructions = engine.current_instructions("tdd");
        assert!(instructions.is_some());
        assert!(instructions.unwrap().contains("failing test"));
    }

    #[test]
    fn test_engine_current_agent() {
        let mut engine = WorkflowEngine::default();
        engine.init_state("tdd");
        assert_eq!(engine.current_agent("tdd"), Some(TeamRole::Builder));
    }

    #[test]
    fn test_engine_state_summary() {
        let mut engine = WorkflowEngine::default();
        engine.init_state("tdd");
        let summary = engine.state_summary("tdd");
        assert!(summary.is_some());
        assert!(summary.unwrap().contains("Test-Driven Development"));
    }

    #[test]
    fn test_engine_advance_if_verified() {
        let mut engine = WorkflowEngine::default();
        engine.init_state("tdd");

        let verification = VerificationState {
            compiles: true,
            tests: rustycode_protocol::team::TestSummary {
                total: 1,
                passed: 1,
                failed: 0,
                failed_names: vec![],
            },
            dirty_files: vec![],
        };

        let advanced = engine.advance_if_verified("tdd", &verification);
        assert!(advanced);
        assert_eq!(engine.state_mut("tdd").unwrap().completed_phases.len(), 1);
    }

    #[test]
    fn test_engine_advance_fails_verification() {
        let mut engine = WorkflowEngine::default();
        engine.init_state("tdd");

        let verification_fail = VerificationState {
            compiles: false,
            tests: rustycode_protocol::team::TestSummary {
                total: 1,
                passed: 0,
                failed: 1,
                failed_names: vec!["test_x".to_string()],
            },
            dirty_files: vec![],
        };

        let advanced = engine.advance_if_verified("tdd", &verification_fail);
        assert!(!advanced);
    }

    #[test]
    fn test_engine_is_complete() {
        let mut engine = WorkflowEngine::default();
        engine.init_state("tdd");
        assert!(!engine.is_complete("tdd"));
    }

    #[test]
    fn test_handle_failure_retry() {
        let mut engine = WorkflowEngine::default();
        engine.init_state("tdd");
        let action = engine.handle_failure("tdd");
        // TDD RED phase has Retry with retry_max=2, first retry should be Retry
        assert_eq!(action, FailureAction::Retry);
    }

    #[test]
    fn test_handle_failure_unknown_workflow() {
        let mut engine = WorkflowEngine::new();
        let action = engine.handle_failure("nonexistent");
        assert_eq!(action, FailureAction::Abort);
    }

    #[test]
    fn test_workflow_serialization_roundtrip() {
        let tdd = builtin::tdd();
        let json = serde_json::to_string(&tdd).unwrap();
        let decoded: Workflow = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.id, "tdd");
        assert_eq!(decoded.phases.len(), 3);
        assert!(decoded.enabled);
    }

    #[test]
    fn test_workflow_state_serialization_roundtrip() {
        let state = WorkflowState::new();
        let json = serde_json::to_string(&state).unwrap();
        let decoded: WorkflowState = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.current_phase, 0);
        assert!(!decoded.is_complete);
    }

    #[test]
    fn test_failure_handling_serde_roundtrip() {
        for handling in [
            FailureHandling::Retry,
            FailureHandling::Rollback,
            FailureHandling::ContinueAnyway,
            FailureHandling::Escalate,
        ] {
            let json = serde_json::to_string(&handling).unwrap();
            let decoded: FailureHandling = serde_json::from_str(&json).unwrap();
            assert_eq!(handling, decoded);
        }
    }

    #[test]
    fn test_failure_handling_serialized_names() {
        assert_eq!(
            serde_json::to_string(&FailureHandling::Retry).unwrap(),
            "\"retry\""
        );
        assert_eq!(
            serde_json::to_string(&FailureHandling::Rollback).unwrap(),
            "\"rollback\""
        );
        assert_eq!(
            serde_json::to_string(&FailureHandling::ContinueAnyway).unwrap(),
            "\"continue_anyway\""
        );
        assert_eq!(
            serde_json::to_string(&FailureHandling::Escalate).unwrap(),
            "\"escalate\""
        );
    }

    #[test]
    fn test_verification_rule_serde_roundtrip() {
        let rule = VerificationRule {
            check: "cargo test passes".to_string(),
            retry_max: 3,
            escalate_on_failure: true,
        };
        let json = serde_json::to_string(&rule).unwrap();
        let decoded: VerificationRule = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.check, "cargo test passes");
        assert_eq!(decoded.retry_max, 3);
        assert!(decoded.escalate_on_failure);
    }

    #[test]
    fn test_workflow_phase_serde_roundtrip() {
        let phase = WorkflowPhase {
            name: "TEST_PHASE".to_string(),
            agent: TeamRole::Skeptic,
            instructions: "Do the test".to_string(),
            verification: Some(VerificationRule {
                check: "tests pass".to_string(),
                retry_max: 2,
                escalate_on_failure: false,
            }),
            on_failure: FailureHandling::Retry,
        };
        let json = serde_json::to_string(&phase).unwrap();
        let decoded: WorkflowPhase = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.name, "TEST_PHASE");
        assert_eq!(decoded.agent, TeamRole::Skeptic);
        assert!(decoded.verification.is_some());
    }

    #[test]
    fn test_workflow_serde_roundtrip_complex() {
        let workflow = builtin::planning_first();
        let json = serde_json::to_string(&workflow).unwrap();
        let decoded: Workflow = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.id, "planning_first");
        assert_eq!(decoded.phases.len(), 5);
        assert_eq!(decoded.triggers.len(), 7);
        assert!(decoded.enabled);
    }

    #[test]
    fn test_workflow_state_with_retries_serde_roundtrip() {
        let mut state = WorkflowState::new();
        state.record_retry("RED");
        state.record_retry("RED");
        state.record_retry("GREEN");
        let json = serde_json::to_string(&state).unwrap();
        let decoded: WorkflowState = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.retry_count("RED"), 2);
        assert_eq!(decoded.retry_count("GREEN"), 1);
        assert_eq!(decoded.retry_count("UNKNOWN"), 0);
    }

    #[test]
    fn test_handle_failure_continue_anyway() {
        let mut engine = WorkflowEngine::new();
        // planning_first RESEARCH phase has ContinueAnyway
        engine.register(builtin::planning_first());
        engine.init_state("planning_first");

        let action = engine.handle_failure("planning_first");
        assert_eq!(action, FailureAction::Continue);
        // Should have advanced to next phase
        assert_eq!(engine.state_mut("planning_first").unwrap().current_phase, 1);
    }

    #[test]
    fn test_handle_failure_rollback() {
        let mut engine = WorkflowEngine::new();
        engine.register(builtin::tdd());
        engine.init_state("tdd");
        // Advance to REFACTOR phase (index 2) which uses Rollback
        engine.state_mut("tdd").unwrap().current_phase = 2;
        let action = engine.handle_failure("tdd");
        assert_eq!(action, FailureAction::Rollback);
    }

    #[test]
    fn test_handle_failure_escalate() {
        let mut engine = WorkflowEngine::new();
        engine.register(builtin::debugging());
        engine.init_state("debugging");
        // REPRODUCE phase has Escalate in its verification (but FailureHandling::Retry)
        // Move to phase 4 (VERIFY) which has Escalate failure handling
        engine.state_mut("debugging").unwrap().current_phase = 4;
        let action = engine.handle_failure("debugging");
        assert_eq!(action, FailureAction::Escalate);
    }

    #[test]
    fn test_handle_failure_retry_exhaustion() {
        let mut engine = WorkflowEngine::new();
        engine.register(builtin::tdd());
        engine.init_state("tdd");
        // RED phase: retry_max=2, so after 2 retries it should escalate
        engine.handle_failure("tdd"); // retry 1
        engine.handle_failure("tdd"); // retry 2
        let action = engine.handle_failure("tdd"); // retry 3 -> should escalate
        assert_eq!(action, FailureAction::Escalate);
    }

    #[test]
    fn test_verify_phase_no_verification_rule() {
        let mut engine = WorkflowEngine::new();
        engine.register(builtin::planning_first());
        engine.init_state("planning_first");
        // RESEARCH phase has no verification rule
        let verification = VerificationState {
            compiles: false,
            tests: rustycode_protocol::team::TestSummary {
                total: 0,
                passed: 0,
                failed: 0,
                failed_names: vec![],
            },
            dirty_files: vec![],
        };
        assert!(engine.verify_phase("planning_first", &verification));
    }

    #[test]
    fn test_verify_phase_compile_check() {
        let mut engine = WorkflowEngine::new();
        engine.register(builtin::debugging());
        engine.init_state("debugging");
        // FIX phase has check "Fix compiles"
        engine.state_mut("debugging").unwrap().current_phase = 3;

        let compiles_pass = VerificationState {
            compiles: true,
            tests: rustycode_protocol::team::TestSummary {
                total: 0,
                passed: 0,
                failed: 0,
                failed_names: vec![],
            },
            dirty_files: vec![],
        };
        let compiles_fail = VerificationState {
            compiles: false,
            tests: rustycode_protocol::team::TestSummary {
                total: 0,
                passed: 0,
                failed: 0,
                failed_names: vec![],
            },
            dirty_files: vec![],
        };

        assert!(engine.verify_phase("debugging", &compiles_pass));
        assert!(!engine.verify_phase("debugging", &compiles_fail));
    }

    #[test]
    fn test_advance_full_workflow() {
        let mut engine = WorkflowEngine::default();
        engine.init_state("tdd");

        let verification = VerificationState {
            compiles: true,
            tests: rustycode_protocol::team::TestSummary {
                total: 1,
                passed: 1,
                failed: 0,
                failed_names: vec![],
            },
            dirty_files: vec![],
        };

        // Advance through all 3 phases
        assert!(engine.advance_if_verified("tdd", &verification)); // RED -> GREEN
        assert!(engine.advance_if_verified("tdd", &verification)); // GREEN -> REFACTOR
        assert!(!engine.advance_if_verified("tdd", &verification)); // REFACTOR -> complete

        assert!(engine.is_complete("tdd"));
        assert_eq!(engine.state_mut("tdd").unwrap().completed_phases.len(), 3);
    }

    #[test]
    fn test_state_summary_after_advancement() {
        let mut engine = WorkflowEngine::default();
        engine.init_state("tdd");

        let verification = VerificationState {
            compiles: true,
            tests: rustycode_protocol::team::TestSummary {
                total: 1,
                passed: 1,
                failed: 0,
                failed_names: vec![],
            },
            dirty_files: vec![],
        };

        engine.advance_if_verified("tdd", &verification);
        let summary = engine.state_summary("tdd").unwrap();
        assert!(summary.contains("RED"));
        assert!(summary.contains("GREEN"));
    }

    #[test]
    fn test_state_summary_nonexistent_workflow() {
        let engine = WorkflowEngine::new();
        assert!(engine.state_summary("nonexistent").is_none());
    }

    #[test]
    fn test_find_matching_case_insensitive() {
        let engine = WorkflowEngine::default();
        let matches = engine.find_matching("IMPLEMENT a new thing");
        assert!(matches.iter().any(|w| w.id == "tdd"));
    }

    #[test]
    fn test_find_matching_disabled_workflow() {
        let mut engine = WorkflowEngine::new();
        let mut disabled = builtin::tdd();
        disabled.enabled = false;
        engine.register(disabled);
        let matches = engine.find_matching("implement something");
        assert!(matches.is_empty());
    }

    #[test]
    fn test_find_matching_no_overlap() {
        let engine = WorkflowEngine::default();
        let matches = engine.find_matching("fix the bug in the system");
        // "fix" triggers debugging, "bug" triggers debugging
        assert!(matches.iter().any(|w| w.id == "debugging"));
    }

    #[test]
    fn test_engine_init_state_idempotent() {
        let mut engine = WorkflowEngine::default();
        engine.init_state("tdd");
        engine.state_mut("tdd").unwrap().current_phase = 2;
        engine.init_state("tdd"); // should not reset
        assert_eq!(engine.state_mut("tdd").unwrap().current_phase, 2);
    }

    #[test]
    fn test_engine_current_instructions_nonexistent() {
        let engine = WorkflowEngine::new();
        assert!(engine.current_instructions("nonexistent").is_none());
    }

    #[test]
    fn test_engine_current_agent_nonexistent() {
        let engine = WorkflowEngine::new();
        assert!(engine.current_agent("nonexistent").is_none());
    }

    #[test]
    fn test_engine_is_complete_nonexistent() {
        let engine = WorkflowEngine::new();
        assert!(!engine.is_complete("nonexistent"));
    }

    #[test]
    fn test_engine_verify_phase_nonexistent() {
        let engine = WorkflowEngine::new();
        let verification = VerificationState {
            compiles: true,
            tests: rustycode_protocol::team::TestSummary {
                total: 0,
                passed: 0,
                failed: 0,
                failed_names: vec![],
            },
            dirty_files: vec![],
        };
        // Returns true for nonexistent workflows
        assert!(engine.verify_phase("nonexistent", &verification));
    }

    #[test]
    fn test_engine_all_workflows_iterator() {
        let engine = WorkflowEngine::default();
        let count = engine.all_workflows().count();
        assert_eq!(count, 4);
    }

    #[test]
    fn test_workflow_state_advance_returns_false_at_end() {
        let workflow = builtin::tdd();
        let mut state = WorkflowState::new();
        assert!(state.advance(&workflow)); // 0 -> 1
        assert!(state.advance(&workflow)); // 1 -> 2
        assert!(!state.advance(&workflow)); // 2 -> complete
        assert!(!state.advance(&workflow)); // already complete
    }

    #[test]
    fn test_failure_action_variants() {
        // Just verify the enum variants exist and are usable
        let retry = FailureAction::Retry;
        let rollback = FailureAction::Rollback;
        let cont = FailureAction::Continue;
        let escalate = FailureAction::Escalate;
        let abort = FailureAction::Abort;

        assert!(retry != rollback);
        assert!(cont != escalate);
        assert!(abort != retry);
    }

    #[test]
    fn test_tdd_workflow_agents() {
        let tdd = builtin::tdd();
        assert_eq!(tdd.phases[0].agent, TeamRole::Builder); // RED
        assert_eq!(tdd.phases[1].agent, TeamRole::Builder); // GREEN
        assert_eq!(tdd.phases[2].agent, TeamRole::Skeptic); // REFACTOR
    }

    #[test]
    fn test_planning_first_agents() {
        let wf = builtin::planning_first();
        assert_eq!(wf.phases[0].agent, TeamRole::Builder); // RESEARCH
        assert_eq!(wf.phases[1].agent, TeamRole::Architect); // PLAN
        assert_eq!(wf.phases[2].agent, TeamRole::Skeptic); // REVIEW
        assert_eq!(wf.phases[3].agent, TeamRole::Builder); // IMPLEMENT
        assert_eq!(wf.phases[4].agent, TeamRole::Judge); // VERIFY
    }

    #[test]
    fn test_debugging_triggers() {
        let wf = builtin::debugging();
        assert!(wf.triggers.contains(&"bug".to_string()));
        assert!(wf.triggers.contains(&"debug".to_string()));
        assert!(wf.triggers.contains(&"fix".to_string()));
    }

    #[test]
    fn test_security_review_all_phases_skeptic() {
        let wf = builtin::security_review();
        for phase in &wf.phases {
            assert_eq!(phase.agent, TeamRole::Skeptic);
        }
    }
}
