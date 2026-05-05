// Copyright 2025 The RustyCode Authors. All rights reserved.
// Use of this source code is governed by an MIT-style license.

//! Hook registry for managing and executing event hooks

use crate::hooks::{Hook, HookContext, HookPhase};
use crate::Result;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Hook registry for managing event hooks
///
/// The registry maintains hooks organized by phase and executes them in priority order.
#[derive(Default)]
pub struct HookRegistry {
    /// Pre-publish hooks
    pre_publish: RwLock<Vec<Arc<dyn Hook>>>,
    /// Post-publish hooks
    post_publish: RwLock<Vec<Arc<dyn Hook>>>,
    /// Error hooks
    on_error: RwLock<Vec<Arc<dyn Hook>>>,
}

impl HookRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a hook
    ///
    pub async fn register(&self, hook: Arc<dyn Hook>) {
        match hook.phase() {
            HookPhase::PrePublish => {
                let mut hooks = self.pre_publish.write().await;
                hooks.push(hook);
                // Keep sorted by priority (descending)
                hooks.sort_by_key(|b| std::cmp::Reverse(b.priority()));
            }
            HookPhase::PostPublish => {
                let mut hooks = self.post_publish.write().await;
                hooks.push(hook);
                // Keep sorted by priority (descending)
                hooks.sort_by_key(|b| std::cmp::Reverse(b.priority()));
            }
            HookPhase::OnError => {
                let mut hooks = self.on_error.write().await;
                hooks.push(hook);
                // Keep sorted by priority (descending)
                hooks.sort_by_key(|b| std::cmp::Reverse(b.priority()));
            }
        }
    }

    /// Execute pre-publish hooks
    ///
    pub async fn execute_pre_publish(&self, context: &HookContext) -> Result<()> {
        let hooks = self.pre_publish.read().await;
        for hook in hooks.iter() {
            hook.execute(context)?;
        }
        Ok(())
    }

    /// Execute post-publish hooks
    ///
    pub async fn execute_post_publish(&self, context: &HookContext) -> Result<()> {
        let hooks = self.post_publish.read().await;
        for hook in hooks.iter() {
            hook.execute(context)?;
        }
        Ok(())
    }

    /// Execute error hooks
    ///
    pub async fn execute_on_error(&self, context: &HookContext) -> Result<()> {
        let hooks = self.on_error.read().await;
        for hook in hooks.iter() {
            hook.execute(context)?;
        }
        Ok(())
    }

    /// Get the number of registered hooks for a phase
    pub async fn hook_count(&self, phase: HookPhase) -> usize {
        match phase {
            HookPhase::PrePublish => self.pre_publish.read().await.len(),
            HookPhase::PostPublish => self.post_publish.read().await.len(),
            HookPhase::OnError => self.on_error.read().await.len(),
        }
    }

    /// Clear all hooks for a phase
    pub async fn clear(&self, phase: HookPhase) {
        match phase {
            HookPhase::PrePublish => {
                self.pre_publish.write().await.clear();
            }
            HookPhase::PostPublish => {
                self.post_publish.write().await.clear();
            }
            HookPhase::OnError => {
                self.on_error.write().await.clear();
            }
        }
    }

    /// Clear all hooks
    pub async fn clear_all(&self) {
        self.pre_publish.write().await.clear();
        self.post_publish.write().await.clear();
        self.on_error.write().await.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::SessionStartedEvent;
    use crate::hooks::{FunctionHook, HookPriority};
    use crate::EventBusError;
    use rustycode_protocol::SessionId;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[tokio::test]
    async fn test_registry_creation() {
        let registry = HookRegistry::new();
        assert_eq!(registry.hook_count(HookPhase::PrePublish).await, 0);
        assert_eq!(registry.hook_count(HookPhase::PostPublish).await, 0);
        assert_eq!(registry.hook_count(HookPhase::OnError).await, 0);
    }

    #[tokio::test]
    async fn test_register_hooks() {
        let registry = HookRegistry::new();

        let hook1 = Arc::new(FunctionHook::new(
            "hook1",
            HookPriority::High,
            HookPhase::PrePublish,
            |_context| Ok(()),
        ));

        let hook2 = Arc::new(FunctionHook::new(
            "hook2",
            HookPriority::Low,
            HookPhase::PostPublish,
            |_context| Ok(()),
        ));

        let hook3 = Arc::new(FunctionHook::new(
            "hook3",
            HookPriority::Medium,
            HookPhase::OnError,
            |_context| Ok(()),
        ));

        registry.register(hook1).await;
        registry.register(hook2).await;
        registry.register(hook3).await;

        assert_eq!(registry.hook_count(HookPhase::PrePublish).await, 1);
        assert_eq!(registry.hook_count(HookPhase::PostPublish).await, 1);
        assert_eq!(registry.hook_count(HookPhase::OnError).await, 1);
    }

    #[tokio::test]
    async fn test_priority_ordering() {
        let registry = HookRegistry::new();

        // Track execution order
        let execution_order = Arc::new(AtomicUsize::new(0));
        let order1 = execution_order.clone();
        let order2 = execution_order.clone();
        let order3 = execution_order.clone();

        // Register hooks in reverse priority order
        let hook_low = Arc::new(FunctionHook::new(
            "low",
            HookPriority::Low,
            HookPhase::PrePublish,
            move |_context| {
                order1.fetch_add(1, Ordering::SeqCst);
                Ok(())
            },
        ));

        let hook_high = Arc::new(FunctionHook::new(
            "high",
            HookPriority::High,
            HookPhase::PrePublish,
            move |_context| {
                order2.fetch_add(1, Ordering::SeqCst);
                Ok(())
            },
        ));

        let hook_medium = Arc::new(FunctionHook::new(
            "medium",
            HookPriority::Medium,
            HookPhase::PrePublish,
            move |_context| {
                order3.fetch_add(1, Ordering::SeqCst);
                Ok(())
            },
        ));

        // Register in low, high, medium order
        registry.register(hook_low).await;
        registry.register(hook_high).await;
        registry.register(hook_medium).await;

        // Create context and execute
        let event = SessionStartedEvent::new(
            SessionId::new(),
            "Test task".to_string(),
            "Test context".to_string(),
        );
        let context = HookContext::new(Box::new(event), HookPhase::PrePublish);

        registry.execute_pre_publish(&context).await.unwrap();

        // Verify execution order: high (3), medium (2), low (1)
        assert_eq!(execution_order.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_hook_execution() {
        let registry = HookRegistry::new();

        let executed = Arc::new(AtomicUsize::new(0));
        let executed_clone = executed.clone();

        let hook = Arc::new(FunctionHook::new(
            "test_hook",
            HookPriority::High,
            HookPhase::PrePublish,
            move |_context| {
                executed_clone.fetch_add(1, Ordering::SeqCst);
                Ok(())
            },
        ));

        registry.register(hook).await;

        let event = SessionStartedEvent::new(
            SessionId::new(),
            "Test task".to_string(),
            "Test context".to_string(),
        );
        let context = HookContext::new(Box::new(event), HookPhase::PrePublish);

        registry.execute_pre_publish(&context).await.unwrap();
        assert_eq!(executed.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_hook_execution_error() {
        let registry = HookRegistry::new();

        let hook = Arc::new(FunctionHook::new(
            "error_hook",
            HookPriority::High,
            HookPhase::PrePublish,
            |_context| Err(EventBusError::HookError("Test error".to_string())),
        ));

        registry.register(hook).await;

        let event = SessionStartedEvent::new(
            SessionId::new(),
            "Test task".to_string(),
            "Test context".to_string(),
        );
        let context = HookContext::new(Box::new(event), HookPhase::PrePublish);

        let result = registry.execute_pre_publish(&context).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_clear_hooks() {
        let registry = HookRegistry::new();

        let hook = Arc::new(FunctionHook::new(
            "hook",
            HookPriority::High,
            HookPhase::PrePublish,
            |_context| Ok(()),
        ));

        registry.register(hook).await;
        assert_eq!(registry.hook_count(HookPhase::PrePublish).await, 1);

        registry.clear(HookPhase::PrePublish).await;
        assert_eq!(registry.hook_count(HookPhase::PrePublish).await, 0);
    }

    #[tokio::test]
    async fn test_clear_all_hooks() {
        let registry = HookRegistry::new();

        let hook1 = Arc::new(FunctionHook::new(
            "hook1",
            HookPriority::High,
            HookPhase::PrePublish,
            |_context| Ok(()),
        ));

        let hook2 = Arc::new(FunctionHook::new(
            "hook2",
            HookPriority::High,
            HookPhase::PostPublish,
            |_context| Ok(()),
        ));

        let hook3 = Arc::new(FunctionHook::new(
            "hook3",
            HookPriority::High,
            HookPhase::OnError,
            |_context| Ok(()),
        ));

        registry.register(hook1).await;
        registry.register(hook2).await;
        registry.register(hook3).await;

        assert_eq!(registry.hook_count(HookPhase::PrePublish).await, 1);
        assert_eq!(registry.hook_count(HookPhase::PostPublish).await, 1);
        assert_eq!(registry.hook_count(HookPhase::OnError).await, 1);

        registry.clear_all().await;

        assert_eq!(registry.hook_count(HookPhase::PrePublish).await, 0);
        assert_eq!(registry.hook_count(HookPhase::PostPublish).await, 0);
        assert_eq!(registry.hook_count(HookPhase::OnError).await, 0);
    }

    #[tokio::test]
    async fn test_multiple_hooks_same_priority() {
        let registry = HookRegistry::new();

        let hook1 = Arc::new(FunctionHook::new(
            "hook1",
            HookPriority::High,
            HookPhase::PrePublish,
            |_context| Ok(()),
        ));

        let hook2 = Arc::new(FunctionHook::new(
            "hook2",
            HookPriority::High,
            HookPhase::PrePublish,
            |_context| Ok(()),
        ));

        registry.register(hook1).await;
        registry.register(hook2).await;

        assert_eq!(registry.hook_count(HookPhase::PrePublish).await, 2);
    }

    #[tokio::test]
    async fn test_execute_all_phases() {
        let registry = HookRegistry::new();

        let pre_count = Arc::new(AtomicUsize::new(0));
        let post_count = Arc::new(AtomicUsize::new(0));
        let error_count = Arc::new(AtomicUsize::new(0));

        let pre_hook = Arc::new(FunctionHook::new(
            "pre",
            HookPriority::High,
            HookPhase::PrePublish,
            {
                let count = pre_count.clone();
                move |_context| {
                    count.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                }
            },
        ));

        let post_hook = Arc::new(FunctionHook::new(
            "post",
            HookPriority::High,
            HookPhase::PostPublish,
            {
                let count = post_count.clone();
                move |_context| {
                    count.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                }
            },
        ));

        let error_hook = Arc::new(FunctionHook::new(
            "error",
            HookPriority::High,
            HookPhase::OnError,
            {
                let count = error_count.clone();
                move |_context| {
                    count.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                }
            },
        ));

        registry.register(pre_hook).await;
        registry.register(post_hook).await;
        registry.register(error_hook).await;

        let event = SessionStartedEvent::new(
            SessionId::new(),
            "Test task".to_string(),
            "Test context".to_string(),
        );

        let pre_context = HookContext::new(Box::new(event.clone()), HookPhase::PrePublish);
        let post_context = HookContext::new(Box::new(event.clone()), HookPhase::PostPublish);
        let error_context = HookContext::new_error(
            Box::new(event),
            EventBusError::HookError("Test error".to_string()),
        );

        registry.execute_pre_publish(&pre_context).await.unwrap();
        registry.execute_post_publish(&post_context).await.unwrap();
        registry.execute_on_error(&error_context).await.unwrap();

        assert_eq!(pre_count.load(Ordering::SeqCst), 1);
        assert_eq!(post_count.load(Ordering::SeqCst), 1);
        assert_eq!(error_count.load(Ordering::SeqCst), 1);
    }

    // ── Additional hook registry tests ─────────────

    #[tokio::test]
    async fn test_default_is_same_as_new() {
        let registry1 = HookRegistry::new();
        let registry2 = HookRegistry::default();
        assert_eq!(registry1.hook_count(HookPhase::PrePublish).await, 0);
        assert_eq!(registry2.hook_count(HookPhase::PrePublish).await, 0);
    }

    #[tokio::test]
    async fn test_execute_empty_phases() {
        let registry = HookRegistry::new();

        let event = SessionStartedEvent::new(
            SessionId::new(),
            "Test task".to_string(),
            "Test context".to_string(),
        );

        // Executing with no hooks should succeed
        let ctx = HookContext::new(Box::new(event.clone()), HookPhase::PrePublish);
        registry.execute_pre_publish(&ctx).await.unwrap();

        let ctx = HookContext::new(Box::new(event.clone()), HookPhase::PostPublish);
        registry.execute_post_publish(&ctx).await.unwrap();

        let ctx = HookContext::new_error(Box::new(event), EventBusError::HookError("x".into()));
        registry.execute_on_error(&ctx).await.unwrap();
    }

    #[tokio::test]
    async fn test_clear_phase_does_not_affect_others() {
        let registry = HookRegistry::new();

        let hook_pre = Arc::new(FunctionHook::new(
            "pre",
            HookPriority::High,
            HookPhase::PrePublish,
            |_context| Ok(()),
        ));
        let hook_post = Arc::new(FunctionHook::new(
            "post",
            HookPriority::High,
            HookPhase::PostPublish,
            |_context| Ok(()),
        ));

        registry.register(hook_pre).await;
        registry.register(hook_post).await;

        registry.clear(HookPhase::PrePublish).await;

        assert_eq!(registry.hook_count(HookPhase::PrePublish).await, 0);
        assert_eq!(registry.hook_count(HookPhase::PostPublish).await, 1);
        assert_eq!(registry.hook_count(HookPhase::OnError).await, 0);
    }

    #[tokio::test]
    async fn test_clear_already_empty_phase() {
        let registry = HookRegistry::new();
        // Clearing an already-empty phase should be a no-op
        registry.clear(HookPhase::OnError).await;
        assert_eq!(registry.hook_count(HookPhase::OnError).await, 0);
    }

    #[tokio::test]
    async fn test_hook_error_stops_execution() {
        let registry = HookRegistry::new();
        let executed = Arc::new(AtomicUsize::new(0));

        // Register a failing hook first (high priority)
        let hook_fail = Arc::new(FunctionHook::new(
            "fail",
            HookPriority::High,
            HookPhase::PrePublish,
            |_context| Err(EventBusError::HookError("fail".into())),
        ));

        // Register a second hook that should NOT run
        let exec_clone = executed.clone();
        let hook_after = Arc::new(FunctionHook::new(
            "after",
            HookPriority::Low,
            HookPhase::PrePublish,
            move |_context| {
                exec_clone.fetch_add(1, Ordering::SeqCst);
                Ok(())
            },
        ));

        registry.register(hook_fail).await;
        registry.register(hook_after).await;

        let event = SessionStartedEvent::new(SessionId::new(), "task".into(), "detail".into());
        let ctx = HookContext::new(Box::new(event), HookPhase::PrePublish);

        let result = registry.execute_pre_publish(&ctx).await;
        assert!(result.is_err());
        assert_eq!(executed.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn test_many_hooks_same_phase() {
        let registry = HookRegistry::new();
        let count = Arc::new(AtomicUsize::new(0));

        for _ in 0..10 {
            let c = count.clone();
            let hook = Arc::new(FunctionHook::new(
                "hook",
                HookPriority::Medium,
                HookPhase::PostPublish,
                move |_context| {
                    c.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                },
            ));
            registry.register(hook).await;
        }

        assert_eq!(registry.hook_count(HookPhase::PostPublish).await, 10);

        let event = SessionStartedEvent::new(SessionId::new(), "task".into(), "detail".into());
        let ctx = HookContext::new(Box::new(event), HookPhase::PostPublish);
        registry.execute_post_publish(&ctx).await.unwrap();
        assert_eq!(count.load(Ordering::SeqCst), 10);
    }

    #[tokio::test]
    async fn test_register_all_phases_counts() {
        let registry = HookRegistry::new();

        for phase in [
            HookPhase::PrePublish,
            HookPhase::PostPublish,
            HookPhase::OnError,
        ] {
            for i in 0..3 {
                let hook = Arc::new(FunctionHook::new(
                    format!("hook-{phase:?}-{i}"),
                    HookPriority::Medium,
                    phase,
                    |_context| Ok(()),
                ));
                registry.register(hook).await;
            }
        }

        assert_eq!(registry.hook_count(HookPhase::PrePublish).await, 3);
        assert_eq!(registry.hook_count(HookPhase::PostPublish).await, 3);
        assert_eq!(registry.hook_count(HookPhase::OnError).await, 3);
    }

    #[tokio::test]
    async fn test_clear_all_then_register() {
        let registry = HookRegistry::new();

        let hook = Arc::new(FunctionHook::new(
            "hook",
            HookPriority::High,
            HookPhase::PrePublish,
            |_context| Ok(()),
        ));
        registry.register(hook).await;
        registry.clear_all().await;
        assert_eq!(registry.hook_count(HookPhase::PrePublish).await, 0);

        // Re-register after clear
        let hook2 = Arc::new(FunctionHook::new(
            "hook2",
            HookPriority::Medium,
            HookPhase::PrePublish,
            |_context| Ok(()),
        ));
        registry.register(hook2).await;
        assert_eq!(registry.hook_count(HookPhase::PrePublish).await, 1);
    }
}
