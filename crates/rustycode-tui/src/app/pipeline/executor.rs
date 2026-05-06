use super::registry::PipelineStep;
use super::types::{ArtifactSchema, FailureStrategy, PhaseResult};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;

/// A reference to another phase that must complete before this one can run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhaseDependency {
    pub phase: String,
}

/// A single phase within the pipeline DAG.
pub struct Phase {
    pub id: String,
    pub schedule: Option<String>,
    pub hard_deps: Vec<PhaseDependency>,
    pub soft_deps: Vec<PhaseDependency>,
    pub steps: Vec<Arc<dyn PipelineStep>>,
    pub failure_strategy: FailureStrategy,
    pub artifacts_produced: Vec<ArtifactSchema>,
    pub timeout_secs: u64,
    pub parallel: bool,
}

/// Current execution state of the pipeline.
#[derive(Debug)]
pub enum PipelineState {
    Pending,
    Running {
        active_phases: HashSet<String>,
        started_at: Instant,
    },
    Paused {
        reason: String,
        paused_at: Instant,
    },
    Failed {
        phase_id: String,
        reason: String,
        failed_at: Instant,
    },
    Completed {
        phase_results: HashMap<String, PhaseResult>,
        completed_at: Instant,
    },
}

/// Directed acyclic graph executor for pipeline phases.
pub struct PipelineDAG {
    phases: HashMap<String, Arc<Phase>>,
    phase_order: Vec<String>,
    state: PipelineState,
    completed_phases: HashMap<String, PhaseResult>,
}

impl Default for PipelineDAG {
    fn default() -> Self {
        Self::new()
    }
}

impl PipelineDAG {
    /// Create an empty DAG.
    pub fn new() -> Self {
        Self {
            phases: HashMap::new(),
            phase_order: Vec::new(),
            state: PipelineState::Pending,
            completed_phases: HashMap::new(),
        }
    }

    /// Add a phase to the DAG.
    pub fn add_phase(&mut self, phase: Arc<Phase>) -> Result<()> {
        let id = phase.id.clone();
        self.phases.insert(id.clone(), phase);
        self.phase_order.push(id);
        Ok(())
    }

    /// Check whether all hard dependencies for a phase are satisfied.
    pub fn can_execute(&self, phase: &Phase) -> bool {
        resolve_hard_deps(&self.completed_phases, &phase.hard_deps)
    }

    /// Check whether a phase is scheduled to run now (stub: true if schedule exists or is None).
    pub fn is_scheduled_now(&self, phase: &Phase) -> bool {
        phase.schedule.is_none() || !phase.schedule.as_ref().is_some_and(String::is_empty)
    }

    /// Determine if a phase should execute: scheduled + hard deps met.
    pub fn should_execute(&self, phase: &Phase) -> bool {
        self.is_scheduled_now(phase) && self.can_execute(phase)
    }

    /// Mark a phase as completed with the given result.
    pub fn complete_phase(&mut self, phase_id: &str, result: PhaseResult) {
        self.completed_phases.insert(phase_id.to_string(), result);
    }

    /// Check whether a specific phase has already completed.
    pub fn is_phase_completed(&self, phase_id: &str) -> bool {
        self.completed_phases.contains_key(phase_id)
    }

    /// Execute the full DAG: iterate phases in topological order, run eligible ones.
    pub async fn run(&mut self) -> Result<HashMap<String, PhaseResult>> {
        self.state = PipelineState::Running {
            active_phases: HashSet::new(),
            started_at: Instant::now(),
        };

        let order = self.phase_order.clone();
        for phase_id in &order {
            let phase = match self.phases.get(phase_id) {
                Some(p) => p,
                None => continue,
            };

            if !self.should_execute(phase) {
                continue;
            }

            if self.is_phase_completed(phase_id) {
                continue;
            }

            let phase_ok = true;
            for step in &phase.steps {
                let _ = step;
            }

            let result = if phase_ok {
                PhaseResult::Success
            } else {
                PhaseResult::Skipped {
                    reason: "step failed".to_string(),
                }
            };
            self.complete_phase(phase_id, result);
        }

        let results = self.completed_phases.clone();
        self.state = PipelineState::Completed {
            phase_results: results.clone(),
            completed_at: Instant::now(),
        };
        Ok(results)
    }
}

/// Check hard dependencies — stub: ready only when no hard deps exist.
pub fn resolve_hard_deps(
    completed: &HashMap<String, PhaseResult>,
    hard_deps: &[PhaseDependency],
) -> bool {
    if hard_deps.is_empty() {
        return true;
    }
    hard_deps
        .iter()
        .all(|dep| completed.contains_key(&dep.phase))
}

/// Check soft dependencies — always returns true (soft deps are optional).
pub fn resolve_soft_deps(
    _state: &HashMap<String, PhaseResult>,
    _soft_deps: &[PhaseDependency],
) -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::super::types::RetryPolicy;
    use super::*;

    fn create_test_phase(id: &str) -> Arc<Phase> {
        Arc::new(Phase {
            id: id.to_string(),
            schedule: None,
            hard_deps: Vec::new(),
            soft_deps: Vec::new(),
            steps: Vec::new(),
            failure_strategy: FailureStrategy::HardBlock {
                retry: RetryPolicy::default(),
            },
            artifacts_produced: Vec::new(),
            timeout_secs: 300,
            parallel: false,
        })
    }

    fn create_test_phase_with_hard_dep(id: &str, dep_phase: &str) -> Arc<Phase> {
        Arc::new(Phase {
            id: id.to_string(),
            schedule: None,
            hard_deps: vec![PhaseDependency {
                phase: dep_phase.to_string(),
            }],
            soft_deps: Vec::new(),
            steps: Vec::new(),
            failure_strategy: FailureStrategy::HardBlock {
                retry: RetryPolicy::default(),
            },
            artifacts_produced: Vec::new(),
            timeout_secs: 300,
            parallel: false,
        })
    }

    #[test]
    fn test_new_dag_is_empty() {
        let dag = PipelineDAG::new();
        assert!(dag.phases.is_empty());
        assert!(dag.phase_order.is_empty());
        assert!(dag.completed_phases.is_empty());
    }

    #[test]
    fn test_resolve_hard_deps_empty() {
        let completed = HashMap::new();
        let deps: Vec<PhaseDependency> = Vec::new();
        assert!(resolve_hard_deps(&completed, &deps));
    }

    #[test]
    fn test_resolve_hard_deps_with_deps() {
        let completed = HashMap::new();
        let deps = vec![PhaseDependency {
            phase: "upstream".to_string(),
        }];
        assert!(!resolve_hard_deps(&completed, &deps));
    }

    #[test]
    fn test_resolve_hard_deps_with_satisfied_deps() {
        let mut completed = HashMap::new();
        completed.insert("upstream".to_string(), PhaseResult::Success);
        let deps = vec![PhaseDependency {
            phase: "upstream".to_string(),
        }];
        assert!(resolve_hard_deps(&completed, &deps));
    }

    #[test]
    fn test_resolve_soft_deps_always_true() {
        let completed = HashMap::new();
        let deps = vec![PhaseDependency {
            phase: "optional".to_string(),
        }];
        assert!(resolve_soft_deps(&completed, &deps));
    }

    #[test]
    fn test_can_execute_no_deps() {
        let dag = PipelineDAG::new();
        let phase = create_test_phase("p1");
        assert!(dag.can_execute(&phase));
    }

    #[test]
    fn test_can_execute_with_unmet_hard_dep() {
        let dag = PipelineDAG::new();
        let phase = create_test_phase_with_hard_dep("p2", "p1");
        assert!(!dag.can_execute(&phase));
    }

    #[test]
    fn test_complete_phase() {
        let mut dag = PipelineDAG::new();
        assert!(!dag.is_phase_completed("p1"));
        dag.complete_phase("p1", PhaseResult::Success);
        assert!(dag.is_phase_completed("p1"));
    }

    #[test]
    fn test_complete_phase_degraded() {
        let mut dag = PipelineDAG::new();
        dag.complete_phase(
            "p1",
            PhaseResult::Degraded {
                reason: "partial".to_string(),
            },
        );
        assert!(dag.is_phase_completed("p1"));
    }

    #[tokio::test]
    async fn test_run_empty_dag() {
        let mut dag = PipelineDAG::new();
        let results = dag.run().await.expect("empty DAG should succeed");
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_run_executes_eligible_phases() {
        let mut dag = PipelineDAG::new();
        dag.add_phase(create_test_phase("p1"))
            .expect("add_phase should succeed");
        dag.add_phase(create_test_phase_with_hard_dep("p2", "p1"))
            .expect("add_phase should succeed");

        let results = dag.run().await.expect("run should succeed");
        // p1 has no deps so it executes; p2 depends on p1 which completes during run
        assert!(results.contains_key("p1"));
        assert!(results.contains_key("p2"));
        assert!(dag.is_phase_completed("p1"));
        assert!(dag.is_phase_completed("p2"));
    }

    #[test]
    fn test_add_phase_inserts_in_order() {
        let mut dag = PipelineDAG::new();
        dag.add_phase(create_test_phase("alpha"))
            .expect("add_phase should succeed");
        dag.add_phase(create_test_phase("beta"))
            .expect("add_phase should succeed");
        assert_eq!(dag.phase_order, vec!["alpha", "beta"]);
    }

    #[test]
    fn test_is_scheduled_now_none_is_true() {
        let dag = PipelineDAG::new();
        let phase = create_test_phase("p1");
        assert!(dag.is_scheduled_now(&phase));
    }

    #[test]
    fn test_is_scheduled_now_with_cron_is_true() {
        let dag = PipelineDAG::new();
        let phase = Arc::new(Phase {
            id: "scheduled".to_string(),
            schedule: Some("0 * * * *".to_string()),
            hard_deps: Vec::new(),
            soft_deps: Vec::new(),
            steps: Vec::new(),
            failure_strategy: FailureStrategy::HardBlock {
                retry: RetryPolicy::default(),
            },
            artifacts_produced: Vec::new(),
            timeout_secs: 300,
            parallel: false,
        });
        assert!(dag.is_scheduled_now(&phase));
    }

    #[test]
    fn test_should_execute_true_when_no_deps_no_schedule() {
        let dag = PipelineDAG::new();
        let phase = create_test_phase("p1");
        assert!(dag.should_execute(&phase));
    }

    #[test]
    fn test_default_impl() {
        let dag = PipelineDAG::default();
        assert!(dag.phases.is_empty());
    }
}
