//! PlanManager — bridges plan lifecycle with team coordination.
//!
//! The PlanManager is the component that makes the team system plan-driven:
//!
//! 1. **Create**: Profile a task → generate a structured plan with steps
//! 2. **Drive**: Execute each step through the Builder→Skeptic→Judge loop
//! 3. **Adapt**: Modify the plan when steps fail (reorder, add, remove)
//! 4. **Track**: Maintain plan progress alongside trust/coordination state
//!
//! # Architecture
//!
//! ```text
//! Task → PlanManager
//!         │
//!         ├── Profile task → Generate Plan
//!         │
//!         ├── For each PlanStep:
//!         │       │
//!         │       ├── Builder: implement step
//!         │       ├── Skeptic: review changes
//!         │       └── Judge: verify (compile + test)
//!         │
//!         ├── On step failure:
//!         │       ├── Retry (same approach)
//!         │       ├── Adapt (modify remaining steps)
//!         │       └── Escalate (ask user)
//!         │
//!         └── Return PlanOutcome
//! ```

use anyhow::Result;
use chrono::Utc;
use rustycode_protocol::{team::*, Plan, PlanId, PlanStatus, PlanStep, SessionId, StepStatus};
use tracing::{debug, info};

/// Manages the lifecycle of a plan within the team coordination loop.
///
/// This is the "project manager" role — it knows the overall plan,
/// tracks which step we're on, decides when to adapt, and determines
/// when the task is complete.
pub struct PlanManager {
    /// The plan being executed.
    plan: Plan,
    /// Whether the plan has been adapted (modified from its original form).
    adaptations: Vec<PlanAdaptation>,
    /// Track retries per step index.
    step_retries: Vec<u32>,
    /// Maximum retries per step before adapting.
    max_retries_per_step: u32,
    /// Maximum plan adaptations before escalating.
    max_adaptations: u32,
}

/// A modification to the plan made during execution.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PlanAdaptation {
    /// When this adaptation was made.
    pub at_step: usize,
    /// What triggered the adaptation.
    pub trigger: AdaptationTrigger,
    /// What changed.
    pub change: AdaptationChange,
    /// Why.
    pub reason: String,
}

/// Why the plan was adapted.
#[non_exhaustive]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum AdaptationTrigger {
    /// Step failed too many times.
    StepFailed { step_index: usize, retry_count: u32 },
    /// Skeptic detected a fundamental issue with the approach.
    ApproachFlawed { step_index: usize, issue: String },
    /// New information discovered during execution.
    Discovery { step_index: usize, insight: String },
    /// Dependency between steps changed.
    DependencyChanged { reason: String },
}

/// What changed in the plan.
#[non_exhaustive]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum AdaptationChange {
    /// Steps were reordered.
    Reordered { new_order: Vec<usize> },
    /// Steps were added.
    StepsAdded {
        after_index: usize,
        new_steps: Vec<String>,
    },
    /// Steps were removed (merged or deemed unnecessary).
    StepsRemoved { indices: Vec<usize>, reason: String },
    /// A step's description was updated.
    StepModified {
        index: usize,
        new_description: String,
    },
    /// Approach changed — regenerated remaining steps.
    ApproachChanged {
        from_step: usize,
        new_approach: String,
    },
}

/// The outcome of plan-driven execution.
#[non_exhaustive]
#[derive(Debug, Clone)]
pub enum PlanOutcome {
    /// All plan steps completed successfully.
    Complete {
        /// The final state of the plan.
        plan: Plan,
        /// Files modified during execution.
        files_modified: Vec<String>,
        /// Number of adaptations made.
        adaptations: usize,
        /// Final trust score.
        final_trust: f64,
    },
    /// Plan was adapted and needs re-approval.
    NeedsReplan {
        /// The adapted plan.
        plan: Plan,
        /// What changed and why.
        adaptation: PlanAdaptation,
        /// Steps completed so far.
        completed_steps: usize,
    },
    /// Execution stopped before completion.
    Stopped {
        /// Why it stopped.
        reason: PlanStopReason,
        /// The plan in its current state.
        plan: Plan,
        /// Steps completed before stopping.
        completed_steps: usize,
        /// Files modified before stopping.
        files_modified: Vec<String>,
    },
}

/// Why plan execution stopped.
#[non_exhaustive]
#[derive(Debug, Clone)]
pub enum PlanStopReason {
    /// Budget (turns/tokens) exhausted.
    BudgetExhausted,
    /// Trust degraded below threshold.
    TrustCollapsed,
    /// Too many adaptations without progress.
    AdaptationLimitReached,
    /// User requested stop.
    UserStop,
    /// Fatal error during execution.
    FatalError(String),
}

impl PlanManager {
    pub fn new(plan: Plan) -> Self {
        let step_count = plan.steps.len();
        Self {
            plan,
            adaptations: Vec::new(),
            step_retries: vec![0; step_count],
            max_retries_per_step: 3,
            max_adaptations: 5,
        }
    }

    /// Create a plan from a task description using the profiler.
    ///
    /// This generates a plan appropriate for the task's risk level.
    pub fn create_plan(session_id: SessionId, task: &str, profile: &TaskProfile) -> Self {
        let plan = generate_plan(session_id, task, profile);
        let step_count = plan.steps.len();
        Self {
            plan,
            adaptations: Vec::new(),
            step_retries: vec![0; step_count],
            max_retries_per_step: match profile.risk {
                RiskLevel::Critical => 2,
                RiskLevel::High => 2,
                RiskLevel::Moderate => 3,
                RiskLevel::Low => 5,
                #[allow(unreachable_patterns)]
                _ => 3,
            },
            max_adaptations: 5,
        }
    }

    /// Get the current plan.
    pub fn plan(&self) -> &Plan {
        &self.plan
    }

    /// Get the task profile used to create this plan.
    pub fn profile(&self) -> TaskProfile {
        self.plan.task_profile.clone().unwrap_or_default()
    }

    /// Get the current step index (which step is being worked on).
    pub fn current_step_index(&self) -> Option<usize> {
        self.plan.current_step_index
    }

    /// Get the current step being executed.
    pub fn current_step(&self) -> Option<&PlanStep> {
        self.plan
            .current_step_index
            .and_then(|i| self.plan.steps.get(i))
    }

    /// Get all completed steps.
    pub fn completed_steps(&self) -> Vec<&PlanStep> {
        self.plan
            .steps
            .iter()
            .filter(|s| matches!(s.execution_status, StepStatus::Completed))
            .collect()
    }

    /// Get all remaining (pending) steps.
    pub fn remaining_steps(&self) -> Vec<&PlanStep> {
        self.plan
            .steps
            .iter()
            .filter(|s| matches!(s.execution_status, StepStatus::Pending))
            .collect()
    }

    /// How many adaptations have been made.
    pub fn adaptation_count(&self) -> usize {
        self.adaptations.len()
    }

    /// Advance to the next step.
    ///
    /// Returns the new step index, or None if all steps are done.
    pub fn advance_to_next_step(&mut self) -> Option<usize> {
        // Find the next pending step
        for (i, step) in self.plan.steps.iter_mut().enumerate() {
            if matches!(step.execution_status, StepStatus::Pending) {
                step.execution_status = StepStatus::InProgress;
                step.started_at = Some(Utc::now());
                self.plan.current_step_index = Some(i);
                self.plan.status = PlanStatus::Executing;
                debug!("Advancing to plan step {}: {}", i, step.title);
                return Some(i);
            }
        }
        // All steps completed or no more pending
        None
    }

    /// Mark the current step as completed.
    pub fn complete_current_step(&mut self, results: Vec<String>) {
        if let Some(idx) = self.plan.current_step_index {
            if let Some(step) = self.plan.steps.get_mut(idx) {
                step.execution_status = StepStatus::Completed;
                step.completed_at = Some(Utc::now());
                step.results = results;
                debug!("Completed plan step {}: {}", idx, step.title);

                // Reset retry count for next step
                if idx + 1 < self.step_retries.len() {
                    self.step_retries[idx + 1] = 0;
                }
            }

            // Check if all steps are done
            let all_done = self
                .plan
                .steps
                .iter()
                .all(|s| matches!(s.execution_status, StepStatus::Completed));
            if all_done {
                self.plan.status = PlanStatus::Completed;
                self.plan.execution_completed_at = Some(Utc::now());
                self.plan.current_step_index = None;
                info!("All plan steps completed!");
            }
        }
    }

    /// Record a failure on the current step and decide what to do.
    ///
    /// Returns the recommended action: retry, adapt, or escalate.
    pub fn handle_step_failure(&mut self, error: &str) -> StepFailureAction {
        let idx = match self.plan.current_step_index {
            Some(i) => i,
            None => return StepFailureAction::Escalate("no current step".to_string()),
        };

        // Increment retry count
        if idx < self.step_retries.len() {
            self.step_retries[idx] += 1;
        }
        let retries = self.step_retries.get(idx).copied().unwrap_or(0);

        if let Some(step) = self.plan.steps.get_mut(idx) {
            step.errors.push(error.to_string());
        }

        debug!(
            "Step {} failed (retry {}/{}): {}",
            idx, retries, self.max_retries_per_step, error
        );

        // Check adaptation limit first — if we've adapted too many times, escalate immediately
        if (self.adaptations.len() as u32) >= self.max_adaptations {
            return StepFailureAction::Escalate(format!(
                "Step '{}' failed with {} plan adaptations. Need human guidance.",
                self.plan
                    .steps
                    .get(idx)
                    .map(|s| s.title.as_str())
                    .unwrap_or("unknown"),
                self.adaptations.len()
            ));
        }

        if retries <= self.max_retries_per_step {
            // Can retry
            StepFailureAction::Retry {
                step_index: idx,
                attempt: retries,
                max_attempts: self.max_retries_per_step,
            }
        } else {
            // Max retries reached — adapt the plan
            let adaptation = PlanAdaptation {
                at_step: idx,
                trigger: AdaptationTrigger::StepFailed {
                    step_index: idx,
                    retry_count: retries,
                },
                change: AdaptationChange::ApproachChanged {
                    from_step: idx,
                    new_approach: format!("try different approach after {} failures", retries),
                },
                reason: format!(
                    "Step '{}' failed {} times: {}",
                    self.plan
                        .steps
                        .get(idx)
                        .map(|s| s.title.as_str())
                        .unwrap_or("unknown"),
                    retries,
                    error
                ),
            };
            self.adaptations.push(adaptation.clone());
            StepFailureAction::Adapt {
                step_index: idx,
                adaptation,
            }
        }
    }

    /// Adapt the remaining steps based on a discovery or change.
    ///
    /// This is called when the builder or skeptic discovers something
    /// that affects the plan (e.g., a hidden dependency, an API that
    /// doesn't work as documented).
    pub fn adapt_plan(
        &mut self,
        trigger: AdaptationTrigger,
        change: AdaptationChange,
        reason: String,
    ) -> Result<()> {
        let at_step = self.plan.current_step_index.unwrap_or(0);

        let adaptation = PlanAdaptation {
            at_step,
            trigger,
            change: change.clone(),
            reason,
        };

        // Apply the change
        match change {
            AdaptationChange::StepsAdded {
                after_index,
                ref new_steps,
            } => {
                let insert_pos = after_index + 1;
                for (i, step_desc) in new_steps.iter().enumerate() {
                    let step = PlanStep {
                        order: insert_pos + i,
                        title: step_desc.clone(),
                        description: step_desc.clone(),
                        tools: vec![],
                        expected_outcome: String::new(),
                        rollback_hint: String::new(),
                        execution_status: StepStatus::Pending,
                        tool_calls: vec![],
                        tool_executions: vec![],
                        results: vec![],
                        errors: vec![],
                        started_at: None,
                        completed_at: None,
                    };
                    self.plan.steps.insert(insert_pos + i, step);
                }
                // Re-index all steps
                for (i, step) in self.plan.steps.iter_mut().enumerate() {
                    step.order = i;
                }
                // Expand retry tracking
                self.step_retries.resize(self.plan.steps.len(), 0);
                info!(
                    "Plan adapted: added {} steps after step {}",
                    new_steps.len(),
                    after_index
                );
            }
            AdaptationChange::StepsRemoved {
                ref indices,
                reason: _,
            } => {
                // Remove in reverse order to preserve indices
                let mut sorted = indices.clone();
                sorted.sort_unstable_by(|a, b| b.cmp(a));
                for idx in sorted {
                    if idx < self.plan.steps.len() {
                        self.plan.steps.remove(idx);
                    }
                }
                // Re-index
                for (i, step) in self.plan.steps.iter_mut().enumerate() {
                    step.order = i;
                }
                self.step_retries.resize(self.plan.steps.len(), 0);
                info!("Plan adapted: removed {} steps", indices.len());
            }
            AdaptationChange::StepModified {
                index,
                ref new_description,
            } => {
                if let Some(step) = self.plan.steps.get_mut(index) {
                    step.description = new_description.clone();
                    // Reset the step for re-execution
                    step.execution_status = StepStatus::Pending;
                    step.errors.clear();
                    step.results.clear();
                    if index < self.step_retries.len() {
                        self.step_retries[index] = 0;
                    }
                }
                info!("Plan adapted: modified step {}", index);
            }
            AdaptationChange::ApproachChanged {
                from_step,
                ref new_approach,
            } => {
                // Reset remaining steps to pending
                for i in from_step..self.plan.steps.len() {
                    if let Some(step) = self.plan.steps.get_mut(i) {
                        if !matches!(step.execution_status, StepStatus::Completed) {
                            step.execution_status = StepStatus::Pending;
                            step.errors.clear();
                            step.results.clear();
                        }
                    }
                    if i < self.step_retries.len() {
                        self.step_retries[i] = 0;
                    }
                }
                self.plan.approach = new_approach.clone();
                info!(
                    "Plan adapted: approach changed from step {}: {}",
                    from_step, new_approach
                );
            }
            AdaptationChange::Reordered { ref new_order } => {
                let mut new_steps = Vec::with_capacity(self.plan.steps.len());
                for &idx in new_order {
                    if let Some(step) = self.plan.steps.get(idx).cloned() {
                        new_steps.push(step);
                    }
                }
                // Re-index
                for (i, step) in new_steps.iter_mut().enumerate() {
                    step.order = i;
                }
                self.plan.steps = new_steps;
                self.step_retries.resize(self.plan.steps.len(), 0);
                info!("Plan adapted: steps reordered");
            }
        }

        self.adaptations.push(adaptation);
        Ok(())
    }

    /// Check if the plan is complete.
    pub fn is_complete(&self) -> bool {
        matches!(self.plan.status, PlanStatus::Completed)
    }

    /// Check if the plan has failed.
    pub fn is_failed(&self) -> bool {
        matches!(self.plan.status, PlanStatus::Failed)
    }

    /// Get a summary of plan progress for the coordinator briefing.
    pub fn progress_summary(&self) -> PlanProgress {
        let total = self.plan.steps.len();
        let completed = self
            .plan
            .steps
            .iter()
            .filter(|s| matches!(s.execution_status, StepStatus::Completed))
            .count();
        let failed = self
            .plan
            .steps
            .iter()
            .filter(|s| matches!(s.execution_status, StepStatus::Failed))
            .count();
        let in_progress = self
            .plan
            .steps
            .iter()
            .filter(|s| matches!(s.execution_status, StepStatus::InProgress))
            .count();

        PlanProgress {
            total_steps: total,
            completed_steps: completed,
            failed_steps: failed,
            in_progress_steps: in_progress,
            current_step_title: self.current_step().map(|s| s.title.clone()),
            adaptations: self.adaptations.len(),
            plan_status: self.plan.status.clone(),
        }
    }

    /// Generate a briefing context string for the current step.
    ///
    /// This gives each agent role the right view of where we are in the plan.
    pub fn step_context_for_role(&self, role: TeamRole) -> String {
        let progress = self.progress_summary();
        let current = match self.current_step() {
            Some(s) => format!(
                "## Current Step ({} of {}): {}\n{}\nExpected: {}",
                progress.completed_steps + 1,
                progress.total_steps,
                s.title,
                s.description,
                s.expected_outcome,
            ),
            None => "No active step.".to_string(),
        };

        let completed_summary = if progress.completed_steps > 0 {
            let titles: Vec<_> = self
                .plan
                .steps
                .iter()
                .filter(|s| matches!(s.execution_status, StepStatus::Completed))
                .map(|s| format!("- ✅ {}", s.title))
                .collect();
            format!("## Completed Steps\n{}", titles.join("\n"))
        } else {
            String::new()
        };

        let upcoming = match role {
            TeamRole::Builder | TeamRole::Scalpel => {
                // Builder/Scalpel see upcoming steps to plan ahead
                let upcoming: Vec<_> = self
                    .plan
                    .steps
                    .iter()
                    .filter(|s| matches!(s.execution_status, StepStatus::Pending))
                    .take(3)
                    .map(|s| format!("- {}", s.title))
                    .collect();
                if upcoming.is_empty() {
                    String::new()
                } else {
                    format!("## Upcoming Steps\n{}", upcoming.join("\n"))
                }
            }
            TeamRole::Skeptic => {
                // Skeptic sees files to modify for review scope
                let files = &self.plan.files_to_modify;
                if files.is_empty() {
                    String::new()
                } else {
                    format!(
                        "## Files in Scope\n{}",
                        files
                            .iter()
                            .map(|f| format!("- {}", f))
                            .collect::<Vec<_>>()
                            .join("\n")
                    )
                }
            }
            TeamRole::Architect => {
                // Architect sees the full step list (it plans structure for all of them)
                let steps: Vec<_> = self
                    .plan
                    .steps
                    .iter()
                    .filter(|s| matches!(s.execution_status, StepStatus::Pending))
                    .map(|s| format!("- {}", s.title))
                    .collect();
                if steps.is_empty() {
                    String::new()
                } else {
                    format!("## All Pending Steps\n{}", steps.join("\n"))
                }
            }
            TeamRole::Judge => {
                // Judge sees risks to check for
                let risks = &self.plan.risks;
                if risks.is_empty() {
                    String::new()
                } else {
                    format!(
                        "## Risks to Verify\n{}",
                        risks
                            .iter()
                            .map(|r| format!("- {}", r))
                            .collect::<Vec<_>>()
                            .join("\n")
                    )
                }
            }
            TeamRole::Coordinator => {
                // Coordinator sees the full progress and adaptation history
                let adapt_summary = if self.adaptations.is_empty() {
                    String::new()
                } else {
                    format!(
                        "\n## Adaptations ({} total)\n{}",
                        self.adaptations.len(),
                        self.adaptations
                            .iter()
                            .map(|a| format!("- Step {}: {}", a.at_step, a.reason))
                            .collect::<Vec<_>>()
                            .join("\n")
                    )
                };
                adapt_summary
            }
            #[allow(unreachable_patterns)]
            _ => String::new(),
        };

        vec![current, completed_summary, upcoming]
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    /// Start execution of the plan.
    pub fn start_execution(&mut self) {
        self.plan.status = PlanStatus::Executing;
        self.plan.execution_started_at = Some(Utc::now());
        self.advance_to_next_step();
    }

    /// Get the plan's task description.
    pub fn task(&self) -> &str {
        &self.plan.task
    }

    /// Get the plan's approach description.
    pub fn approach(&self) -> &str {
        &self.plan.approach
    }
}

/// What to do when a step fails.
#[non_exhaustive]
#[derive(Debug, Clone)]
pub enum StepFailureAction {
    /// Retry the same step.
    Retry {
        step_index: usize,
        attempt: u32,
        max_attempts: u32,
    },
    /// Adapt the plan (modify remaining steps).
    Adapt {
        step_index: usize,
        adaptation: PlanAdaptation,
    },
    /// Escalate to the user.
    Escalate(String),
}

/// Summary of plan progress.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PlanProgress {
    pub total_steps: usize,
    pub completed_steps: usize,
    pub failed_steps: usize,
    pub in_progress_steps: usize,
    pub current_step_title: Option<String>,
    pub adaptations: usize,
    pub plan_status: PlanStatus,
}

impl std::fmt::Display for PlanProgress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{} steps", self.completed_steps, self.total_steps)?;
        if let Some(ref title) = self.current_step_title {
            write!(f, " — current: {}", title)?;
        }
        if self.adaptations > 0 {
            write!(f, " ({} adaptations)", self.adaptations)?;
        }
        Ok(())
    }
}

/// Generate a plan from a task profile.
///
/// This creates a structured plan with steps appropriate for the task's
/// complexity and risk level. The steps are derived from the profile's
/// risk/reach/familiarity signals.
fn generate_plan(session_id: SessionId, task: &str, profile: &TaskProfile) -> Plan {
    let steps = match profile.risk {
        RiskLevel::Low => vec![
            make_step(
                0,
                "Understand the task",
                "Read relevant files and understand what needs to change.",
                &["Read", "Grep"],
            ),
            make_step(
                1,
                "Implement the change",
                "Make the required modifications.",
                &["Write", "Bash"],
            ),
            make_step(
                2,
                "Verify",
                "Run tests to confirm the change works.",
                &["Bash"],
            ),
        ],
        RiskLevel::Moderate => vec![
            make_step(
                0,
                "Analyze scope",
                "Identify all files that need to change and understand dependencies.",
                &["Read", "Grep", "lsp_references"],
            ),
            make_step(
                1,
                "Plan approach",
                "Determine the implementation strategy.",
                &[],
            ),
            make_step(
                2,
                "Implement core change",
                "Write the main code changes.",
                &["Write", "Bash"],
            ),
            make_step(
                3,
                "Handle edge cases",
                "Address error handling and edge cases.",
                &["Write"],
            ),
            make_step(
                4,
                "Add/update tests",
                "Ensure test coverage for the changes.",
                &["Write", "Bash"],
            ),
            make_step(
                5,
                "Full verification",
                "Run all tests and checks.",
                &["Bash"],
            ),
        ],
        RiskLevel::High => vec![
            make_step(
                0,
                "Deep analysis",
                "Thoroughly understand the codebase area and all dependencies.",
                &["Read", "Grep", "lsp_references", "lsp_hover"],
            ),
            make_step(
                1,
                "Design solution",
                "Plan the approach with consideration for all affected components.",
                &[],
            ),
            make_step(
                2,
                "Implement with safety",
                "Write changes incrementally with error handling.",
                &["Write", "Bash"],
            ),
            make_step(
                3,
                "Add comprehensive tests",
                "Write tests covering normal paths, edge cases, and failure modes.",
                &["Write", "Bash"],
            ),
            make_step(
                4,
                "Integration verification",
                "Verify the changes work with the broader system.",
                &["Bash"],
            ),
            make_step(
                5,
                "Review for regressions",
                "Check that existing functionality is not broken.",
                &["Bash", "Grep"],
            ),
        ],
        RiskLevel::Critical => vec![
            make_step(
                0,
                "Full audit",
                "Audit the entire affected area including all callers and dependents.",
                &["Read", "Grep", "lsp_references", "lsp_hover"],
            ),
            make_step(
                1,
                "Risk assessment",
                "Document all risks and create mitigation strategies.",
                &[],
            ),
            make_step(
                2,
                "Design with defense-in-depth",
                "Design the solution with multiple safety layers.",
                &[],
            ),
            make_step(
                3,
                "Implement with logging",
                "Write changes with comprehensive logging and error handling.",
                &["Write", "Bash"],
            ),
            make_step(
                4,
                "Security-focused tests",
                "Write tests that specifically target security properties.",
                &["Write", "Bash"],
            ),
            make_step(
                5,
                "Integration testing",
                "Test the changes in a realistic integration scenario.",
                &["Bash"],
            ),
            make_step(
                6,
                "Regression suite",
                "Run the full test suite and check for any regressions.",
                &["Bash"],
            ),
            make_step(
                7,
                "Final review",
                "Review all changes one more time before considering complete.",
                &["Read", "Grep"],
            ),
        ],
        #[allow(unreachable_patterns)]
        _ => vec![
            make_step(
                0,
                "Understand the task",
                "Read relevant files and understand what needs to change.",
                &["Read", "Grep"],
            ),
            make_step(
                1,
                "Implement the change",
                "Make the required modifications.",
                &["Write", "Bash"],
            ),
            make_step(
                2,
                "Verify",
                "Run tests to confirm the change works.",
                &["Bash"],
            ),
        ],
    };

    Plan {
        id: PlanId::new(),
        session_id,
        milestone_id: None,
        task: task.to_string(),
        created_at: Utc::now(),
        status: PlanStatus::Draft,
        summary: task.to_string(),
        approach: format!("{:?} risk, {:?} reach plan", profile.risk, profile.reach),
        steps,
        files_to_modify: vec![],
        risks: profile.signals.iter().map(|s| s.evidence.clone()).collect(),
        current_step_index: None,
        execution_started_at: None,
        execution_completed_at: None,
        execution_error: None,
        task_profile: Some(profile.clone()),
    }
}

fn make_step(order: usize, title: &str, description: &str, tools: &[&str]) -> PlanStep {
    PlanStep {
        order,
        title: title.to_string(),
        description: description.to_string(),
        tools: tools.iter().map(|t| t.to_string()).collect(),
        expected_outcome: String::new(),
        rollback_hint: String::new(),
        execution_status: StepStatus::Pending,
        tool_calls: vec![],
        tool_executions: vec![],
        results: vec![],
        errors: vec![],
        started_at: None,
        completed_at: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_session_id() -> SessionId {
        SessionId::new()
    }

    fn test_profile() -> TaskProfile {
        TaskProfile {
            risk: RiskLevel::Moderate,
            reach: ReachLevel::Local,
            familiarity: Familiarity::SomewhatKnown,
            reversibility: Reversibility::Moderate,
            strategy: ReasoningStrategy::default(),
            signals: vec![],
        }
    }

    #[test]
    fn plan_manager_creates_plan_from_profile() {
        let mgr = PlanManager::create_plan(test_session_id(), "fix auth bug", &test_profile());
        let plan = mgr.plan();
        assert_eq!(plan.steps.len(), 6); // Moderate risk = 6 steps
        assert!(matches!(plan.status, PlanStatus::Draft));
    }

    #[test]
    fn plan_manager_low_risk_has_fewer_steps() {
        let profile = TaskProfile {
            risk: RiskLevel::Low,
            ..test_profile()
        };
        let mgr = PlanManager::create_plan(test_session_id(), "fix typo", &profile);
        assert_eq!(mgr.plan().steps.len(), 3);
    }

    #[test]
    fn plan_manager_critical_risk_has_most_steps() {
        let profile = TaskProfile {
            risk: RiskLevel::Critical,
            ..test_profile()
        };
        let mgr = PlanManager::create_plan(test_session_id(), "fix security vuln", &profile);
        assert_eq!(mgr.plan().steps.len(), 8);
    }

    #[test]
    fn advance_through_steps() {
        let mut mgr = PlanManager::create_plan(test_session_id(), "task", &test_profile());

        mgr.start_execution();
        assert_eq!(mgr.current_step_index(), Some(0));
        assert!(mgr.current_step().unwrap().title.contains("Analyze"));

        mgr.complete_current_step(vec!["found 3 files".to_string()]);
        let next = mgr.advance_to_next_step();
        assert_eq!(next, Some(1));
    }

    #[test]
    fn complete_all_steps() {
        let profile = TaskProfile {
            risk: RiskLevel::Low,
            ..test_profile()
        };
        let mut mgr = PlanManager::create_plan(test_session_id(), "task", &profile);

        mgr.start_execution();
        for _ in 0..3 {
            mgr.complete_current_step(vec![]);
            mgr.advance_to_next_step();
        }
        assert!(mgr.is_complete());
        assert!(matches!(mgr.plan().status, PlanStatus::Completed));
    }

    #[test]
    fn step_failure_retries_then_adapts() {
        let mut mgr = PlanManager::create_plan(test_session_id(), "task", &test_profile());
        mgr.start_execution();

        // Retry within limits
        let action = mgr.handle_step_failure("test failed");
        assert!(matches!(
            action,
            StepFailureAction::Retry { attempt: 1, .. }
        ));

        let action = mgr.handle_step_failure("test failed again");
        assert!(matches!(
            action,
            StepFailureAction::Retry { attempt: 2, .. }
        ));

        let action = mgr.handle_step_failure("still failing");
        assert!(matches!(
            action,
            StepFailureAction::Retry { attempt: 3, .. }
        ));

        // Should now adapt
        let action = mgr.handle_step_failure("max retries");
        assert!(matches!(action, StepFailureAction::Adapt { .. }));
        assert_eq!(mgr.adaptation_count(), 1);
    }

    #[test]
    fn too_many_adaptations_escalates() {
        let mut mgr = PlanManager::create_plan(test_session_id(), "task", &test_profile());
        mgr.start_execution();

        // Force max adaptations
        for _ in 0..5 {
            mgr.adapt_plan(
                AdaptationTrigger::Discovery {
                    step_index: 0,
                    insight: "test".to_string(),
                },
                AdaptationChange::ApproachChanged {
                    from_step: 0,
                    new_approach: "different".to_string(),
                },
                "test".to_string(),
            )
            .unwrap();
        }

        // Now handle failure should escalate
        let action = mgr.handle_step_failure("failing");
        assert!(matches!(action, StepFailureAction::Escalate(_)));
    }

    #[test]
    fn adapt_adds_steps() {
        let mut mgr = PlanManager::create_plan(test_session_id(), "task", &test_profile());
        mgr.start_execution();

        let original_len = mgr.plan().steps.len();
        mgr.adapt_plan(
            AdaptationTrigger::Discovery {
                step_index: 0,
                insight: "need to also fix the config".to_string(),
            },
            AdaptationChange::StepsAdded {
                after_index: 0,
                new_steps: vec!["Update configuration file".to_string()],
            },
            "discovered config dependency".to_string(),
        )
        .unwrap();

        assert_eq!(mgr.plan().steps.len(), original_len + 1);
        assert_eq!(mgr.plan().steps[1].title, "Update configuration file");
    }

    #[test]
    fn adapt_removes_steps() {
        let mut mgr = PlanManager::create_plan(test_session_id(), "task", &test_profile());
        mgr.start_execution();

        let original_len = mgr.plan().steps.len();
        mgr.adapt_plan(
            AdaptationTrigger::Discovery {
                step_index: 0,
                insight: "step 1 is unnecessary".to_string(),
            },
            AdaptationChange::StepsRemoved {
                indices: vec![1],
                reason: "merged into step 0".to_string(),
            },
            "unnecessary step".to_string(),
        )
        .unwrap();

        assert_eq!(mgr.plan().steps.len(), original_len - 1);
    }

    #[test]
    fn progress_summary_tracks_state() {
        let mut mgr = PlanManager::create_plan(test_session_id(), "task", &test_profile());
        mgr.start_execution();

        let progress = mgr.progress_summary();
        assert_eq!(progress.total_steps, 6);
        assert_eq!(progress.completed_steps, 0);
        assert_eq!(progress.in_progress_steps, 1);

        mgr.complete_current_step(vec!["done".to_string()]);
        mgr.advance_to_next_step();

        let progress = mgr.progress_summary();
        assert_eq!(progress.completed_steps, 1);
        assert_eq!(progress.in_progress_steps, 1);
    }

    #[test]
    fn step_context_for_role_filters_by_role() {
        let mut mgr = PlanManager::create_plan(test_session_id(), "task", &test_profile());
        mgr.start_execution();

        // Builder should see upcoming steps
        let builder_ctx = mgr.step_context_for_role(TeamRole::Builder);
        assert!(builder_ctx.contains("Current Step"));
        assert!(builder_ctx.contains("Upcoming Steps"));

        // Skeptic should see files in scope (empty in this case)
        let skeptic_ctx = mgr.step_context_for_role(TeamRole::Skeptic);
        assert!(skeptic_ctx.contains("Current Step"));

        // Judge should see risks
        let judge_ctx = mgr.step_context_for_role(TeamRole::Judge);
        assert!(judge_ctx.contains("Current Step"));
    }

    #[test]
    fn progress_display_format() {
        let mut mgr = PlanManager::create_plan(test_session_id(), "task", &test_profile());
        mgr.start_execution();

        let progress = mgr.progress_summary();
        let display = format!("{}", progress);
        assert!(display.contains("0/6 steps"));
    }

    #[test]
    fn empty_plan_has_no_steps() {
        let plan = Plan {
            id: PlanId::new(),
            session_id: test_session_id(),
            task: "empty task".to_string(),
            approach: "none".to_string(),
            summary: String::new(),
            steps: vec![],
            status: PlanStatus::Draft,
            current_step_index: None,
            execution_started_at: None,
            execution_completed_at: None,
            execution_error: None,
            task_profile: None,

            milestone_id: None,
            files_to_modify: vec![],
            risks: vec![],
            created_at: Utc::now(),
        };
        let mut mgr = PlanManager::new(plan);
        assert_eq!(mgr.plan().steps.len(), 0);
        assert!(mgr.current_step().is_none());
        assert!(mgr.current_step_index().is_none());

        // Advance on empty plan returns None
        assert!(mgr.advance_to_next_step().is_none());
    }

    #[test]
    fn failure_without_active_step_escalates() {
        let mut mgr = PlanManager::create_plan(test_session_id(), "task", &test_profile());
        // Don't start execution — no current step
        assert!(mgr.current_step_index().is_none());

        let action = mgr.handle_step_failure("something broke");
        assert!(matches!(action, StepFailureAction::Escalate(_)));
    }

    #[test]
    fn complete_step_twice_is_idempotent() {
        let mut mgr = PlanManager::create_plan(test_session_id(), "task", &test_profile());
        mgr.start_execution();

        mgr.complete_current_step(vec!["done".to_string()]);

        // Completing again should not panic or corrupt state
        mgr.complete_current_step(vec!["done again".to_string()]);
        assert!(matches!(
            mgr.plan().steps[0].execution_status,
            StepStatus::Completed
        ));
    }

    #[test]
    fn advance_past_last_step_returns_none() {
        let profile = TaskProfile {
            risk: RiskLevel::Low,
            ..test_profile()
        };
        let mut mgr = PlanManager::create_plan(test_session_id(), "task", &profile);
        mgr.start_execution();

        // Complete all 3 steps
        for _ in 0..3 {
            mgr.complete_current_step(vec![]);
            mgr.advance_to_next_step();
        }

        // Advancing past the end should return None
        assert!(mgr.advance_to_next_step().is_none());
        assert!(mgr.is_complete());
    }

    #[test]
    fn remaining_steps_decreases_as_completed() {
        let mut mgr = PlanManager::create_plan(test_session_id(), "task", &test_profile());
        mgr.start_execution();

        let initial_remaining = mgr.remaining_steps().len();
        assert_eq!(initial_remaining, 5); // 6 total, 1 in progress

        mgr.complete_current_step(vec![]);
        mgr.advance_to_next_step();

        assert_eq!(mgr.remaining_steps().len(), 4);
    }

    #[test]
    fn single_step_plan_completes() {
        let plan = Plan {
            id: PlanId::new(),
            session_id: test_session_id(),
            task: "single step".to_string(),
            approach: "just do it".to_string(),
            summary: "do one thing".to_string(),
            steps: vec![PlanStep {
                order: 0,
                title: "Do the thing".to_string(),
                description: "Do it".to_string(),
                tools: vec![],
                expected_outcome: "done".to_string(),
                rollback_hint: String::new(),
                execution_status: StepStatus::Pending,
                tool_calls: vec![],
                tool_executions: vec![],
                results: vec![],
                errors: vec![],
                started_at: None,
                completed_at: None,
            }],
            status: PlanStatus::Draft,
            current_step_index: None,
            execution_started_at: None,
            execution_completed_at: None,
            execution_error: None,
            task_profile: None,

            milestone_id: None,
            files_to_modify: vec![],
            risks: vec![],
            created_at: Utc::now(),
        };
        let mut mgr = PlanManager::new(plan);
        mgr.start_execution();
        assert_eq!(mgr.current_step_index(), Some(0));

        mgr.complete_current_step(vec!["done".to_string()]);
        let next = mgr.advance_to_next_step();
        assert!(next.is_none());
        assert!(mgr.is_complete());
    }
}
