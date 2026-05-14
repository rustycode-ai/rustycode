//! Lightweight thinking guide — nudges the model through the bug-fix workflow.
//!
//! Unlike the heavy structured_thinking tool (write-only sink with 8+ params),
//! this tool has 3 fields, returns actionable guidance, and tracks turn budget.

use anyhow::Result;
use serde::Deserialize;
use serde_json::Value;

/// Workflow phases for a bug-fix task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    /// Reading files, understanding the codebase and the bug report.
    Explore,
    /// Found the relevant code, understanding root cause.
    Locate,
    /// Decided what to change, about to edit.
    Plan,
    /// Made the edit(s), ready to wrap up.
    Execute,
    /// Task complete, model should respond with summary.
    Done,
}

#[allow(clippy::trivially_copy_pass_by_ref, clippy::use_self)]
impl Phase {
    fn label(&self) -> &'static str {
        match self {
            Self::Explore => "EXPLORE",
            Self::Locate => "LOCATE",
            Self::Plan => "PLAN",
            Self::Execute => "EXECUTE",
            Self::Done => "DONE",
        }
    }

    fn next_hint(&self) -> &'static str {
        match self {
            Self::Explore => "Use find_symbol or grep to locate the relevant code.",
            Self::Locate => "You understand the bug. Decide the minimal fix and move to plan.",
            Self::Plan => "State your fix plan briefly, then edit the file.",
            Self::Execute => {
                "Edit made. If confident, respond with a brief summary — no more tool calls."
            }
            Self::Done => "Stop using tools. Write your summary.",
        }
    }

    /// Check if the model is going backwards in the workflow.
    fn is_regression(&self, previous: Phase) -> bool {
        matches!(
            (previous, *self),
            (Phase::Plan, Phase::Explore)
                | (Phase::Execute, Phase::Explore)
                | (Phase::Execute, Phase::Locate)
                | (Phase::Done, Phase::Explore)
                | (Phase::Done, Phase::Locate)
                | (Phase::Done, Phase::Plan)
        )
    }
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

        // Detect regressions (going backwards in workflow)
        let regression_warning = if let Some(last) = state.last_phase {
            if params.phase.is_regression(last) {
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
            Some("Confidence has been below 50 for 3+ turns. Read the most relevant file and make a decision.".to_string())
        } else {
            None
        };

        state.last_phase = Some(params.phase);

        let remaining = state.remaining();
        let budget_warning = if remaining <= 5 {
            Some(format!(
                "Only {} turns remaining. Focus on making the edit now.",
                remaining
            ))
        } else if remaining <= 10 {
            Some(format!(
                "{} turns remaining. Move toward fixing the bug.",
                remaining
            ))
        } else {
            None
        };

        // Build guidance response
        let mut response = serde_json::json!({
            "phase": params.phase.label(),
            "next_step": params.phase.next_hint(),
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

        // If confidence >= 85 and past locate phase, encourage acting
        if params.confidence >= 85 && matches!(params.phase, Phase::Locate | Phase::Plan) {
            response["nudge"] =
                serde_json::json!("High confidence — skip further reading. Make the edit now.");
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
         to stay on track. Returns: next step suggestion, turn budget, and warnings \
         if you're going in circles or running low on turns."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "thought": {
                    "type": "string",
                    "description": "What you're thinking or about to do (1-2 sentences)"
                },
                "phase": {
                    "type": "string",
                    "enum": ["explore", "locate", "plan", "execute", "done"],
                    "description": "Current phase: explore (reading), locate (found bug), plan (deciding fix), execute (editing), done (finished)"
                },
                "confidence": {
                    "type": "integer",
                    "minimum": 0,
                    "maximum": 100,
                    "description": "How confident are you in your understanding? (0-100)"
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
    }

    #[test]
    fn phase_next_hints_exist() {
        assert!(!Phase::Explore.next_hint().is_empty());
        assert!(!Phase::Done.next_hint().is_empty());
    }

    #[test]
    fn regression_detection() {
        assert!(Phase::Explore.is_regression(Phase::Plan));
        assert!(Phase::Explore.is_regression(Phase::Execute));
        assert!(!Phase::Locate.is_regression(Phase::Explore));
        assert!(!Phase::Plan.is_regression(Phase::Locate));
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
        // confidence is optional
        assert!(!required.iter().any(|r| r == "confidence"));
    }

    #[test]
    fn execute_returns_guidance() {
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
        assert!(parsed["next_step"].is_string());
        assert_eq!(parsed["turns_remaining"], 29);
    }

    #[test]
    fn high_confidence_nudge() {
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
    fn regression_warning() {
        let _lock = TEST_LOCK.lock().unwrap();
        reset();
        configure(30);
        let tool = ThinkingGuideTool::new();
        let ctx = rustycode_tools_api::ToolContext::new(std::path::Path::new("/tmp"));

        // First call: plan phase
        let params1 = serde_json::json!({
            "thought": "Planning the fix",
            "phase": "plan",
            "confidence": 80
        });
        let _ = tool.execute(params1, &ctx).unwrap();

        // Second call: regresses back to explore
        let params2 = serde_json::json!({
            "thought": "Let me re-read the files",
            "phase": "explore",
            "confidence": 60
        });
        let result = tool.execute(params2, &ctx).unwrap();
        let parsed: Value = serde_json::from_str(&result.text).unwrap();
        assert!(parsed["warning"].is_string());
        assert!(parsed["warning"].as_str().unwrap().contains("back to"));
    }

    #[test]
    fn budget_warning_at_low_turns() {
        let _lock = TEST_LOCK.lock().unwrap();
        reset();
        configure(5); // Only 5 turns
        let tool = ThinkingGuideTool::new();
        let ctx = rustycode_tools_api::ToolContext::new(std::path::Path::new("/tmp"));

        // Use 4 turns
        for i in 0..4 {
            let params = serde_json::json!({
                "thought": format!("Step {i}"),
                "phase": "explore"
            });
            let _ = tool.execute(params, &ctx).unwrap();
        }

        // 5th call should get budget warning
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
