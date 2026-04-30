//! Trial lifecycle hooks — callbacks for trial events.

use std::sync::Arc;

/// Events in the trial lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TrialEvent {
    /// Trial has been submitted to the queue.
    Start,
    /// Container environment is being set up.
    EnvironmentStart,
    /// Agent is about to execute.
    AgentStart,
    /// Verification is about to run.
    VerificationStart,
    /// Trial completed (success or failure).
    End,
    /// Trial was cancelled.
    Cancel,
}

impl TrialEvent {
    /// All trial events.
    pub const fn all() -> &'static [Self] {
        &[
            Self::Start,
            Self::EnvironmentStart,
            Self::AgentStart,
            Self::VerificationStart,
            Self::End,
            Self::Cancel,
        ]
    }
}

/// Context provided to hook callbacks.
#[derive(Debug, Clone)]
pub struct HookContext {
    /// Name of the task being trialed.
    pub task_name: String,
    /// Name of the agent.
    pub agent_name: String,
    /// Current attempt number (1-based).
    pub attempt: usize,
    /// The event that triggered this hook.
    pub event: TrialEvent,
}

/// Alias to disambiguate from protocol's `HookContext`.
pub type BenchHookContext = HookContext;

/// A hook callback function.
pub type HookCallback = Arc<dyn Fn(HookContext) + Send + Sync>;

/// Hook manager for trial lifecycle events.
#[derive(Default)]
pub struct Hooks {
    callbacks: Vec<(TrialEvent, HookCallback)>,
}

impl Hooks {
    /// Create a new hook manager.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a callback for a specific event.
    pub fn on(&mut self, event: TrialEvent, callback: HookCallback) -> &mut Self {
        self.callbacks.push((event, callback));
        self
    }

    /// Fire all callbacks for the given event.
    pub fn fire(&self, event: TrialEvent, ctx: &HookContext) {
        for (evt, cb) in &self.callbacks {
            if *evt == event {
                cb(ctx.clone());
            }
        }
    }

    /// Register a callback for trial start.
    pub fn on_start(&mut self, callback: HookCallback) -> &mut Self {
        self.on(TrialEvent::Start, callback)
    }

    /// Register a callback for trial end.
    pub fn on_end(&mut self, callback: HookCallback) -> &mut Self {
        self.on(TrialEvent::End, callback)
    }

    /// Register a callback for trial cancel.
    pub fn on_cancel(&mut self, callback: HookCallback) -> &mut Self {
        self.on(TrialEvent::Cancel, callback)
    }
}

/// Logging hook that prints trial events.
pub fn logging_hook(ctx: HookContext) {
    match ctx.event {
        TrialEvent::Start => {
            tracing::info!(
                "[hook] Trial started: {} (attempt {})",
                ctx.task_name,
                ctx.attempt
            );
        }
        TrialEvent::EnvironmentStart => {
            tracing::info!("[hook] Environment starting for: {}", ctx.task_name);
        }
        TrialEvent::AgentStart => {
            tracing::info!(
                "[hook] Agent {} starting for: {}",
                ctx.agent_name,
                ctx.task_name
            );
        }
        TrialEvent::VerificationStart => {
            tracing::info!("[hook] Verification starting for: {}", ctx.task_name);
        }
        TrialEvent::End => {
            tracing::info!("[hook] Trial ended: {}", ctx.task_name);
        }
        TrialEvent::Cancel => {
            tracing::warn!("[hook] Trial cancelled: {}", ctx.task_name);
        }
    }
}

/// Progress bar hook that updates an indicatif progress bar.
pub fn progress_hook(pb: indicatif::ProgressBar) -> impl Fn(HookContext) + Send + Sync {
    move |ctx| match ctx.event {
        TrialEvent::Start => {
            pb.set_message(format!("Starting: {}", ctx.task_name));
        }
        TrialEvent::End => {
            pb.inc(1);
            pb.set_message(format!("Completed: {}", ctx.task_name));
        }
        TrialEvent::Cancel => {
            pb.set_message(format!("Cancelled: {}", ctx.task_name));
        }
        _ => {}
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[test]
    fn hooks_fire_callback() {
        let fired = Arc::new(Mutex::new(false));
        let fired_clone = fired.clone();

        let mut hooks = Hooks::new();
        hooks.on(
            TrialEvent::Start,
            Arc::new(move |_ctx| {
                *fired_clone.lock().unwrap() = true;
            }),
        );

        let ctx = HookContext {
            task_name: "test".to_string(),
            agent_name: "oracle".to_string(),
            attempt: 1,
            event: TrialEvent::Start,
        };
        hooks.fire(TrialEvent::Start, &ctx);
        assert!(*fired.lock().unwrap());
    }

    #[test]
    fn hooks_dont_fire_wrong_event() {
        let fired = Arc::new(Mutex::new(false));
        let fired_clone = fired.clone();

        let mut hooks = Hooks::new();
        hooks.on(
            TrialEvent::End,
            Arc::new(move |_ctx| {
                *fired_clone.lock().unwrap() = true;
            }),
        );

        let ctx = HookContext {
            task_name: "test".to_string(),
            agent_name: "oracle".to_string(),
            attempt: 1,
            event: TrialEvent::Start,
        };
        hooks.fire(TrialEvent::Start, &ctx);
        assert!(!*fired.lock().unwrap());
    }

    #[test]
    fn hooks_on_start_convenience() {
        let count = Arc::new(Mutex::new(0));
        let count_clone = count.clone();

        let mut hooks = Hooks::new();
        hooks.on_start(Arc::new(move |_ctx| {
            *count_clone.lock().unwrap() += 1;
        }));

        let ctx = HookContext {
            task_name: "t1".to_string(),
            agent_name: "a1".to_string(),
            attempt: 1,
            event: TrialEvent::Start,
        };
        hooks.fire(TrialEvent::Start, &ctx);
        assert_eq!(*count.lock().unwrap(), 1);
    }

    #[test]
    fn trial_event_all_contains_all() {
        assert_eq!(TrialEvent::all().len(), 6);
    }

    #[test]
    fn logging_hook_doesnt_panic() {
        let ctx = HookContext {
            task_name: "test".to_string(),
            agent_name: "code".to_string(),
            attempt: 1,
            event: TrialEvent::Start,
        };
        logging_hook(ctx); // Should not panic
    }
}
