//! Task routing types shared across the workspace.
//!
//! These types model the decision that routes a user task into a workflow,
//! team, agent, and fallback action. They are intentionally serializable so
//! the CLI, runtime, and prompt builders can all consume the same shape.

use crate::agent_protocol::AgentRole;
use crate::intent::IntentCategory;
use crate::modes::WorkingMode;
use crate::team::TeamRole;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// The workflow selected for a task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum TaskWorkflow {
    Code,
    Debug,
    Plan,
    Research,
    Test,
    Analysis,
    Ask,
}

impl TaskWorkflow {
    /// Convert the workflow to a stable string key.
    pub const fn as_key(&self) -> &'static str {
        match self {
            Self::Code => "code",
            Self::Debug => "debug",
            Self::Plan => "plan",
            Self::Research => "research",
            Self::Test => "test",
            Self::Analysis => "analysis",
            Self::Ask => "ask",
        }
    }

    /// Pick the default workflow for a detected intent.
    pub fn from_intent(intent: IntentCategory) -> Self {
        match intent {
            IntentCategory::Implementation => Self::Code,
            IntentCategory::Investigation => Self::Research,
            IntentCategory::Explanation => Self::Ask,
            IntentCategory::Refactoring => Self::Code,
            IntentCategory::Planning => Self::Plan,
            IntentCategory::Testing => Self::Test,
            IntentCategory::Analytical => Self::Analysis,
            IntentCategory::Diagnostic => Self::Debug,
        }
    }

    /// Convert the workflow to the preferred working mode.
    pub fn recommended_mode(self) -> WorkingMode {
        match self {
            Self::Code => WorkingMode::Code,
            Self::Debug => WorkingMode::Debug,
            Self::Plan => WorkingMode::Plan,
            Self::Research => WorkingMode::Debug,
            Self::Test => WorkingMode::Test,
            Self::Analysis => WorkingMode::Debug,
            Self::Ask => WorkingMode::Ask,
        }
    }

    /// Default team role for this workflow.
    pub fn default_team(self) -> TeamRole {
        match self {
            Self::Code => TeamRole::Builder,
            Self::Debug => TeamRole::Skeptic,
            Self::Plan => TeamRole::Architect,
            Self::Research => TeamRole::Coordinator,
            Self::Test => TeamRole::Judge,
            Self::Analysis => TeamRole::Skeptic,
            Self::Ask => TeamRole::Coordinator,
        }
    }

    /// Default specialist agent for this workflow.
    pub fn default_agent(self) -> AgentRole {
        match self {
            Self::Code => AgentRole::Worker,
            Self::Debug => AgentRole::Researcher,
            Self::Plan => AgentRole::Planner,
            Self::Research => AgentRole::Researcher,
            Self::Test => AgentRole::Reviewer,
            Self::Analysis => AgentRole::Researcher,
            Self::Ask => AgentRole::Researcher,
        }
    }

    /// Default skill hints for this workflow.
    pub fn default_skills(self) -> Vec<String> {
        match self {
            Self::Code => vec!["write_code".into(), "verify_changes".into()],
            Self::Debug => vec!["inspect_errors".into(), "run_tests".into()],
            Self::Plan => vec!["research".into(), "design".into()],
            Self::Research => vec!["inspect_files".into(), "search_docs".into()],
            Self::Test => vec!["write_tests".into(), "run_tests".into()],
            Self::Analysis => vec!["measure".into(), "compare_evidence".into()],
            Self::Ask => vec!["explain".into(), "answer_questions".into()],
        }
    }
}

impl fmt::Display for TaskWorkflow {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_key())
    }
}

impl FromStr for TaskWorkflow {
    type Err = ();

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input.to_lowercase().as_str() {
            "code" | "implementation" | "implement" => Ok(Self::Code),
            "debug" | "diagnostic" => Ok(Self::Debug),
            "plan" | "planning" => Ok(Self::Plan),
            "research" | "investigation" => Ok(Self::Research),
            "test" | "testing" => Ok(Self::Test),
            "analysis" | "analytical" => Ok(Self::Analysis),
            "ask" | "explanation" => Ok(Self::Ask),
            _ => Err(()),
        }
    }
}

/// The execution pattern selected for a task.
///
/// This is the orchestration layer above the workflow:
/// - `direct`: no additional harness required
/// - `ultrawork`: lightweight progress/retry loop
/// - `omo`: multi-agent analysis and cross-checking
/// - `sparv`: long-lived, checkpointed session
/// - `architect`: design then apply
/// - `pipeline`: staged team execution
/// - `dag`: independent subtask decomposition
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum TaskHarness {
    /// No additional harness needed.
    #[default]
    Direct,
    /// Lightweight progress/retry harness.
    Ultrawork,
    /// Multi-agent parallel analysis harness.
    Omo,
    /// Long-lived, checkpointed session harness.
    Sparv,
    /// Two-phase design then apply harness.
    Architect,
    /// Staged team execution harness.
    Pipeline,
    /// RFC/DAG-style decomposition harness.
    Dag,
    /// Tiered model orchestration harness (Musician → Editor → Composer).
    Tiered,
}

/// The reasoning depth requested for the task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum TaskThinkingMode {
    /// Default reasoning depth.
    #[default]
    Standard,
    /// Extra deliberation for complex design or debugging.
    Deep,
    /// Long-form reasoning for strategic or high-risk work.
    Extended,
}

impl TaskThinkingMode {
    /// Convert the thinking mode to a stable string key.
    pub const fn as_key(&self) -> &'static str {
        match self {
            Self::Standard => "standard",
            Self::Deep => "deep",
            Self::Extended => "extended",
        }
    }

    /// A short explanation of why this thinking mode is recommended.
    pub const fn summary(&self) -> &'static str {
        match self {
            Self::Standard => "Use normal reasoning depth.",
            Self::Deep => "Use extended reasoning for difficult or multi-step work.",
            Self::Extended => "Use the deepest reasoning budget for strategic, high-risk work.",
        }
    }
}

/// The reasoning style to use for the task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum TaskThinkingStyle {
    /// Balanced reasoning with no special bias.
    #[default]
    Standard,
    /// Start with a plan and decompose before acting.
    PlanFirst,
    /// Analyze before changing anything.
    ReflectFirst,
    /// Build from evidence and observed facts.
    EvidenceFirst,
    /// Form hypotheses and test them against the codebase.
    HypothesisFirst,
    /// Compare tradeoffs before choosing a path.
    TradeoffFirst,
}

impl TaskThinkingStyle {
    /// Convert the style to a stable string key.
    pub const fn as_key(&self) -> &'static str {
        match self {
            Self::Standard => "standard",
            Self::PlanFirst => "plan_first",
            Self::ReflectFirst => "reflect_first",
            Self::EvidenceFirst => "evidence_first",
            Self::HypothesisFirst => "hypothesis_first",
            Self::TradeoffFirst => "tradeoff_first",
        }
    }

    /// A short explanation of why this style is recommended.
    pub const fn summary(&self) -> &'static str {
        match self {
            Self::Standard => "Use balanced reasoning.",
            Self::PlanFirst => {
                "Break work into phases, dependencies, and responsibilities before acting."
            }
            Self::ReflectFirst => "Analyze symptoms and risks before changing the system.",
            Self::EvidenceFirst => "Ground decisions in direct evidence from the workspace.",
            Self::HypothesisFirst => "Frame candidate explanations and test them structurally.",
            Self::TradeoffFirst => "Compare options and constraints before selecting a path.",
        }
    }
}

impl fmt::Display for TaskThinkingStyle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_key())
    }
}

impl FromStr for TaskThinkingStyle {
    type Err = ();

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input.to_lowercase().as_str() {
            "standard" | "balanced" => Ok(Self::Standard),
            "plan_first" | "plan-first" | "plan" => Ok(Self::PlanFirst),
            "reflect_first" | "reflect-first" | "reflect" => Ok(Self::ReflectFirst),
            "evidence_first" | "evidence-first" | "evidence" => Ok(Self::EvidenceFirst),
            "hypothesis_first" | "hypothesis-first" | "hypothesis" => Ok(Self::HypothesisFirst),
            "tradeoff_first" | "tradeoff-first" | "tradeoff" => Ok(Self::TradeoffFirst),
            _ => Err(()),
        }
    }
}

/// The full thinking profile that shapes reasoning behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct TaskThinkingProfile {
    /// How much deliberate reasoning to apply.
    pub depth: TaskThinkingMode,
    /// What kind of reasoning posture to adopt.
    pub style: TaskThinkingStyle,
    /// Whether to break the task into explicit subproblems.
    pub decompose_tasks: bool,
    /// Whether to ask clarifying questions if information is missing.
    pub ask_clarifying_questions: bool,
    /// Whether to assign explicit responsibility to subparts.
    pub assign_responsibility: bool,
    /// Whether to define dependencies between subparts.
    pub define_dependencies: bool,
    /// Whether to verify before acting or finalizing.
    pub verify_before_acting: bool,
}

impl Default for TaskThinkingProfile {
    fn default() -> Self {
        Self {
            depth: TaskThinkingMode::Standard,
            style: TaskThinkingStyle::Standard,
            decompose_tasks: false,
            ask_clarifying_questions: false,
            assign_responsibility: false,
            define_dependencies: false,
            verify_before_acting: false,
        }
    }
}

impl TaskThinkingProfile {
    /// Convert the profile to a concise summary for prompts and logs.
    pub fn summary(&self) -> String {
        let mut lines = vec![
            format!("- Depth: {}", self.depth),
            format!("- Style: {}", self.style),
            format!("- Decompose tasks: {}", self.decompose_tasks),
            format!(
                "- Ask clarifying questions: {}",
                self.ask_clarifying_questions
            ),
            format!("- Assign responsibility: {}", self.assign_responsibility),
            format!("- Define dependencies: {}", self.define_dependencies),
            format!("- Verify before acting: {}", self.verify_before_acting),
        ];
        lines.push(format!("- Depth guidance: {}", self.depth.summary()));
        lines.push(format!("- Style guidance: {}", self.style.summary()));
        lines.join("\n")
    }
}

/// A single step in the execution plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TaskExecutionStep {
    pub title: String,
    pub owner: String,
    #[serde(default)]
    pub depends_on: Vec<String>,
    pub description: String,
    pub verification: String,
}

/// Structured execution plan attached to a handoff.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TaskExecutionPlan {
    pub next_step: String,
    #[serde(default)]
    pub responsibilities: Vec<String>,
    #[serde(default)]
    pub dependencies: Vec<String>,
    #[serde(default)]
    pub verification: Vec<String>,
    #[serde(default)]
    pub steps: Vec<TaskExecutionStep>,
}

impl TaskExecutionPlan {
    /// Build a plan from the current routing decision.
    pub fn from_decision(decision: &TaskRoutingDecision, reason: &str) -> Self {
        let next_step = match decision.workflow {
            TaskWorkflow::Code => "Start implementation with the smallest safe slice",
            TaskWorkflow::Debug => "Reproduce the issue and isolate the fault boundary",
            TaskWorkflow::Plan => "Draft the plan and decompose it into phases",
            TaskWorkflow::Research => "Gather evidence from the codebase and docs",
            TaskWorkflow::Test => "Identify or create the minimal failing test",
            TaskWorkflow::Analysis => "Frame the analysis and collect comparative evidence",
            TaskWorkflow::Ask => "Answer directly or ask a concise clarifying question",
        }
        .to_string();

        let mut responsibilities = vec![
            format!("Primary owner: {}", decision.agent),
            format!("Coordination: {}", decision.team),
        ];
        if decision.thinking.assign_responsibility {
            responsibilities.push("Assign per-step owners before execution".into());
        }

        let mut dependencies = decision.missing_info.clone();
        if decision.thinking.define_dependencies {
            dependencies.push("Order work by dependency chain".into());
        }

        let mut verification = vec![format!(
            "Verify that {} workflow aligns with {}",
            decision.workflow, decision.harness
        )];
        if decision.thinking.verify_before_acting {
            verification.push("Confirm assumptions before making changes".into());
        }
        verification.push(reason.to_string());

        let mut steps = vec![TaskExecutionStep {
            title: next_step.clone(),
            owner: decision.agent.to_string(),
            depends_on: dependencies.clone(),
            description: decision.thinking.style.summary().to_string(),
            verification: "Prove the next step is safe and bounded".to_string(),
        }];

        if decision.thinking.decompose_tasks {
            steps.push(TaskExecutionStep {
                title: "Decompose work".to_string(),
                owner: decision.team.to_string(),
                depends_on: vec!["next_step".into()],
                description: "Break the task into independently verifiable pieces".to_string(),
                verification: "Dependencies and owners are explicit".to_string(),
            });
        }

        Self {
            next_step,
            responsibilities,
            dependencies,
            verification,
            steps: {
                steps.shrink_to_fit();
                steps
            },
        }
    }

    /// Convert the plan to a concise summary for prompts and logs.
    pub fn summary(&self) -> String {
        let mut lines = vec![
            format!("- Next step: {}", self.next_step),
            format!("- Responsibilities: {}", self.responsibilities.join(", ")),
            format!("- Dependencies: {}", self.dependencies.join(", ")),
            format!("- Verification: {}", self.verification.join(", ")),
        ];

        for step in &self.steps {
            lines.push(format!(
                "- Step: {} | owner: {} | depends_on: {} | verify: {}",
                step.title,
                step.owner,
                if step.depends_on.is_empty() {
                    "none".to_string()
                } else {
                    step.depends_on.join(", ")
                },
                step.verification
            ));
        }

        lines.join("\n")
    }
}

impl fmt::Display for TaskThinkingProfile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.summary())
    }
}

impl fmt::Display for TaskThinkingMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_key())
    }
}

impl FromStr for TaskThinkingMode {
    type Err = ();

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input.to_lowercase().as_str() {
            "standard" | "normal" | "default" => Ok(Self::Standard),
            "deep" | "think" => Ok(Self::Deep),
            "extended" | "ultradeep" | "ultra-deep" | "max" => Ok(Self::Extended),
            _ => Err(()),
        }
    }
}

impl TaskHarness {
    /// Convert the harness to a stable string key.
    pub const fn as_key(&self) -> &'static str {
        match self {
            Self::Direct => "direct",
            Self::Ultrawork => "ultrawork",
            Self::Omo => "omo",
            Self::Sparv => "sparv",
            Self::Architect => "architect",
            Self::Pipeline => "pipeline",
            Self::Dag => "dag",
            Self::Tiered => "tiered",
        }
    }

    /// A short explanation of why this harness is recommended.
    pub const fn summary(&self) -> &'static str {
        match self {
            Self::Direct => "No harness needed; proceed directly.",
            Self::Ultrawork => "Use a lightweight retry/progress harness for execution work.",
            Self::Omo => "Use a multi-agent analysis harness for parallel review or comparison.",
            Self::Sparv => "Use a long-lived session harness for checkpointed, resumable work.",
            Self::Architect => "Use a design-then-apply harness for complex implementation work.",
            Self::Pipeline => {
                "Use a staged pipeline harness for plan/execute/verify/fix workflows."
            }
            Self::Dag => "Use a DAG harness for decomposed work with independent subtasks.",
            Self::Tiered => "Use tiered model orchestration (Musician → Editor → Composer).",
        }
    }
}

impl fmt::Display for TaskHarness {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_key())
    }
}

impl FromStr for TaskHarness {
    type Err = ();

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input.to_lowercase().as_str() {
            "direct" | "none" | "directly" => Ok(Self::Direct),
            "ultrawork" | "ultraworks" | "ultra_work" => Ok(Self::Ultrawork),
            "omo" => Ok(Self::Omo),
            "sparv" => Ok(Self::Sparv),
            "architect" | "architect-mode" | "design" | "apply" => Ok(Self::Architect),
            "pipeline" | "team" | "stage" | "staged" => Ok(Self::Pipeline),
            "dag" | "rfc" | "decompose" => Ok(Self::Dag),
            _ => Err(()),
        }
    }
}

/// The action the routing decision wants the runtime to take.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum TaskRoutingAction {
    Proceed,
    Clarify { questions: usize },
    Research { passes: usize },
    Handoff,
}

/// Context provided to team assembly that influences roster composition.
///
/// When `Some`, the team assembler uses the intent category, thinking depth,
/// and required specialists to produce a richer roster. When `None`, the
/// standard `assemble_team()` path is used unchanged.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssemblyContext {
    /// The classified intent category for the task.
    pub intent_category: IntentCategory,
    /// The reasoning depth recommended for the task.
    pub thinking_depth: TaskThinkingMode,
    /// Classification confidence (0.0–1.0).
    pub confidence: f64,
    /// Named specialists that should be added as `Scalpel` roles.
    pub required_specialists: Vec<String>,
}

impl Default for AssemblyContext {
    fn default() -> Self {
        Self {
            intent_category: IntentCategory::Implementation,
            thinking_depth: TaskThinkingMode::Standard,
            confidence: 0.5,
            required_specialists: Vec::new(),
        }
    }
}

/// A single role assignment within an assembled team roster.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentRoleAssignment {
    /// The team role (Builder, Skeptic, Judge, etc.).
    pub role: TeamRole,
    /// Optional specialization label (e.g., "security", "performance").
    pub specialization: Option<String>,
    /// Priority for budget-capped sizing (higher = kept first).
    pub priority: u8,
}

/// A structured routing decision for a task.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskRoutingDecision {
    pub intent: IntentCategory,
    pub confidence: f64,
    pub action: TaskRoutingAction,
    pub workflow: TaskWorkflow,
    #[serde(default)]
    pub harness: TaskHarness,
    #[serde(default)]
    pub thinking: TaskThinkingProfile,
    #[serde(default)]
    pub execution_plan: TaskExecutionPlan,
    pub team: TeamRole,
    pub agent: AgentRole,
    #[serde(default)]
    pub skills: Vec<String>,
    #[serde(default)]
    pub missing_info: Vec<String>,
    /// A richer roster assembled from `AssemblyContext`. Absent in legacy paths.
    #[serde(default)]
    pub roster: Option<Vec<AgentRoleAssignment>>,
}

impl TaskRoutingDecision {
    /// Render a concise human-readable summary of the decision.
    pub fn summary(&self) -> String {
        let action = match self.action {
            TaskRoutingAction::Proceed => "proceed".to_string(),
            TaskRoutingAction::Clarify { questions } => {
                format!("clarify ({} questions)", questions)
            }
            TaskRoutingAction::Research { passes } => {
                format!("research ({} passes)", passes)
            }
            TaskRoutingAction::Handoff => "handoff".to_string(),
        };

        let mut lines = vec![
            format!("- Intent: {:?}", self.intent),
            format!("- Confidence: {:.2}", self.confidence),
            format!("- Workflow: {}", self.workflow),
            format!("- Harness: {}", self.harness),
            format!("- Thinking:\n{}", self.thinking.summary()),
            format!("- Next step: {}", self.execution_plan.next_step),
            format!("- Execution plan:\n{}", self.execution_plan.summary()),
            format!("- Team: {}", self.team),
            format!("- Agent: {}", self.agent),
            format!("- Action: {}", action),
        ];

        if !self.skills.is_empty() {
            lines.push(format!("- Skills: {}", self.skills.join(", ")));
        }

        if !self.missing_info.is_empty() {
            lines.push(format!(
                "- Missing information: {}",
                self.missing_info.join(", ")
            ));
        }

        if let Some(roster) = &self.roster {
            let roles: Vec<String> = roster
                .iter()
                .map(|a| {
                    a.specialization
                        .as_deref()
                        .map(|s| format!("{}:{}", a.role, s))
                        .unwrap_or_else(|| a.role.to_string())
                })
                .collect();
            lines.push(format!("- Roster: {}", roles.join(", ")));
        }

        lines.join("\n")
    }

    /// Return the action label used in prompt text.
    pub fn action_label(&self) -> &'static str {
        match self.action {
            TaskRoutingAction::Proceed => "proceed",
            TaskRoutingAction::Clarify { .. } => "clarify",
            TaskRoutingAction::Research { .. } => "research",
            TaskRoutingAction::Handoff => "handoff",
        }
    }

    /// Produce a structured handoff payload from this decision.
    pub fn handoff_payload(&self, reason: impl Into<String>) -> TaskRoutingHandoff {
        let reason = reason.into();
        TaskRoutingHandoff {
            intent: self.intent,
            confidence: self.confidence,
            workflow: self.workflow,
            harness: self.harness,
            thinking: self.thinking,
            team: self.team,
            agent: self.agent,
            skills: self.skills.clone(),
            missing_info: self.missing_info.clone(),
            execution_plan: self.execution_plan.clone(),
            reason,
        }
    }
}

/// Structured payload used when rustytool should take over with another workflow.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskRoutingHandoff {
    pub intent: IntentCategory,
    pub confidence: f64,
    pub workflow: TaskWorkflow,
    #[serde(default)]
    pub harness: TaskHarness,
    #[serde(default)]
    pub thinking: TaskThinkingProfile,
    pub team: TeamRole,
    pub agent: AgentRole,
    #[serde(default)]
    pub skills: Vec<String>,
    #[serde(default)]
    pub missing_info: Vec<String>,
    #[serde(default)]
    pub execution_plan: TaskExecutionPlan,
    pub reason: String,
}

impl TaskRoutingHandoff {
    /// Serialize the payload as compact JSON.
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_string())
    }
}

/// Marker that wraps a routing handoff payload in assistant output.
pub const HANDOFF_START: &str = "[RUSTYTOOL_HANDOFF]";
pub const HANDOFF_END: &str = "[/RUSTYTOOL_HANDOFF]";

/// Render a full handoff block suitable for the assistant to emit.
pub fn render_handoff_block(handoff: &TaskRoutingHandoff) -> String {
    format!(
        "{start}\n{payload}\n{end}",
        start = HANDOFF_START,
        payload = handoff.to_json(),
        end = HANDOFF_END
    )
}

/// Attempt to parse a routing handoff from assistant output.
pub fn parse_handoff_block(text: &str) -> Option<TaskRoutingHandoff> {
    let start = text.find(HANDOFF_START)? + HANDOFF_START.len();
    let end = text[start..].find(HANDOFF_END)? + start;
    let payload = text[start..end].trim();
    serde_json::from_str(payload).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workflow_roundtrip() {
        for workflow in [
            TaskWorkflow::Code,
            TaskWorkflow::Debug,
            TaskWorkflow::Plan,
            TaskWorkflow::Research,
            TaskWorkflow::Test,
            TaskWorkflow::Analysis,
            TaskWorkflow::Ask,
        ] {
            let json = serde_json::to_string(&workflow).unwrap();
            let decoded: TaskWorkflow = serde_json::from_str(&json).unwrap();
            assert_eq!(decoded, workflow);
            assert_eq!(TaskWorkflow::from_str(workflow.as_key()).unwrap(), workflow);
        }
    }

    #[test]
    fn handoff_block_roundtrip() {
        let handoff = TaskRoutingHandoff {
            intent: IntentCategory::Planning,
            confidence: 0.41,
            workflow: TaskWorkflow::Plan,
            harness: TaskHarness::Pipeline,
            thinking: TaskThinkingProfile {
                depth: TaskThinkingMode::Extended,
                style: TaskThinkingStyle::PlanFirst,
                decompose_tasks: true,
                ask_clarifying_questions: true,
                assign_responsibility: true,
                define_dependencies: true,
                verify_before_acting: true,
            },
            team: TeamRole::Architect,
            agent: AgentRole::Planner,
            skills: vec!["research".into(), "design".into()],
            missing_info: vec!["constraints".into()],
            execution_plan: TaskExecutionPlan {
                next_step: "Draft the plan and decompose it into phases".into(),
                responsibilities: vec!["Primary owner: planner".into()],
                dependencies: vec!["constraints".into()],
                verification: vec!["Confirm assumptions before making changes".into()],
                steps: vec![TaskExecutionStep {
                    title: "Draft the plan and decompose it into phases".into(),
                    owner: "planner".into(),
                    depends_on: vec!["constraints".into()],
                    description: "Break the task into phased deliverables".into(),
                    verification: "Prove the next step is safe and bounded".into(),
                }],
            },
            reason: "Need more constraints".into(),
        };

        let rendered = render_handoff_block(&handoff);
        let parsed = parse_handoff_block(&rendered).unwrap();
        assert_eq!(parsed, handoff);
    }

    #[test]
    fn assembly_context_default() {
        let ctx = AssemblyContext::default();
        assert_eq!(ctx.intent_category, IntentCategory::Implementation);
        assert_eq!(ctx.thinking_depth, TaskThinkingMode::Standard);
        assert_eq!(ctx.confidence, 0.5);
        assert!(ctx.required_specialists.is_empty());
    }

    #[test]
    fn assembly_context_serde_roundtrip() {
        let ctx = AssemblyContext {
            intent_category: IntentCategory::Diagnostic,
            thinking_depth: TaskThinkingMode::Deep,
            confidence: 0.85,
            required_specialists: vec!["security".into(), "performance".into()],
        };
        let json = serde_json::to_string(&ctx).unwrap();
        let decoded: AssemblyContext = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.intent_category, ctx.intent_category);
        assert_eq!(decoded.thinking_depth, ctx.thinking_depth);
        assert!((decoded.confidence - ctx.confidence).abs() < f64::EPSILON);
        assert_eq!(decoded.required_specialists, ctx.required_specialists);
    }

    #[test]
    fn agent_role_assignment_serde_roundtrip() {
        let a = AgentRoleAssignment {
            role: TeamRole::Scalpel,
            specialization: Some("security".into()),
            priority: 0,
        };
        let json = serde_json::to_string(&a).unwrap();
        let decoded: AgentRoleAssignment = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, a);
    }

    #[test]
    fn task_routing_decision_roster_backward_compat() {
        let json = r#"{
            "intent": "Implementation",
            "confidence": 0.8,
            "action": "proceed",
            "workflow": "code",
            "team": "builder",
            "agent": "builder"
        }"#;
        let decoded: TaskRoutingDecision = serde_json::from_str(json).unwrap();
        assert!(decoded.roster.is_none());
    }

    #[test]
    fn task_routing_decision_roster_present() {
        let json = r#"{
            "intent": "Implementation",
            "confidence": 0.9,
            "action": "proceed",
            "workflow": "code",
            "team": "builder",
            "agent": "builder",
            "roster": [
                {"role": "coordinator", "specialization": null, "priority": 5},
                {"role": "builder", "specialization": null, "priority": 4},
                {"role": "scalpel", "specialization": "security", "priority": 0}
            ]
        }"#;
        let decoded: TaskRoutingDecision = serde_json::from_str(json).unwrap();
        let roster = decoded.roster.unwrap();
        assert_eq!(roster.len(), 3);
        assert_eq!(roster[0].role, TeamRole::Coordinator);
        assert_eq!(roster[2].role, TeamRole::Scalpel);
        assert_eq!(roster[2].specialization.as_deref(), Some("security"));
    }
}
