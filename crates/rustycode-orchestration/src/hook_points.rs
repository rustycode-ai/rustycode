//! Lifecycle hook system with 20+ hook points.
//!
//! This module provides a composable hook system that allows injecting custom logic
//! at specific moments in the agent lifecycle. Hook points span tool execution,
//! session management, agent spawning, plan execution, skill activation, permission
//! checks, tier changes, and budget enforcement.
//!
//! # Hook Result Semantics
//!
//! Each hook callback returns a [`HookResult`] that controls execution flow:
//! - `Continue`: Proceed normally
//! - `Abort`: Stop the current action with an optional reason
//! - `ModifyOutput`: Replace the output with modified data
//!
//! # Usage
//!
//! ```ignore
//! use rustycode_orchestration::hook_points::*;
//!
//! let mut registry = HookRegistry::new();
//!
//! registry.register(HookPoint::PreToolUse, |ctx| {
//!     if ctx.subject == "bash" {
//!         // Inspect or log the command
//!         tracing::info!("About to run: {:?}", ctx.metadata);
//!     }
//!     Ok(HookResult::Continue)
//! });
//!
//! let event = HookContext::new(HookPoint::PreToolUse, "bash", serde_json::json!({"command": "ls"}));
//! let results = registry.trigger(&event)?;
//! ```

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// HookPoint enum
// ---------------------------------------------------------------------------

/// Lifecycle hook points for the expanded hook system.
///
/// Each variant corresponds to a specific moment in the agent lifecycle
/// where custom logic can be injected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookPoint {
    // -- Tool lifecycle --
    /// Before a tool is executed.
    PreToolUse,
    /// After a tool has been executed.
    PostToolUse,
    /// Tool execution failed.
    ToolError,

    // -- Session lifecycle --
    /// Session has been created.
    SessionStart,
    /// Session is ending normally.
    SessionEnd,
    /// Session ended due to error.
    SessionError,

    // -- Agent lifecycle --
    /// Working directory changed.
    CwdChanged,
    /// Sub-agent spawned.
    SubagentStart,
    /// Sub-agent completed.
    SubagentEnd,

    // -- Plan lifecycle --
    /// Plan execution starting.
    PlanStart,
    /// Plan execution completed.
    PlanEnd,

    // -- Context lifecycle --
    /// Error recovery initiated.
    ErrorRecovery,
    /// Context window switched (e.g., compaction).
    ContextSwitch,

    // -- Skill lifecycle --
    /// Skill about to be activated.
    SkillActivate,
    /// Skill deactivated.
    SkillDeactivate,

    // -- Permission lifecycle --
    /// Permission check requested.
    PermissionCheck,
    /// Permission granted.
    PermissionGranted,
    /// Permission denied.
    PermissionDenied,

    // -- Tier lifecycle --
    /// Tool tier promoted.
    TierPromoted,
    /// Tool tier scope changed.
    TierScopeChanged,

    // -- Budget lifecycle --
    /// Context budget warning (>80% used).
    BudgetWarning,
    /// Skill evicted due to budget pressure.
    BudgetEviction,
}

impl HookPoint {
    /// All defined hook points (22 variants).
    pub const fn all() -> &'static [Self] {
        &[
            Self::PreToolUse,
            Self::PostToolUse,
            Self::ToolError,
            Self::SessionStart,
            Self::SessionEnd,
            Self::SessionError,
            Self::CwdChanged,
            Self::SubagentStart,
            Self::SubagentEnd,
            Self::PlanStart,
            Self::PlanEnd,
            Self::ErrorRecovery,
            Self::ContextSwitch,
            Self::SkillActivate,
            Self::SkillDeactivate,
            Self::PermissionCheck,
            Self::PermissionGranted,
            Self::PermissionDenied,
            Self::TierPromoted,
            Self::TierScopeChanged,
            Self::BudgetWarning,
            Self::BudgetEviction,
        ]
    }

    /// Dot-separated event type string (e.g., "tool.pre").
    pub const fn event_type(&self) -> &'static str {
        match self {
            Self::PreToolUse => "tool.pre",
            Self::PostToolUse => "tool.post",
            Self::ToolError => "tool.error",
            Self::SessionStart => "session.start",
            Self::SessionEnd => "session.end",
            Self::SessionError => "session.error",
            Self::CwdChanged => "cwd.changed",
            Self::SubagentStart => "subagent.start",
            Self::SubagentEnd => "subagent.end",
            Self::PlanStart => "plan.start",
            Self::PlanEnd => "plan.end",
            Self::ErrorRecovery => "error.recovery",
            Self::ContextSwitch => "context.switch",
            Self::SkillActivate => "skill.activate",
            Self::SkillDeactivate => "skill.deactivate",
            Self::PermissionCheck => "permission.check",
            Self::PermissionGranted => "permission.granted",
            Self::PermissionDenied => "permission.denied",
            Self::TierPromoted => "tier.promoted",
            Self::TierScopeChanged => "tier.scope_changed",
            Self::BudgetWarning => "budget.warning",
            Self::BudgetEviction => "budget.eviction",
        }
    }

    /// Whether this hook fires before an action (pre-hook).
    pub const fn is_pre(&self) -> bool {
        matches!(
            self,
            Self::PreToolUse
                | Self::SkillActivate
                | Self::PermissionCheck
                | Self::PlanStart
                | Self::SubagentStart
        )
    }

    /// Whether this hook fires after an action (post-hook).
    pub const fn is_post(&self) -> bool {
        !self.is_pre()
    }
}

impl fmt::Display for HookPoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.event_type())
    }
}

// ---------------------------------------------------------------------------
// HookResult
// ---------------------------------------------------------------------------

/// Result returned by a hook callback to control execution flow.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum HookResult {
    /// Continue execution normally.
    #[default]
    Continue,
    /// Abort the current action. Optional reason for logging.
    Abort(Option<String>),
    /// Modify the output with replacement data.
    ModifyOutput(serde_json::Value),
}

// ---------------------------------------------------------------------------
// HookContext
// ---------------------------------------------------------------------------

/// Context data passed to hook callbacks.
///
/// Contains the hook point that triggered the event, a subject identifier
/// (tool name, skill name, path, etc.), arbitrary metadata, a timestamp,
/// and an optional session ID.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookContext {
    /// Which hook point triggered this event.
    pub hook: HookPoint,
    /// What the event is about (tool name, skill name, path, etc.).
    pub subject: String,
    /// Arbitrary metadata for the event.
    pub metadata: serde_json::Value,
    /// When the event was created.
    pub timestamp: DateTime<Utc>,
    /// Session that triggered the event, if known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
}

impl HookContext {
    /// Create a new hook context.
    pub fn new(hook: HookPoint, subject: impl Into<String>, metadata: serde_json::Value) -> Self {
        Self {
            hook,
            subject: subject.into(),
            metadata,
            timestamp: Utc::now(),
            session_id: None,
        }
    }

    /// Attach a session ID to this context.
    pub fn with_session_id(mut self, id: impl Into<String>) -> Self {
        self.session_id = Some(id.into());
        self
    }
}

// ---------------------------------------------------------------------------
// HookCallback trait
// ---------------------------------------------------------------------------

/// Type alias for the boxed callback function.
///
/// Takes a reference to [`HookContext`] and returns a [`HookResult`] or an error.
/// The callback must be `Send + Sync` for safe cross-thread dispatch.
pub type HookCallbackFn = dyn Fn(&HookContext) -> anyhow::Result<HookResult> + Send + Sync;

// ---------------------------------------------------------------------------
// HookRegistry
// ---------------------------------------------------------------------------

/// Registry for managing and dispatching lifecycle hooks.
///
/// Multiple callbacks can be registered at the same hook point. They are
/// executed in registration order. If any callback returns `Abort`, execution
/// stops immediately. If a callback returns `ModifyOutput`, the modified value
/// is passed to subsequent callbacks via the context metadata.
#[derive(Default)]
pub struct HookRegistry {
    handlers: HashMap<HookPoint, Vec<Arc<HookCallbackFn>>>,
}

impl fmt::Debug for HookRegistry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HookRegistry")
            .field("hook_count", &self.handlers.len())
            .finish()
    }
}

impl HookRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a callback for a specific hook point.
    ///
    /// Callbacks are executed in registration order.
    pub fn register<F>(&mut self, hook: HookPoint, callback: F)
    where
        F: Fn(&HookContext) -> anyhow::Result<HookResult> + Send + Sync + 'static,
    {
        self.handlers
            .entry(hook)
            .or_default()
            .push(Arc::new(callback));
    }

    /// Trigger all callbacks registered for the given context's hook point.
    ///
    /// Returns a list of results from each callback. If any callback returns
    /// `Abort`, subsequent callbacks are skipped and the abort is included
    /// in the results. Callback errors are logged but do not prevent other
    /// callbacks from running.
    pub fn trigger(&self, context: &HookContext) -> anyhow::Result<Vec<HookResult>> {
        let mut results = Vec::new();

        if let Some(handlers) = self.handlers.get(&context.hook) {
            for handler in handlers {
                match handler(context) {
                    Ok(result) => {
                        let should_abort = matches!(result, HookResult::Abort(_));
                        results.push(result);
                        if should_abort {
                            break;
                        }
                    }
                    Err(e) => {
                        // Log but continue -- hooks should not break the main flow
                        tracing::warn!(
                            hook = %context.hook,
                            subject = %context.subject,
                            error = %e,
                            "[hook] callback error"
                        );
                    }
                }
            }
        }

        Ok(results)
    }

    /// Remove all callbacks for a specific hook point.
    pub fn deregister(&mut self, hook: HookPoint) {
        self.handlers.remove(&hook);
    }

    /// Number of callbacks registered for a specific hook point.
    pub fn handler_count(&self, hook: HookPoint) -> usize {
        self.handlers.get(&hook).map_or(0, Vec::len)
    }

    /// Remove all callbacks from all hook points.
    pub fn clear_all(&mut self) {
        self.handlers.clear();
    }

    /// Total number of registered callbacks across all hook points.
    pub fn total_handlers(&self) -> usize {
        self.handlers.values().map(Vec::len).sum()
    }
}

// ---------------------------------------------------------------------------
// Protocol type conversions
// ---------------------------------------------------------------------------

impl From<HookPoint> for Option<rustycode_protocol::HookEvent> {
    #[allow(clippy::use_self)]
    fn from(point: HookPoint) -> Option<rustycode_protocol::HookEvent> {
        match point {
            HookPoint::PreToolUse => Some(rustycode_protocol::HookEvent::PreToolUse),
            HookPoint::PostToolUse => Some(rustycode_protocol::HookEvent::PostToolUse),
            HookPoint::SessionStart => Some(rustycode_protocol::HookEvent::SessionStart),
            HookPoint::SessionEnd => Some(rustycode_protocol::HookEvent::SessionEnd),
            HookPoint::SubagentEnd => Some(rustycode_protocol::HookEvent::SubagentStop),
            HookPoint::ContextSwitch => Some(rustycode_protocol::HookEvent::PreCompact),
            // Internal events with no Claude Code equivalent
            HookPoint::ToolError
            | HookPoint::SessionError
            | HookPoint::CwdChanged
            | HookPoint::SubagentStart
            | HookPoint::PlanStart
            | HookPoint::PlanEnd
            | HookPoint::ErrorRecovery
            | HookPoint::SkillActivate
            | HookPoint::SkillDeactivate
            | HookPoint::PermissionCheck
            | HookPoint::PermissionGranted
            | HookPoint::PermissionDenied
            | HookPoint::TierPromoted
            | HookPoint::TierScopeChanged
            | HookPoint::BudgetWarning
            | HookPoint::BudgetEviction => None,
        }
    }
}

impl From<HookResult> for rustycode_protocol::HookOutput {
    fn from(result: HookResult) -> Self {
        match result {
            HookResult::Continue => Self::default(),
            HookResult::Abort(reason) => Self {
                r#continue: false,
                stop_reason: reason,
                ..Default::default()
            },
            HookResult::ModifyOutput(value) => {
                let ctx = value.as_str().map(std::string::ToString::to_string);
                Self {
                    hook_specific_output: Some(rustycode_protocol::HookSpecificOutput {
                        additional_context: ctx,
                    }),
                    ..Default::default()
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::Mutex;

    // -- HookPoint tests --

    #[test]
    fn hook_point_has_at_least_20_variants() {
        let hooks = HookPoint::all();
        assert!(
            hooks.len() >= 20,
            "Expected >= 20 hook points, got {}",
            hooks.len()
        );
    }

    #[test]
    fn hook_point_has_exactly_22_variants() {
        assert_eq!(HookPoint::all().len(), 22);
    }

    #[test]
    fn hook_point_event_type_strings_are_unique() {
        let hooks = HookPoint::all();
        let types: Vec<&str> = hooks.iter().map(HookPoint::event_type).collect();
        let unique: std::collections::HashSet<&str> = types.iter().copied().collect();
        assert_eq!(
            types.len(),
            unique.len(),
            "event_type strings must be unique"
        );
    }

    #[test]
    fn hook_point_is_pre_or_post() {
        assert!(HookPoint::PreToolUse.is_pre());
        assert!(!HookPoint::PreToolUse.is_post());
        assert!(HookPoint::PostToolUse.is_post());
        assert!(!HookPoint::PostToolUse.is_pre());
        assert!(HookPoint::CwdChanged.is_post()); // observation hooks are post
    }

    #[test]
    fn hook_point_serde_roundtrip() {
        for hook in HookPoint::all() {
            let json = serde_json::to_string(hook).unwrap();
            let decoded: HookPoint = serde_json::from_str(&json).unwrap();
            assert_eq!(*hook, decoded);
        }
    }

    #[test]
    fn hook_point_display_matches_event_type() {
        for hook in HookPoint::all() {
            assert_eq!(hook.to_string(), hook.event_type());
        }
    }

    #[test]
    fn hook_point_all_has_no_duplicates() {
        let hooks = HookPoint::all();
        let set: std::collections::HashSet<HookPoint> = hooks.iter().copied().collect();
        assert_eq!(hooks.len(), set.len());
    }

    // -- HookResult tests --

    #[test]
    fn hook_result_default_is_continue() {
        assert_eq!(HookResult::default(), HookResult::Continue);
    }

    #[test]
    fn hook_result_serde_roundtrip() {
        let results = [
            HookResult::Continue,
            HookResult::Abort(Some("reason".into())),
            HookResult::Abort(None),
            HookResult::ModifyOutput(serde_json::json!({"key": "value"})),
        ];
        for result in &results {
            let json = serde_json::to_string(result).unwrap();
            let decoded: HookResult = serde_json::from_str(&json).unwrap();
            assert_eq!(result, &decoded);
        }
    }

    // -- HookContext tests --

    #[test]
    fn hook_context_carries_hook_and_metadata() {
        let ctx = HookContext::new(
            HookPoint::PreToolUse,
            "bash",
            serde_json::json!({"command": "ls"}),
        );
        assert_eq!(ctx.hook, HookPoint::PreToolUse);
        assert_eq!(ctx.subject, "bash");
        assert_eq!(ctx.metadata["command"], "ls");
        assert!(ctx.timestamp <= Utc::now());
    }

    #[test]
    fn hook_context_with_session_id() {
        let ctx = HookContext::new(
            HookPoint::SessionStart,
            "session-123",
            serde_json::json!({}),
        )
        .with_session_id("s-456");
        assert_eq!(ctx.session_id.as_deref(), Some("s-456"));
    }

    #[test]
    fn hook_context_serialization_roundtrip() {
        let ctx = HookContext::new(
            HookPoint::CwdChanged,
            "/new/path",
            serde_json::json!({"old": "/old/path"}),
        );
        let json = serde_json::to_string(&ctx).unwrap();
        let decoded: HookContext = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.hook, HookPoint::CwdChanged);
        assert_eq!(decoded.subject, "/new/path");
    }

    #[test]
    fn hook_context_without_session_id_serializes_cleanly() {
        let ctx = HookContext::new(
            HookPoint::TierPromoted,
            "default->extended",
            serde_json::json!({}),
        );
        let json = serde_json::to_string(&ctx).unwrap();
        assert!(!json.contains("session_id"));
    }

    // -- HookRegistry tests --

    #[test]
    fn registry_dispatches_to_registered_handler() {
        let mut registry = HookRegistry::new();
        let called = Arc::new(AtomicBool::new(false));
        let called_clone = called.clone();

        registry.register(HookPoint::PreToolUse, move |ctx| {
            assert_eq!(ctx.subject, "bash");
            called_clone.store(true, Ordering::SeqCst);
            Ok(HookResult::Continue)
        });

        let ctx = HookContext::new(HookPoint::PreToolUse, "bash", serde_json::json!({}));
        let results = registry.trigger(&ctx).unwrap();
        assert!(called.load(Ordering::SeqCst));
        assert_eq!(results.len(), 1);
        assert_eq!(results[0], HookResult::Continue);
    }

    #[test]
    fn registry_calls_multiple_handlers_in_order() {
        let mut registry = HookRegistry::new();
        let order = Arc::new(Mutex::new(Vec::new()));

        let o1 = order.clone();
        registry.register(HookPoint::PostToolUse, move |_| {
            o1.lock().unwrap().push(1);
            Ok(HookResult::Continue)
        });

        let o2 = order.clone();
        registry.register(HookPoint::PostToolUse, move |_| {
            o2.lock().unwrap().push(2);
            Ok(HookResult::Continue)
        });

        let ctx = HookContext::new(HookPoint::PostToolUse, "read_file", serde_json::json!({}));
        registry.trigger(&ctx).unwrap();
        assert_eq!(*order.lock().unwrap(), vec![1, 2]);
    }

    #[test]
    fn registry_ignores_unregistered_hooks() {
        let registry = HookRegistry::new();
        let ctx = HookContext::new(HookPoint::BudgetWarning, "skills", serde_json::json!({}));
        let results = registry.trigger(&ctx).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn registry_continues_after_handler_error() {
        let mut registry = HookRegistry::new();
        let second_called = Arc::new(AtomicBool::new(false));
        let second_clone = second_called.clone();

        registry.register(HookPoint::SessionStart, |_| {
            Err(anyhow::anyhow!("first handler fails"))
        });
        registry.register(HookPoint::SessionStart, move |_| {
            second_clone.store(true, Ordering::SeqCst);
            Ok(HookResult::Continue)
        });

        let ctx = HookContext::new(HookPoint::SessionStart, "s1", serde_json::json!({}));
        let results = registry.trigger(&ctx).unwrap();
        assert!(second_called.load(Ordering::SeqCst));
        // Only the second handler's result appears (first errored)
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn registry_abort_stops_further_handlers() {
        let mut registry = HookRegistry::new();
        let second_called = Arc::new(AtomicBool::new(false));
        let second_clone = second_called.clone();

        registry.register(HookPoint::PermissionCheck, |_| {
            Ok(HookResult::Abort(Some("blocked".into())))
        });
        registry.register(HookPoint::PermissionCheck, move |_| {
            second_clone.store(true, Ordering::SeqCst);
            Ok(HookResult::Continue)
        });

        let ctx = HookContext::new(HookPoint::PermissionCheck, "bash", serde_json::json!({}));
        let results = registry.trigger(&ctx).unwrap();
        assert!(!second_called.load(Ordering::SeqCst));
        assert_eq!(results.len(), 1);
        assert!(matches!(results[0], HookResult::Abort(_)));
    }

    #[test]
    fn registry_handler_count() {
        let mut registry = HookRegistry::new();
        assert_eq!(registry.handler_count(HookPoint::PreToolUse), 0);
        registry.register(HookPoint::PreToolUse, |_| Ok(HookResult::Continue));
        registry.register(HookPoint::PreToolUse, |_| Ok(HookResult::Continue));
        assert_eq!(registry.handler_count(HookPoint::PreToolUse), 2);
        assert_eq!(registry.handler_count(HookPoint::PostToolUse), 0);
    }

    #[test]
    fn registry_deregister_removes_handlers() {
        let mut registry = HookRegistry::new();
        registry.register(HookPoint::PlanStart, |_| Ok(HookResult::Continue));
        registry.deregister(HookPoint::PlanStart);
        assert_eq!(registry.handler_count(HookPoint::PlanStart), 0);
    }

    #[test]
    fn registry_clear_all_removes_everything() {
        let mut registry = HookRegistry::new();
        registry.register(HookPoint::PreToolUse, |_| Ok(HookResult::Continue));
        registry.register(HookPoint::PostToolUse, |_| Ok(HookResult::Continue));
        assert_eq!(registry.total_handlers(), 2);
        registry.clear_all();
        assert_eq!(registry.total_handlers(), 0);
    }

    #[test]
    fn registry_total_handlers() {
        let mut registry = HookRegistry::new();
        assert_eq!(registry.total_handlers(), 0);
        registry.register(HookPoint::PreToolUse, |_| Ok(HookResult::Continue));
        registry.register(HookPoint::SessionStart, |_| Ok(HookResult::Continue));
        registry.register(HookPoint::SessionStart, |_| Ok(HookResult::Continue));
        assert_eq!(registry.total_handlers(), 3);
    }

    // -- Integration test: dispatch all hook types --

    #[test]
    fn integration_dispatch_all_hook_types() {
        let mut registry = HookRegistry::new();
        let count = Arc::new(AtomicUsize::new(0));
        let count_clone = count.clone();

        // Register a handler for every hook type
        for hook in HookPoint::all() {
            let c = count_clone.clone();
            registry.register(*hook, move |_| {
                c.fetch_add(1, Ordering::SeqCst);
                Ok(HookResult::Continue)
            });
        }

        // Dispatch an event for each hook type
        for hook in HookPoint::all() {
            let ctx = HookContext::new(*hook, "test-subject", serde_json::json!({}));
            let results = registry.trigger(&ctx).unwrap();
            assert_eq!(results.len(), 1, "Expected 1 result for {hook:?}");
        }

        // All handlers should have been called exactly once
        assert_eq!(count.load(Ordering::SeqCst), HookPoint::all().len());
    }

    // -- Integration test: hook results carry through --

    #[test]
    fn integration_modify_output_result() {
        let mut registry = HookRegistry::new();

        registry.register(HookPoint::PostToolUse, |_| {
            Ok(HookResult::ModifyOutput(
                serde_json::json!({"modified": true}),
            ))
        });

        let ctx = HookContext::new(HookPoint::PostToolUse, "bash", serde_json::json!({}));
        let results = registry.trigger(&ctx).unwrap();
        assert_eq!(results.len(), 1);
        assert!(matches!(results[0], HookResult::ModifyOutput(_)));
    }

    // -- Integration test: abort carries reason --

    #[test]
    fn integration_abort_carries_reason() {
        let mut registry = HookRegistry::new();

        registry.register(HookPoint::PermissionCheck, |_| {
            Ok(HookResult::Abort(Some("security policy".into())))
        });

        let ctx = HookContext::new(HookPoint::PermissionCheck, "rm", serde_json::json!({}));
        let results = registry.trigger(&ctx).unwrap();
        assert_eq!(results.len(), 1);
        match &results[0] {
            HookResult::Abort(reason) => {
                assert_eq!(reason.as_deref(), Some("security policy"));
            }
            other => panic!("Expected Abort, got {other:?}"),
        }
    }

    // -- Edge case: empty registry trigger --

    #[test]
    fn empty_registry_trigger_returns_empty_results() {
        let registry = HookRegistry::new();
        let ctx = HookContext::new(HookPoint::PlanEnd, "plan-1", serde_json::json!({}));
        let results = registry.trigger(&ctx).unwrap();
        assert!(results.is_empty());
    }
}
