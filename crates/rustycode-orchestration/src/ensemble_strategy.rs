use crate::bus::BusHandle;
use crate::error::Result;
use crate::execution_trace::ExecutionTrace;
use crate::shared_workspace::SharedWorkspace;
use crate::types::Step;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StrategyKind {
    DecomposeAndDelegate,
    ParallelVote,
    SequentialReview,
    Adversarial,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyOutcome {
    pub steps: Vec<Step>,
    pub confidence: f64,
    pub strategy_used: StrategyKind,
    pub participants: Vec<String>,
    pub notes: String,
}

impl StrategyOutcome {
    pub fn is_confident(&self) -> bool {
        self.confidence >= 0.6
    }
}

#[async_trait]
pub trait ReasoningStrategy: Send + Sync {
    fn kind(&self) -> StrategyKind;

    async fn execute(
        &self,
        task: &str,
        trace: &ExecutionTrace,
        workspace: Arc<SharedWorkspace>,
        bus: BusHandle,
    ) -> Result<StrategyOutcome>;
}

pub struct EnsembleStrategy {
    kind: StrategyKind,
    participants: Vec<ParticipantSpec>,
}

#[derive(Debug, Clone)]
pub struct ParticipantSpec {
    pub role: String,
    pub weight: f64,
    pub can_veto: bool,
}

impl EnsembleStrategy {
    pub fn decompose_and_delegate() -> Self {
        Self {
            kind: StrategyKind::DecomposeAndDelegate,
            participants: vec![
                ParticipantSpec {
                    role: "decomposer".into(),
                    weight: 1.0,
                    can_veto: false,
                },
                ParticipantSpec {
                    role: "skeptic".into(),
                    weight: 0.5,
                    can_veto: true,
                },
                ParticipantSpec {
                    role: "integrator".into(),
                    weight: 1.0,
                    can_veto: false,
                },
            ],
        }
    }

    pub fn parallel_vote() -> Self {
        Self {
            kind: StrategyKind::ParallelVote,
            participants: vec![
                ParticipantSpec {
                    role: "worker-a".into(),
                    weight: 1.0,
                    can_veto: false,
                },
                ParticipantSpec {
                    role: "worker-b".into(),
                    weight: 1.0,
                    can_veto: false,
                },
                ParticipantSpec {
                    role: "skeptic".into(),
                    weight: 0.8,
                    can_veto: true,
                },
                ParticipantSpec {
                    role: "judge".into(),
                    weight: 1.5,
                    can_veto: false,
                },
            ],
        }
    }

    pub fn sequential_review() -> Self {
        Self {
            kind: StrategyKind::SequentialReview,
            participants: vec![
                ParticipantSpec {
                    role: "planner".into(),
                    weight: 1.0,
                    can_veto: false,
                },
                ParticipantSpec {
                    role: "implementer".into(),
                    weight: 1.0,
                    can_veto: false,
                },
                ParticipantSpec {
                    role: "reviewer".into(),
                    weight: 1.0,
                    can_veto: true,
                },
            ],
        }
    }

    pub fn adversarial() -> Self {
        Self {
            kind: StrategyKind::Adversarial,
            participants: vec![
                ParticipantSpec {
                    role: "proposer".into(),
                    weight: 1.0,
                    can_veto: false,
                },
                ParticipantSpec {
                    role: "skeptic".into(),
                    weight: 1.0,
                    can_veto: true,
                },
            ],
        }
    }

    pub const fn with_participants(kind: StrategyKind, participants: Vec<ParticipantSpec>) -> Self {
        Self { kind, participants }
    }

    pub fn select_for_complexity(complexity_score: u8) -> Self {
        match complexity_score {
            0..=30 => Self::sequential_review(),
            31..=60 => Self::parallel_vote(),
            61..=80 => Self::decompose_and_delegate(),
            _ => Self::adversarial(),
        }
    }

    pub const fn kind(&self) -> StrategyKind {
        self.kind
    }

    pub fn participants(&self) -> &[ParticipantSpec] {
        &self.participants
    }

    async fn run_decompose_and_delegate(
        &self,
        task: &str,
        trace: &ExecutionTrace,
        workspace: &SharedWorkspace,
        bus: &BusHandle,
    ) -> Result<StrategyOutcome> {
        let task_id = trace.task_id.clone();

        let sub_tasks = decompose_task(task, trace);
        let mut all_steps = Vec::new();
        let mut total_confidence = 0.0f64;
        let mut participant_names = Vec::new();

        for (i, sub) in sub_tasks.iter().enumerate() {
            let step = Step {
                id: format!("ensemble-{task_id}-{i}"),
                index: i as u8,
                description: sub.clone(),
                expected_output_type: crate::types::OutputType::Verification,
                suggested_tool: Some("bash".into()),
                retry_on_failure: true,
                required_resources: crate::guard::RequiredResources::default(),
            };

            workspace
                .write(
                    format!("ensemble.subtask.{i}").into(),
                    serde_json::json!({
                        "task": sub,
                        "step_id": step.id,
                    }),
                    "decomposer".into(),
                    Some(step.id.clone()),
                )
                .await;

            bus.publish(crate::bus::OrchestrationEvent::PartialResult {
                step_id: step.id.clone(),
                content: format!("delegated: {sub}"),
            });

            all_steps.push(step);
            total_confidence += 1.0;
        }

        #[allow(clippy::cast_precision_loss)]
        let sub_task_count = sub_tasks.len().max(1) as f64;
        total_confidence /= sub_task_count;

        for spec in &self.participants {
            participant_names.push(spec.role.clone());
        }

        Ok(StrategyOutcome {
            steps: all_steps,
            confidence: total_confidence,
            strategy_used: self.kind,
            participants: participant_names,
            notes: format!(
                "decomposed into {} sub-tasks from {} failures",
                sub_tasks.len(),
                trace.failures().len()
            ),
        })
    }

    async fn run_parallel_vote(
        &self,
        task: &str,
        trace: &ExecutionTrace,
        workspace: &SharedWorkspace,
        bus: &BusHandle,
    ) -> Result<StrategyOutcome> {
        let task_id = trace.task_id.clone();

        let step = Step {
            id: format!("ensemble-{task_id}-0"),
            index: 0,
            description: task.into(),
            expected_output_type: crate::types::OutputType::Verification,
            suggested_tool: Some("bash".into()),
            retry_on_failure: true,
            required_resources: crate::guard::RequiredResources::default(),
        };

        let mut participant_names = Vec::new();
        let total_weight: f64 = self.participants.iter().map(|p| p.weight).sum();
        let weighted_confidence = if total_weight > 0.0 { 0.85 } else { 0.0 };

        for spec in &self.participants {
            participant_names.push(format!("{}(w={:.1})", spec.role, spec.weight));

            workspace
                .write(
                    format!("ensemble.vote.{}", spec.role).into(),
                    serde_json::json!({
                        "participant": spec.role,
                        "weight": spec.weight,
                        "can_veto": spec.can_veto,
                        "task": task,
                    }),
                    spec.role.clone(),
                    Some(step.id.clone()),
                )
                .await;

            bus.publish(crate::bus::OrchestrationEvent::PartialResult {
                step_id: step.id.clone(),
                content: format!("{}: vote cast", spec.role),
            });
        }

        let past_failures = trace.failures().len();
        #[allow(clippy::cast_precision_loss)]
        let failure_penalty = past_failures as f64 * 0.05;
        let confidence = weighted_confidence * (1.0 - failure_penalty).max(0.3);

        Ok(StrategyOutcome {
            steps: vec![step],
            confidence,
            strategy_used: self.kind,
            participants: participant_names,
            notes: format!(
                "parallel vote with {} participants, {} past failures",
                self.participants.len(),
                past_failures
            ),
        })
    }

    async fn run_sequential_review(
        &self,
        task: &str,
        trace: &ExecutionTrace,
        workspace: &SharedWorkspace,
        bus: &BusHandle,
    ) -> Result<StrategyOutcome> {
        let task_id = trace.task_id.clone();

        let plan_step = Step {
            id: format!("ensemble-{task_id}-plan"),
            index: 0,
            description: format!("Plan: {task}"),
            expected_output_type: crate::types::OutputType::Verification,
            suggested_tool: Some("noop".into()),
            retry_on_failure: false,
            required_resources: crate::guard::RequiredResources::default(),
        };

        let impl_step = Step {
            id: format!("ensemble-{task_id}-impl"),
            index: 1,
            description: format!("Implement: {task}"),
            expected_output_type: crate::types::OutputType::Verification,
            suggested_tool: Some("bash".into()),
            retry_on_failure: true,
            required_resources: crate::guard::RequiredResources::default(),
        };

        workspace
            .write(
                "ensemble.plan".into(),
                serde_json::json!({ "task": task, "planned_by": "planner" }),
                "planner".into(),
                Some(plan_step.id.clone()),
            )
            .await;

        bus.publish(crate::bus::OrchestrationEvent::PartialResult {
            step_id: plan_step.id.clone(),
            content: format!("planned: {task}"),
        });

        let confidence = if trace.failures().is_empty() {
            0.9
        } else {
            0.7
        };

        let participant_names: Vec<String> =
            self.participants.iter().map(|p| p.role.clone()).collect();

        Ok(StrategyOutcome {
            steps: vec![plan_step, impl_step],
            confidence,
            strategy_used: self.kind,
            participants: participant_names,
            notes: "sequential plan→implement→review pipeline".to_string(),
        })
    }

    async fn run_adversarial(
        &self,
        task: &str,
        trace: &ExecutionTrace,
        workspace: &SharedWorkspace,
        bus: &BusHandle,
    ) -> Result<StrategyOutcome> {
        let task_id = trace.task_id.clone();

        let proposal = Step {
            id: format!("ensemble-{task_id}-proposal"),
            index: 0,
            description: format!("Propose solution: {task}"),
            expected_output_type: crate::types::OutputType::Verification,
            suggested_tool: Some("bash".into()),
            retry_on_failure: true,
            required_resources: crate::guard::RequiredResources::default(),
        };

        workspace
            .write(
                "ensemble.proposal".into(),
                serde_json::json!({
                    "task": task,
                    "proposed_by": "proposer",
                    "failure_count": trace.failures().len(),
                }),
                "proposer".into(),
                Some(proposal.id.clone()),
            )
            .await;

        bus.publish(crate::bus::OrchestrationEvent::PartialResult {
            step_id: proposal.id.clone(),
            content: format!("proposed: {task}"),
        });

        bus.publish(crate::bus::OrchestrationEvent::Objection {
            step_id: proposal.id.clone(),
            reason: "adversarial review pending".into(),
        });

        let confidence = 0.75;

        Ok(StrategyOutcome {
            steps: vec![proposal],
            confidence,
            strategy_used: self.kind,
            participants: vec!["proposer".into(), "skeptic".into()],
            notes: "adversarial propose-challenge cycle".into(),
        })
    }
}

#[async_trait]
impl ReasoningStrategy for EnsembleStrategy {
    fn kind(&self) -> StrategyKind {
        self.kind
    }

    async fn execute(
        &self,
        task: &str,
        trace: &ExecutionTrace,
        workspace: Arc<SharedWorkspace>,
        bus: BusHandle,
    ) -> Result<StrategyOutcome> {
        match self.kind {
            StrategyKind::DecomposeAndDelegate => {
                self.run_decompose_and_delegate(task, trace, &workspace, &bus)
                    .await
            }
            StrategyKind::ParallelVote => {
                self.run_parallel_vote(task, trace, &workspace, &bus).await
            }
            StrategyKind::SequentialReview => {
                self.run_sequential_review(task, trace, &workspace, &bus)
                    .await
            }
            StrategyKind::Adversarial => self.run_adversarial(task, trace, &workspace, &bus).await,
        }
    }
}

fn decompose_task(task: &str, trace: &ExecutionTrace) -> Vec<String> {
    let failures = trace.failures();
    if failures.is_empty() {
        let sentences: Vec<&str> = task.split(['.', ';']).collect();
        let sub_tasks: Vec<String> = sentences
            .iter()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(ToString::to_string)
            .collect();
        if sub_tasks.len() > 1 {
            return sub_tasks;
        }
        return vec![task.to_string()];
    }

    let mut sub_tasks = Vec::new();
    for failure in failures {
        if let Some(signal) = &failure.error_signal {
            sub_tasks.push(format!(
                "Fix {} error in step {}: {}",
                signal.category, failure.step_id, signal.message
            ));
        }
    }

    if sub_tasks.is_empty() {
        sub_tasks.push(task.to_string());
    }

    sub_tasks
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn make_trace(task_id: &str) -> ExecutionTrace {
        ExecutionTrace::new(task_id.into())
    }

    fn make_trace_with_failure(task_id: &str) -> ExecutionTrace {
        let mut trace = ExecutionTrace::new(task_id.into());
        let error = crate::error_signal::ErrorSignal::new(
            crate::error_signal::SignalCategory::LogicError,
            Some(1),
            "assertion failed".into(),
            "step-1".into(),
            "bash".into(),
        );
        trace.append(crate::execution_trace::TraceEntry::new_failure(
            "step-1".into(),
            0,
            2,
            "bash".into(),
            serde_json::json!({}),
            "error".into(),
            Some(1),
            error,
            0.01,
        ));
        trace
    }

    #[tokio::test]
    async fn test_decompose_and_delegate_produces_steps() {
        let strategy = EnsembleStrategy::decompose_and_delegate();
        let trace = make_trace("t1");
        let workspace = Arc::new(SharedWorkspace::new());
        let bus = BusHandle::new(16);

        let outcome = strategy
            .execute("do A; then do B", &trace, workspace, bus)
            .await
            .unwrap();

        assert_eq!(outcome.strategy_used, StrategyKind::DecomposeAndDelegate);
        assert!(!outcome.steps.is_empty());
        assert!(outcome.confidence > 0.0);
        assert!(outcome.participants.contains(&"decomposer".to_string()));
    }

    #[tokio::test]
    async fn test_parallel_vote_produces_single_step() {
        let strategy = EnsembleStrategy::parallel_vote();
        let trace = make_trace("t2");
        let workspace = Arc::new(SharedWorkspace::new());
        let bus = BusHandle::new(16);

        let outcome = strategy
            .execute("fix the bug", &trace, workspace, bus)
            .await
            .unwrap();

        assert_eq!(outcome.strategy_used, StrategyKind::ParallelVote);
        assert_eq!(outcome.steps.len(), 1);
        assert!(outcome.confidence > 0.5);
    }

    #[tokio::test]
    async fn test_sequential_review_produces_plan_and_impl() {
        let strategy = EnsembleStrategy::sequential_review();
        let trace = make_trace("t3");
        let workspace = Arc::new(SharedWorkspace::new());
        let bus = BusHandle::new(16);

        let outcome = strategy
            .execute("implement feature X", &trace, workspace, bus)
            .await
            .unwrap();

        assert_eq!(outcome.strategy_used, StrategyKind::SequentialReview);
        assert_eq!(outcome.steps.len(), 2);
        assert!(outcome.steps[0].description.starts_with("Plan:"));
        assert!(outcome.steps[1].description.starts_with("Implement:"));
    }

    #[tokio::test]
    async fn test_adversarial_produces_proposal() {
        let strategy = EnsembleStrategy::adversarial();
        let trace = make_trace("t4");
        let workspace = Arc::new(SharedWorkspace::new());
        let bus = BusHandle::new(16);

        let outcome = strategy
            .execute("solve complex problem", &trace, workspace, bus)
            .await
            .unwrap();

        assert_eq!(outcome.strategy_used, StrategyKind::Adversarial);
        assert_eq!(outcome.steps.len(), 1);
        assert!(outcome.participants.contains(&"proposer".to_string()));
        assert!(outcome.participants.contains(&"skeptic".to_string()));
    }

    #[tokio::test]
    async fn test_decompose_with_failures_targets_fixes() {
        let strategy = EnsembleStrategy::decompose_and_delegate();
        let trace = make_trace_with_failure("t5");
        let workspace = Arc::new(SharedWorkspace::new());
        let bus = BusHandle::new(16);

        let outcome = strategy
            .execute("fix the test", &trace, workspace, bus)
            .await
            .unwrap();

        assert!(outcome.notes.contains("failure"));
    }

    #[test]
    fn test_select_for_complexity() {
        assert_eq!(
            EnsembleStrategy::select_for_complexity(20).kind(),
            StrategyKind::SequentialReview
        );
        assert_eq!(
            EnsembleStrategy::select_for_complexity(50).kind(),
            StrategyKind::ParallelVote
        );
        assert_eq!(
            EnsembleStrategy::select_for_complexity(70).kind(),
            StrategyKind::DecomposeAndDelegate
        );
        assert_eq!(
            EnsembleStrategy::select_for_complexity(95).kind(),
            StrategyKind::Adversarial
        );
    }

    #[test]
    fn test_strategy_outcome_is_confident() {
        let high = StrategyOutcome {
            steps: vec![],
            confidence: 0.8,
            strategy_used: StrategyKind::ParallelVote,
            participants: vec![],
            notes: String::new(),
        };
        assert!(high.is_confident());

        let low = StrategyOutcome {
            steps: vec![],
            confidence: 0.3,
            strategy_used: StrategyKind::Adversarial,
            participants: vec![],
            notes: String::new(),
        };
        assert!(!low.is_confident());
    }

    #[tokio::test]
    async fn test_ensemble_writes_to_workspace() {
        let strategy = EnsembleStrategy::sequential_review();
        let trace = make_trace("t6");
        let workspace = Arc::new(SharedWorkspace::new());
        let bus = BusHandle::new(16);

        strategy
            .execute("task", &trace, workspace.clone(), bus)
            .await
            .unwrap();

        assert!(workspace.contains("ensemble.plan").await);
    }

    #[tokio::test]
    async fn test_ensemble_publishes_partial_result() {
        let strategy = EnsembleStrategy::parallel_vote();
        let trace = make_trace("t7");
        let workspace = Arc::new(SharedWorkspace::new());
        let bus = BusHandle::new(16);

        let mut rx = bus.subscribe();

        strategy
            .execute("task", &trace, workspace, bus)
            .await
            .unwrap();

        let event = rx.try_recv().unwrap();
        assert!(matches!(
            event,
            crate::bus::OrchestrationEvent::PartialResult { .. }
        ));
    }

    #[tokio::test]
    async fn test_with_custom_participants() {
        let strategy = EnsembleStrategy::with_participants(
            StrategyKind::ParallelVote,
            vec![
                ParticipantSpec {
                    role: "alpha".into(),
                    weight: 2.0,
                    can_veto: false,
                },
                ParticipantSpec {
                    role: "beta".into(),
                    weight: 1.0,
                    can_veto: true,
                },
            ],
        );
        let trace = make_trace("t8");
        let workspace = Arc::new(SharedWorkspace::new());
        let bus = BusHandle::new(16);

        let outcome = strategy
            .execute("task", &trace, workspace, bus)
            .await
            .unwrap();

        assert_eq!(outcome.participants.len(), 2);
        assert!(outcome.participants[0].contains("alpha"));
    }

    #[test]
    fn test_strategy_kind_serialization_roundtrip() {
        for kind in [
            StrategyKind::DecomposeAndDelegate,
            StrategyKind::ParallelVote,
            StrategyKind::SequentialReview,
            StrategyKind::Adversarial,
        ] {
            let json = serde_json::to_string(&kind).unwrap();
            let back: StrategyKind = serde_json::from_str(&json).unwrap();
            assert_eq!(kind, back);
        }
    }

    #[test]
    fn test_participant_spec_debug() {
        let spec = ParticipantSpec {
            role: "worker".into(),
            weight: 1.0,
            can_veto: false,
        };
        let debug = format!("{spec:?}");
        assert!(debug.contains("worker"));
    }

    #[test]
    fn test_strategy_outcome_confidence_boundary() {
        let exactly = StrategyOutcome {
            steps: vec![],
            confidence: 0.6,
            strategy_used: StrategyKind::Adversarial,
            participants: vec![],
            notes: String::new(),
        };
        assert!(exactly.is_confident());

        let below = StrategyOutcome {
            steps: vec![],
            confidence: 0.59,
            strategy_used: StrategyKind::Adversarial,
            participants: vec![],
            notes: String::new(),
        };
        assert!(!below.is_confident());
    }

    #[tokio::test]
    async fn test_parallel_vote_failure_penalty() {
        let strategy = EnsembleStrategy::parallel_vote();
        let trace = make_trace_with_failure("t-penalty");
        let workspace = Arc::new(SharedWorkspace::new());
        let bus = BusHandle::new(16);

        let outcome = strategy
            .execute("task", &trace, workspace, bus)
            .await
            .unwrap();

        // With failures, confidence should be penalized from the 0.85 base
        assert!(outcome.confidence < 0.85);
        assert!(outcome.notes.contains("past failures"));
    }

    #[tokio::test]
    async fn test_sequential_review_confidence_with_failures() {
        let strategy = EnsembleStrategy::sequential_review();
        let trace = make_trace_with_failure("t-sr");
        let workspace = Arc::new(SharedWorkspace::new());
        let bus = BusHandle::new(16);

        let outcome = strategy
            .execute("task", &trace, workspace, bus)
            .await
            .unwrap();

        // With failures, confidence should be 0.7 (not 0.9)
        assert!((outcome.confidence - 0.7).abs() < f64::EPSILON);
    }
}
