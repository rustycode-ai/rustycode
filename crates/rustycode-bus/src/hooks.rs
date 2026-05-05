// Copyright 2025 The RustyCode Authors. All rights reserved.
// Use of this source code is governed by an MIT-style license.

//! Hook system for event bus
//!
//! Provides priority-based hook execution for pre/post processing of events.

use crate::{Event, EventBusError};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Hook execution priority
///
/// Hooks are executed in priority order: High -> Medium -> Low
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[non_exhaustive]
pub enum HookPriority {
    /// High priority hooks execute first
    High = 3,
    /// Medium priority hooks execute after high priority
    Medium = 2,
    /// Low priority hooks execute last
    Low = 1,
}

impl fmt::Display for HookPriority {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::High => write!(f, "High"),
            Self::Medium => write!(f, "Medium"),
            Self::Low => write!(f, "Low"),
        }
    }
}

/// Hook execution phase
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum HookPhase {
    /// Before event is published to subscribers
    PrePublish,
    /// After event has been published to all subscribers
    PostPublish,
    /// When an error occurs during event processing
    OnError,
}

impl fmt::Display for HookPhase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PrePublish => write!(f, "PrePublish"),
            Self::PostPublish => write!(f, "PostPublish"),
            Self::OnError => write!(f, "OnError"),
        }
    }
}

/// Context provided to hooks during execution
pub struct HookContext {
    /// The event being processed
    pub event: Box<dyn Event>,
    /// The phase of hook execution
    pub phase: HookPhase,
    /// Hook execution timestamp
    pub timestamp: DateTime<Utc>,
    /// Optional error information (only present in `OnError` phase)
    pub error: Option<EventBusError>,
}

impl HookContext {
    pub fn new(event: Box<dyn Event>, phase: HookPhase) -> Self {
        Self {
            event,
            phase,
            timestamp: Utc::now(),
            error: None,
        }
    }

    /// Create a new error context
    pub fn new_error(event: Box<dyn Event>, error: EventBusError) -> Self {
        Self {
            event,
            phase: HookPhase::OnError,
            timestamp: Utc::now(),
            error: Some(error),
        }
    }
}

impl fmt::Debug for HookContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HookContext")
            .field("event_type", &self.event.event_type())
            .field("phase", &self.phase)
            .field("timestamp", &self.timestamp)
            .field("error", &self.error)
            .finish()
    }
}

/// Result of hook execution
pub type HookResult = Result<(), EventBusError>;

/// Trait for event hooks
///
/// Hooks can be registered with the event bus to process events at different phases.
pub trait Hook: Send + Sync + 'static {
    /// Get the hook priority
    fn priority(&self) -> HookPriority;

    /// Get the hook phase
    fn phase(&self) -> HookPhase;

    /// Execute the hook
    ///
    fn execute(&self, context: &HookContext) -> HookResult;

    /// Get hook name for debugging
    fn name(&self) -> &str {
        std::any::type_name::<Self>()
    }
}

/// Simple function-based hook
pub struct FunctionHook {
    /// Hook name
    name: String,
    /// Hook priority
    priority: HookPriority,
    /// Hook phase
    phase: HookPhase,
    /// Hook execution function
    f: Box<dyn Fn(&HookContext) -> HookResult + Send + Sync>,
}

impl FunctionHook {
    pub fn new(
        name: impl Into<String>,
        priority: HookPriority,
        phase: HookPhase,
        f: impl Fn(&HookContext) -> HookResult + Send + Sync + 'static,
    ) -> Self {
        Self {
            name: name.into(),
            priority,
            phase,
            f: Box::new(f),
        }
    }
}

impl Hook for FunctionHook {
    fn priority(&self) -> HookPriority {
        self.priority
    }

    fn phase(&self) -> HookPhase {
        self.phase
    }

    fn execute(&self, context: &HookContext) -> HookResult {
        (self.f)(context)
    }

    fn name(&self) -> &str {
        &self.name
    }
}

/// Alias for [`HookContext`] to avoid name collision with protocol's hook context.
/// Use this in new code when referring to the event bus hook context.
pub type EventHookContext = HookContext;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::SessionStartedEvent;
    use rustycode_protocol::SessionId;

    #[test]
    fn test_priority_ordering() {
        assert!(HookPriority::High > HookPriority::Medium);
        assert!(HookPriority::Medium > HookPriority::Low);
        assert!(HookPriority::High > HookPriority::Low);
    }

    #[test]
    fn test_priority_display() {
        assert_eq!(HookPriority::High.to_string(), "High");
        assert_eq!(HookPriority::Medium.to_string(), "Medium");
        assert_eq!(HookPriority::Low.to_string(), "Low");
    }

    #[test]
    fn test_phase_display() {
        assert_eq!(HookPhase::PrePublish.to_string(), "PrePublish");
        assert_eq!(HookPhase::PostPublish.to_string(), "PostPublish");
        assert_eq!(HookPhase::OnError.to_string(), "OnError");
    }

    #[test]
    fn test_hook_context_creation() {
        let event = SessionStartedEvent::new(
            SessionId::new(),
            "Test task".to_string(),
            "Test context".to_string(),
        );
        let context = HookContext::new(Box::new(event), HookPhase::PrePublish);

        assert_eq!(context.phase, HookPhase::PrePublish);
        assert!(context.error.is_none());
        assert_eq!(context.event.event_type(), "session.started");
    }

    #[test]
    fn test_hook_context_error() {
        let event = SessionStartedEvent::new(
            SessionId::new(),
            "Test task".to_string(),
            "Test context".to_string(),
        );
        let error = EventBusError::HookError("Test error".to_string());
        let context = HookContext::new_error(Box::new(event), error);

        assert_eq!(context.phase, HookPhase::OnError);
        assert!(context.error.is_some());
        assert_eq!(context.event.event_type(), "session.started");
    }

    #[test]
    fn test_function_hook() {
        let hook = FunctionHook::new(
            "test_hook",
            HookPriority::High,
            HookPhase::PrePublish,
            |_context| Ok(()),
        );

        assert_eq!(hook.priority(), HookPriority::High);
        assert_eq!(hook.phase(), HookPhase::PrePublish);
        assert_eq!(hook.name(), "test_hook");
    }

    #[test]
    fn test_function_hook_execution() {
        let executed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let executed_clone = executed.clone();

        let hook = FunctionHook::new(
            "test_hook",
            HookPriority::High,
            HookPhase::PrePublish,
            move |_context| {
                executed_clone.store(true, std::sync::atomic::Ordering::SeqCst);
                Ok(())
            },
        );

        let event = SessionStartedEvent::new(
            SessionId::new(),
            "Test task".to_string(),
            "Test context".to_string(),
        );
        let context = HookContext::new(Box::new(event), HookPhase::PrePublish);

        hook.execute(&context).unwrap();
        assert!(executed.load(std::sync::atomic::Ordering::SeqCst));
    }

    #[test]
    fn test_function_hook_error() {
        let hook = FunctionHook::new(
            "test_hook",
            HookPriority::High,
            HookPhase::PrePublish,
            |_context| Err(EventBusError::HookError("Hook failed".to_string())),
        );

        let event = SessionStartedEvent::new(
            SessionId::new(),
            "Test task".to_string(),
            "Test context".to_string(),
        );
        let context = HookContext::new(Box::new(event), HookPhase::PrePublish);

        let result = hook.execute(&context);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().to_string(), "Hook error: Hook failed");
    }

    // ── Serde roundtrips for HookPriority and HookPhase ─────────────

    #[test]
    fn test_priority_serde_roundtrip() {
        for priority in [HookPriority::High, HookPriority::Medium, HookPriority::Low] {
            let json = serde_json::to_string(&priority).unwrap();
            let decoded: HookPriority = serde_json::from_str(&json).unwrap();
            assert_eq!(priority, decoded);
        }
    }

    #[test]
    fn test_phase_serde_roundtrip() {
        for phase in [
            HookPhase::PrePublish,
            HookPhase::PostPublish,
            HookPhase::OnError,
        ] {
            let json = serde_json::to_string(&phase).unwrap();
            let decoded: HookPhase = serde_json::from_str(&json).unwrap();
            assert_eq!(phase, decoded);
        }
    }

    #[test]
    fn test_priority_serde_values() {
        let high_json = serde_json::to_value(HookPriority::High).unwrap();
        assert_eq!(high_json, serde_json::json!("High"));

        let medium_json = serde_json::to_value(HookPriority::Medium).unwrap();
        assert_eq!(medium_json, serde_json::json!("Medium"));

        let low_json = serde_json::to_value(HookPriority::Low).unwrap();
        assert_eq!(low_json, serde_json::json!("Low"));
    }

    #[test]
    fn test_phase_serde_values() {
        let pre = serde_json::to_value(HookPhase::PrePublish).unwrap();
        assert_eq!(pre, serde_json::json!("PrePublish"));

        let post = serde_json::to_value(HookPhase::PostPublish).unwrap();
        assert_eq!(post, serde_json::json!("PostPublish"));

        let on_err = serde_json::to_value(HookPhase::OnError).unwrap();
        assert_eq!(on_err, serde_json::json!("OnError"));
    }

    #[test]
    fn test_priority_ordering_values() {
        assert_eq!(HookPriority::High as i32, 3);
        assert_eq!(HookPriority::Medium as i32, 2);
        assert_eq!(HookPriority::Low as i32, 1);
    }

    #[test]
    fn test_priority_total_order() {
        let mut priorities = vec![HookPriority::Low, HookPriority::High, HookPriority::Medium];
        priorities.sort();
        assert_eq!(
            priorities,
            vec![HookPriority::Low, HookPriority::Medium, HookPriority::High]
        );
    }

    #[test]
    fn test_hook_context_debug() {
        let event = SessionStartedEvent::new(
            SessionId::new(),
            "Test task".to_string(),
            "Test context".to_string(),
        );
        let context = HookContext::new(Box::new(event), HookPhase::PrePublish);
        let debug_str = format!("{context:?}");
        assert!(debug_str.contains("HookContext"));
        assert!(debug_str.contains("session.started"));
        assert!(debug_str.contains("PrePublish"));
    }

    #[test]
    fn test_hook_context_error_debug() {
        let event = SessionStartedEvent::new(
            SessionId::new(),
            "Test task".to_string(),
            "Test context".to_string(),
        );
        let context = HookContext::new_error(
            Box::new(event),
            EventBusError::HookError("test error".to_string()),
        );
        let debug_str = format!("{context:?}");
        assert!(debug_str.contains("OnError"));
        assert!(debug_str.contains("HookError"));
    }

    #[test]
    fn test_hook_context_error_has_no_error_phase_mismatch() {
        // new() always sets error to None regardless
        let event = SessionStartedEvent::new(SessionId::new(), "task".into(), "detail".into());
        let ctx = HookContext::new(Box::new(event), HookPhase::OnError);
        assert_eq!(ctx.phase, HookPhase::OnError);
        assert!(ctx.error.is_none(), "new() should not set error field");
    }

    #[test]
    fn test_hook_context_new_error_sets_on_error_phase() {
        let event = SessionStartedEvent::new(SessionId::new(), "task".into(), "detail".into());
        let ctx = HookContext::new_error(
            Box::new(event),
            EventBusError::SerializationError("bad json".into()),
        );
        assert_eq!(ctx.phase, HookPhase::OnError);
        assert!(ctx.error.is_some());
        match &ctx.error {
            Some(EventBusError::SerializationError(msg)) => assert_eq!(msg, "bad json"),
            other => panic!("Expected SerializationError, got {other:?}"),
        }
    }

    #[test]
    fn test_function_hook_name() {
        let hook = FunctionHook::new(
            "my_custom_hook",
            HookPriority::Medium,
            HookPhase::PostPublish,
            |_| Ok(()),
        );
        assert_eq!(hook.name(), "my_custom_hook");
    }

    #[test]
    fn test_function_hook_empty_name() {
        let hook = FunctionHook::new("", HookPriority::Low, HookPhase::PrePublish, |_| Ok(()));
        assert_eq!(hook.name(), "");
    }

    #[test]
    fn test_hook_priority_copy() {
        let p = HookPriority::High;
        let p2 = p; // Copy semantics
        assert_eq!(p, p2);
    }

    #[test]
    fn test_hook_phase_copy() {
        let p = HookPhase::OnError;
        let p2 = p;
        assert_eq!(p, p2);
    }

    #[test]
    fn test_hook_priority_hash() {
        use std::collections::HashSet;
        let set: HashSet<HookPriority> = vec![
            HookPriority::High,
            HookPriority::Medium,
            HookPriority::Low,
            HookPriority::High, // duplicate
        ]
        .into_iter()
        .collect();
        assert_eq!(set.len(), 3);
    }
}
