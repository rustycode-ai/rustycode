//! Team-based agent orchestration types.
//!
//! Defines the data model for a team of specialized agents that cross-check
//! each other's work, with built-in hallucination detection and trust tracking.
//!
//! # Architecture
//!
//! ```text
//! Task → TaskProfiler → TeamAssembler → Coordinator
//!                                         │
//!                              ┌──────────┼──────────┐
//!                              │          │          │
//!                           Builder    Skeptic     Judge
//!                           (writes)  (reviews)  (verifies)
//!                              │          │          │
//!                              └──────────┼──────────┘
//!                                         │
//!                                    Coordinator
//!                                    (manages, tracks trust)
//! ```

use serde::{Deserialize, Serialize};
use std::fmt;

// TASK PROFILING — assesses what team and attitude a task needs

/// Assessment of a task's characteristics. Used to assemble the right team
/// with the right attitude. Derived from signals (keywords, file patterns,
/// git context), NOT from an LLM call. Like IntentGate — fast, deterministic.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct TaskProfile {
    /// How bad would it be if this went wrong?
    pub risk: RiskLevel,
    /// How many files/systems are involved?
    pub reach: ReachLevel,
    /// Have we worked in this area before?
    pub familiarity: Familiarity,
    /// Can we undo this easily?
    pub reversibility: Reversibility,
    /// The reasoning strategy to use for this task.
    pub strategy: ReasoningStrategy,
    /// Signals that contributed to this profile (for debugging/explanation)
    pub signals: Vec<ProfileSignal>,
}

/// How much damage would a mistake cause?
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum RiskLevel {
    /// Typos, comments, docs. Getting it wrong is harmless.
    #[default]
    Low,
    /// Feature code with tests. Mistakes are caught by CI.
    Moderate,
    /// Core logic, shared modules. Bugs affect many consumers.
    High,
    /// Auth, security, data integrity, production infra.
    Critical,
}

impl RiskLevel {
    pub fn as_f64(&self) -> f64 {
        match self {
            Self::Low => 0.0,
            Self::Moderate => 0.33,
            Self::High => 0.66,
            Self::Critical => 1.0,
        }
    }
}

/// How many files/systems does this touch?
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ReachLevel {
    /// Single file, isolated change.
    #[default]
    SingleFile,
    /// 2-5 files in the same module.
    Local,
    /// Many files across multiple modules.
    Wide,
    /// Cross-cutting change affecting the whole codebase.
    SystemWide,
}

impl ReachLevel {
    pub fn file_count_range(&self) -> (usize, usize) {
        match self {
            Self::SingleFile => (1, 1),
            Self::Local => (2, 5),
            Self::Wide => (6, 20),
            Self::SystemWide => (21, usize::MAX),
        }
    }
}

/// How well do we know this area of the codebase?
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Familiarity {
    /// We've worked here many times. High confidence.
    #[default]
    WellKnown,
    /// We've touched this area a few times.
    SomewhatKnown,
    /// Never been here. Could be anything.
    Unknown,
}

/// Can we undo this easily?
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Reversibility {
    /// git revert and we're fine.
    #[default]
    Easy,
    /// Revert is possible but might need cleanup.
    Moderate,
    /// Can't be fully undone (data loss, external effects).
    Hard,
    /// Permanent (deleted data, published releases).
    Irreversible,
}

/// Reasoning strategy for task execution.
///
/// Determines how the team approaches a task based on its characteristics.
/// Inspired by AutoAgent's Evolving Cognition and Superpowers' Enforceable Workflows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ReasoningStrategy {
    /// Analytical approach: Performance tuning, in-depth review
    Analytical,
    /// Diagnostic approach: Troubleshooting, fixing existing issues
    Diagnostic,
    /// Plan-first approach: Architect → Plan → Execute
    /// Used for: High-risk, unfamiliar, complex tasks
    #[default]
    PlanFirst,
    /// Act-first approach: Skip Architect, dive into Builder
    /// Used for: Low-risk, familiar, simple tasks
    ActFirst,
    /// Reflect-first approach: Analyze before acting
    /// Used for: Debugging, investigation, troubleshooting tasks
    ReflectFirst,
    /// Parallel approach: Multi-agent execution of independent subtasks
    /// Used for: Tasks with clear independent components
    Parallel,
    /// TDD approach: Test-first (Red → Green → Refactor)
    /// Used for: Feature development with clear requirements
    TDD,
}

impl ReasoningStrategy {
    /// Determine the appropriate strategy from task profile and context.
    pub fn from_task_profile(profile: &TaskProfile, task: &str) -> Self {
        let task_lower = task.to_lowercase();

        // Check for analytical tasks
        let analytical_keywords = ["performance", "optimize", "analyze", "review", "audit"];
        if analytical_keywords.iter().any(|kw| task_lower.contains(kw)) {
            return ReasoningStrategy::Analytical;
        }

        // Check for diagnostic tasks
        let diagnostic_keywords = [
            "fix the bug",
            "fix",
            "debug",
            "investigate",
            "why",
            "broken",
            "failing",
            "not working",
        ];
        if diagnostic_keywords.iter().any(|kw| task_lower.contains(kw)) {
            return ReasoningStrategy::Diagnostic;
        }

        // Check for TDD-appropriate tasks (feature requests with clear scope)
        let tdd_keywords = ["add", "implement", "feature", "create", "new"];
        let is_feature_request = tdd_keywords.iter().any(|kw| task_lower.contains(kw));

        if is_feature_request
            && profile.risk == RiskLevel::Low
            && matches!(profile.reach, ReachLevel::SingleFile | ReachLevel::Local)
        {
            return ReasoningStrategy::TDD;
        }

        // Check for simple, familiar tasks
        if profile.risk == RiskLevel::Low
            && matches!(profile.familiarity, Familiarity::WellKnown)
            && matches!(profile.reach, ReachLevel::SingleFile)
        {
            return ReasoningStrategy::ActFirst;
        }

        // High-risk or critical tasks need planning
        if profile.risk == RiskLevel::High || profile.risk == RiskLevel::Critical {
            return ReasoningStrategy::PlanFirst;
        }

        // Default to plan-first for unknown scenarios
        ReasoningStrategy::PlanFirst
    }

    /// Whether this strategy skips the Architect phase.
    pub fn skips_architect(&self) -> bool {
        matches!(self, Self::ActFirst | Self::TDD)
    }

    /// Whether this strategy requires special execution flow.
    pub fn requires_special_flow(&self) -> bool {
        matches!(self, Self::ReflectFirst | Self::Parallel | Self::TDD)
    }
}

/// A signal that contributed to the task profile. For debugging and explanation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProfileSignal {
    /// What was detected.
    pub kind: SignalKind,
    /// The evidence that triggered this signal.
    pub evidence: String,
    /// How much weight this signal carries (0.0-1.0).
    pub weight: f64,
}

/// Kinds of profiling signals.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum SignalKind {
    /// Keyword in the task description (e.g., "auth", "security", "refactor").
    Keyword,
    /// File path pattern (e.g., files in auth/, config/).
    PathPattern,
    /// Number of files that would be touched.
    FileCount,
    /// Whether tests exist near the change area.
    TestCoverage,
    /// User's own framing ("just a quick..." vs "be careful...").
    UserFraming,
    /// Past trust score with similar tasks.
    HistoricalTrust,
    /// Whether the area is in a critical path (build, CI, deploy).
    CriticalPath,
}

// TEAM COMPOSITION — who's on the team and how they behave

/// Which agent roles are active and with what attitude.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamConfig {
    /// The roles active for this task.
    pub roles: Vec<TeamRole>,
    /// The attitude each role should adopt.
    pub attitude: AgentAttitude,
    /// Who is the current builder (for rotation tracking).
    pub builder_generation: u32,
}

/// The roles in the team. Each has a distinct job and perspective.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum TeamRole {
    /// Writes code. Optimistic, moves fast. Can hallucinate APIs.
    Builder,
    /// Reviews code. Never writes. Checks claims against reality.
    Skeptic,
    /// Runs tests, compilation, file checks. Pure empirical evidence.
    Judge,
    /// Manages the team. Tracks trust, detects stuck/degrading, escalates.
    Coordinator,
    /// Plans structure before any code is written. Read-only access.
    /// Produces a StructuralDeclaration that constrains all Builder work.
    Architect,
    /// Makes targeted surgical fixes after Judge failures. No redesign.
    Scalpel,
}

impl TeamRole {
    /// Whether this role can modify files.
    pub fn can_write(&self) -> bool {
        matches!(self, Self::Builder | Self::Scalpel)
    }

    /// Whether this role should see the builder's reasoning (vs just the diff).
    pub fn sees_reasoning(&self) -> bool {
        matches!(self, Self::Coordinator)
    }

    /// Whether this role is read-only (cannot modify files).
    pub fn is_read_only(&self) -> bool {
        matches!(self, Self::Architect | Self::Skeptic | Self::Coordinator)
    }

    /// Which tools this role is allowed to use.
    pub fn allowed_tools(&self) -> ToolSet {
        match self {
            Self::Builder => ToolSet::All,
            Self::Scalpel => ToolSet::TargetedFix,
            Self::Skeptic => ToolSet::ReadOnly,
            Self::Judge => ToolSet::VerificationOnly,
            Self::Coordinator => ToolSet::ReadOnly,
            Self::Architect => ToolSet::ReadOnly,
        }
    }
}

/// Tool access level for a role.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ToolSet {
    /// Can use all tools.
    All,
    /// Can only read files, grep, glob, lsp.
    ReadOnly,
    /// Can only run tests, compilation, file existence checks.
    VerificationOnly,
    /// Can read files and make targeted edits (scalpel — surgical fixes only).
    TargetedFix,
}

// AGENT ATTITUDE — how strict/helpful/adversarial each agent should be

/// Configurable attitude for an agent. Not binary — multiple knobs that shift
/// based on task profile and trust dynamics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentAttitude {
    /// How much evidence is required before acting.
    pub burden_of_proof: BurdenOfProof,
    /// How deeply to examine claims.
    pub review_depth: ReviewDepth,
    /// How to deliver feedback.
    pub tone: FeedbackTone,
    /// How many failures before escalating.
    pub patience: u32,
    /// Whether to pre-check before applying changes.
    pub pre_flight: bool,
    /// What happens on veto.
    pub on_veto: VetoAction,
}

impl Default for AgentAttitude {
    fn default() -> Self {
        Self::standard()
    }
}

impl AgentAttitude {
    /// Light attitude for low-risk, well-known work.
    pub fn light() -> Self {
        Self {
            burden_of_proof: BurdenOfProof::Light,
            review_depth: ReviewDepth::Surface,
            tone: FeedbackTone::Encouraging,
            patience: 5,
            pre_flight: false,
            on_veto: VetoAction::RetryWithFeedback,
        }
    }

    /// Standard attitude for moderate-risk work.
    pub fn standard() -> Self {
        Self {
            burden_of_proof: BurdenOfProof::Standard,
            review_depth: ReviewDepth::Normal,
            tone: FeedbackTone::Neutral,
            patience: 3,
            pre_flight: false,
            on_veto: VetoAction::RetryWithFeedback,
        }
    }

    /// Strict attitude for high-risk or unfamiliar work.
    pub fn strict() -> Self {
        Self {
            burden_of_proof: BurdenOfProof::BeyondReasonableDoubt,
            review_depth: ReviewDepth::Deep,
            tone: FeedbackTone::Adversarial,
            patience: 2,
            pre_flight: true,
            on_veto: VetoAction::EscalateToUser,
        }
    }

    /// Crisis attitude when builder has been caught hallucinating.
    pub fn crisis() -> Self {
        Self {
            burden_of_proof: BurdenOfProof::BeyondReasonableDoubt,
            review_depth: ReviewDepth::Forensic,
            tone: FeedbackTone::Adversarial,
            patience: 1,
            pre_flight: true,
            on_veto: VetoAction::EscalateToUser,
        }
    }

    /// Tighten attitude by one level (e.g., after a failure).
    pub fn tighten(&mut self) {
        self.burden_of_proof = match self.burden_of_proof {
            BurdenOfProof::Light => BurdenOfProof::Standard,
            BurdenOfProof::Standard => BurdenOfProof::BeyondReasonableDoubt,
            BurdenOfProof::BeyondReasonableDoubt => BurdenOfProof::BeyondReasonableDoubt,
        };
        self.review_depth = match self.review_depth {
            ReviewDepth::Surface => ReviewDepth::Normal,
            ReviewDepth::Normal => ReviewDepth::Deep,
            ReviewDepth::Deep => ReviewDepth::Forensic,
            ReviewDepth::Forensic => ReviewDepth::Forensic,
        };
        self.patience = self.patience.saturating_sub(1).max(1);
        self.pre_flight = true;
    }

    /// Relax attitude by one level (e.g., after consistent success).
    pub fn relax(&mut self) {
        self.burden_of_proof = match self.burden_of_proof {
            BurdenOfProof::Light => BurdenOfProof::Light,
            BurdenOfProof::Standard => BurdenOfProof::Light,
            BurdenOfProof::BeyondReasonableDoubt => BurdenOfProof::Standard,
        };
        self.tone = match self.tone {
            FeedbackTone::Adversarial => FeedbackTone::Neutral,
            FeedbackTone::Neutral => FeedbackTone::Encouraging,
            FeedbackTone::Encouraging => FeedbackTone::Encouraging,
        };
        self.patience = (self.patience + 1).min(5);
    }
}

/// How much evidence is required before the builder can act.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum BurdenOfProof {
    /// Act first, verify later.
    Light,
    /// Show your reasoning.
    #[default]
    Standard,
    /// Prove every claim with evidence.
    BeyondReasonableDoubt,
}

/// How deeply the skeptic reviews.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ReviewDepth {
    /// Check the diff looks reasonable.
    Surface,
    /// Check logic, imports, types.
    #[default]
    Normal,
    /// Verify every referenced symbol exists.
    Deep,
    /// Full forensic: symbol resolution, pattern consistency, edge cases.
    Forensic,
}

/// How the skeptic delivers feedback.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum FeedbackTone {
    /// "Looks good, just this one thing..."
    Encouraging,
    /// "Line 42 has a bug."
    #[default]
    Neutral,
    /// "Prove that this import resolves. Show me the file."
    Adversarial,
}

/// What happens when the skeptic vetoes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum VetoAction {
    /// Give feedback, let the builder retry.
    #[default]
    RetryWithFeedback,
    /// Builder must get pre-approval before applying changes.
    SupervisedRetry,
    /// Stop and ask the user.
    EscalateToUser,
}

// TRUST TRACKING — how much do we trust each agent

/// Trust score for an agent. Starts at 0.7 (moderate trust), adjusts based
/// on outcomes. This is NOT about the LLM — it's about whether this specific
/// builder instance is producing reliable work in THIS conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustScore {
    /// 0.0 = hallucinating, 1.0 = fully trusted.
    pub value: f64,
    /// What caused trust changes.
    pub history: Vec<TrustEvent>,
}

impl Default for TrustScore {
    fn default() -> Self {
        Self {
            value: 0.7,
            history: Vec::new(),
        }
    }
}

impl TrustScore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record an event and adjust trust accordingly.
    pub fn record(&mut self, event: TrustEvent) {
        let delta = event.delta();
        self.value = (self.value + delta).clamp(0.0, 1.0);
        self.history.push(event);
    }

    /// Is this agent trusted enough to work autonomously?
    pub fn is_autonomous(&self) -> bool {
        self.value >= 0.5
    }

    /// Should this agent be supervised?
    pub fn needs_supervision(&self) -> bool {
        self.value < 0.5 && self.value >= 0.25
    }

    /// Should we stop and escalate?
    pub fn should_escalate(&self) -> bool {
        self.value < 0.25
    }
}

/// An event that affects trust between agents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustEvent {
    /// What happened.
    pub kind: TrustEventKind,
    /// When it happened.
    pub turn: u32,
    /// Brief description for debugging.
    pub note: String,
}

impl TrustEvent {
    /// How much this event changes trust (-1.0 to +1.0).
    pub fn delta(&self) -> f64 {
        self.kind.delta()
    }
}

/// Kinds of trust-affecting events.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum TrustEventKind {
    /// Builder's claim was verified as correct.
    ClaimVerified,
    /// Builder's claim was proven false (hallucination).
    ClaimRefuted,
    /// Builder completed a task without issues.
    TaskCompleted,
    /// Builder's fix made tests pass.
    FixVerified,
    /// Builder tried the same approach that already failed.
    RepeatedFailure,
    /// Skeptic caught a hallucination.
    HallucinationCaught,
    /// Builder produced code that doesn't compile.
    CompilationFailed,
    /// Builder introduced new test failures.
    RegressionsIntroduced,
}

impl TrustEventKind {
    pub fn delta(&self) -> f64 {
        match self {
            Self::ClaimVerified => 0.05,
            Self::ClaimRefuted => -0.15,
            Self::TaskCompleted => 0.10,
            Self::FixVerified => 0.08,
            Self::RepeatedFailure => -0.10,
            Self::HallucinationCaught => -0.20,
            Self::CompilationFailed => -0.05,
            Self::RegressionsIntroduced => -0.15,
        }
    }
}

// BRIEFING — the structured context that replaces raw conversation history

/// A structured briefing rebuilt from disk every turn. This is what an agent
/// sees — NOT the raw conversation transcript. Fresh mind, curated context.
///
/// Key principle: state lives on disk, not in context. The briefing is
/// reconstructed from the current state of the world, not from memory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Briefing {
    /// What we're doing. Never changes during a task.
    pub task: String,
    /// Current state of the code (refreshed from disk every turn).
    pub relevant_code: Vec<FileSnippet>,
    /// Compressed history of attempts (max 5, oldest dropped).
    pub attempts: Vec<AttemptSummary>,
    /// Key insights discovered so far (grows, but curated).
    pub insights: Vec<String>,
    /// What we're trying right now (single approach).
    pub current_approach: String,
    /// Constraints that never change.
    pub constraints: Vec<String>,
    /// The latest test/compilation results (from Judge).
    pub verification_state: Option<VerificationState>,
    /// Architectural contract from Architect phase. None until Architect has run.
    /// Once set, Builder must implement only declared modules.
    pub structural_declaration: Option<StructuralDeclaration>,
    /// Project-specific learnings from past tasks (user preferences, codebase quirks, etc.).
    pub learnings: String,
    /// Few-shot examples: similar past tasks and their solutions (for in-context learning).
    pub few_shot_examples: String,
}

impl Briefing {
    /// Create a minimal briefing for a new task.
    pub fn new(task: impl Into<String>) -> Self {
        Self {
            task: task.into(),
            relevant_code: Vec::new(),
            attempts: Vec::new(),
            insights: Vec::new(),
            current_approach: String::new(),
            constraints: Vec::new(),
            verification_state: None,
            structural_declaration: None,
            learnings: String::new(),
            few_shot_examples: String::new(),
        }
    }

    /// Add an attempt summary, dropping the oldest if we exceed 5.
    pub fn push_attempt(&mut self, summary: AttemptSummary) {
        if self.attempts.len() >= 5 {
            self.attempts.remove(0);
        }
        self.attempts.push(summary);
    }
}

/// A snapshot of file content relevant to the current task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileSnippet {
    /// File path relative to project root.
    pub path: String,
    /// The content (may be truncated).
    pub content: String,
    /// Line range if truncated.
    pub line_range: Option<(usize, usize)>,
}

/// Compressed summary of a single attempt — NOT the raw transcript.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttemptSummary {
    /// What approach was tried.
    pub approach: String,
    /// Files that were changed.
    pub files_changed: Vec<String>,
    /// Why it failed (or "success").
    pub outcome: AttemptOutcome,
    /// One-line root cause.
    pub root_cause: String,
    /// Which builder generation made this attempt.
    pub builder_generation: u32,
}

/// What happened with an attempt.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum AttemptOutcome {
    /// Tests pass, task complete.
    Success,
    /// Tests failed.
    TestFailure,
    /// Code doesn't compile.
    CompilationError,
    /// Skeptic caught a problem.
    Vetoed(String),
    /// Approach didn't address the task.
    WrongApproach,
    /// Timed out.
    Timeout,
}

/// Current verification state from the Judge.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationState {
    /// Does the code compile?
    pub compiles: bool,
    /// How many tests pass vs fail.
    pub tests: TestSummary,
    /// Files that have been modified since last verification.
    pub dirty_files: Vec<String>,
}

/// Summary of test results.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TestSummary {
    #[serde(default)]
    pub total: usize,
    #[serde(default)]
    pub passed: usize,
    #[serde(default)]
    pub failed: usize,
    #[serde(default)]
    pub failed_names: Vec<String>,
}

// APPROACH FINGERPRINT — for doom loop detection at the strategy level

/// A fingerprint of the *strategy* being used, not the exact tool calls.
/// Used to detect when the builder is repeating an approach it already tried.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApproachFingerprint {
    /// Normalized description of the approach (e.g., "edit auth.rs validate_token function").
    pub strategy: String,
    /// Files that would be modified.
    pub target_files: Vec<String>,
    /// The approach category.
    pub category: ApproachCategory,
}

impl ApproachFingerprint {
    /// Create a fingerprint from a strategy description and target files.
    pub fn new(strategy: impl Into<String>, target_files: Vec<String>) -> Self {
        let strategy = strategy.into();
        let category = ApproachCategory::classify(&strategy);
        Self {
            strategy,
            target_files,
            category,
        }
    }

    /// Check if two fingerprints represent the same approach.
    pub fn is_same_approach(&self, other: &Self) -> bool {
        // Same category AND overlapping target files = same approach
        if self.category != other.category {
            return false;
        }
        let overlap = self
            .target_files
            .iter()
            .any(|f| other.target_files.contains(f));
        overlap
    }
}

/// High-level categorization of an approach.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ApproachCategory {
    /// Adding new code (new function, new file, new module).
    Addition,
    /// Modifying existing code (edit, refactor, fix).
    Modification,
    /// Removing code (delete, simplify).
    Removal,
    /// Changing configuration (Cargo.toml, .env, settings).
    Configuration,
    /// Investigation only (read, grep, search).
    Investigation,
}

impl ApproachCategory {
    /// Classify an approach from its description.
    pub fn classify(strategy: &str) -> Self {
        let lower = strategy.to_lowercase();
        if lower.contains("add")
            || lower.contains("create")
            || lower.contains("new")
            || lower.contains("implement")
        {
            Self::Addition
        } else if lower.contains("delete") || lower.contains("remove") || lower.contains("clean") {
            Self::Removal
        } else if lower.contains("config")
            || lower.contains("cargo")
            || lower.contains("toml")
            || lower.contains("settings")
        {
            Self::Configuration
        } else if lower.contains("read")
            || lower.contains("Grep")
            || lower.contains("search")
            || lower.contains("find")
            || lower.contains("investigate")
        {
            Self::Investigation
        } else {
            Self::Modification
        }
    }
}

// TEAM LOOP STATE — the coordinator's view of the team's progress

/// The coordinator's view of where the team is in the execution loop.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamLoopState {
    /// Current turn number.
    pub turn: u32,
    /// Trust score for the current builder.
    pub builder_trust: TrustScore,
    /// Approach fingerprints from previous attempts.
    pub previous_approaches: Vec<ApproachFingerprint>,
    /// Delta tracking: is the situation improving or getting worse?
    pub progress_delta: ProgressDelta,
    /// Current team config (may evolve during execution).
    pub team_config: TeamConfig,
    /// Whether the loop should stop and why.
    pub stop_reason: Option<StopReason>,
    /// Whether the loop should escalate to the user.
    pub escalation: Option<Escalation>,
    /// Tests passed count from the last judge verification (for computing deltas).
    pub last_tests_passed: Option<usize>,
}

impl TeamLoopState {
    /// Create initial state for a task.
    pub fn new(team_config: TeamConfig) -> Self {
        Self {
            turn: 0,
            builder_trust: TrustScore::new(),
            previous_approaches: Vec::new(),
            progress_delta: ProgressDelta::new(),
            team_config,
            stop_reason: None,
            escalation: None,
            last_tests_passed: None,
        }
    }

    /// Check if we should stop, based on all stopping conditions.
    pub fn check_stop_conditions(&mut self) {
        // Already have a stop reason
        if self.stop_reason.is_some() {
            return;
        }

        // Trust too low — escalate
        if self.builder_trust.should_escalate() {
            self.stop_reason = Some(StopReason::TrustExhausted);
            self.escalation = Some(Escalation::trust_exhausted(
                self.builder_trust.value,
                self.builder_trust.history.last().cloned(),
            ));
            return;
        }

        // Delta negative for 2+ consecutive turns
        if self.progress_delta.is_degrading() {
            self.stop_reason = Some(StopReason::Degrading);
            self.escalation = Some(Escalation::degrading(
                self.progress_delta.recent_deltas().to_vec(),
            ));
            return;
        }

        // Builder needs supervision — tighten attitude
        if self.builder_trust.needs_supervision() {
            self.team_config.attitude.tighten();
        }
    }

    /// Check if the builder is repeating an approach.
    /// Requires at least 2 prior occurrences of the same approach category
    /// targeting overlapping files before declaring a doom loop. A single
    /// retry with the same file is normal; 3+ indicates a stuck pattern.
    pub fn is_repeating_approach(&self, fingerprint: &ApproachFingerprint) -> bool {
        let match_count = self
            .previous_approaches
            .iter()
            .filter(|prev| prev.is_same_approach(fingerprint))
            .count();
        match_count >= 2
    }

    /// Record a completed turn.
    pub fn record_turn(&mut self, fingerprint: ApproachFingerprint, tests_delta: i32) {
        self.turn = self.turn.saturating_add(1);
        self.previous_approaches.push(fingerprint);
        self.progress_delta.push(tests_delta);
    }
}

/// Why the team loop stopped.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum StopReason {
    /// All tests pass, task complete.
    TaskComplete,
    /// Trust score dropped below threshold.
    TrustExhausted,
    /// Situation is getting worse over multiple turns.
    Degrading,
    /// Builder repeated the same approach.
    DoomLoop,
    /// Budget (tokens/time) exhausted.
    BudgetExhausted,
    /// User requested stop.
    UserStop,
}

/// Progress tracking: are things getting better or worse?
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProgressDelta {
    /// History of test count changes per turn.
    deltas: Vec<i32>,
}

impl ProgressDelta {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a delta (positive = tests fixed, negative = new failures).
    pub fn push(&mut self, delta: i32) {
        if self.deltas.len() >= 10 {
            self.deltas.remove(0);
        }
        self.deltas.push(delta);
    }

    /// Get recent deltas.
    pub fn recent_deltas(&self) -> &[i32] {
        &self.deltas
    }

    /// Are things getting worse for 2+ consecutive turns?
    pub fn is_degrading(&self) -> bool {
        let recent: &[i32] = &self.deltas;
        if recent.len() < 2 {
            return false;
        }
        let last_two = &recent[recent.len() - 2..];
        last_two.iter().all(|d| *d < 0)
    }

    /// Are things improving?
    pub fn is_improving(&self) -> bool {
        let recent: &[i32] = &self.deltas;
        if recent.is_empty() {
            return false;
        }
        recent.last().is_some_and(|d| *d > 0)
    }
}

/// An escalation to the user. Always brings evidence, options, and a recommendation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Escalation {
    /// What level of escalation this is.
    pub level: EscalationLevel,
    /// What we tried and what happened.
    pub evidence: String,
    /// What we think the options are.
    pub options: Vec<EscalationOption>,
    /// What we recommend.
    pub recommendation: usize,
}

impl Escalation {
    /// Builder trust exhausted.
    pub fn trust_exhausted(trust: f64, last_event: Option<TrustEvent>) -> Self {
        let evidence = format!(
            "Builder trust dropped to {:.2}. Last event: {}",
            trust,
            last_event
                .as_ref()
                .map(|e| e.note.as_str())
                .unwrap_or("none")
        );
        Self {
            level: EscalationLevel::Level3,
            evidence,
            options: vec![
                EscalationOption {
                    label: "Reset with fresh builder".to_string(),
                    description: "Start with a new builder instance, carrying only the briefing."
                        .to_string(),
                },
                EscalationOption {
                    label: "Take over manually".to_string(),
                    description: "You provide direction and the team executes your plan."
                        .to_string(),
                },
                EscalationOption {
                    label: "Abort task".to_string(),
                    description: "Stop here. Revert to last known good state.".to_string(),
                },
            ],
            recommendation: 0,
        }
    }

    /// Situation is degrading.
    pub fn degrading(recent_deltas: Vec<i32>) -> Self {
        let evidence = format!(
            "Progress is going backwards. Recent test deltas: {:?}",
            recent_deltas
        );
        Self {
            level: EscalationLevel::Level2,
            evidence,
            options: vec![
                EscalationOption {
                    label: "Try different strategy".to_string(),
                    description: "Coordinator picks a new approach based on what failed."
                        .to_string(),
                },
                EscalationOption {
                    label: "Narrow the scope".to_string(),
                    description: "Break the task into smaller pieces and tackle one at a time."
                        .to_string(),
                },
                EscalationOption {
                    label: "Provide guidance".to_string(),
                    description: "Tell me which direction to go and I'll follow.".to_string(),
                },
            ],
            recommendation: 1,
        }
    }
}

/// Escalation levels. Higher = more user involvement needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum EscalationLevel {
    /// Coordinator handles it (not shown to user).
    Level1,
    /// Ask user for a decision with options.
    Level2,
    /// Ask user for direction (out of ideas).
    Level3,
}

/// An option presented to the user during escalation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EscalationOption {
    /// Short label for the option.
    pub label: String,
    /// What this option means.
    pub description: String,
}

// TEAM ASSEMBLY — how profiles map to teams

impl TaskProfile {
    /// Assemble the right team for this profile.
    pub fn assemble_team(&self) -> TeamConfig {
        self.assemble_team_with_context(None)
    }

    /// Assemble the right team, optionally using `AssemblyContext` to enrich
    /// the roster with specialists, architects, and budget-capped sizing.
    pub fn assemble_team_with_context(
        &self,
        context: Option<&crate::task_routing::AssemblyContext>,
    ) -> TeamConfig {
        let mut attitude = match (self.risk, self.reach, self.familiarity) {
            // Critical risk — always strict
            (RiskLevel::Critical, _, _) => AgentAttitude::strict(),
            // High risk or wide reach — strict
            (RiskLevel::High, _, _) | (_, ReachLevel::SystemWide, _) => AgentAttitude::strict(),
            // Unknown territory — strict
            (_, _, Familiarity::Unknown) => AgentAttitude::strict(),
            // Moderate risk, local reach — standard
            (RiskLevel::Moderate, ReachLevel::Local | ReachLevel::SingleFile, _) => {
                AgentAttitude::standard()
            }
            // Default to standard
            _ => AgentAttitude::standard(),
        };

        // Adjust attitude based on strategy
        match self.strategy {
            ReasoningStrategy::Analytical => attitude.tighten(),
            ReasoningStrategy::Diagnostic => attitude.tighten(),
            _ => {}
        }

        let mut roles = vec![TeamRole::Coordinator];

        // Architect inclusion for analytical/complex tasks
        if matches!(self.strategy, ReasoningStrategy::Analytical) {
            roles.push(TeamRole::Architect);
        }

        // Always need a builder for anything beyond explanation
        if self.risk != RiskLevel::Low || self.reach != ReachLevel::SingleFile {
            roles.push(TeamRole::Builder);
            roles.push(TeamRole::Judge);
        } else {
            // Light: builder only, no skeptic or judge
            roles.push(TeamRole::Builder);
            return TeamConfig {
                roles,
                attitude,
                builder_generation: 0,
            };
        }

        // Add skeptic for moderate+ risk or diagnostic
        if self.risk >= RiskLevel::Moderate
            || matches!(self.strategy, ReasoningStrategy::Diagnostic)
        {
            roles.push(TeamRole::Skeptic);
        }

        if let Some(ctx) = context {
            if matches!(
                ctx.thinking_depth,
                crate::task_routing::TaskThinkingMode::Deep
                    | crate::task_routing::TaskThinkingMode::Extended
            ) && !roles.contains(&TeamRole::Architect)
            {
                roles.push(TeamRole::Architect);
            }

            for _spec in &ctx.required_specialists {
                roles.push(TeamRole::Scalpel);
            }
        }

        TeamConfig {
            roles,
            attitude,
            builder_generation: 0,
        }
    }
}

/// Priority value used for budget-capped team sizing.
/// Higher priority roles are retained first when trimming.
pub const fn role_priority(role: TeamRole) -> u8 {
    match role {
        TeamRole::Coordinator => 5,
        TeamRole::Builder => 4,
        TeamRole::Judge => 3,
        TeamRole::Architect => 2,
        TeamRole::Skeptic => 1,
        TeamRole::Scalpel => 0,
    }
}

/// Build a roster of `AgentRoleAssignment`s from a `TeamConfig` and optional
/// `AssemblyContext`, capped to `max_size` by priority.
pub fn build_roster(
    config: &TeamConfig,
    context: Option<&crate::task_routing::AssemblyContext>,
    max_size: usize,
) -> Vec<crate::task_routing::AgentRoleAssignment> {
    use crate::task_routing::AgentRoleAssignment;

    let mut assignments: Vec<AgentRoleAssignment> = config
        .roles
        .iter()
        .filter(|r| !matches!(r, TeamRole::Scalpel))
        .map(|&role| AgentRoleAssignment {
            role,
            specialization: None,
            priority: role_priority(role),
        })
        .collect();

    if let Some(ctx) = context {
        for spec in &ctx.required_specialists {
            assignments.push(AgentRoleAssignment {
                role: TeamRole::Scalpel,
                specialization: Some(spec.clone()),
                priority: role_priority(TeamRole::Scalpel),
            });
        }
    }

    assignments.sort_by(|a, b| b.priority.cmp(&a.priority));
    assignments.truncate(max_size);
    assignments
}

impl fmt::Display for RiskLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Low => write!(f, "low"),
            Self::Moderate => write!(f, "moderate"),
            Self::High => write!(f, "high"),
            Self::Critical => write!(f, "critical"),
        }
    }
}

impl fmt::Display for TeamRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Builder => write!(f, "builder"),
            Self::Skeptic => write!(f, "skeptic"),
            Self::Judge => write!(f, "judge"),
            Self::Coordinator => write!(f, "coordinator"),
            Self::Architect => write!(f, "architect"),
            Self::Scalpel => write!(f, "scalpel"),
        }
    }
}

// STRUCTURED TURNS — typed agent outputs, not free-form prose

/// A single file change with a diff summary, not the full diff.
///
/// This is the token-efficient way to communicate "what changed" between
/// agents. The skeptic sees the summary + hunk (not the full file), and the
/// judge sees only paths + line counts.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FileChange {
    /// Path relative to project root.
    pub path: String,
    /// One-line human summary: "added fn validate_jwt()", "fixed null check on line 42".
    #[serde(default)]
    pub summary: String,
    /// Unified diff hunk — only changed lines with ±3 lines of context.
    #[serde(default)]
    pub diff_hunk: String,
    /// Lines added (approximate, from diff).
    #[serde(default)]
    pub lines_added: usize,
    /// Lines removed (approximate, from diff).
    #[serde(default)]
    pub lines_removed: usize,
}

/// Builder's structured response — facts, not reasoning.
///
/// The builder produces this after each turn. The coordinator uses it to:
/// - Build an approach fingerprint (from `approach` + `changes`)
/// - Feed the skeptic (claims to verify)
/// - Feed the judge (files to compile/test)
///
/// The builder's *reasoning* goes to disk (`.rustycode/session/{id}/reasoning/`),
/// NOT into the context of other agents.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BuilderTurn {
    /// What approach was taken, in one line.
    #[serde(default)]
    pub approach: String,
    /// Files changed, with summaries and diff hunks.
    #[serde(default)]
    pub changes: Vec<FileChange>,
    /// What the builder claims to have accomplished.
    #[serde(default)]
    pub claims: Vec<String>,
    /// Confidence level 0.0–1.0.
    #[serde(default = "default_confidence")]
    pub confidence: f64,
    /// Whether the builder considers the task done.
    #[serde(default)]
    pub done: bool,
    /// Optional: request escalation to another phase (e.g., "Architect", "SecurityReview").
    /// Set when Builder encounters structural uncertainty or security-sensitive changes.
    #[serde(default)]
    pub escalation: Option<super::agent_protocol::EscalationRequest>,
}

impl BuilderTurn {
    /// Check if this Builder turn requests escalation.
    pub fn needs_escalation(&self) -> bool {
        self.escalation.is_some()
    }

    /// Get the escalation target if requested.
    pub fn escalation_target(&self) -> Option<super::agent_protocol::EscalationTarget> {
        self.escalation.as_ref().map(|e| e.target)
    }

    /// Request escalation to Architect.
    pub fn request_architect(reason: impl Into<String>) -> Self {
        Self {
            approach: String::new(),
            changes: vec![],
            claims: vec![],
            confidence: 0.5,
            done: false,
            escalation: Some(super::agent_protocol::EscalationRequest {
                target: super::agent_protocol::EscalationTarget::Architect,
                reason: reason.into(),
                question: None,
            }),
        }
    }
}

/// A claim the skeptic refuted, with evidence.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RefutedClaim {
    /// The original claim.
    pub claim: String,
    /// What's actually on disk / in the code.
    pub evidence: String,
}

/// Skeptic's structured response — verdict + evidence, no opinions.
///
/// The skeptic reviews the builder's claims and diffs, NOT the builder's
/// reasoning. This prevents the skeptic from being influenced by the
/// builder's confidence or argumentation style.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SkepticTurn {
    /// Overall verdict.
    #[serde(default)]
    pub verdict: SkepticVerdict,
    /// Claims verified against the actual code.
    #[serde(default)]
    pub verified: Vec<String>,
    /// Claims refuted with evidence from disk.
    #[serde(default)]
    pub refuted: Vec<RefutedClaim>,
    /// New insights discovered during review.
    #[serde(default)]
    pub insights: Vec<String>,
}

/// The skeptic's verdict on a builder's turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
#[derive(Default)]
pub enum SkepticVerdict {
    /// Claims verified, code looks correct.
    Approve,
    /// Some claims need revision, but no hallucination.
    #[default]
    NeedsWork,
    /// Hallucination or critical issue detected — hard stop.
    Veto,
}

/// Judge's structured response — empirical results only.
///
/// The judge doesn't read claims or opinions. It runs tests and compilation
/// and reports facts. This is the ground truth that the coordinator uses to
/// decide progress.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct JudgeTurn {
    /// Does the code compile?
    #[serde(default)]
    pub compiles: bool,
    /// Test results.
    #[serde(default)]
    pub tests: TestSummary,
    /// Files that were modified since last judge run.
    #[serde(default)]
    pub dirty_files: Vec<String>,
    /// Compilation errors (if any), truncated to first 10 lines.
    #[serde(default)]
    pub compile_errors: Vec<String>,
}

// STRUCTURAL DECLARATION — the Architect's binding contract

/// The Architect's output — a binding contract for all subsequent Builder work.
///
/// Once produced, the Builder may only touch declared modules using declared deps.
/// The Skeptic enforces structural compliance. The Scalpel is exempt (targeted fixes only).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct StructuralDeclaration {
    /// Modules to create or modify, with explicit exports and imports.
    pub modules: Vec<ModuleDeclaration>,
    /// Traits/interfaces shared across modules — defined once, used everywhere.
    pub interfaces: Vec<InterfaceDeclaration>,
    /// Cargo.toml dependency changes — decided before any code is written.
    #[serde(default)]
    pub dependencies: DependencyChanges,
}

/// A single module the Builder will create or modify.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ModuleDeclaration {
    /// Relative path from crate root, e.g. "src/team/architect.rs"
    pub path: String,
    /// Whether to create this file fresh or modify an existing one.
    pub action: ModuleAction,
    /// Public symbols this module exports (structs, enums, fns, traits).
    pub exports: Vec<String>,
    /// Import paths this module will use (e.g. "anyhow::Result").
    pub imports: Vec<String>,
    /// One-line purpose statement — prevents scope creep.
    pub purpose: String,
}

/// Whether the Architect is creating a new module or modifying an existing one.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum ModuleAction {
    #[default]
    Create,
    Modify,
}

/// A trait/interface shared across module boundaries.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct InterfaceDeclaration {
    /// Trait name.
    pub name: String,
    /// Which module file defines this trait.
    pub defined_in: String,
    /// Method signatures as strings (not parsed — readable and unambiguous).
    pub methods: Vec<String>,
    /// Module paths that implement this trait.
    pub implementors: Vec<String>,
}

/// Cargo dependency decisions — resolved before Builder touches any code.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct DependencyChanges {
    /// New crates to add (name + version or features).
    pub add: Vec<DependencySpec>,
    /// Existing crates to remove (by name).
    pub remove: Vec<String>,
    /// Explicitly acknowledged retained deps (prevents silent accumulation).
    pub keep: Vec<String>,
}

/// A new dependency to add.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct DependencySpec {
    pub name: String,
    pub version: String,
    pub features: Vec<String>,
    pub reason: String,
}

/// The Architect's structured output turn.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ArchitectTurn {
    /// The binding structural declaration.
    #[serde(default)]
    pub declaration: StructuralDeclaration,
    /// Why this structure was chosen.
    #[serde(default)]
    pub rationale: String,
    /// Architect's confidence in the declaration (0.0–1.0).
    #[serde(default = "default_confidence")]
    pub confidence: f64,
}

/// The Scalpel's structured output turn — targeted fixes only, no redesign.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ScalpelTurn {
    /// Specific fixes applied.
    #[serde(default)]
    pub fixes: Vec<ScalpelFix>,
    /// Whether all targeted failures are resolved.
    #[serde(default)]
    pub done: bool,
}

/// A single targeted fix by the Scalpel.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ScalpelFix {
    /// File that was fixed.
    pub file: String,
    /// The specific issue from Judge output.
    pub issue: String,
    /// What was done to fix it.
    pub action: String,
}

// ROLE-FILTERED BRIEFINGS — each role sees a different slice of reality

/// Token budget allocation per role. Derived from task profile.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenBudget {
    /// Maximum tokens for the briefing context.
    pub briefing: usize,
    /// Maximum tokens for the agent's response.
    pub response: usize,
    /// Maximum tokens for tool call results in a single turn.
    pub tool_results: usize,
}

impl TokenBudget {
    /// Token budgets for a builder role.
    pub fn for_builder(profile: &TaskProfile) -> Self {
        let base = match profile.risk {
            RiskLevel::Low => 4000,
            RiskLevel::Moderate => 6000,
            RiskLevel::High => 8000,
            RiskLevel::Critical => 10000,
        };
        Self {
            briefing: base,
            response: 2048,
            tool_results: 3000,
        }
    }

    /// Token budgets for a skeptic role.
    pub fn for_skeptic(profile: &TaskProfile) -> Self {
        let base = match profile.risk {
            RiskLevel::Low => 2000,
            RiskLevel::Moderate => 3000,
            RiskLevel::High => 5000,
            RiskLevel::Critical => 6000,
        };
        Self {
            briefing: base,
            response: 1024,
            tool_results: 1500,
        }
    }

    /// Token budgets for a judge role.
    pub fn for_judge(_profile: &TaskProfile) -> Self {
        // Judge always needs less — it runs tests, not prose
        Self {
            briefing: 2000,
            response: 512,
            tool_results: 3000,
        }
    }

    /// Token budgets for the coordinator.
    pub fn for_coordinator(profile: &TaskProfile) -> Self {
        let base = match profile.risk {
            RiskLevel::Low => 1500,
            RiskLevel::Moderate => 2000,
            RiskLevel::High => 3000,
            RiskLevel::Critical => 4000,
        };
        Self {
            briefing: base,
            response: 1024,
            tool_results: 1000,
        }
    }

    /// Token budgets for the architect role.
    ///
    /// Architect needs generous briefing (reads codebase) but no write tools.
    pub fn for_architect(profile: &TaskProfile) -> Self {
        let base = match profile.risk {
            RiskLevel::Low => 4000,
            RiskLevel::Moderate => 6000,
            RiskLevel::High => 8000,
            RiskLevel::Critical => 10000,
        };
        Self {
            briefing: base,
            response: 2048,
            tool_results: 500, // read-only, minimal tool results
        }
    }
}

/// A briefing filtered for a specific role. Each role sees exactly what it
/// needs — no more, no less. This is the core token-saving mechanism.
///
/// | Role        | Sees                                        | Doesn't see                    |
/// |-------------|---------------------------------------------|--------------------------------|
/// | Builder     | Full code for target files, summaries for related | Builder reasoning from prev turns |
/// | Skeptic     | Claims + diffs, no builder reasoning        | Builder confidence, approach    |
/// | Judge       | File paths + test config                    | Claims, opinions, reasoning    |
/// | Coordinator | Trust scores + progress deltas              | Code, diffs, reasoning         |
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoleBriefing {
    /// Which role this briefing is for.
    pub role: TeamRole,
    /// The task description (all roles see this).
    pub task: String,
    /// Token budget for this role.
    pub budget: TokenBudget,
    /// Code snippets — filtered by role.
    pub code: Vec<FileSnippet>,
    /// Attempt history — filtered by role.
    pub attempts: Vec<AttemptSummary>,
    /// Constraints that never change.
    pub constraints: Vec<String>,
    /// Insights relevant to this role.
    pub insights: Vec<String>,
    /// Current verification state (builder and judge only).
    pub verification: Option<VerificationState>,
    /// Trust state (coordinator only).
    pub trust_context: Option<TrustContext>,
}

/// Trust context visible only to the coordinator.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustContext {
    /// Current builder trust value.
    pub builder_trust: f64,
    /// Number of turns so far.
    pub turn: u32,
    /// Whether trust is degrading.
    pub degrading: bool,
    /// Builder generation (how many rotations).
    pub builder_generation: u32,
}

impl RoleBriefing {
    /// Build a role-filtered briefing from the full briefing state.
    ///
    /// This is the single entry point for creating briefings. The coordinator
    /// calls this before each agent turn to produce the right context slice.
    #[allow(clippy::too_many_arguments)]
    pub fn for_role(
        role: TeamRole,
        task: &str,
        code: &[FileSnippet],
        attempts: &[AttemptSummary],
        constraints: &[String],
        insights: &[String],
        verification: Option<&VerificationState>,
        trust: Option<&TrustScore>,
        state: Option<&TeamLoopState>,
        profile: &TaskProfile,
    ) -> Self {
        let budget = match role {
            TeamRole::Builder | TeamRole::Scalpel => TokenBudget::for_builder(profile),
            TeamRole::Skeptic => TokenBudget::for_skeptic(profile),
            TeamRole::Judge => TokenBudget::for_judge(profile),
            TeamRole::Coordinator => TokenBudget::for_coordinator(profile),
            TeamRole::Architect => TokenBudget::for_architect(profile),
        };

        // Filter code visibility by role.
        let filtered_code = match role {
            TeamRole::Builder | TeamRole::Scalpel => code.to_vec(), // Builder/Scalpel see full code for target files
            TeamRole::Skeptic | TeamRole::Architect => code
                .iter()
                .map(|s| FileSnippet {
                    // Skeptic/Architect see only first 20 lines (enough for context, not full detail)
                    content: s.content.lines().take(20).collect::<Vec<_>>().join("\n"),
                    ..s.clone()
                })
                .collect(),
            TeamRole::Judge | TeamRole::Coordinator => vec![], // Judge/Coordinator don't read code
        };

        // Filter attempt history by role.
        let filtered_attempts = match role {
            TeamRole::Builder | TeamRole::Scalpel => {
                // Builder/Scalpel see last 3 attempts (to avoid repeating them)
                let start = attempts.len().saturating_sub(3);
                attempts[start..].to_vec()
            }
            TeamRole::Skeptic => {
                // Skeptic sees last 2 attempts (what to check against)
                let start = attempts.len().saturating_sub(2);
                attempts[start..].to_vec()
            }
            TeamRole::Architect => vec![], // Architect runs before attempts exist
            TeamRole::Judge => vec![],     // Judge doesn't need attempt history
            TeamRole::Coordinator => {
                // Coordinator sees all attempts (for progress tracking)
                attempts.to_vec()
            }
        };

        // Filter insights by role.
        let filtered_insights = match role {
            TeamRole::Builder | TeamRole::Scalpel => {
                // Builder and scalpel see all insights (they need context)
                insights.to_vec()
            }
            TeamRole::Skeptic | TeamRole::Architect => {
                // Skeptic and architect see all insights (they need context)
                insights.to_vec()
            }
            TeamRole::Judge => vec![], // Judge doesn't need insights
            TeamRole::Coordinator => {
                // Coordinator sees high-level insights only (first 3)
                insights.iter().take(3).cloned().collect()
            }
        };

        // Verification state: builder and judge see it.
        let filtered_verification = match role {
            TeamRole::Builder | TeamRole::Judge => verification.cloned(),
            _ => None,
        };

        // Trust context: coordinator only.
        let trust_context = match (role, trust, state) {
            (TeamRole::Coordinator, Some(t), Some(s)) => Some(TrustContext {
                builder_trust: t.value,
                turn: s.turn,
                degrading: s.progress_delta.is_degrading(),
                builder_generation: s.team_config.builder_generation,
            }),
            _ => None,
        };

        Self {
            role,
            task: task.to_string(),
            budget,
            code: filtered_code,
            attempts: filtered_attempts,
            constraints: constraints.to_vec(),
            insights: filtered_insights,
            verification: filtered_verification,
            trust_context,
        }
    }

    /// Estimate the token count of this briefing.
    pub fn estimated_tokens(&self) -> usize {
        let mut count = 0usize;

        // Task description
        count += self.task.split_whitespace().count() * 2;

        // Code snippets
        for snippet in &self.code {
            count += snippet.content.split_whitespace().count() * 2;
        }

        // Attempt summaries — each has approach + root_cause
        for attempt in &self.attempts {
            count += attempt.approach.split_whitespace().count() * 2;
            count += attempt.root_cause.split_whitespace().count() * 2;
        }

        // Constraints
        for c in &self.constraints {
            count += c.split_whitespace().count() * 2;
        }

        // Insights
        for i in &self.insights {
            count += i.split_whitespace().count() * 2;
        }

        // Verification state (~50 tokens)
        if self.verification.is_some() {
            count += 50;
        }

        // Trust context (~30 tokens)
        if self.trust_context.is_some() {
            count += 30;
        }

        count
    }

    /// Check if the briefing fits within its token budget.
    pub fn within_budget(&self) -> bool {
        self.estimated_tokens() <= self.budget.briefing
    }

    /// Truncate code snippets to fit within budget. Returns self for chaining.
    pub fn truncated_to_budget(mut self) -> Self {
        let budget = self.budget.briefing;
        let other_tokens = self.estimated_tokens()
            - self
                .code
                .iter()
                .map(|s| s.content.split_whitespace().count() * 2)
                .sum::<usize>();

        let code_budget = budget.saturating_sub(other_tokens);
        let mut used = 0usize;

        self.code.retain(|s| {
            let tokens = s.content.split_whitespace().count() * 2;
            if used + tokens <= code_budget {
                used += tokens;
                true
            } else {
                false
            }
        });

        self
    }
}

fn default_confidence() -> f64 {
    0.5
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trust_score_starts_moderate() {
        let trust = TrustScore::new();
        assert!(trust.is_autonomous());
        assert!(!trust.needs_supervision());
        assert!(!trust.should_escalate());
        assert!((trust.value - 0.7).abs() < 0.01);
    }

    #[test]
    fn trust_decreases_on_hallucination() {
        let mut trust = TrustScore::new();
        trust.record(TrustEvent {
            kind: TrustEventKind::HallucinationCaught,
            turn: 1,
            note: "claimed foo::bar exists".to_string(),
        });
        assert!((trust.value - 0.5).abs() < 0.01);
    }

    #[test]
    fn trust_increases_on_verified_claim() {
        let mut trust = TrustScore::new();
        trust.record(TrustEvent {
            kind: TrustEventKind::TaskCompleted,
            turn: 1,
            note: "task completed".to_string(),
        });
        assert!(trust.value > 0.7);
    }

    #[test]
    fn trust_escalation_threshold() {
        let mut trust = TrustScore::new();
        // Two hallucinations should push below 0.25
        trust.record(TrustEvent {
            kind: TrustEventKind::HallucinationCaught,
            turn: 1,
            note: "first".to_string(),
        });
        trust.record(TrustEvent {
            kind: TrustEventKind::HallucinationCaught,
            turn: 2,
            note: "second".to_string(),
        });
        trust.record(TrustEvent {
            kind: TrustEventKind::ClaimRefuted,
            turn: 3,
            note: "third".to_string(),
        });
        assert!(trust.should_escalate());
    }

    #[test]
    fn progress_delta_detects_degrading() {
        let mut delta = ProgressDelta::new();
        delta.push(1); // improving
        assert!(!delta.is_degrading());
        delta.push(-2); // one bad turn
        assert!(!delta.is_degrading());
        delta.push(-1); // two bad turns in a row
        assert!(delta.is_degrading());
    }

    #[test]
    fn approach_fingerprint_detects_repeats() {
        let fp1 = ApproachFingerprint::new(
            "edit auth.rs validate_token".to_string(),
            vec!["src/auth.rs".to_string()],
        );
        let fp2 = ApproachFingerprint::new(
            "modify auth.rs validate_token function".to_string(),
            vec!["src/auth.rs".to_string(), "src/lib.rs".to_string()],
        );
        assert!(fp1.is_same_approach(&fp2));
    }

    #[test]
    fn approach_fingerprint_different_approaches() {
        let fp1 =
            ApproachFingerprint::new("edit auth.rs".to_string(), vec!["src/auth.rs".to_string()]);
        let fp2 = ApproachFingerprint::new(
            "add new auth module".to_string(),
            vec!["src/auth_new.rs".to_string()],
        );
        // Different categories (Modification vs Addition) and no overlapping files
        assert!(!fp1.is_same_approach(&fp2));
    }

    #[test]
    fn briefing_limits_attempts() {
        let mut briefing = Briefing::new("fix the bug");
        for i in 0..7 {
            briefing.push_attempt(AttemptSummary {
                approach: format!("attempt {}", i),
                files_changed: vec![],
                outcome: AttemptOutcome::TestFailure,
                root_cause: "tests fail".to_string(),
                builder_generation: 0,
            });
        }
        assert_eq!(briefing.attempts.len(), 5);
        assert_eq!(briefing.attempts[0].approach, "attempt 2");
    }

    #[test]
    fn attitude_tightens_and_relaxes() {
        let mut att = AgentAttitude::standard();
        assert_eq!(att.patience, 3);
        assert!(!att.pre_flight);

        att.tighten();
        assert_eq!(att.burden_of_proof, BurdenOfProof::BeyondReasonableDoubt);
        assert!(att.pre_flight);

        att.relax();
        assert_eq!(att.burden_of_proof, BurdenOfProof::Standard);
    }

    #[test]
    fn team_assembly_light() {
        let profile = TaskProfile {
            risk: RiskLevel::Low,
            reach: ReachLevel::SingleFile,
            familiarity: Familiarity::WellKnown,
            reversibility: Reversibility::Easy,
            strategy: ReasoningStrategy::default(),
            signals: vec![],
        };
        let team = profile.assemble_team();
        assert!(team.roles.contains(&TeamRole::Builder));
        assert!(team.roles.contains(&TeamRole::Coordinator));
        assert!(!team.roles.contains(&TeamRole::Skeptic));
        assert!(!team.roles.contains(&TeamRole::Judge));
    }

    #[test]
    fn team_assembly_strict() {
        let profile = TaskProfile {
            risk: RiskLevel::Critical,
            reach: ReachLevel::Wide,
            familiarity: Familiarity::Unknown,
            reversibility: Reversibility::Hard,
            strategy: ReasoningStrategy::default(),
            signals: vec![],
        };
        let team = profile.assemble_team();
        assert!(team.roles.contains(&TeamRole::Builder));
        assert!(team.roles.contains(&TeamRole::Skeptic));
        assert!(team.roles.contains(&TeamRole::Judge));
        assert!(team.roles.contains(&TeamRole::Coordinator));
        assert_eq!(
            team.attitude.burden_of_proof,
            BurdenOfProof::BeyondReasonableDoubt
        );
    }

    #[test]
    fn team_loop_state_detects_doom() {
        let team_config = TaskProfile {
            risk: RiskLevel::Moderate,
            reach: ReachLevel::Local,
            familiarity: Familiarity::SomewhatKnown,
            reversibility: Reversibility::Moderate,
            strategy: ReasoningStrategy::default(),
            signals: vec![],
        }
        .assemble_team();
        let mut state = TeamLoopState::new(team_config);

        // Record first approach
        let fp =
            ApproachFingerprint::new("edit auth.rs".to_string(), vec!["src/auth.rs".to_string()]);
        state.record_turn(fp.clone(), -1);

        // A different approach should not be flagged
        let different_fp = ApproachFingerprint::new(
            "add new test file".to_string(),
            vec!["tests/auth_test.rs".to_string()],
        );
        assert!(!state.is_repeating_approach(&different_fp));

        // Record the same approach again (second occurrence)
        let fp2 = ApproachFingerprint::new(
            "modify auth.rs".to_string(),
            vec!["src/auth.rs".to_string()],
        );
        state.record_turn(fp2.clone(), -1);

        // Now a third similar approach should be flagged as repeating
        let fp3 = ApproachFingerprint::new(
            "update auth.rs".to_string(),
            vec!["src/auth.rs".to_string()],
        );
        assert!(state.is_repeating_approach(&fp3));
    }

    #[test]
    fn team_role_tool_access() {
        assert_eq!(TeamRole::Builder.allowed_tools(), ToolSet::All);
        assert_eq!(TeamRole::Skeptic.allowed_tools(), ToolSet::ReadOnly);
        assert_eq!(TeamRole::Judge.allowed_tools(), ToolSet::VerificationOnly);
        assert!(TeamRole::Builder.can_write());
        assert!(!TeamRole::Skeptic.can_write());
        assert!(!TeamRole::Judge.can_write());
    }

    #[test]
    fn trust_event_deltas() {
        assert!(TrustEventKind::ClaimVerified.delta() > 0.0);
        assert!(TrustEventKind::HallucinationCaught.delta() < 0.0);
        assert!(TrustEventKind::TaskCompleted.delta() > 0.0);
        assert!(TrustEventKind::RepeatedFailure.delta() < 0.0);
        // Hallucination is worse than repeated failure
        assert!(
            TrustEventKind::HallucinationCaught.delta() < TrustEventKind::RepeatedFailure.delta()
        );
    }

    #[test]
    fn approach_categories() {
        assert_eq!(
            ApproachCategory::classify("add new function"),
            ApproachCategory::Addition
        );
        assert_eq!(
            ApproachCategory::classify("create module"),
            ApproachCategory::Addition
        );
        assert_eq!(
            ApproachCategory::classify("delete unused code"),
            ApproachCategory::Removal
        );
        assert_eq!(
            ApproachCategory::classify("update Cargo.toml"),
            ApproachCategory::Configuration
        );
        assert_eq!(
            ApproachCategory::classify("read the file"),
            ApproachCategory::Investigation
        );
        assert_eq!(
            ApproachCategory::classify("fix the bug"),
            ApproachCategory::Modification
        );
    }

    #[test]
    fn escalation_has_options() {
        let esc = Escalation::trust_exhausted(0.2, None);
        assert_eq!(esc.level, EscalationLevel::Level3);
        assert!(!esc.options.is_empty());
        assert!(esc.recommendation < esc.options.len());
    }

    #[test]
    fn builder_turn_escalation_helpers() {
        use crate::agent_protocol::{EscalationRequest, EscalationTarget};

        // Test needs_escalation
        let turn = BuilderTurn {
            approach: "test".to_string(),
            changes: vec![],
            claims: vec![],
            confidence: 0.8,
            done: false,
            escalation: None,
        };
        assert!(!turn.needs_escalation());

        // Test with escalation
        let turn_with_esc = BuilderTurn {
            approach: "test".to_string(),
            changes: vec![],
            claims: vec![],
            confidence: 0.5,
            done: false,
            escalation: Some(EscalationRequest {
                target: EscalationTarget::Architect,
                reason: "unclear boundaries".to_string(),
                question: None,
            }),
        };
        assert!(turn_with_esc.needs_escalation());
        assert_eq!(
            turn_with_esc.escalation_target(),
            Some(EscalationTarget::Architect)
        );

        // Test request_architect helper
        let architect_request = BuilderTurn::request_architect("need guidance");
        assert!(architect_request.needs_escalation());
        assert_eq!(
            architect_request.escalation_target(),
            Some(EscalationTarget::Architect)
        );
        assert_eq!(
            architect_request.escalation.unwrap().reason,
            "need guidance"
        );
    }

    // ── Phase D: Team Composition Integration tests ──

    fn default_profile() -> TaskProfile {
        TaskProfile {
            risk: RiskLevel::Moderate,
            reach: ReachLevel::Local,
            familiarity: Familiarity::WellKnown,
            reversibility: Reversibility::Easy,
            strategy: ReasoningStrategy::ActFirst,
            signals: vec![],
        }
    }

    #[test]
    fn d1_assembly_context_adds_specialists_as_scalpel() {
        let profile = default_profile();
        let ctx = crate::task_routing::AssemblyContext {
            intent_category: crate::intent::IntentCategory::Implementation,
            thinking_depth: crate::task_routing::TaskThinkingMode::Standard,
            confidence: 0.8,
            required_specialists: vec!["security".into(), "performance".into()],
        };
        let team = profile.assemble_team_with_context(Some(&ctx));
        let scalpel_count = team
            .roles
            .iter()
            .filter(|r| **r == TeamRole::Scalpel)
            .count();
        assert_eq!(scalpel_count, 2);
    }

    #[test]
    fn d1_assemble_team_without_context_unchanged() {
        let profile = default_profile();
        let baseline = profile.assemble_team();
        let with_none = profile.assemble_team_with_context(None);
        assert_eq!(baseline.roles, with_none.roles);
    }

    #[test]
    fn d1_assembly_context_adds_architect_for_deep_thinking() {
        let mut profile = default_profile();
        profile.strategy = ReasoningStrategy::ActFirst;
        let ctx = crate::task_routing::AssemblyContext {
            intent_category: crate::intent::IntentCategory::Implementation,
            thinking_depth: crate::task_routing::TaskThinkingMode::Deep,
            confidence: 0.8,
            required_specialists: vec![],
        };
        let team = profile.assemble_team_with_context(Some(&ctx));
        assert!(team.roles.contains(&TeamRole::Architect));
    }

    #[test]
    fn d3_specialist_fallback_no_context_gives_standard_team() {
        let profile = default_profile();
        let team = profile.assemble_team();
        assert!(team.roles.contains(&TeamRole::Coordinator));
        assert!(team.roles.contains(&TeamRole::Builder));
        assert!(team.roles.contains(&TeamRole::Judge));
    }

    #[test]
    fn d5_budget_capped_team_sizing() {
        let profile = default_profile();
        let ctx = crate::task_routing::AssemblyContext {
            intent_category: crate::intent::IntentCategory::Implementation,
            thinking_depth: crate::task_routing::TaskThinkingMode::Extended,
            confidence: 0.9,
            required_specialists: vec![
                "security".into(),
                "performance".into(),
                "testing".into(),
                "docs".into(),
            ],
        };
        let team = profile.assemble_team_with_context(Some(&ctx));
        let roster = build_roster(&team, Some(&ctx), 3);
        assert_eq!(roster.len(), 3);
        assert!(roster[0].priority >= roster[1].priority);
        assert!(roster[1].priority >= roster[2].priority);
    }

    #[test]
    fn d5_budget_capped_preserves_high_priority() {
        let profile = default_profile();
        let ctx = crate::task_routing::AssemblyContext {
            intent_category: crate::intent::IntentCategory::Implementation,
            thinking_depth: crate::task_routing::TaskThinkingMode::Standard,
            confidence: 0.7,
            required_specialists: vec!["x".into()],
        };
        let team = profile.assemble_team_with_context(Some(&ctx));
        let roster = build_roster(&team, Some(&ctx), 2);
        assert_eq!(roster.len(), 2);
        assert_eq!(roster[0].role, TeamRole::Coordinator);
        assert_eq!(roster[1].role, TeamRole::Builder);
    }

    #[test]
    fn d5_build_roster_without_context_no_scalpel() {
        let profile = default_profile();
        let team = profile.assemble_team();
        let roster = build_roster(&team, None, 10);
        assert!(roster.iter().all(|a| a.role != TeamRole::Scalpel));
    }

    #[test]
    fn d7_end_to_end_intent_to_roster() {
        let profile = default_profile();
        let ctx = crate::task_routing::AssemblyContext {
            intent_category: crate::intent::IntentCategory::Implementation,
            thinking_depth: crate::task_routing::TaskThinkingMode::Deep,
            confidence: 0.85,
            required_specialists: vec!["auth".into()],
        };
        let team = profile.assemble_team_with_context(Some(&ctx));
        let roster = build_roster(&team, Some(&ctx), 6);

        let roles: Vec<TeamRole> = roster.iter().map(|a| a.role).collect();
        assert!(roles.contains(&TeamRole::Coordinator));
        assert!(roles.contains(&TeamRole::Builder));
        assert!(roles.contains(&TeamRole::Judge));
        assert!(roles.contains(&TeamRole::Skeptic));
        assert!(roles.contains(&TeamRole::Architect));
        assert!(roles.contains(&TeamRole::Scalpel));

        let scalpel_assignments: Vec<_> = roster
            .iter()
            .filter(|a| a.role == TeamRole::Scalpel)
            .collect();
        assert_eq!(scalpel_assignments.len(), 1);
        assert_eq!(
            scalpel_assignments[0].specialization.as_deref(),
            Some("auth")
        );
    }

    #[test]
    fn role_priority_ordering() {
        assert!(role_priority(TeamRole::Coordinator) > role_priority(TeamRole::Builder));
        assert!(role_priority(TeamRole::Builder) > role_priority(TeamRole::Judge));
        assert!(role_priority(TeamRole::Judge) > role_priority(TeamRole::Architect));
        assert!(role_priority(TeamRole::Architect) > role_priority(TeamRole::Skeptic));
        assert!(role_priority(TeamRole::Skeptic) > role_priority(TeamRole::Scalpel));
    }
}
