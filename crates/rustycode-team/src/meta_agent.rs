//! Meta-Agent for Self-Improvement.
//!
//! Analyzes execution traces to automatically improve the system by:
//! - Identifying recurring failure patterns
//! - Proposing workflow improvements
//! - Updating system prompts with validated learnings
//!
//! # Architecture
//!
//! ```text
//! ExecutionTrace → MetaAgent → ImprovementProposal → User Review → System Update
//!                      │
//!                      └─→ Analyzes: failures, successes, patterns
//! ```

use crate::execution_trace::{ExecutionTrace, TaskOutcome, TurnTrace};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{debug, info};

/// A proposed improvement to the system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImprovementProposal {
    /// Unique identifier for this proposal.
    pub id: String,
    /// What to improve (e.g., "Builder instructions", "Workflow selection").
    pub target: String,
    /// The proposed change.
    pub change: String,
    /// Evidence supporting this proposal (task descriptions).
    pub evidence: Vec<String>,
    /// Confidence score (0.0-1.0).
    pub confidence: f32,
    /// Whether this proposal has been applied.
    pub applied: bool,
}

/// Analysis of a single execution trace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceAnalysis {
    /// Task description.
    pub task: String,
    /// Outcome of the task.
    pub outcome: TaskOutcome,
    /// Root cause if failed.
    pub root_cause: Option<String>,
    /// Patterns observed.
    pub patterns: Vec<String>,
    /// Suggestions for improvement.
    pub suggestions: Vec<String>,
}

/// Meta-Agent that analyzes traces and proposes improvements.
pub struct MetaAgent {
    /// Accumulated traces for analysis.
    traces: Vec<ExecutionTrace>,
    /// Generated proposals.
    proposals: Vec<ImprovementProposal>,
    /// Minimum confidence threshold for proposals.
    min_confidence: f32,
    /// Minimum occurrences to confirm a pattern.
    min_occurrences: usize,
}

impl MetaAgent {
    pub fn new(min_confidence: f32, min_occurrences: usize) -> Self {
        Self {
            traces: Vec::new(),
            proposals: Vec::new(),
            min_confidence,
            min_occurrences,
        }
    }

    /// Add an execution trace for analysis.
    pub fn add_trace(&mut self, trace: ExecutionTrace) {
        debug!(
            "Meta-Agent adding trace: {} (outcome: {:?})",
            trace.task, trace.outcome
        );
        self.traces.push(trace);
    }

    /// Analyze all traces and generate improvement proposals.
    pub fn analyze(&mut self) -> Vec<ImprovementProposal> {
        info!("Meta-Agent analyzing {} traces", self.traces.len());

        // Analyze failures
        let failure_proposals = self.analyze_failures();

        // Analyze successes for positive patterns
        let success_proposals = self.analyze_successes();

        // Analyze workflow effectiveness
        let workflow_proposals = self.analyze_workflows();

        // Merge all proposals
        let mut all_proposals = Vec::new();
        all_proposals.extend(failure_proposals);
        all_proposals.extend(success_proposals);
        all_proposals.extend(workflow_proposals);

        // Deduplicate and merge similar proposals
        self.proposals = self.merge_proposals(all_proposals);

        // Filter by confidence
        self.proposals
            .iter()
            .filter(|p| p.confidence >= self.min_confidence)
            .cloned()
            .collect()
    }

    /// Analyze failure patterns.
    fn analyze_failures(&self) -> Vec<ImprovementProposal> {
        let failures: Vec<&ExecutionTrace> = self
            .traces
            .iter()
            .filter(|t| t.outcome == TaskOutcome::Failed)
            .collect();

        if failures.is_empty() {
            return Vec::new();
        }

        let mut proposals = Vec::new();

        // Group by root cause
        let mut by_root_cause: HashMap<String, Vec<&ExecutionTrace>> = HashMap::new();
        for trace in &failures {
            let root_cause = trace.root_cause.as_deref().unwrap_or("Unknown root cause");
            by_root_cause
                .entry(root_cause.to_string())
                .or_default()
                .push(trace);
        }

        // Generate proposals for common failure modes
        for (root_cause, traces) in by_root_cause {
            if traces.len() >= self.min_occurrences {
                let confidence = ((traces.len() as f32) / (failures.len() as f32)).min(1.0);

                proposals.push(ImprovementProposal {
                    id: format!("failure-{}", root_cause.to_lowercase().replace(' ', "-")),
                    target: "Workflow Selection".to_string(),
                    change: format!(
                        "When encountering {}, consider alternative approaches early",
                        root_cause
                    ),
                    evidence: traces.iter().map(|t| t.task.clone()).collect(),
                    confidence,
                    applied: false,
                });
            }
        }

        // Analyze turn-by-turn errors
        let error_patterns = self.extract_error_patterns(&failures);
        proposals.extend(error_patterns);

        proposals
    }

    /// Extract error patterns from failures.
    fn extract_error_patterns(&self, failures: &[&ExecutionTrace]) -> Vec<ImprovementProposal> {
        if failures.is_empty() {
            return Vec::new();
        }
        let mut proposals = Vec::new();

        // Count error types across all turns
        let mut error_counts: HashMap<String, Vec<&ExecutionTrace>> = HashMap::new();

        for trace in failures {
            for turn in &trace.turns {
                for error in &turn.errors {
                    let error_type = self.categorize_error(error);
                    error_counts.entry(error_type).or_default().push(trace);
                }
            }
        }

        // Generate proposals for common error types
        for (error_type, traces) in error_counts {
            if traces.len() >= self.min_occurrences {
                let confidence = ((traces.len() as f32) / (failures.len() as f32) * 0.8).min(0.95);

                proposals.push(ImprovementProposal {
                    id: format!("error-{}", error_type.to_lowercase().replace(' ', "-")),
                    target: "Builder Instructions".to_string(),
                    change: format!(
                        "Prevent {}: {}",
                        error_type,
                        self.get_prevention_advice(&error_type)
                    ),
                    evidence: traces.iter().map(|t| t.task.clone()).collect(),
                    confidence,
                    applied: false,
                });
            }
        }

        proposals
    }

    /// Categorize an error message.
    fn categorize_error(&self, error: &str) -> String {
        let error_lower = error.to_lowercase();

        if error_lower.contains("borrow") || error_lower.contains("ownership") {
            "Borrow checker issues".to_string()
        } else if error_lower.contains("type") || error_lower.contains("mismatched") {
            "Type mismatch".to_string()
        } else if error_lower.contains("trait") || error_lower.contains("implement") {
            "Trait implementation missing".to_string()
        } else if error_lower.contains("lifetime") {
            "Lifetime annotation required".to_string()
        } else if error_lower.contains("cannot find") || error_lower.contains("undeclared") {
            "Undefined symbol".to_string()
        } else if error_lower.contains("test") || error_lower.contains("assert") {
            "Test failure".to_string()
        } else if error_lower.contains("compilation") || error_lower.contains("syntax") {
            "Compilation error".to_string()
        } else {
            "Other error".to_string()
        }
    }

    /// Get prevention advice for an error type.
    fn get_prevention_advice(&self, error_type: &str) -> &str {
        match error_type {
            "Borrow checker issues" => "Clone values or restructure ownership before mutation",
            "Type mismatch" => "Add explicit type annotations for complex expressions",
            "Trait implementation missing" => "Check trait bounds and implement required traits",
            "Lifetime annotation required" => {
                "Add explicit lifetime parameters to function signatures"
            }
            "Undefined symbol" => "Verify imports and module visibility",
            "Test failure" => "Review test assumptions and edge cases",
            "Compilation error" => "Run cargo check frequently during implementation",
            _ => "Review error messages carefully and search for similar issues",
        }
    }

    /// Analyze success patterns.
    fn analyze_successes(&self) -> Vec<ImprovementProposal> {
        let successes: Vec<&ExecutionTrace> = self
            .traces
            .iter()
            .filter(|t| t.outcome == TaskOutcome::Success)
            .collect();

        if successes.is_empty() {
            return Vec::new();
        }

        let mut proposals = Vec::new();

        // Find common patterns in successful tasks
        let mut successful_approaches: HashMap<String, Vec<&ExecutionTrace>> = HashMap::new();

        for trace in &successes {
            // Extract approach from first turn
            if let Some(first_turn) = trace.turns.first() {
                let approach = self.extract_approach_category(first_turn);
                successful_approaches
                    .entry(approach)
                    .or_default()
                    .push(trace);
            }
        }

        // Generate proposals for successful patterns
        for (approach, traces) in successful_approaches {
            if traces.len() >= self.min_occurrences {
                let confidence = 0.5 + ((traces.len() as f32) / (successes.len() as f32) * 0.4);

                proposals.push(ImprovementProposal {
                    id: format!("success-{}", approach.to_lowercase().replace(' ', "-")),
                    target: "Workflow Recommendations".to_string(),
                    change: format!("For similar tasks, use approach: {}", approach),
                    evidence: traces.iter().map(|t| t.task.clone()).collect(),
                    confidence,
                    applied: false,
                });
            }
        }

        proposals
    }

    /// Extract approach category from a turn.
    fn extract_approach_category(&self, turn: &TurnTrace) -> String {
        let action_lower = turn.action.to_lowercase();

        if action_lower.contains("test") || action_lower.contains("spec") {
            "Test-first approach".to_string()
        } else if action_lower.contains("plan") || action_lower.contains("design") {
            "Plan-first approach".to_string()
        } else if action_lower.contains("refactor") || action_lower.contains("clean") {
            "Refactor-first approach".to_string()
        } else if action_lower.contains("fix") || action_lower.contains("patch") {
            "Direct fix approach".to_string()
        } else {
            "Standard approach".to_string()
        }
    }

    /// Analyze workflow effectiveness.
    fn analyze_workflows(&self) -> Vec<ImprovementProposal> {
        let mut proposals = Vec::new();

        // Group traces by workflow type (inferred from task)
        let mut workflow_stats: HashMap<String, (usize, usize)> = HashMap::new();

        for trace in &self.traces {
            let workflow = self.infer_workflow(&trace.task);
            let entry = workflow_stats.entry(workflow).or_insert((0, 0));
            if trace.outcome == TaskOutcome::Success {
                entry.0 += 1;
            }
            entry.1 += 1;
        }

        // Calculate success rates and generate proposals
        for (workflow, (successes, total)) in workflow_stats {
            if total >= self.min_occurrences {
                let success_rate = successes as f32 / total as f32;

                if success_rate < 0.5 {
                    // Low success rate - suggest workflow change
                    proposals.push(ImprovementProposal {
                        id: format!("workflow-improve-{}", workflow),
                        target: format!("{} Workflow", workflow),
                        change: format!("Current success rate {:.0}%. Consider adding verification steps or adjusting phase instructions.", success_rate * 100.0),
                        evidence: vec![format!("{} tasks with {} workflow", total, workflow)],
                        confidence: 1.0 - success_rate,
                        applied: false,
                    });
                } else if success_rate > 0.8 {
                    // High success rate - recommend this workflow
                    proposals.push(ImprovementProposal {
                        id: format!("workflow-recommend-{}", workflow),
                        target: "Workflow Selection".to_string(),
                        change: format!(
                            "{} workflow has {:.0}% success rate - prioritize for similar tasks",
                            workflow,
                            success_rate * 100.0
                        ),
                        evidence: vec![format!(
                            "{} successful tasks out of {} with {} workflow",
                            successes, total, workflow
                        )],
                        confidence: success_rate,
                        applied: false,
                    });
                }
            }
        }

        proposals
    }

    /// Infer workflow type from task description.
    fn infer_workflow(&self, task: &str) -> String {
        let task_lower = task.to_lowercase();

        if task_lower.contains("test") {
            "TDD".to_string()
        } else if task_lower.contains("plan") || task_lower.contains("design") {
            "Planning".to_string()
        } else if task_lower.contains("debug") || task_lower.contains("fix") {
            "Debugging".to_string()
        } else if task_lower.contains("security") || task_lower.contains("auth") {
            "Security".to_string()
        } else {
            "Standard".to_string()
        }
    }

    /// Merge similar proposals.
    fn merge_proposals(&self, proposals: Vec<ImprovementProposal>) -> Vec<ImprovementProposal> {
        let mut merged: HashMap<String, ImprovementProposal> = HashMap::new();

        for proposal in proposals {
            let key = format!("{}:{}", proposal.target, proposal.change);

            if let Some(existing) = merged.get_mut(&key) {
                // Merge evidence and increase confidence
                existing.evidence.extend(proposal.evidence);
                existing.confidence = (existing.confidence + proposal.confidence) / 2.0;
            } else {
                merged.insert(key, proposal);
            }
        }

        merged.into_values().collect()
    }

    /// Get all proposals (including low-confidence ones for inspection).
    pub fn all_proposals(&self) -> &[ImprovementProposal] {
        &self.proposals
    }

    /// Get high-confidence proposals.
    pub fn confirmed_proposals(&self) -> Vec<&ImprovementProposal> {
        self.proposals
            .iter()
            .filter(|p| p.confidence >= self.min_confidence && !p.applied)
            .collect()
    }

    /// Mark a proposal as applied.
    pub fn mark_applied(&mut self, proposal_id: &str) -> bool {
        if let Some(proposal) = self.proposals.iter_mut().find(|p| p.id == proposal_id) {
            proposal.applied = true;
            true
        } else {
            false
        }
    }

    /// Get statistics about the Meta-Agent.
    pub fn stats(&self) -> MetaAgentStats {
        let failures = self
            .traces
            .iter()
            .filter(|t| t.outcome == TaskOutcome::Failed)
            .count();
        let successes = self
            .traces
            .iter()
            .filter(|t| t.outcome == TaskOutcome::Success)
            .count();

        MetaAgentStats {
            total_traces: self.traces.len(),
            total_failures: failures,
            total_successes: successes,
            success_rate: if self.traces.is_empty() {
                0.0
            } else {
                successes as f32 / self.traces.len() as f32
            },
            total_proposals: self.proposals.len(),
            confirmed_proposals: self.confirmed_proposals().len(),
        }
    }

    /// Clear old traces (keep only recent ones for memory efficiency).
    pub fn retain_recent(&mut self, max_traces: usize) {
        if self.traces.len() > max_traces {
            let keep = self.traces.len() - max_traces;
            self.traces.drain(0..keep);
            debug!("Meta-Agent retained {} recent traces", max_traces);
        }
    }
}

/// Statistics about the Meta-Agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetaAgentStats {
    pub total_traces: usize,
    pub total_failures: usize,
    pub total_successes: usize,
    pub success_rate: f32,
    pub total_proposals: usize,
    pub confirmed_proposals: usize,
}

/// Analyze a single trace and return insights.
pub fn analyze_trace(trace: &ExecutionTrace) -> TraceAnalysis {
    let mut patterns = Vec::new();
    let mut suggestions = Vec::new();

    // Analyze turns for patterns
    for turn in &trace.turns {
        if !turn.files_changed.is_empty() {
            patterns.push(format!(
                "Modified {} files in turn {}",
                turn.files_changed.len(),
                turn.turn_number
            ));
        }
        if !turn.errors.is_empty() {
            suggestions.push(format!(
                "Turn {} had errors: consider more careful planning",
                turn.turn_number
            ));
        }
    }

    // Analyze outcome
    let root_cause = match trace.outcome {
        TaskOutcome::Failed => {
            suggestions.push(
                "Task failed - review approach and consider alternative strategies".to_string(),
            );
            trace.root_cause.clone()
        }
        TaskOutcome::Success => {
            patterns.push("Task completed successfully".to_string());
            None
        }
        TaskOutcome::Cancelled => {
            suggestions.push("Task was cancelled - consider why user interrupted".to_string());
            Some("Cancelled by user".to_string())
        }
    };

    TraceAnalysis {
        task: trace.task.clone(),
        outcome: trace.outcome,
        root_cause,
        patterns,
        suggestions,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution_trace::ExecutionTraceBuilder;

    fn create_test_trace(task: &str, outcome: TaskOutcome, turns: u32) -> ExecutionTrace {
        ExecutionTraceBuilder::new(task, outcome)
            .total_turns(turns)
            .build()
    }

    #[test]
    fn test_meta_agent_creation() {
        let agent = MetaAgent::new(0.5, 2);
        let stats = agent.stats();
        assert_eq!(stats.total_traces, 0);
        assert_eq!(stats.total_proposals, 0);
    }

    #[test]
    fn test_meta_agent_analyze_failures() {
        let mut agent = MetaAgent::new(0.3, 1);

        // Add multiple failures with same root cause
        for _ in 0..3 {
            agent.add_trace(create_test_trace("Fix auth bug", TaskOutcome::Failed, 5));
        }

        let proposals = agent.analyze();
        assert!(
            !proposals.is_empty(),
            "Should generate proposals from failures"
        );
    }

    #[test]
    fn test_meta_agent_analyze_successes() {
        let mut agent = MetaAgent::new(0.3, 1);

        // Add multiple successes
        for _ in 0..3 {
            agent.add_trace(create_test_trace(
                "Implement feature",
                TaskOutcome::Success,
                3,
            ));
        }

        let proposals = agent.analyze();
        // Should have success-based proposals
        let success_proposals: Vec<_> = proposals
            .iter()
            .filter(|p| p.target.contains("Workflow") || p.target.contains("Recommendations"))
            .collect();
        assert!(
            !success_proposals.is_empty(),
            "Should generate proposals from successes"
        );
    }

    #[test]
    fn test_meta_agent_stats() {
        let mut agent = MetaAgent::new(0.5, 2);

        agent.add_trace(create_test_trace("Success 1", TaskOutcome::Success, 3));
        agent.add_trace(create_test_trace("Success 2", TaskOutcome::Success, 2));
        agent.add_trace(create_test_trace("Failure 1", TaskOutcome::Failed, 5));

        let stats = agent.stats();
        assert_eq!(stats.total_traces, 3);
        assert_eq!(stats.total_successes, 2);
        assert_eq!(stats.total_failures, 1);
        assert!((stats.success_rate - 0.667).abs() < 0.01);
    }

    #[test]
    fn test_meta_agent_retain_recent() {
        let mut agent = MetaAgent::new(0.5, 2);

        for i in 0..10 {
            agent.add_trace(create_test_trace(
                &format!("Task {}", i),
                TaskOutcome::Success,
                1,
            ));
        }

        assert_eq!(agent.stats().total_traces, 10);

        agent.retain_recent(5);

        assert_eq!(agent.stats().total_traces, 5);
    }

    #[test]
    fn test_error_categorization() {
        let agent = MetaAgent::new(0.5, 2);

        assert_eq!(
            agent.categorize_error("cannot borrow as mutable"),
            "Borrow checker issues"
        );
        assert_eq!(agent.categorize_error("mismatched types"), "Type mismatch");
        assert_eq!(agent.categorize_error("test failed"), "Test failure");
        assert_eq!(agent.categorize_error("some random error"), "Other error");
    }

    #[test]
    fn test_proposal_merge() {
        let agent = MetaAgent::new(0.5, 2);

        let proposals = vec![
            ImprovementProposal {
                id: "prop1".to_string(),
                target: "Workflow".to_string(),
                change: "Add verification step".to_string(),
                evidence: vec!["task1".to_string()],
                confidence: 0.7,
                applied: false,
            },
            ImprovementProposal {
                id: "prop2".to_string(),
                target: "Workflow".to_string(),
                change: "Add verification step".to_string(),
                evidence: vec!["task2".to_string()],
                confidence: 0.8,
                applied: false,
            },
        ];

        let merged = agent.merge_proposals(proposals);
        assert_eq!(merged.len(), 1, "Similar proposals should be merged");
        assert_eq!(merged[0].evidence.len(), 2, "Evidence should be combined");
    }

    #[test]
    fn test_trace_analysis() {
        let trace = create_test_trace("Test task", TaskOutcome::Failed, 3);
        let analysis = analyze_trace(&trace);

        assert_eq!(analysis.task, "Test task");
        assert_eq!(analysis.outcome, TaskOutcome::Failed);
        assert!(!analysis.suggestions.is_empty());
    }

    #[test]
    fn test_analyze_empty_traces() {
        let mut agent = MetaAgent::new(0.5, 2);
        let proposals = agent.analyze();
        assert!(proposals.is_empty());
    }

    #[test]
    fn test_analyze_no_failures() {
        let mut agent = MetaAgent::new(0.3, 1);
        agent.add_trace(create_test_trace("success task", TaskOutcome::Success, 2));
        let proposals = agent.analyze();
        // Should not panic (division-by-zero guard)
        assert!(proposals.is_empty() || proposals.iter().all(|p| p.confidence.is_finite()));
    }

    #[test]
    fn test_retain_recent_keeps_latest() {
        let mut agent = MetaAgent::new(0.3, 1);
        for i in 0..10 {
            agent.add_trace(create_test_trace(
                &format!("task-{i}"),
                TaskOutcome::Failed,
                1,
            ));
        }
        agent.retain_recent(3);
        assert_eq!(agent.stats().total_traces, 3);
    }
}
