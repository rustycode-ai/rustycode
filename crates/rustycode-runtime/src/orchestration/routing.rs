//! Shared task routing logic used by the CLI, prompt builder, and headless flow.

use crate::orchestration::llm_intent::{
    ClassificationSource, EnhancedIntentAssessment, LlmFallbackBudget, LlmIntentClassifier,
};
use rustycode_classification::{RoleRouter, UnifiedTaskClassifier};
use rustycode_config::{TaskRoutingConfig, TaskRoutingOverride};
use rustycode_llm::provider::LLMProvider;
use rustycode_llm::provider_metadata::ModelInfo;
use rustycode_protocol::intent::{classify_intent_with_confidence, IntentCategory};
use rustycode_protocol::task_routing::{
    parse_handoff_block, render_handoff_block, AssemblyContext, TaskExecutionPlan, TaskHarness,
    TaskRoutingAction, TaskRoutingDecision, TaskRoutingHandoff, TaskThinkingMode,
    TaskThinkingProfile, TaskThinkingStyle, TaskWorkflow,
};
use rustycode_protocol::{build_roster, AgentRole, TeamRole, WorkingMode};
use rustycode_team::profiler::TaskProfiler;
use std::str::FromStr;
use std::sync::Arc;

#[derive(Debug, Clone)]
struct EffectiveRoutingConfig {
    confidence_threshold: f64,
    max_clarifying_questions: usize,
    max_research_passes: usize,
    workflow_override: Option<TaskWorkflow>,
    team_override: Option<TeamRole>,
    agent_override: Option<AgentRole>,
    skills_override: Vec<String>,
    max_team_size: usize,
}

impl EffectiveRoutingConfig {
    fn from_base(base: &TaskRoutingConfig) -> Self {
        Self {
            confidence_threshold: base.confidence_threshold,
            max_clarifying_questions: base.max_clarifying_questions,
            max_research_passes: base.max_research_passes,
            workflow_override: None,
            team_override: None,
            agent_override: None,
            skills_override: Vec::new(),
            max_team_size: base.max_team_size,
        }
    }

    fn apply_override(&mut self, override_cfg: &TaskRoutingOverride) {
        if let Some(confidence_threshold) = override_cfg.confidence_threshold {
            self.confidence_threshold = confidence_threshold;
        }
        if let Some(max_clarifying_questions) = override_cfg.max_clarifying_questions {
            self.max_clarifying_questions = max_clarifying_questions;
        }
        if let Some(max_research_passes) = override_cfg.max_research_passes {
            self.max_research_passes = max_research_passes;
        }
        if let Some(workflow) = override_cfg
            .workflow
            .as_deref()
            .and_then(|value| TaskWorkflow::from_str(value).ok())
        {
            self.workflow_override = Some(workflow);
        }
        if let Some(team) = override_cfg.team.as_deref().and_then(parse_team_role) {
            self.team_override = Some(team);
        }
        if let Some(agent) = override_cfg.agent.as_deref().and_then(parse_agent_role) {
            self.agent_override = Some(agent);
        }
        if !override_cfg.skills.is_empty() {
            self.skills_override = override_cfg.skills.clone();
        }
    }
}

/// Resolve the task routing decision from a prompt and routing config.
///
/// This async function uses LlmIntentClassifier for enhanced classification with optional LLM fallback.
/// For now, provider is None, so classification uses only heuristics + fallback tracking.
pub async fn resolve_task_routing(
    task: &str,
    routing: Option<&TaskRoutingConfig>,
    interactive: bool,
) -> TaskRoutingDecision {
    let assessment = if task.trim().is_empty() {
        EnhancedIntentAssessment {
            category: IntentCategory::Implementation,
            confidence: 0.40,
            source: crate::orchestration::llm_intent::ClassificationSource::Heuristic,
        }
    } else {
        let config = routing.cloned().unwrap_or_default();
        let budget = LlmFallbackBudget::new(config.max_llm_fallback_calls);
        let threshold = config.llm_fallback_threshold;
        // B7-B8: provider will be Some(&Arc<dyn LLMProvider>); for now use None for heuristic-only
        LlmIntentClassifier::classify(task, None, &budget, threshold).await
    };

    let profile = TaskProfiler::new().profile(task);
    let config = routing.cloned().unwrap_or_default();
    let mut effective = EffectiveRoutingConfig::from_base(&config);

    if let Some(intent_override) = config.intent_overrides.get(assessment.category.as_key()) {
        effective.apply_override(intent_override);
    }

    let mut workflow = effective
        .workflow_override
        .unwrap_or_else(|| TaskWorkflow::from_intent(assessment.category));

    if let Some(workflow_override) = config.workflow_overrides.get(workflow.as_key()) {
        effective.apply_override(workflow_override);
        if let Some(override_workflow) = effective.workflow_override {
            workflow = override_workflow;
        }
    }

    let team = effective
        .team_override
        .unwrap_or_else(|| select_team(workflow, &profile));
    let agent = effective.agent_override.unwrap_or_else(|| {
        let classification = UnifiedTaskClassifier::new().classify(task);
        RoleRouter::select_for_score(
            &classification.signals,
            workflow,
            classification.complexity_score,
        )
    });
    let skills = if effective.skills_override.is_empty() {
        select_skills(workflow, &profile, assessment.category)
    } else {
        effective.skills_override
    };

    let missing_info = infer_missing_info(task, assessment.category);
    let harness = select_harness(task, workflow, &profile, assessment.category);
    let thinking = select_thinking_profile(
        task,
        workflow,
        &profile,
        assessment.category,
        assessment.confidence,
        &missing_info,
    );
    let execution_plan = TaskExecutionPlan::from_decision(
        &TaskRoutingDecision {
            intent: assessment.category,
            confidence: assessment.confidence,
            action: TaskRoutingAction::Proceed,
            workflow,
            harness,
            thinking,
            execution_plan: TaskExecutionPlan::default(),
            team,
            agent,
            skills: skills.clone(),
            missing_info: missing_info.clone(),
            roster: None,
        },
        "Structured execution plan generated from routing decision.",
    );
    let action = if !config.enabled || assessment.confidence >= effective.confidence_threshold {
        TaskRoutingAction::Proceed
    } else if interactive {
        TaskRoutingAction::Clarify {
            questions: effective
                .max_clarifying_questions
                .max(1)
                .min(missing_info.len().max(1)),
        }
    } else if effective.max_research_passes > 0 {
        TaskRoutingAction::Research {
            passes: effective.max_research_passes,
        }
    } else {
        TaskRoutingAction::Handoff
    };

    TaskRoutingDecision {
        intent: assessment.category,
        confidence: assessment.confidence,
        action,
        workflow,
        harness,
        thinking,
        execution_plan,
        team,
        agent,
        skills,
        missing_info,
        roster: None,
    }
}

/// Build the headless routing preface used by the CLI.
pub async fn build_headless_routing_preface(
    task: &str,
    routing: &TaskRoutingConfig,
    mode: Option<&WorkingMode>,
) -> String {
    let decision = resolve_task_routing(task, Some(routing), false).await;
    let mode_label = mode
        .map(std::string::ToString::to_string)
        .unwrap_or_else(|| decision.workflow.recommended_mode().to_string());

    let handoff_example =
        render_handoff_block(&decision.handoff_payload(
            "Uncertainty remained after research; handing off to the next workflow.",
        ));

    format!(
        "## Routing Policy\n\
        - Working mode: {mode_label}\n\
        - Routing threshold: {:.2}\n\
        {}\n\
\n\
        If the confidence is at or above the threshold, proceed with the selected workflow.\n\
        If the confidence is below the threshold, do targeted research first: inspect relevant files, \
        search existing docs/tests, and infer the intended workflow from evidence.\n\
        Repeat research for up to {} passes when important information is still missing.\n\
        Suggested harness: {} ({})\n\
        Suggested thinking depth: {} ({})\n\
        If uncertainty remains after research, emit a concise handoff using the structured payload below:\n\
        {}\n",
        routing.confidence_threshold,
        decision.summary(),
        routing.max_research_passes,
        decision.harness,
        decision.harness.summary(),
        decision.thinking,
        decision.thinking.summary(),
        handoff_example
    )
}

/// Parse a handoff block from model output.
pub fn parse_task_routing_handoff(text: &str) -> Option<TaskRoutingHandoff> {
    parse_handoff_block(text)
}

fn select_team(
    workflow: TaskWorkflow,
    profile: &rustycode_protocol::team::TaskProfile,
) -> TeamRole {
    match workflow {
        TaskWorkflow::Code => {
            if matches!(
                profile.risk,
                rustycode_protocol::team::RiskLevel::High
                    | rustycode_protocol::team::RiskLevel::Critical
            ) || matches!(
                profile.reach,
                rustycode_protocol::team::ReachLevel::SystemWide
            ) {
                TeamRole::Architect
            } else {
                TeamRole::Builder
            }
        }
        TaskWorkflow::Debug | TaskWorkflow::Research | TaskWorkflow::Analysis => {
            if matches!(
                profile.risk,
                rustycode_protocol::team::RiskLevel::High
                    | rustycode_protocol::team::RiskLevel::Critical
            ) {
                TeamRole::Coordinator
            } else {
                TeamRole::Skeptic
            }
        }
        TaskWorkflow::Plan => TeamRole::Architect,
        TaskWorkflow::Test => TeamRole::Judge,
        TaskWorkflow::Ask => TeamRole::Coordinator,
        _ => TeamRole::Coordinator,
    }
}

fn select_skills(
    workflow: TaskWorkflow,
    profile: &rustycode_protocol::team::TaskProfile,
    intent: IntentCategory,
) -> Vec<String> {
    let mut skills = workflow.default_skills();

    if matches!(
        profile.strategy,
        rustycode_protocol::team::ReasoningStrategy::TDD
    ) {
        skills.push("test_first".into());
    }

    if matches!(
        profile.strategy,
        rustycode_protocol::team::ReasoningStrategy::ReflectFirst
    ) {
        skills.push("evidence_first".into());
    }

    if matches!(
        intent,
        IntentCategory::Diagnostic | IntentCategory::Analytical
    ) {
        skills.push("root_cause_analysis".into());
    }

    if matches!(
        profile.risk,
        rustycode_protocol::team::RiskLevel::High | rustycode_protocol::team::RiskLevel::Critical
    ) {
        skills.push("preflight_check".into());
    }

    skills.sort();
    skills.dedup();
    skills
}

fn select_harness(
    task: &str,
    workflow: TaskWorkflow,
    profile: &rustycode_protocol::team::TaskProfile,
    intent: IntentCategory,
) -> TaskHarness {
    let lower = task.to_lowercase();

    let has_parallel_signals = matches!(
        profile.strategy,
        rustycode_protocol::team::ReasoningStrategy::Parallel
    ) || matches!(intent, IntentCategory::Analytical)
        || [
            "compare",
            "benchmark",
            "audit",
            "review",
            "analyze",
            "parallel",
            "multiple agents",
            "cross-check",
            "fan out",
        ]
        .iter()
        .any(|kw| lower.contains(kw));
    if has_parallel_signals {
        return TaskHarness::Omo;
    }

    let has_dag_signals = [
        "rfc",
        "dag",
        "decompose",
        "decomposition",
        "break down",
        "subtask",
        "subtasks",
        "independent",
        "worktree",
        "slice",
        "slices",
        "split into",
        "parallel branches",
    ]
    .iter()
    .any(|kw| lower.contains(kw));
    if has_dag_signals {
        return TaskHarness::Dag;
    }

    let has_architect_signals = [
        "architect",
        "architecture",
        "design",
        "design phase",
        "two-phase",
        "apply plan",
        "plan then implement",
    ]
    .iter()
    .any(|kw| lower.contains(kw));
    if has_architect_signals {
        return TaskHarness::Architect;
    }

    let has_long_running_signals = [
        "multi-step",
        "checkpoint",
        "resume",
        "long-running",
        "session",
        "compaction",
        "memory",
        "persist",
    ]
    .iter()
    .any(|kw| lower.contains(kw));
    if has_long_running_signals {
        return TaskHarness::Sparv;
    }

    let has_pipeline_signals = [
        "team",
        "pipeline",
        "prd",
        "roadmap",
        "milestone",
        "phase",
        "verify",
        "fix",
        "staged",
        "stage",
        "plan-execute",
        "migration",
        "rollout plan",
    ]
    .iter()
    .any(|kw| lower.contains(kw));
    if has_pipeline_signals {
        return TaskHarness::Pipeline;
    }

    if matches!(workflow, TaskWorkflow::Ask) {
        return TaskHarness::Direct;
    }

    TaskHarness::Ultrawork
}

fn select_thinking_profile(
    task: &str,
    workflow: TaskWorkflow,
    profile: &rustycode_protocol::team::TaskProfile,
    intent: IntentCategory,
    confidence: f64,
    missing_info: &[String],
) -> TaskThinkingProfile {
    let lower = task.to_lowercase();

    // Deep thinking for architecture/design work (keyword-driven, not broad)
    let has_deep_keywords = ["design", "architecture", "architect", "tradeoff"]
        .iter()
        .any(|kw| lower.contains(kw));
    if has_deep_keywords {
        return TaskThinkingProfile {
            depth: TaskThinkingMode::Deep,
            style: if matches!(workflow, TaskWorkflow::Debug | TaskWorkflow::Research) {
                TaskThinkingStyle::ReflectFirst
            } else if matches!(intent, IntentCategory::Analytical) {
                TaskThinkingStyle::EvidenceFirst
            } else {
                TaskThinkingStyle::PlanFirst
            },
            decompose_tasks: matches!(
                workflow,
                TaskWorkflow::Code | TaskWorkflow::Plan | TaskWorkflow::Analysis
            ),
            ask_clarifying_questions: confidence < 0.8,
            assign_responsibility: matches!(
                workflow,
                TaskWorkflow::Code | TaskWorkflow::Plan | TaskWorkflow::Research
            ),
            define_dependencies: matches!(workflow, TaskWorkflow::Code | TaskWorkflow::Plan),
            verify_before_acting: true,
        };
    }

    let has_extended_signals = confidence < 0.55
        || matches!(
            profile.risk,
            rustycode_protocol::team::RiskLevel::High
                | rustycode_protocol::team::RiskLevel::Critical
        )
        || matches!(
            profile.reach,
            rustycode_protocol::team::ReachLevel::Wide
                | rustycode_protocol::team::ReachLevel::SystemWide
        )
        || matches!(
            profile.reversibility,
            rustycode_protocol::team::Reversibility::Hard
                | rustycode_protocol::team::Reversibility::Irreversible
        )
        || missing_info.len() >= 4
        || matches!(
            workflow,
            TaskWorkflow::Code | TaskWorkflow::Debug | TaskWorkflow::Plan | TaskWorkflow::Analysis
        )
        || matches!(
            intent,
            IntentCategory::Implementation
                | IntentCategory::Investigation
                | IntentCategory::Planning
                | IntentCategory::Diagnostic
                | IntentCategory::Analytical
                | IntentCategory::Refactoring
        )
        || [
            "critical",
            "production",
            "security",
            "migration",
            "release",
            "rollout",
            "checkpoint",
            "resume",
            "long-running",
            "session",
            "memory",
            "debug",
            "investigate",
            "refactor",
            "analyze",
            "reason",
            "multi-step",
            "root cause",
        ]
        .iter()
        .any(|kw| lower.contains(kw));
    if has_extended_signals {
        return TaskThinkingProfile {
            depth: TaskThinkingMode::Extended,
            style: TaskThinkingStyle::TradeoffFirst,
            decompose_tasks: true,
            ask_clarifying_questions: true,
            assign_responsibility: true,
            define_dependencies: true,
            verify_before_acting: true,
        };
    }

    TaskThinkingProfile {
        depth: TaskThinkingMode::Standard,
        style: if matches!(
            intent,
            IntentCategory::Explanation | IntentCategory::Investigation
        ) {
            TaskThinkingStyle::EvidenceFirst
        } else {
            TaskThinkingStyle::Standard
        },
        decompose_tasks: false,
        ask_clarifying_questions: confidence < 0.7,
        assign_responsibility: false,
        define_dependencies: false,
        verify_before_acting: matches!(
            intent,
            IntentCategory::Testing | IntentCategory::Diagnostic
        ),
    }
}

fn infer_missing_info(task: &str, intent: IntentCategory) -> Vec<String> {
    if task.trim().is_empty() {
        return vec!["goal".into(), "scope".into()];
    }

    let mut missing = match intent {
        IntentCategory::Implementation => vec![
            "target file or module".into(),
            "acceptance criteria".into(),
            "constraints".into(),
        ],
        IntentCategory::Investigation => vec![
            "specific symptom".into(),
            "error output".into(),
            "reproduction steps".into(),
        ],
        IntentCategory::Explanation => vec!["code location".into(), "desired depth".into()],
        IntentCategory::Refactoring => {
            vec!["preserved behavior".into(), "scope of the refactor".into()]
        }
        IntentCategory::Planning => vec!["constraints".into(), "success criteria".into()],
        IntentCategory::Testing => vec!["failing test name".into(), "expected behavior".into()],
        IntentCategory::Analytical => {
            vec!["metric of interest".into(), "comparison baseline".into()]
        }
        IntentCategory::Diagnostic => vec!["error message".into(), "reproduction steps".into()],
        _ => vec!["additional context".into()],
    };

    if task.len() < 20 {
        missing.push("additional context".into());
    }

    missing.sort();
    missing.dedup();
    missing
}

fn parse_team_role(input: &str) -> Option<TeamRole> {
    match input.to_lowercase().as_str() {
        "builder" => Some(TeamRole::Builder),
        "skeptic" => Some(TeamRole::Skeptic),
        "judge" => Some(TeamRole::Judge),
        "coordinator" => Some(TeamRole::Coordinator),
        "architect" => Some(TeamRole::Architect),
        "scalpel" => Some(TeamRole::Scalpel),
        _ => None,
    }
}

fn parse_agent_role(input: &str) -> Option<AgentRole> {
    match input.to_lowercase().as_str() {
        "architect" => Some(AgentRole::Architect),
        "builder" => Some(AgentRole::Builder),
        "skeptic" => Some(AgentRole::Skeptic),
        "judge" => Some(AgentRole::Judge),
        "scalpel" => Some(AgentRole::Scalpel),
        "coordinator" => Some(AgentRole::Coordinator),
        "planner" => Some(AgentRole::Planner),
        "worker" => Some(AgentRole::Worker),
        "reviewer" => Some(AgentRole::Reviewer),
        "researcher" => Some(AgentRole::Researcher),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Phase B: Async routing with LLM-augmented classification
// ---------------------------------------------------------------------------

/// Resolve the task routing decision using an async pipeline with optional
/// LLM-augmented intent classification.
///
/// Unlike the sync [`resolve_task_routing`], this function accepts an optional
/// LLM provider so that low-confidence heuristic results can be upgraded via
/// an LLM call. When no provider is supplied it falls back to heuristic-only
/// classification (identical outcome to the sync path).
///
/// **Disagreement resolution**: if both heuristic and LLM produce results the
/// one with the higher confidence wins.
pub async fn resolve_task_routing_async(
    task: &str,
    routing: Option<&TaskRoutingConfig>,
    interactive: bool,
    provider: Option<Arc<dyn LLMProvider>>,
) -> TaskRoutingDecision {
    let heuristic_assessment = if task.trim().is_empty() {
        EnhancedIntentAssessment {
            category: IntentCategory::Implementation,
            confidence: 0.40,
            source: ClassificationSource::Heuristic,
        }
    } else {
        let heuristic = classify_intent_with_confidence(task);
        EnhancedIntentAssessment {
            category: heuristic.category,
            confidence: heuristic.confidence,
            source: ClassificationSource::Heuristic,
        }
    };

    let config = routing.cloned().unwrap_or_default();
    let budget = LlmFallbackBudget::new(config.max_llm_fallback_calls);
    let threshold = config.llm_fallback_threshold;

    let assessment = if provider.is_some() && heuristic_assessment.confidence < threshold {
        let llm_assessment =
            LlmIntentClassifier::classify(task, provider.as_ref(), &budget, threshold).await;

        if llm_assessment.confidence > heuristic_assessment.confidence {
            llm_assessment
        } else {
            EnhancedIntentAssessment {
                category: heuristic_assessment.category,
                confidence: heuristic_assessment.confidence,
                source: ClassificationSource::HeuristicFallback,
            }
        }
    } else {
        heuristic_assessment
    };

    let profile = TaskProfiler::new().profile(task);
    let mut effective = EffectiveRoutingConfig::from_base(&config);

    if let Some(intent_override) = config.intent_overrides.get(assessment.category.as_key()) {
        effective.apply_override(intent_override);
    }

    let mut workflow = effective
        .workflow_override
        .unwrap_or_else(|| TaskWorkflow::from_intent(assessment.category));

    if let Some(workflow_override) = config.workflow_overrides.get(workflow.as_key()) {
        effective.apply_override(workflow_override);
        if let Some(override_workflow) = effective.workflow_override {
            workflow = override_workflow;
        }
    }

    let team = effective
        .team_override
        .unwrap_or_else(|| select_team(workflow, &profile));
    let agent = effective.agent_override.unwrap_or_else(|| {
        let classification = UnifiedTaskClassifier::new().classify(task);
        RoleRouter::select_for_score(
            &classification.signals,
            workflow,
            classification.complexity_score,
        )
    });
    let skills = if effective.skills_override.is_empty() {
        select_skills(workflow, &profile, assessment.category)
    } else {
        effective.skills_override
    };

    let missing_info = infer_missing_info(task, assessment.category);
    let harness = select_harness(task, workflow, &profile, assessment.category);
    let thinking = select_thinking_profile(
        task,
        workflow,
        &profile,
        assessment.category,
        assessment.confidence,
        &missing_info,
    );

    let assembly_context = AssemblyContext {
        intent_category: assessment.category,
        thinking_depth: thinking.depth,
        confidence: assessment.confidence,
        required_specialists: Vec::new(),
    };
    let team_config = profile.assemble_team_with_context(Some(&assembly_context));
    let roster = build_roster(
        &team_config,
        Some(&assembly_context),
        effective.max_team_size,
    );

    let execution_plan = TaskExecutionPlan::from_decision(
        &TaskRoutingDecision {
            intent: assessment.category,
            confidence: assessment.confidence,
            action: TaskRoutingAction::Proceed,
            workflow,
            harness,
            thinking,
            execution_plan: TaskExecutionPlan::default(),
            team,
            agent,
            skills: skills.clone(),
            missing_info: missing_info.clone(),
            roster: Some(roster.clone()),
        },
        "Structured execution plan generated from routing decision.",
    );
    let action = if !config.enabled || assessment.confidence >= effective.confidence_threshold {
        TaskRoutingAction::Proceed
    } else if interactive {
        TaskRoutingAction::Clarify {
            questions: effective
                .max_clarifying_questions
                .max(1)
                .min(missing_info.len().max(1)),
        }
    } else if effective.max_research_passes > 0 {
        TaskRoutingAction::Research {
            passes: effective.max_research_passes,
        }
    } else {
        TaskRoutingAction::Handoff
    };

    TaskRoutingDecision {
        intent: assessment.category,
        confidence: assessment.confidence,
        action,
        workflow,
        harness,
        thinking,
        execution_plan,
        team,
        agent,
        skills,
        missing_info,
        roster: Some(roster),
    }
}

// ---------------------------------------------------------------------------
// Phase C: Thinking bridge — routing-level → provider-level thinking config
// ---------------------------------------------------------------------------

/// Check whether a model supports extended thinking based on its identifier.
///
/// Uses a simple heuristic: models whose ID contains "claude" along with at
/// least one of "opus", "sonnet", or "haiku" are considered thinking-capable.
pub fn model_supports_thinking(model_info: &ModelInfo) -> bool {
    let id = model_info.model_id.to_lowercase();
    let is_claude = id.contains("claude");
    let has_capability = id.contains("opus")
        || id.contains("sonnet")
        || id.contains("haiku")
        || id.contains("4-5")
        || id.contains("4.5")
        || id.contains("3-7")
        || id.contains("3.7")
        || id.contains("4-7")
        || id.contains("4.7");
    is_claude && has_capability
}

/// Convert routing-level [`TaskThinkingProfile`] into a provider-level
/// [`ThinkingConfig`](rustycode_llm::provider::ThinkingConfig).
///
/// # Arguments
/// * `thinking_profile` — The routing decision's thinking profile
/// * `model_info` — Metadata about the target model
/// * `got_already_active` — Whether Graph-of-Thoughts is already active
///
/// # Mutual exclusion
/// If GoT is already active (`got_already_active == true`), native thinking
/// is always disabled (returns `None`) to avoid conflicting reasoning modes.
///
/// # Mapping
/// | Depth | Thinking-capable model | Non-thinking model |
/// |-------|----------------------|--------------------|
/// | Standard | `None` | `None` |
/// | Deep | `enabled(10_000)` | `None` |
/// | Extended | `enabled(32_000)` | `None` |
pub fn thinking_bridge(
    thinking_profile: &TaskThinkingProfile,
    model_info: &ModelInfo,
    got_already_active: bool,
) -> Option<rustycode_llm::provider::ThinkingConfig> {
    if got_already_active {
        return None;
    }

    if matches!(thinking_profile.depth, TaskThinkingMode::Standard) {
        return None;
    }

    if !model_supports_thinking(model_info) {
        return None;
    }

    match thinking_profile.depth {
        TaskThinkingMode::Deep => Some(rustycode_llm::provider::ThinkingConfig::enabled(10_000)),
        TaskThinkingMode::Extended => {
            Some(rustycode_llm::provider::ThinkingConfig::enabled(32_000))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustycode_config::TaskRoutingConfig;

    #[tokio::test]
    async fn routes_low_confidence_interactive_to_clarify() {
        let config = TaskRoutingConfig {
            confidence_threshold: 0.95,
            ..Default::default()
        };
        let decision = resolve_task_routing("implement auth", Some(&config), true).await;

        assert!(matches!(decision.action, TaskRoutingAction::Clarify { .. }));
        assert_eq!(decision.workflow, TaskWorkflow::Code);
        assert!(!decision.missing_info.is_empty());
    }

    #[tokio::test]
    async fn routes_low_confidence_headless_to_research() {
        let config = TaskRoutingConfig {
            confidence_threshold: 0.95,
            ..Default::default()
        };
        let decision = resolve_task_routing("implement auth", Some(&config), false).await;

        assert!(matches!(
            decision.action,
            TaskRoutingAction::Research { .. }
        ));
        assert_eq!(decision.workflow, TaskWorkflow::Code);
    }

    #[tokio::test]
    async fn parses_handoff_payload() {
        let decision = resolve_task_routing(
            "plan the migration",
            Some(&TaskRoutingConfig::default()),
            false,
        )
        .await;
        let block = render_handoff_block(&decision.handoff_payload("need more constraints"));
        let parsed = parse_task_routing_handoff(&block).unwrap();
        assert_eq!(parsed.workflow, TaskWorkflow::Plan);
        assert_eq!(parsed.harness, TaskHarness::Pipeline);
        assert_eq!(parsed.thinking, decision.thinking);
        assert_eq!(parsed.execution_plan, decision.execution_plan);
        assert!(!parsed.execution_plan.next_step.is_empty());
        assert_eq!(parsed.reason, "need more constraints");
    }

    #[tokio::test]
    async fn routes_long_running_sessions_to_sparv() {
        let decision = resolve_task_routing(
            "keep this session alive for a long-running release rollout with checkpoints",
            Some(&TaskRoutingConfig::default()),
            false,
        )
        .await;

        assert_eq!(decision.harness, TaskHarness::Sparv);
        assert_eq!(decision.thinking.depth, TaskThinkingMode::Extended);
    }

    // -----------------------------------------------------------------------
    // Phase B: resolve_task_routing async + resolve_task_routing_async tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn async_routing_without_provider_matches_async_path() {
        let config = TaskRoutingConfig::default();
        let sync_like_decision = resolve_task_routing("implement auth", Some(&config), false).await;
        let async_decision =
            resolve_task_routing_async("implement auth", Some(&config), false, None).await;

        // Both use heuristic-only (no provider), so results should match
        assert_eq!(sync_like_decision.intent, async_decision.intent);
        assert_eq!(sync_like_decision.workflow, async_decision.workflow);
        assert_eq!(sync_like_decision.harness, async_decision.harness);
    }

    #[tokio::test]
    async fn async_routing_empty_task_returns_implementation() {
        let decision = resolve_task_routing_async("", None, false, None).await;

        assert_eq!(decision.intent, IntentCategory::Implementation);
        assert!(decision.confidence < 0.5);
    }

    #[tokio::test]
    async fn async_routing_low_confidence_headless_research() {
        let config = TaskRoutingConfig {
            confidence_threshold: 0.95,
            ..Default::default()
        };
        let decision =
            resolve_task_routing_async("implement auth", Some(&config), false, None).await;

        assert!(matches!(
            decision.action,
            TaskRoutingAction::Research { .. }
        ));
    }

    // -----------------------------------------------------------------------
    // Phase C: model_supports_thinking + thinking_bridge tests
    // -----------------------------------------------------------------------

    fn make_model_info(model_id: &str) -> ModelInfo {
        ModelInfo {
            model_id: model_id.to_string(),
            display_name: model_id.to_string(),
            description: String::new(),
            context_window: 200_000,
            supports_tools: true,
            use_cases: vec![],
            cost_tier: 3,
        }
    }

    #[test]
    fn model_supports_thinking_claude_opus() {
        assert!(model_supports_thinking(&make_model_info("claude-opus-4-7")));
    }

    #[test]
    fn model_supports_thinking_claude_sonnet() {
        assert!(model_supports_thinking(&make_model_info(
            "claude-sonnet-4-5-20250514"
        )));
    }

    #[test]
    fn model_supports_thinking_claude_haiku() {
        assert!(model_supports_thinking(&make_model_info(
            "claude-haiku-4-5"
        )));
    }

    #[test]
    fn model_supports_thinking_rejects_gpt() {
        assert!(!model_supports_thinking(&make_model_info("gpt-4o")));
    }

    #[test]
    fn model_supports_thinking_rejects_gemini() {
        assert!(!model_supports_thinking(&make_model_info("gemini-2.5-pro")));
    }

    #[test]
    fn model_supports_thinking_rejects_bare_claude() {
        assert!(!model_supports_thinking(&make_model_info("claude-instant")));
    }

    #[test]
    fn thinking_bridge_standard_returns_none() {
        let profile = TaskThinkingProfile {
            depth: TaskThinkingMode::Standard,
            ..TaskThinkingProfile::default()
        };
        let model = make_model_info("claude-opus-4-7");
        assert!(thinking_bridge(&profile, &model, false).is_none());
    }

    #[test]
    fn thinking_bridge_deep_thinking_model() {
        let profile = TaskThinkingProfile {
            depth: TaskThinkingMode::Deep,
            ..TaskThinkingProfile::default()
        };
        let model = make_model_info("claude-sonnet-4-5");
        let config = thinking_bridge(&profile, &model, false).unwrap();
        assert_eq!(config.budget_tokens, Some(10_000));
    }

    #[test]
    fn thinking_bridge_extended_thinking_model() {
        let profile = TaskThinkingProfile {
            depth: TaskThinkingMode::Extended,
            ..TaskThinkingProfile::default()
        };
        let model = make_model_info("claude-opus-4-7");
        let config = thinking_bridge(&profile, &model, false).unwrap();
        assert_eq!(config.budget_tokens, Some(32_000));
    }

    #[test]
    fn thinking_bridge_deep_non_thinking_model_returns_none() {
        let profile = TaskThinkingProfile {
            depth: TaskThinkingMode::Deep,
            ..TaskThinkingProfile::default()
        };
        let model = make_model_info("gpt-4o");
        assert!(thinking_bridge(&profile, &model, false).is_none());
    }

    #[test]
    fn thinking_bridge_extended_non_thinking_model_returns_none() {
        let profile = TaskThinkingProfile {
            depth: TaskThinkingMode::Extended,
            ..TaskThinkingProfile::default()
        };
        let model = make_model_info("gpt-4o");
        assert!(thinking_bridge(&profile, &model, false).is_none());
    }

    #[test]
    fn thinking_bridge_got_active_returns_none_even_with_extended() {
        let profile = TaskThinkingProfile {
            depth: TaskThinkingMode::Extended,
            ..TaskThinkingProfile::default()
        };
        let model = make_model_info("claude-opus-4-7");
        assert!(thinking_bridge(&profile, &model, true).is_none());
    }

    #[test]
    fn thinking_bridge_got_active_returns_none_with_deep() {
        let profile = TaskThinkingProfile {
            depth: TaskThinkingMode::Deep,
            ..TaskThinkingProfile::default()
        };
        let model = make_model_info("claude-sonnet-4-5");
        assert!(thinking_bridge(&profile, &model, true).is_none());
    }

    #[test]
    fn thinking_bridge_got_active_standard_still_none() {
        let profile = TaskThinkingProfile {
            depth: TaskThinkingMode::Standard,
            ..TaskThinkingProfile::default()
        };
        let model = make_model_info("claude-opus-4-7");
        assert!(thinking_bridge(&profile, &model, true).is_none());
    }

    #[tokio::test]
    async fn routes_parallel_analysis_to_omo_harness() {
        let decision = resolve_task_routing(
            "compare these implementations and analyze the tradeoffs",
            Some(&TaskRoutingConfig::default()),
            false,
        )
        .await;

        assert_eq!(decision.harness, TaskHarness::Omo);
    }

    #[tokio::test]
    async fn routes_simple_execution_to_ultrawork() {
        let decision = resolve_task_routing(
            "implement a small helper function",
            Some(&TaskRoutingConfig::default()),
            false,
        )
        .await;

        assert_eq!(decision.harness, TaskHarness::Ultrawork);
    }

    #[tokio::test]
    async fn routes_high_risk_migration_to_pipeline() {
        let decision = resolve_task_routing(
            "refactor the auth migration and rollout plan",
            Some(&TaskRoutingConfig::default()),
            false,
        )
        .await;

        assert_eq!(decision.harness, TaskHarness::Pipeline);
        assert_eq!(decision.thinking.depth, TaskThinkingMode::Extended);
        assert!(decision.thinking.decompose_tasks);
    }

    #[tokio::test]
    async fn routes_decomposition_to_dag() {
        let decision = resolve_task_routing(
            "break this feature into independent subtasks and plan the DAG",
            Some(&TaskRoutingConfig::default()),
            false,
        )
        .await;

        assert_eq!(decision.harness, TaskHarness::Dag);
    }

    #[tokio::test]
    async fn routes_design_work_to_architect() {
        let decision = resolve_task_routing(
            "design the authentication architecture and implementation plan",
            Some(&TaskRoutingConfig::default()),
            false,
        )
        .await;

        assert_eq!(decision.harness, TaskHarness::Architect);
        assert_eq!(decision.thinking.depth, TaskThinkingMode::Deep);
        assert_eq!(decision.thinking.style.to_string(), "plan_first");
    }

    // B6: New async integration tests for LlmIntentClassifier wiring

    #[tokio::test]
    async fn resolve_task_routing_uses_heuristic_for_high_confidence() {
        // Clear Implementation task should be high confidence and use heuristic path
        let task = "Write a Rust function to calculate the Fibonacci sequence.";
        let config = TaskRoutingConfig {
            confidence_threshold: 0.5,
            llm_fallback_threshold: 0.65,
            max_llm_fallback_calls: 0, // No LLM budget
            ..Default::default()
        };
        let decision = resolve_task_routing(task, Some(&config), false).await;

        // Should be classified as Implementation with decent confidence
        assert_eq!(decision.intent, IntentCategory::Implementation);
        assert!(
            decision.confidence > 0.5,
            "High-confidence task should exceed threshold"
        );
        // Action should be Proceed since confidence is high
        assert!(matches!(decision.action, TaskRoutingAction::Proceed));
    }

    #[tokio::test]
    async fn resolve_task_routing_low_confidence_path_works() {
        // Ambiguous task should trigger low-confidence path
        let task = "help";
        let config = TaskRoutingConfig {
            confidence_threshold: 0.95, // Very high threshold
            llm_fallback_threshold: 0.6,
            max_llm_fallback_calls: 0, // No LLM budget, so fallback to heuristic
            ..Default::default()
        };
        let decision = resolve_task_routing(task, Some(&config), false).await;

        // Should complete routing even without LLM provider
        // The action should be Research (headless mode with low confidence)
        assert!(matches!(
            decision.action,
            TaskRoutingAction::Research { .. }
        ));
        // Config should still be applied
        assert_eq!(decision.workflow, TaskWorkflow::Code);
    }
}
