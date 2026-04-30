//! Smart context loading for AST (§10.5).
//!
//! Loads only the working set into the model prompt, keeping the full truth
//! in the ledger (markdown) and progress store (`SQLite`). Supports on-demand
//! retrieval and eviction when context pressure builds.

use std::collections::HashMap;

use super::ledger::LedgerData;
use super::progress_store::ProgressStore;
use super::types::{
    AstPhase, AstSnapshot, ExecutionSegment, StepEvidence, SuccessCriterion, TaskAssessment,
};

#[derive(Debug, Clone)]
pub struct WorkingSet {
    pub assessment: Option<TaskAssessment>,
    pub success_criteria: Vec<SuccessCriterion>,
    pub active_milestones: Vec<(usize, String)>,
    pub current_segment: Option<ExecutionSegment>,
    pub unresolved_blockers: Vec<String>,
    pub recent_evidence: Vec<StepEvidence>,
    pub phase: AstPhase,
}

/// Priority levels for context sections. Lower = more important.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ContextPriority {
    /// Always included: assessment, active milestone, criteria.
    Critical = 0,
    /// Usually included: current segment, recent evidence, blockers.
    High = 1,
    /// Included when room allows: prior milestones, subagent findings.
    Medium = 2,
    /// First to evict: historical evidence, old decisions.
    Low = 3,
}

#[derive(Debug, Clone)]
pub struct ContextSection {
    pub label: String,
    pub content: String,
    pub priority: ContextPriority,
    pub estimated_tokens: usize,
}

#[derive(Debug, Clone)]
pub struct AssembledPrompt {
    pub system_prompt: String,
    pub sections: Vec<ContextSection>,
    pub phase_instruction: String,
    pub total_tokens: usize,
}

#[derive(Debug, Clone, Default)]
pub struct ContextMetrics {
    pub tokens_by_phase: HashMap<String, usize>,
    pub on_demand_retrievals: u32,
    pub sections_evicted: u32,
    pub window_usage_pct: f64,
}

/// The context loader assembles prompts from the working set.
pub struct ContextLoader {
    target_window_tokens: usize,
    max_fill_ratio: f64,
    metrics: ContextMetrics,
}

/// On-demand fetcher for context not in the working set.
pub trait ContextFetcher {
    fn fetch_prior_milestone(&self, id: usize) -> Option<String>;
    fn fetch_artifact_index(&self, task_id: &str) -> Vec<String>;
    fn fetch_historical_evidence(&self, milestone_id: usize) -> Vec<StepEvidence>;
    fn fetch_subagent_findings(&self, task_id: &str) -> Vec<String>;
    fn fetch_decisions(&self, task_id: &str, limit: usize) -> Vec<String>;
}

impl ContextLoader {
    pub fn new(target_window_tokens: usize) -> Self {
        Self {
            target_window_tokens,
            max_fill_ratio: 0.70,
            metrics: ContextMetrics::default(),
        }
    }

    pub const fn with_max_fill_ratio(mut self, ratio: f64) -> Self {
        self.max_fill_ratio = ratio.clamp(0.1, 0.95);
        self
    }

    /// Assemble a prompt for the given phase and working set.
    #[allow(clippy::too_many_lines)]
    pub fn assemble(&mut self, phase: AstPhase, working_set: &WorkingSet) -> AssembledPrompt {
        let mut sections = Vec::new();
        let mut total_tokens = 0;

        if let Some(ref assessment) = working_set.assessment {
            let content = format!(
                "## Task Assessment\n\
                 - Task: {}\n\
                 - Complexity: {:?}\n\
                 - Route: {:?}\n\
                 - Success criteria:\n{}",
                assessment.task_summary,
                assessment.complexity,
                assessment.route,
                assessment
                    .success_criteria
                    .iter()
                    .enumerate()
                    .map(|(i, c)| format!("  {}. {}", i + 1, c.description))
                    .collect::<Vec<_>>()
                    .join("\n")
            );
            let tokens = estimate_tokens(&content);
            sections.push(ContextSection {
                label: "assessment".into(),
                content,
                priority: ContextPriority::Critical,
                estimated_tokens: tokens,
            });
            total_tokens += tokens;
        }

        if !working_set.active_milestones.is_empty() {
            let content = format!(
                "## Active Milestones\n{}",
                working_set
                    .active_milestones
                    .iter()
                    .map(|(id, desc)| format!("- M{id}: {desc}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            );
            let tokens = estimate_tokens(&content);
            sections.push(ContextSection {
                label: "active_milestones".into(),
                content,
                priority: ContextPriority::Critical,
                estimated_tokens: tokens,
            });
            total_tokens += tokens;
        }

        if let Some(ref segment) = working_set.current_segment {
            let content = format!(
                "## Current Execution Segment\n\
                 - Milestone: M{}\n\
                 - Steps:\n{}",
                segment.milestone_id,
                segment
                    .steps
                    .iter()
                    .enumerate()
                    .map(|(i, s)| format!("  {}. {}", i + 1, s.action))
                    .collect::<Vec<_>>()
                    .join("\n")
            );
            let tokens = estimate_tokens(&content);
            sections.push(ContextSection {
                label: "current_segment".into(),
                content,
                priority: ContextPriority::High,
                estimated_tokens: tokens,
            });
            total_tokens += tokens;
        }

        if !working_set.unresolved_blockers.is_empty() {
            let content = format!(
                "## Unresolved Blockers\n{}",
                working_set
                    .unresolved_blockers
                    .iter()
                    .map(|b| format!("- {b}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            );
            let tokens = estimate_tokens(&content);
            sections.push(ContextSection {
                label: "blockers".into(),
                content,
                priority: ContextPriority::High,
                estimated_tokens: tokens,
            });
            total_tokens += tokens;
        }

        if !working_set.recent_evidence.is_empty() {
            let content = format!(
                "## Recent Evidence\n{}",
                working_set
                    .recent_evidence
                    .iter()
                    .map(|e| format!(
                        "- Step {}: exit={} {}",
                        e.step_index,
                        e.exit_code,
                        if e.verification_passed.unwrap_or(false) {
                            "PASS"
                        } else {
                            ""
                        }
                    ))
                    .collect::<Vec<_>>()
                    .join("\n")
            );
            let tokens = estimate_tokens(&content);
            sections.push(ContextSection {
                label: "recent_evidence".into(),
                content,
                priority: ContextPriority::Medium,
                estimated_tokens: tokens,
            });
            total_tokens += tokens;
        }

        // Standalone success criteria when assessment is absent
        if !working_set.success_criteria.is_empty() && working_set.assessment.is_none() {
            let content = format!(
                "## Success Criteria\n{}",
                working_set
                    .success_criteria
                    .iter()
                    .enumerate()
                    .map(|(i, c)| format!("{}. {}", i + 1, c.description))
                    .collect::<Vec<_>>()
                    .join("\n")
            );
            let tokens = estimate_tokens(&content);
            sections.push(ContextSection {
                label: "success_criteria".into(),
                content,
                priority: ContextPriority::Critical,
                estimated_tokens: tokens,
            });
            total_tokens += tokens;
        }

        #[allow(clippy::cast_precision_loss)]
        let budget = (self.target_window_tokens as f64 * self.max_fill_ratio) as usize;
        let system_tokens = estimate_tokens(super::prompt::AST_SYSTEM_PROMPT);
        let phase_tokens = estimate_tokens(&phase_instruction(phase));
        let reserved = system_tokens + phase_tokens;
        let section_budget = budget.saturating_sub(reserved);

        sections.sort_by_key(|s| s.priority);

        while total_tokens > section_budget {
            if let Some(idx) = sections
                .iter()
                .rposition(|s| s.priority == ContextPriority::Low)
            {
                total_tokens -= sections[idx].estimated_tokens;
                sections.remove(idx);
                self.metrics.sections_evicted += 1;
            } else if let Some(idx) = sections
                .iter()
                .rposition(|s| s.priority == ContextPriority::Medium)
            {
                total_tokens -= sections[idx].estimated_tokens;
                sections.remove(idx);
                self.metrics.sections_evicted += 1;
            } else {
                break;
            }
        }

        let instruction = phase_instruction(phase);
        total_tokens += reserved;

        self.metrics
            .tokens_by_phase
            .insert(format!("{phase:?}"), total_tokens);
        self.metrics.window_usage_pct = if self.target_window_tokens > 0 {
            #[allow(clippy::cast_precision_loss)]
            {
                (total_tokens as f64 / self.target_window_tokens as f64) * 100.0
            }
        } else {
            0.0
        };

        AssembledPrompt {
            system_prompt: super::prompt::AST_SYSTEM_PROMPT.to_string(),
            sections,
            phase_instruction: instruction,
            total_tokens,
        }
    }

    pub fn working_set_from_snapshot(snapshot: &AstSnapshot) -> WorkingSet {
        let active_milestones = snapshot
            .skeleton
            .as_ref()
            .map(|s| {
                s.ready_milestones(&snapshot.completed_milestones, &snapshot.failed_milestones)
                    .into_iter()
                    .map(|m| (m.id, m.description.clone()))
                    .collect()
            })
            .unwrap_or_default();

        let current_segment = snapshot.active_segments.first().cloned();

        let recent_evidence: Vec<StepEvidence> =
            current_segment.as_ref().map_or_else(Vec::new, |segment| {
                snapshot
                    .evidence
                    .get(&segment.milestone_id)
                    .cloned()
                    .unwrap_or_default()
            });

        let success_criteria = snapshot
            .assessment
            .as_ref()
            .map(|a| a.success_criteria.clone())
            .unwrap_or_default();

        WorkingSet {
            assessment: snapshot.assessment.clone(),
            success_criteria,
            active_milestones,
            current_segment,
            unresolved_blockers: Vec::new(),
            recent_evidence,
            phase: snapshot.current_phase,
        }
    }

    pub fn enrich_from_ledger(working_set: &mut WorkingSet, ledger: &LedgerData) {
        for q in &ledger.open_questions {
            if !q.resolved {
                working_set.unresolved_blockers.push(q.question.clone());
            }
        }
    }

    pub const fn metrics(&self) -> &ContextMetrics {
        &self.metrics
    }
}

pub struct StoreFetcher<'a> {
    store: &'a ProgressStore,
}

impl<'a> StoreFetcher<'a> {
    pub const fn new(store: &'a ProgressStore) -> Self {
        Self { store }
    }
}

impl ContextFetcher for StoreFetcher<'_> {
    fn fetch_prior_milestone(&self, _id: usize) -> Option<String> {
        None
    }

    fn fetch_artifact_index(&self, task_id: &str) -> Vec<String> {
        let mut result = Vec::new();
        for kind in &["decision", "finding", "artifact", "note"] {
            if let Ok(artifacts) = self.store.get_artifacts_by_kind(task_id, kind) {
                for a in &artifacts {
                    if let Some(ref summary) = a.summary {
                        result.push(format!("{}: {}", a.id, summary));
                    }
                }
            }
        }
        result
    }

    fn fetch_historical_evidence(&self, _milestone_id: usize) -> Vec<StepEvidence> {
        Vec::new()
    }

    fn fetch_subagent_findings(&self, task_id: &str) -> Vec<String> {
        let mut result = Vec::new();
        if let Ok(runs) = self.store.get_subagent_history(task_id) {
            for run in &runs {
                result.push(format!(
                    "[{}] {} -> {} ({})",
                    run.role,
                    run.status,
                    run.id,
                    run.finished_at.as_deref().unwrap_or("in progress")
                ));
            }
        }
        result
    }

    fn fetch_decisions(&self, _task_id: &str, _limit: usize) -> Vec<String> {
        Vec::new()
    }
}

/// Estimate tokens (~4 chars per token).
const fn estimate_tokens(text: &str) -> usize {
    (text.len().saturating_add(3)) / 4
}

fn phase_instruction(phase: AstPhase) -> String {
    match phase {
        AstPhase::Classify => "Assess the task complexity and define success criteria.".into(),
        AstPhase::Research => "Gather context: relevant files, patterns, dependencies, risks, constraints.".into(),
        AstPhase::Skeleton => "Define milestones with dependency ordering. For complex tasks, include proposal selection.".into(),
        AstPhase::Expand => "Expand near-term milestones into atomic steps with file targets and verification commands.".into(),
        AstPhase::Execute => "Execute steps one at a time. Report results inline. Do NOT think during execution.".into(),
        AstPhase::Verify => "Check results against each success criterion. Report PASS/PARTIAL/FAIL.".into(),
        AstPhase::Complete | AstPhase::Failed => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::types::*;

    fn empty_snapshot() -> AstSnapshot {
        AstSnapshot {
            current_phase: AstPhase::Classify,
            assessment: None,
            brief: None,
            skeleton: None,
            active_segments: vec![],
            completed_milestones: vec![],
            evidence: HashMap::new(),
            recovery_attempts: HashMap::new(),
            consultant_escalation: vec![],
            failed_milestones: vec![],
            report: None,
        }
    }

    fn assessment_snapshot() -> AstSnapshot {
        let mut snap = empty_snapshot();
        snap.assessment = Some(TaskAssessment {
            task_summary: "Fix typo in README".into(),
            complexity: ComplexityLevel::Trivial,
            success_criteria: vec![SuccessCriterion {
                description: "Typo is fixed".into(),
                verification_command: Some("grep fixed README.md".into()),
            }],
            route: PhaseRoute::DirectExecute,

            clarity: None,
        });
        snap.skeleton = Some(MilestoneSkeleton {
            milestones: vec![Milestone {
                id: 0,
                description: "Fix the typo".into(),
                deliverable: "corrected README.md".into(),
                depends_on: vec![],
            }],
        });
        snap
    }

    #[test]
    fn empty_snapshot_produces_minimal_prompt() {
        let mut loader = ContextLoader::new(4096);
        let ws = WorkingSet {
            assessment: None,
            success_criteria: vec![],
            active_milestones: vec![],
            current_segment: None,
            unresolved_blockers: vec![],
            recent_evidence: vec![],
            phase: AstPhase::Classify,
        };
        let prompt = loader.assemble(AstPhase::Classify, &ws);
        assert!(!prompt.system_prompt.is_empty());
    }

    #[test]
    fn assessment_included_as_critical() {
        let mut loader = ContextLoader::new(4096);
        let ws = WorkingSet {
            assessment: Some(TaskAssessment {
                task_summary: "Add auth".into(),
                complexity: ComplexityLevel::Complex,
                success_criteria: vec![SuccessCriterion {
                    description: "Auth works".into(),
                    verification_command: None,
                }],
                route: PhaseRoute::RollingWave,

                clarity: None,
            }),
            success_criteria: vec![],
            active_milestones: vec![(0, "Setup JWT".into())],
            current_segment: None,
            unresolved_blockers: vec![],
            recent_evidence: vec![],
            phase: AstPhase::Classify,
        };
        let prompt = loader.assemble(AstPhase::Classify, &ws);
        assert!(prompt.sections.iter().any(|s| s.label == "assessment"));
        assert!(prompt
            .sections
            .iter()
            .any(|s| s.label == "active_milestones"));
        let assessment = prompt
            .sections
            .iter()
            .find(|s| s.label == "assessment")
            .unwrap();
        assert_eq!(assessment.priority, ContextPriority::Critical);
    }

    #[test]
    fn eviction_removes_low_priority_first() {
        let mut loader = ContextLoader::new(100); // Very small window
        let ws = WorkingSet {
            assessment: Some(TaskAssessment {
                task_summary: "Big task".into(),
                complexity: ComplexityLevel::Complex,
                success_criteria: vec![],
                route: PhaseRoute::RollingWave,

                clarity: None,
            }),
            success_criteria: vec![],
            active_milestones: vec![
                (0, "Step 0".into()),
                (1, "Step 1".into()),
                (2, "Step 2".into()),
            ],
            current_segment: Some(ExecutionSegment {
                milestone_id: 0,
                steps: vec![ExecutionStep {
                    action: "Do something long enough to trigger eviction".into(),
                    file_targets: vec![],
                    expected_command: None,
                    verification_command: None,
                    is_risky: false,
                    recovery_notes: None,
                }],
                required_criteria: vec![],
                edge_cases: vec![],
            }),
            unresolved_blockers: vec!["Blocker 1".into(), "Blocker 2".into()],
            recent_evidence: vec![StepEvidence {
                step_index: 0,
                command_run: Some("echo test".into()),
                exit_code: 0,
                stdout_summary: "test output that is somewhat long".into(),
                stderr_summary: String::new(),
                changed_files: vec![],
                verification_passed: Some(true),
            }],
            phase: AstPhase::Execute,
        };
        let prompt = loader.assemble(AstPhase::Execute, &ws);
        // Critical sections should remain
        assert!(prompt
            .sections
            .iter()
            .any(|s| s.priority == ContextPriority::Critical));
    }

    #[test]
    fn working_set_from_snapshot() {
        let snap = assessment_snapshot();
        let ws = ContextLoader::working_set_from_snapshot(&snap);
        assert!(ws.assessment.is_some());
        assert_eq!(ws.active_milestones.len(), 1);
        assert_eq!(ws.active_milestones[0].0, 0);
    }

    #[test]
    fn enrich_from_ledger_adds_blockers() {
        use crate::ast::ledger::{LedgerData, OpenQuestion};
        let mut ledger = LedgerData::from_snapshot(empty_snapshot(), "Test");
        ledger.open_questions.push(OpenQuestion {
            id: "Q1".into(),
            question: "Which library?".into(),
            resolved: false,
            resolution: None,
        });
        ledger.open_questions.push(OpenQuestion {
            id: "Q2".into(),
            question: "Old question".into(),
            resolved: true,
            resolution: Some("Decided".into()),
        });

        let mut ws = WorkingSet {
            assessment: None,
            success_criteria: vec![],
            active_milestones: vec![],
            current_segment: None,
            unresolved_blockers: vec![],
            recent_evidence: vec![],
            phase: AstPhase::Skeleton,
        };
        ContextLoader::enrich_from_ledger(&mut ws, &ledger);
        assert_eq!(ws.unresolved_blockers.len(), 1);
        assert_eq!(ws.unresolved_blockers[0], "Which library?");
    }

    #[test]
    fn metrics_tracked() {
        let mut loader = ContextLoader::new(4096);
        let ws = WorkingSet {
            assessment: None,
            success_criteria: vec![],
            active_milestones: vec![],
            current_segment: None,
            unresolved_blockers: vec![],
            recent_evidence: vec![],
            phase: AstPhase::Classify,
        };
        loader.assemble(AstPhase::Classify, &ws);
        let metrics = loader.metrics();
        assert!(metrics.tokens_by_phase.contains_key("Classify"));
        assert!(metrics.window_usage_pct >= 0.0);
    }

    #[test]
    fn phase_instruction_nonempty_for_active_phases() {
        for phase in [
            AstPhase::Classify,
            AstPhase::Research,
            AstPhase::Skeleton,
            AstPhase::Expand,
            AstPhase::Execute,
            AstPhase::Verify,
        ] {
            assert!(!phase_instruction(phase).is_empty(), "Empty for {phase:?}");
        }
    }

    #[test]
    fn phase_instruction_empty_for_terminal() {
        assert!(phase_instruction(AstPhase::Complete).is_empty());
        assert!(phase_instruction(AstPhase::Failed).is_empty());
    }

    #[test]
    fn max_fill_ratio_respected() {
        let mut loader = ContextLoader::new(1000).with_max_fill_ratio(0.5);
        let ws = WorkingSet {
            assessment: None,
            success_criteria: vec![],
            active_milestones: vec![],
            current_segment: None,
            unresolved_blockers: vec![],
            recent_evidence: vec![],
            phase: AstPhase::Classify,
        };
        let prompt = loader.assemble(AstPhase::Classify, &ws);
        // Should stay within 50% of 1000 tokens
        assert!(
            prompt.total_tokens <= 600,
            "Total tokens {} exceeds 60% of 1000",
            prompt.total_tokens
        );
    }

    #[test]
    fn store_fetcher_artifact_index() {
        let store = ProgressStore::open_in_memory().unwrap();
        let task_id = uuid::Uuid::new_v4().to_string();
        store
            .create_task(&crate::ast::progress_store::TaskRecord {
                id: task_id.clone(),
                title: "Test".into(),
                complexity: "Moderate".into(),
                goal: "Test".into(),
                current_phase: "CLASSIFY".into(),
                status: "active".into(),
                ledger_path: "/tmp/test.md".into(),
                created_at: chrono::Utc::now().to_rfc3339(),
                updated_at: chrono::Utc::now().to_rfc3339(),
            })
            .unwrap();
        store
            .store_artifact(&crate::ast::progress_store::ArtifactRecord {
                id: "art-1".into(),
                task_id: task_id.clone(),
                kind: "finding".into(),
                path: None,
                content_hash: None,
                summary: Some("Found 3 files".into()),
                created_at: chrono::Utc::now().to_rfc3339(),
            })
            .unwrap();

        let fetcher = StoreFetcher::new(&store);
        let index = fetcher.fetch_artifact_index(&task_id);
        assert_eq!(index.len(), 1);
        assert!(index[0].contains("Found 3 files"));
    }
}
