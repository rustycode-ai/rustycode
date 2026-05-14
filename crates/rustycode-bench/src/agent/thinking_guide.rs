//! Lightweight thinking guide — structural planning and workflow nudges.
//!
//! Two modes:
//! - **Simple** (bug-fix): explore → locate → plan → execute → done
//! - **Complex** (large scope): scope → decompose → expand → revise → execute → verify → done
//!
//! The model signals complexity via the `scope` field. When scope is "complex",
//! thinking_guide guides through structural decomposition before execution.

use anyhow::Result;
use serde::Deserialize;
use serde_json::Value;

/// Workflow phases for task execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    // ── Complex-mode planning phases ──
    /// Assessing task size and identifying components.
    Scope,
    /// Breaking the task into numbered sub-tasks.
    Decompose,
    /// Fleshing out approach for each sub-task.
    Expand,
    /// Revising the plan based on new understanding.
    Revise,

    // ── Shared execution phases ──
    /// Reading files, understanding the codebase.
    Explore,
    /// Found the relevant code, understanding root cause.
    Locate,
    /// Decided what to change, about to edit.
    Plan,
    /// Making edits.
    Execute,
    /// Checking that the fix works.
    Verify,
    /// Task complete.
    Done,
}

#[allow(clippy::trivially_copy_pass_by_ref, clippy::use_self)]
impl Phase {
    fn label(&self) -> &'static str {
        match self {
            Self::Scope => "SCOPE",
            Self::Decompose => "DECOMPOSE",
            Self::Expand => "EXPAND",
            Self::Revise => "REVISE",
            Self::Explore => "EXPLORE",
            Self::Locate => "LOCATE",
            Self::Plan => "PLAN",
            Self::Execute => "EXECUTE",
            Self::Verify => "VERIFY",
            Self::Done => "DONE",
        }
    }

    fn next_hint(&self, mode: TaskMode) -> &'static str {
        match mode {
            TaskMode::Simple => match self {
                Self::Explore => "Use find_symbol or grep to locate the relevant code.",
                Self::Locate => "You understand the bug. Decide the minimal fix and move to plan.",
                Self::Plan => "State your fix plan briefly, then edit the file.",
                Self::Execute => {
                    "Edit made. If confident, respond with a brief summary — no more tool calls."
                }
                Self::Done => "Stop using tools. Write your summary.",
                _ => "Unexpected phase for simple mode. Move to explore or done.",
            },
            TaskMode::Complex => match self {
                Self::Scope => "Identify all components involved. List what you know and what you need to find out. Assess: is this one change or many?",
                Self::Decompose => "Write your plan as a numbered list in your thought. Example: '1. Fix X in file.rs 2. Update Y in mod.rs 3. Verify with test'. Be concrete — name files and functions.",
                Self::Expand => "Pick the first unexpanded item from your plan. Describe exactly what you'll read and what you'll change. Then do it.",
                Self::Revise => "Update your plan based on what you've learned. Strike through completed items. Add new items if discovered. Drop items that aren't needed.",
                Self::Explore => "Read the files for your current plan item only. Stay focused — don't drift to other items.",
                Self::Locate => "Found the code for this plan item. Decide the minimal change.",
                Self::Plan => "About to edit. Does this match your plan item? If not, revise first.",
                Self::Execute => "Editing plan item. After this edit, check: which plan item is next? Does it still look right?",
                Self::Verify => "Run tests or check behavior. Does this plan item work? If yes, move to the next item.",
                Self::Done => "All plan items done. Stop using tools. Write your summary.",
            },
        }
    }

    /// Check if the model is going backwards in the workflow.
    fn is_regression(&self, previous: Phase) -> bool {
        matches!(
            (previous, *self),
            // Simple mode regressions
            (Phase::Plan, Phase::Explore)
                | (Phase::Execute, Phase::Explore)
                | (Phase::Execute, Phase::Locate)
                | (Phase::Done, Phase::Explore)
                | (Phase::Done, Phase::Locate)
                | (Phase::Done, Phase::Plan)
                // Complex mode regressions (revise is explicitly allowed)
                | (Phase::Execute, Phase::Scope)
                | (Phase::Execute, Phase::Decompose)
                | (Phase::Verify, Phase::Scope)
                | (Phase::Done, Phase::Scope)
        )
    }
}

/// Task complexity mode — detected from model's first call.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum TaskMode {
    #[default]
    Simple,
    Complex,
}

/// Task scope assessment from the model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
enum TaskScope {
    /// Single-file bug fix or small change.
    #[default]
    Simple,
    /// Multi-file change or moderate refactor.
    Moderate,
    /// Large refactor, architectural change, or multi-system task.
    Complex,
}

/// Parameters for the thinking_guide tool.
#[derive(Debug, Deserialize)]
struct ThinkingParams {
    /// What you're thinking or about to do.
    #[allow(dead_code)]
    thought: String,
    /// Current workflow phase.
    phase: Phase,
    /// Confidence in your understanding (0-100).
    #[serde(default = "default_confidence")]
    confidence: u32,
    /// Task scope assessment (only needed on first call).
    #[serde(default)]
    scope: Option<TaskScope>,
}

fn default_confidence() -> u32 {
    50
}

/// State tracked across calls within a session.
#[derive(Debug, Default)]
struct GuideState {
    turns_used: u32,
    max_turns: u32,
    last_phase: Option<Phase>,
    regressions: u32,
    low_confidence_streak: u32,
    /// Detected task mode (simple or complex).
    mode: TaskMode,
    /// How many times the model has revised its plan.
    revision_count: u32,
    /// Whether decomposition has happened (model reached Decompose phase).
    decomposed: bool,
}

impl GuideState {
    fn remaining(&self) -> u32 {
        self.max_turns.saturating_sub(self.turns_used)
    }
}

// Session state — flat global (bench runs one task per process).
use std::sync::Mutex;

static STATE: Mutex<Option<GuideState>> = Mutex::new(None);

fn get_state() -> &'static Mutex<Option<GuideState>> {
    &STATE
}

/// Configure the guide for a new task (call at session start).
pub fn configure(max_turns: u32) {
    let mut guard = get_state().lock().unwrap_or_else(|e| e.into_inner());
    *guard = Some(GuideState {
        max_turns,
        ..GuideState::default()
    });
}

/// Reset the guide state. Used to isolate tests.
#[cfg(test)]
pub fn reset() {
    let mut guard = get_state().lock().unwrap_or_else(|e| e.into_inner());
    *guard = None;
}

/// Test-only lock to serialize tests that touch the global state.
#[cfg(test)]
static TEST_LOCK: Mutex<()> = Mutex::new(());

/// The thinking_guide tool implementation.
pub struct ThinkingGuideTool;

impl Default for ThinkingGuideTool {
    fn default() -> Self {
        Self::new()
    }
}

impl ThinkingGuideTool {
    pub fn new() -> Self {
        Self
    }

    fn execute_inner(&self, params: ThinkingParams) -> Value {
        let mut guard = get_state().lock().unwrap_or_else(|e| e.into_inner());
        let state = guard.get_or_insert_with(|| GuideState {
            max_turns: 30,
            ..GuideState::default()
        });

        state.turns_used = state.turns_used.saturating_add(1);

        // Detect task mode from scope field or phase on first call
        if state.turns_used == 1 {
            if let Some(ref scope) = params.scope {
                match scope {
                    TaskScope::Complex => state.mode = TaskMode::Complex,
                    TaskScope::Moderate => state.mode = TaskMode::Complex,
                    TaskScope::Simple => {}
                }
            }
            // Also detect from first phase
            if matches!(params.phase, Phase::Scope | Phase::Decompose) {
                state.mode = TaskMode::Complex;
            }
        }

        // Track decomposition
        if matches!(params.phase, Phase::Decompose | Phase::Expand) {
            state.decomposed = true;
        }

        // Track revisions
        if params.phase == Phase::Revise {
            state.revision_count = state.revision_count.saturating_add(1);
        }

        // Detect regressions (going backwards in workflow)
        let regression_warning = if let Some(last) = state.last_phase {
            // Allow revise → any planning phase without warning (it's intentional)
            if last == Phase::Revise
                && matches!(
                    params.phase,
                    Phase::Decompose | Phase::Expand | Phase::Scope
                )
            {
                None
            } else if params.phase.is_regression(last) {
                state.regressions = state.regressions.saturating_add(1);
                Some(format!(
                    "You went from {} back to {}. Re-reading code you've already seen wastes turns.",
                    last.label(),
                    params.phase.label()
                ))
            } else {
                None
            }
        } else {
            None
        };

        // Track low confidence streak
        if params.confidence < 50 {
            state.low_confidence_streak = state.low_confidence_streak.saturating_add(1);
        } else {
            state.low_confidence_streak = 0;
        }

        let confidence_warning = if state.low_confidence_streak >= 3 {
            match state.mode {
                TaskMode::Simple => Some(
                    "Confidence has been below 50 for 3+ turns. Read the most relevant file and make a decision.".to_string()
                ),
                TaskMode::Complex => Some(
                    "Confidence has been below 50 for 3+ turns. Consider: (1) narrow scope to what you understand, (2) revise your plan, or (3) this task may need escalation.".to_string()
                ),
            }
        } else {
            None
        };

        state.last_phase = Some(params.phase);

        let remaining = state.remaining();
        let budget_warning = if remaining <= 5 {
            Some(format!(
                "Only {} turns remaining. Focus on the most important change now.",
                remaining
            ))
        } else if remaining <= 10 {
            Some(format!(
                "{} turns remaining. Move toward making edits.",
                remaining
            ))
        } else {
            None
        };

        // Build guidance response
        let mut response = serde_json::json!({
            "phase": params.phase.label(),
            "mode": match state.mode { TaskMode::Simple => "simple", TaskMode::Complex => "complex" },
            "next_step": params.phase.next_hint(state.mode),
            "turns_remaining": remaining,
        });

        if let Some(ref w) = regression_warning {
            response["warning"] = serde_json::json!(w);
        }
        if let Some(ref w) = confidence_warning {
            response["warning"] = serde_json::json!(w);
        }
        if let Some(ref w) = budget_warning {
            response["budget"] = serde_json::json!(w);
        }

        // Mode-specific nudges
        match state.mode {
            TaskMode::Simple => {
                // If confidence >= 85 and past locate phase, encourage acting
                if params.confidence >= 85 && matches!(params.phase, Phase::Locate | Phase::Plan) {
                    response["nudge"] = serde_json::json!(
                        "High confidence — skip further reading. Make the edit now."
                    );
                }
            }
            TaskMode::Complex => {
                // Nudge toward decomposition if in scope phase
                if params.phase == Phase::Scope && params.confidence >= 60 {
                    response["nudge"] = serde_json::json!(
                        "You understand the scope. Write a numbered plan: list each change as a concrete item (file + what to change). Then move to expand."
                    );
                }

                // If in explore without having decomposed, nudge to plan first
                if matches!(params.phase, Phase::Explore | Phase::Locate)
                    && !state.decomposed
                    && state.turns_used <= 3
                {
                    response["nudge"] = serde_json::json!(
                        "You jumped into exploring before making a plan. Pause: write a numbered list of sub-tasks in your thought, then explore each one."
                    );
                }

                // Nudge toward revision if stuck in expand
                if params.phase == Phase::Expand
                    && state.revision_count == 0
                    && state.turns_used > 5
                {
                    response["nudge"] = serde_json::json!(
                        "You've been expanding for a while. Revise your plan based on what you've learned: drop items that aren't needed, then start executing the first item."
                    );
                }

                // Escalation signal: too many revisions or regressions
                if state.revision_count >= 3 || state.regressions >= 4 {
                    response["escalation"] = serde_json::json!({
                        "reason": if state.revision_count >= 3 { "Plan revised 3+ times — scope may be too large for solo agent" } else { "Going in circles — consider reducing scope or escalating" },
                        "recommended_agent": "ensemble",
                        "suggestion": "This task may benefit from parallel exploration. Narrow to the most critical sub-task and execute that first."
                    });
                }

                // Remind about decomposed plan during execution
                if matches!(params.phase, Phase::Execute | Phase::Verify) && state.decomposed {
                    response["reminder"] = serde_json::json!(
                        "Follow your numbered plan. Which item are you on? After this, check: does the next item still make sense?"
                    );
                }
            }
        }

        response
    }
}

impl rustycode_tools_api::Tool for ThinkingGuideTool {
    fn name(&self) -> &'static str {
        "thinking_guide"
    }

    fn description(&self) -> &'static str {
        "Track your reasoning and get workflow guidance. Call once before each action \
         to stay on track. Use 'scope' field for complex tasks to get structural planning \
         guidance (decompose → expand → revise). Returns: next step, turn budget, warnings, \
         and escalation signals for tasks that need team coordination."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "thought": {
                    "type": "string",
                    "description": "What you're thinking, about to do, or your structural plan (can be multi-line for decompose/expand phases)"
                },
                "phase": {
                    "type": "string",
                    "enum": ["scope", "decompose", "expand", "revise", "explore", "locate", "plan", "execute", "verify", "done"],
                    "description": "Current phase. Simple flow: explore→locate→plan→execute→done. Complex flow: scope→decompose→expand→revise→execute→verify→done. Use complex flow for multi-file changes or architectural tasks."
                },
                "confidence": {
                    "type": "integer",
                    "minimum": 0,
                    "maximum": 100,
                    "description": "How confident are you in your understanding? (0-100)"
                },
                "scope": {
                    "type": "string",
                    "enum": ["simple", "moderate", "complex"],
                    "description": "Task scope assessment (only needed on first call). 'complex' or 'moderate' enables structural planning mode."
                }
            },
            "required": ["thought", "phase"]
        })
    }

    fn execute(
        &self,
        params: Value,
        _ctx: &rustycode_tools_api::ToolContext,
    ) -> Result<rustycode_tools_api::ToolOutput> {
        let params: ThinkingParams = serde_json::from_value(params)
            .map_err(|e| anyhow::anyhow!("Invalid thinking_guide parameters: {e}"))?;
        let response = self.execute_inner(params);
        Ok(rustycode_tools_api::ToolOutput::text(response.to_string()))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use rustycode_tools_api::Tool;

    #[test]
    fn phase_labels() {
        assert_eq!(Phase::Explore.label(), "EXPLORE");
        assert_eq!(Phase::Done.label(), "DONE");
        assert_eq!(Phase::Scope.label(), "SCOPE");
        assert_eq!(Phase::Decompose.label(), "DECOMPOSE");
        assert_eq!(Phase::Expand.label(), "EXPAND");
        assert_eq!(Phase::Revise.label(), "REVISE");
        assert_eq!(Phase::Verify.label(), "VERIFY");
    }

    #[test]
    fn phase_next_hints_exist() {
        for phase in [
            Phase::Explore,
            Phase::Locate,
            Phase::Plan,
            Phase::Execute,
            Phase::Done,
            Phase::Scope,
            Phase::Decompose,
            Phase::Expand,
            Phase::Revise,
            Phase::Verify,
        ] {
            assert!(
                !phase.next_hint(TaskMode::Simple).is_empty()
                    || !phase.next_hint(TaskMode::Complex).is_empty(),
                "Phase {:?} has no hints",
                phase
            );
        }
    }

    #[test]
    fn regression_detection() {
        assert!(Phase::Explore.is_regression(Phase::Plan));
        assert!(Phase::Explore.is_regression(Phase::Execute));
        assert!(!Phase::Locate.is_regression(Phase::Explore));
        assert!(!Phase::Plan.is_regression(Phase::Locate));
        // Revise → Decompose is NOT a regression (intentional revision)
        assert!(!Phase::Decompose.is_regression(Phase::Revise));
        // Execute → Scope IS a regression
        assert!(Phase::Scope.is_regression(Phase::Execute));
    }

    #[test]
    fn tool_name() {
        let tool = ThinkingGuideTool::new();
        assert_eq!(tool.name(), "thinking_guide");
    }

    #[test]
    fn schema_has_required_fields() {
        let tool = ThinkingGuideTool::new();
        let schema = tool.parameters_schema();
        let required = schema["required"].as_array().unwrap();
        assert!(required.iter().any(|r| r == "thought"));
        assert!(required.iter().any(|r| r == "phase"));
        // confidence and scope are optional
        assert!(!required.iter().any(|r| r == "confidence"));
        assert!(!required.iter().any(|r| r == "scope"));
    }

    #[test]
    fn schema_has_all_phases() {
        let tool = ThinkingGuideTool::new();
        let schema = tool.parameters_schema();
        let phases = schema["properties"]["phase"]["enum"].as_array().unwrap();
        let names: Vec<&str> = phases.iter().filter_map(|v| v.as_str()).collect();
        assert!(names.contains(&"scope"));
        assert!(names.contains(&"decompose"));
        assert!(names.contains(&"expand"));
        assert!(names.contains(&"revise"));
        assert!(names.contains(&"explore"));
        assert!(names.contains(&"execute"));
        assert!(names.contains(&"verify"));
        assert!(names.contains(&"done"));
    }

    #[test]
    fn execute_returns_guidance_simple_mode() {
        let _lock = TEST_LOCK.lock().unwrap();
        reset();
        configure(30);
        let tool = ThinkingGuideTool::new();
        let ctx = rustycode_tools_api::ToolContext::new(std::path::Path::new("/tmp"));

        let params = serde_json::json!({
            "thought": "Looking for the auth module",
            "phase": "explore",
            "confidence": 40
        });

        let result = tool.execute(params, &ctx).unwrap();
        let parsed: Value = serde_json::from_str(&result.text).unwrap();
        assert_eq!(parsed["phase"], "EXPLORE");
        assert_eq!(parsed["mode"], "simple");
        assert!(parsed["next_step"].is_string());
        assert_eq!(parsed["turns_remaining"], 29);
    }

    #[test]
    fn complex_mode_activated_by_scope_field() {
        let _lock = TEST_LOCK.lock().unwrap();
        reset();
        configure(30);
        let tool = ThinkingGuideTool::new();
        let ctx = rustycode_tools_api::ToolContext::new(std::path::Path::new("/tmp"));

        let params = serde_json::json!({
            "thought": "This touches auth, middleware, and the API layer",
            "phase": "scope",
            "scope": "complex",
            "confidence": 40
        });

        let result = tool.execute(params, &ctx).unwrap();
        let parsed: Value = serde_json::from_str(&result.text).unwrap();
        assert_eq!(parsed["mode"], "complex");
        assert_eq!(parsed["phase"], "SCOPE");
    }

    #[test]
    fn complex_mode_activated_by_decompose_phase() {
        let _lock = TEST_LOCK.lock().unwrap();
        reset();
        configure(30);
        let tool = ThinkingGuideTool::new();
        let ctx = rustycode_tools_api::ToolContext::new(std::path::Path::new("/tmp"));

        let params = serde_json::json!({
            "thought": "This is a multi-file change",
            "phase": "decompose"
        });

        let result = tool.execute(params, &ctx).unwrap();
        let parsed: Value = serde_json::from_str(&result.text).unwrap();
        assert_eq!(parsed["mode"], "complex");
    }

    #[test]
    fn high_confidence_nudge_simple() {
        let _lock = TEST_LOCK.lock().unwrap();
        reset();
        configure(30);
        let tool = ThinkingGuideTool::new();
        let ctx = rustycode_tools_api::ToolContext::new(std::path::Path::new("/tmp"));

        let params = serde_json::json!({
            "thought": "Found the bug in auth.rs line 42",
            "phase": "locate",
            "confidence": 90
        });

        let result = tool.execute(params, &ctx).unwrap();
        let parsed: Value = serde_json::from_str(&result.text).unwrap();
        assert!(parsed["nudge"].is_string());
        assert!(parsed["nudge"].as_str().unwrap().contains("edit"));
    }

    #[test]
    fn scope_nudge_to_decompose() {
        let _lock = TEST_LOCK.lock().unwrap();
        reset();
        configure(30);
        let tool = ThinkingGuideTool::new();
        let ctx = rustycode_tools_api::ToolContext::new(std::path::Path::new("/tmp"));

        // First call: scope phase with decent confidence
        let params = serde_json::json!({
            "thought": "This touches 3 modules",
            "phase": "scope",
            "scope": "moderate",
            "confidence": 65
        });

        let result = tool.execute(params, &ctx).unwrap();
        let parsed: Value = serde_json::from_str(&result.text).unwrap();
        assert!(parsed["nudge"].is_string());
        assert!(parsed["nudge"].as_str().unwrap().contains("numbered plan"));
    }

    #[test]
    fn regression_warning() {
        let _lock = TEST_LOCK.lock().unwrap();
        reset();
        configure(30);
        let tool = ThinkingGuideTool::new();
        let ctx = rustycode_tools_api::ToolContext::new(std::path::Path::new("/tmp"));

        let params1 = serde_json::json!({
            "thought": "Planning the fix",
            "phase": "plan",
            "confidence": 80
        });
        let _ = tool.execute(params1, &ctx).unwrap();

        let params2 = serde_json::json!({
            "thought": "Let me re-read the files",
            "phase": "explore",
            "confidence": 60
        });
        let result = tool.execute(params2, &ctx).unwrap();
        let parsed: Value = serde_json::from_str(&result.text).unwrap();
        assert!(
            parsed["warning"].is_string(),
            "Expected warning, got: {parsed}"
        );
        assert!(parsed["warning"].as_str().unwrap().contains("back to"));
    }

    #[test]
    fn revise_to_decompose_not_a_regression() {
        let _lock = TEST_LOCK.lock().unwrap();
        reset();
        configure(30);
        let tool = ThinkingGuideTool::new();
        let ctx = rustycode_tools_api::ToolContext::new(std::path::Path::new("/tmp"));

        // Set up complex mode
        let params1 = serde_json::json!({
            "thought": "Revising the plan",
            "phase": "revise",
            "scope": "complex"
        });
        let _ = tool.execute(params1, &ctx).unwrap();

        // Going back to decompose after revise should NOT be a regression
        let params2 = serde_json::json!({
            "thought": "Re-decomposing based on new understanding",
            "phase": "decompose"
        });
        let result = tool.execute(params2, &ctx).unwrap();
        let parsed: Value = serde_json::from_str(&result.text).unwrap();
        assert!(parsed["warning"].is_null() || parsed.get("warning").is_none());
    }

    #[test]
    fn escalation_after_many_revisions() {
        let _lock = TEST_LOCK.lock().unwrap();
        reset();
        configure(30);
        let tool = ThinkingGuideTool::new();
        let ctx = rustycode_tools_api::ToolContext::new(std::path::Path::new("/tmp"));

        // Set up complex mode
        let init = serde_json::json!({ "thought": "start", "phase": "scope", "scope": "complex" });
        let _ = tool.execute(init, &ctx).unwrap();

        // Trigger 3 revisions
        for _ in 0..3 {
            let dec = serde_json::json!({ "thought": "decompose", "phase": "decompose" });
            let _ = tool.execute(dec, &ctx).unwrap();
            let rev = serde_json::json!({ "thought": "revise", "phase": "revise" });
            let _ = tool.execute(rev, &ctx).unwrap();
        }

        // 4th revision should trigger escalation
        let rev = serde_json::json!({ "thought": "revise again", "phase": "revise" });
        let result = tool.execute(rev, &ctx).unwrap();
        let parsed: Value = serde_json::from_str(&result.text).unwrap();
        assert!(parsed["escalation"].is_object());
        assert_eq!(parsed["escalation"]["recommended_agent"], "ensemble");
    }

    #[test]
    fn execution_reminder_when_decomposed() {
        let _lock = TEST_LOCK.lock().unwrap();
        reset();
        configure(30);
        let tool = ThinkingGuideTool::new();
        let ctx = rustycode_tools_api::ToolContext::new(std::path::Path::new("/tmp"));

        let params1 = serde_json::json!({
            "thought": "start", "phase": "scope", "scope": "complex"
        });
        let _ = tool.execute(params1, &ctx).unwrap();

        let params2 = serde_json::json!({
            "thought": "1. Fix auth\n2. Update middleware", "phase": "decompose"
        });
        let _ = tool.execute(params2, &ctx).unwrap();

        let params3 = serde_json::json!({
            "thought": "Fixing auth module", "phase": "execute"
        });
        let result = tool.execute(params3, &ctx).unwrap();
        let parsed: Value = serde_json::from_str(&result.text).unwrap();
        assert!(
            parsed["reminder"].is_string(),
            "Expected reminder, got: {parsed}"
        );
        assert!(parsed["reminder"]
            .as_str()
            .unwrap()
            .contains("numbered plan"));
    }

    #[test]
    fn budget_warning_at_low_turns() {
        let _lock = TEST_LOCK.lock().unwrap();
        reset();
        configure(5);
        let tool = ThinkingGuideTool::new();
        let ctx = rustycode_tools_api::ToolContext::new(std::path::Path::new("/tmp"));

        for i in 0..4 {
            let params = serde_json::json!({
                "thought": format!("Step {i}"),
                "phase": "explore"
            });
            let _ = tool.execute(params, &ctx).unwrap();
        }

        let params = serde_json::json!({
            "thought": "Still exploring",
            "phase": "explore"
        });
        let result = tool.execute(params, &ctx).unwrap();
        let parsed: Value = serde_json::from_str(&result.text).unwrap();
        assert!(parsed["budget"].is_string());
        assert_eq!(parsed["turns_remaining"], 0);
    }
}
