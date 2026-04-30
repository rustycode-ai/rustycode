use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleHook {
    PreToolUse,
    PostToolUse,
    ToolError,
    SessionStart,
    SessionEnd,
    SessionError,
    CwdChanged,
    SubagentStart,
    SubagentEnd,
    PlanStart,
    PlanEnd,
    ErrorRecovery,
    ContextSwitch,
    SkillActivate,
    SkillDeactivate,
    PermissionCheck,
    PermissionGranted,
    PermissionDenied,
    TierPromoted,
    TierScopeChanged,
    BudgetWarning,
    BudgetEviction,
}

impl LifecycleHook {
    #[must_use]
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

    #[must_use]
    pub const fn is_pre(&self) -> bool {
        matches!(
            self,
            Self::PreToolUse
                | Self::SessionStart
                | Self::SubagentStart
                | Self::PlanStart
                | Self::SkillActivate
                | Self::PermissionCheck
                | Self::TierPromoted
        )
    }

    #[must_use]
    pub const fn is_post(&self) -> bool {
        !self.is_pre()
    }
}

impl std::fmt::Display for LifecycleHook {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            Self::PreToolUse => "pre_tool_use",
            Self::PostToolUse => "post_tool_use",
            Self::ToolError => "tool_error",
            Self::SessionStart => "session_start",
            Self::SessionEnd => "session_end",
            Self::SessionError => "session_error",
            Self::CwdChanged => "cwd_changed",
            Self::SubagentStart => "subagent_start",
            Self::SubagentEnd => "subagent_end",
            Self::PlanStart => "plan_start",
            Self::PlanEnd => "plan_end",
            Self::ErrorRecovery => "error_recovery",
            Self::ContextSwitch => "context_switch",
            Self::SkillActivate => "skill_activate",
            Self::SkillDeactivate => "skill_deactivate",
            Self::PermissionCheck => "permission_check",
            Self::PermissionGranted => "permission_granted",
            Self::PermissionDenied => "permission_denied",
            Self::TierPromoted => "tier_promoted",
            Self::TierScopeChanged => "tier_scope_changed",
            Self::BudgetWarning => "budget_warning",
            Self::BudgetEviction => "budget_eviction",
        };
        f.write_str(label)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LifecycleEvent {
    pub hook: LifecycleHook,
    pub subject: String,
    pub metadata: serde_json::Value,
}

impl LifecycleEvent {
    pub fn new(
        hook: LifecycleHook,
        subject: impl Into<String>,
        metadata: serde_json::Value,
    ) -> Self {
        Self {
            hook,
            subject: subject.into(),
            metadata,
        }
    }
}

pub type HookHandlerFn = dyn Fn(&LifecycleEvent) -> Result<()> + Send + Sync + 'static;

#[derive(Default)]
pub struct ExpandedHookDispatcher {
    handlers: HashMap<LifecycleHook, Vec<Arc<HookHandlerFn>>>,
}

impl ExpandedHookDispatcher {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register<F>(&mut self, hook: LifecycleHook, handler: F)
    where
        F: Fn(&LifecycleEvent) -> Result<()> + Send + Sync + 'static,
    {
        self.handlers
            .entry(hook)
            .or_default()
            .push(Arc::new(handler));
    }

    pub fn dispatch(&self, event: &LifecycleEvent) -> Result<()> {
        if let Some(handlers) = self.handlers.get(&event.hook) {
            for handler in handlers {
                handler(event)?;
            }
        }
        Ok(())
    }

    pub fn handler_count(&self, hook: LifecycleHook) -> usize {
        self.handlers.get(&hook).map_or(0, Vec::len)
    }

    pub fn clear_handlers(&mut self, hook: LifecycleHook) {
        self.handlers.remove(&hook);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn lifecycle_hook_all_has_expected_count() {
        assert_eq!(LifecycleHook::all().len(), 22);
    }

    #[test]
    fn lifecycle_hook_pre_post_classification() {
        assert!(LifecycleHook::PreToolUse.is_pre());
        assert!(!LifecycleHook::PreToolUse.is_post());
        assert!(LifecycleHook::PostToolUse.is_post());
        assert!(!LifecycleHook::PostToolUse.is_pre());
        assert!(LifecycleHook::CwdChanged.is_post());
    }

    #[test]
    fn lifecycle_hook_display_and_serde_roundtrip() {
        for hook in LifecycleHook::all() {
            let text = hook.to_string();
            assert!(!text.is_empty());
            let json = serde_json::to_string(hook).unwrap();
            let decoded: LifecycleHook = serde_json::from_str(&json).unwrap();
            assert_eq!(*hook, decoded);
        }
    }

    #[test]
    fn lifecycle_event_serialization_roundtrip() {
        let event = LifecycleEvent::new(
            LifecycleHook::PreToolUse,
            "bash",
            serde_json::json!({"command": "ls"}),
        );
        let json = serde_json::to_string(&event).unwrap();
        let decoded: LifecycleEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.hook, LifecycleHook::PreToolUse);
        assert_eq!(decoded.subject, "bash");
    }

    #[test]
    fn dispatcher_register_and_dispatch() {
        let mut dispatcher = ExpandedHookDispatcher::new();
        let count = Arc::new(AtomicUsize::new(0));
        let count_clone = Arc::clone(&count);
        dispatcher.register(LifecycleHook::PreToolUse, move |event| {
            assert_eq!(event.subject, "bash");
            count_clone.fetch_add(1, Ordering::SeqCst);
            Ok(())
        });

        let event = LifecycleEvent::new(
            LifecycleHook::PreToolUse,
            "bash",
            serde_json::json!({"tool": "bash"}),
        );
        dispatcher.dispatch(&event).unwrap();
        assert_eq!(count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn dispatcher_handler_count_and_clear() {
        let mut dispatcher = ExpandedHookDispatcher::new();
        dispatcher.register(LifecycleHook::SessionStart, |_| Ok(()));
        dispatcher.register(LifecycleHook::SessionStart, |_| Ok(()));
        assert_eq!(dispatcher.handler_count(LifecycleHook::SessionStart), 2);
        dispatcher.clear_handlers(LifecycleHook::SessionStart);
        assert_eq!(dispatcher.handler_count(LifecycleHook::SessionStart), 0);
    }

    #[test]
    fn dispatcher_multiple_handlers_run_in_order() {
        let mut dispatcher = ExpandedHookDispatcher::new();
        let log = Arc::new(std::sync::Mutex::new(Vec::new()));
        let log1 = Arc::clone(&log);
        dispatcher.register(LifecycleHook::BudgetWarning, move |_| {
            log1.lock().unwrap().push("first");
            Ok(())
        });
        let log2 = Arc::clone(&log);
        dispatcher.register(LifecycleHook::BudgetWarning, move |_| {
            log2.lock().unwrap().push("second");
            Ok(())
        });

        dispatcher
            .dispatch(&LifecycleEvent::new(
                LifecycleHook::BudgetWarning,
                "session-1",
                serde_json::json!({}),
            ))
            .unwrap();
        assert_eq!(*log.lock().unwrap(), vec!["first", "second"]);
    }
}
