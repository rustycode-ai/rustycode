//! Lifecycle Hook System
//!
//! Composable event handlers for agent execution lifecycle.
//! Allows cross-cutting concerns (tracing, compaction, loop detection)
//! to be plugged into the execution loop without tight coupling.
//!
//! Inspired by `forge_domain`'s Hook system.

use std::sync::Arc;

/// Events in the agent execution lifecycle
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum LifecycleEvent {
    /// Agent execution started
    Start(StartPayload),
    /// Before sending request to LLM
    Request(RequestPayload),
    /// After receiving response from LLM
    Response(ResponsePayload),
    /// Before executing a tool call
    ToolCallStart(ToolCallStartPayload),
    /// After executing a tool call
    ToolCallEnd(ToolCallEndPayload),
    /// Agent execution ended
    End(EndPayload),
}

#[derive(Debug, Clone)]
pub struct StartPayload {
    pub conversation_id: String,
    pub model: String,
}

#[derive(Debug, Clone)]
pub struct RequestPayload {
    pub conversation_id: String,
    pub turn: usize,
    pub message_count: usize,
}

#[derive(Debug, Clone)]
pub struct ResponsePayload {
    pub conversation_id: String,
    pub turn: usize,
    pub finish_reason: Option<String>,
    pub tool_call_count: usize,
    pub usage: Option<UsagePayload>,
}

#[derive(Debug, Clone)]
pub struct ToolCallStartPayload {
    pub conversation_id: String,
    pub tool_name: String,
    pub call_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ToolCallEndPayload {
    pub conversation_id: String,
    pub tool_name: String,
    pub call_id: Option<String>,
    pub is_error: bool,
}

#[derive(Debug, Clone)]
pub struct EndPayload {
    pub conversation_id: String,
    pub total_turns: usize,
    pub total_tool_calls: usize,
}

#[derive(Debug, Clone)]
pub struct UsagePayload {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
}

/// Result of processing a lifecycle event
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum HookResult {
    /// Continue execution normally
    Continue,
    /// Stop execution with a reason
    Stop(String),
}

/// Alias to disambiguate from protocol's `HookResult`.
pub type LifecycleHookResult = HookResult;

/// Trait for handling lifecycle events
pub trait LifecycleHandler: Send + Sync {
    fn handle(&self, event: &LifecycleEvent) -> HookResult;
}

/// A no-op handler that always continues
#[derive(Debug, Default)]
pub struct NoOpHandler;

impl LifecycleHandler for NoOpHandler {
    fn handle(&self, _event: &LifecycleEvent) -> HookResult {
        HookResult::Continue
    }
}

/// Composite handler that runs multiple handlers in sequence
pub struct CompositeHandler {
    handlers: Vec<Arc<dyn LifecycleHandler>>,
}

impl CompositeHandler {
    pub fn new() -> Self {
        Self {
            handlers: Vec::new(),
        }
    }

    pub fn with(mut self, handler: Arc<dyn LifecycleHandler>) -> Self {
        self.handlers.push(handler);
        self
    }

    pub fn add(&mut self, handler: Arc<dyn LifecycleHandler>) {
        self.handlers.push(handler);
    }
}

impl Default for CompositeHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl LifecycleHandler for CompositeHandler {
    fn handle(&self, event: &LifecycleEvent) -> HookResult {
        for handler in &self.handlers {
            match handler.handle(event) {
                HookResult::Continue => continue,
                HookResult::Stop(reason) => return HookResult::Stop(reason),
            }
        }
        HookResult::Continue
    }
}

/// Tracing handler that logs all events
#[derive(Debug, Default)]
pub struct TracingHandler {
    /// Whether to log to stderr
    pub verbose: bool,
}

impl TracingHandler {
    pub const fn new() -> Self {
        Self { verbose: false }
    }

    pub const fn verbose() -> Self {
        Self { verbose: true }
    }
}

impl LifecycleHandler for TracingHandler {
    fn handle(&self, event: &LifecycleEvent) -> HookResult {
        match event {
            LifecycleEvent::Start(p) => {
                log::info!(
                    "[lifecycle] Start conversation={} model={}",
                    p.conversation_id,
                    p.model
                );
            }
            LifecycleEvent::Request(p) => {
                log::debug!(
                    "[lifecycle] Request turn={} messages={}",
                    p.turn,
                    p.message_count
                );
            }
            LifecycleEvent::Response(p) => {
                log::debug!(
                    "[lifecycle] Response turn={} tools={} finish={:?}",
                    p.turn,
                    p.tool_call_count,
                    p.finish_reason
                );
            }
            LifecycleEvent::ToolCallStart(p) => {
                log::debug!("[lifecycle] ToolStart {}", p.tool_name);
            }
            LifecycleEvent::ToolCallEnd(p) => {
                if p.is_error {
                    log::warn!("[lifecycle] ToolEnd {} (error)", p.tool_name);
                } else {
                    log::debug!("[lifecycle] ToolEnd {} (ok)", p.tool_name);
                }
            }
            LifecycleEvent::End(p) => {
                log::info!(
                    "[lifecycle] End turns={} tools={}",
                    p.total_turns,
                    p.total_tool_calls
                );
            }
        }
        HookResult::Continue
    }
}

/// The main hook dispatcher
pub struct LifecycleHooks {
    handlers: CompositeHandler,
}

impl LifecycleHooks {
    pub fn new() -> Self {
        Self {
            handlers: CompositeHandler::new(),
        }
    }

    /// Add a handler
    pub fn with_handler(mut self, handler: Arc<dyn LifecycleHandler>) -> Self {
        self.handlers.add(handler);
        self
    }

    /// Emit an event to all handlers
    pub fn emit(&self, event: LifecycleEvent) -> HookResult {
        self.handlers.handle(&event)
    }

    /// Convenience: emit start event
    pub fn on_start(&self, conversation_id: &str, model: &str) -> HookResult {
        self.emit(LifecycleEvent::Start(StartPayload {
            conversation_id: conversation_id.to_string(),
            model: model.to_string(),
        }))
    }

    /// Convenience: emit request event
    pub fn on_request(
        &self,
        conversation_id: &str,
        turn: usize,
        message_count: usize,
    ) -> HookResult {
        self.emit(LifecycleEvent::Request(RequestPayload {
            conversation_id: conversation_id.to_string(),
            turn,
            message_count,
        }))
    }

    /// Convenience: emit response event
    pub fn on_response(
        &self,
        conversation_id: &str,
        turn: usize,
        finish_reason: Option<&str>,
        tool_call_count: usize,
    ) -> HookResult {
        self.emit(LifecycleEvent::Response(ResponsePayload {
            conversation_id: conversation_id.to_string(),
            turn,
            finish_reason: finish_reason.map(ToString::to_string),
            tool_call_count,
            usage: None,
        }))
    }

    /// Convenience: emit tool call start
    pub fn on_tool_start(
        &self,
        conversation_id: &str,
        tool_name: &str,
        call_id: Option<&str>,
    ) -> HookResult {
        self.emit(LifecycleEvent::ToolCallStart(ToolCallStartPayload {
            conversation_id: conversation_id.to_string(),
            tool_name: tool_name.to_string(),
            call_id: call_id.map(ToString::to_string),
        }))
    }

    /// Convenience: emit tool call end
    pub fn on_tool_end(
        &self,
        conversation_id: &str,
        tool_name: &str,
        call_id: Option<&str>,
        is_error: bool,
    ) -> HookResult {
        self.emit(LifecycleEvent::ToolCallEnd(ToolCallEndPayload {
            conversation_id: conversation_id.to_string(),
            tool_name: tool_name.to_string(),
            call_id: call_id.map(ToString::to_string),
            is_error,
        }))
    }

    /// Convenience: emit end event
    pub fn on_end(
        &self,
        conversation_id: &str,
        total_turns: usize,
        total_tool_calls: usize,
    ) -> HookResult {
        self.emit(LifecycleEvent::End(EndPayload {
            conversation_id: conversation_id.to_string(),
            total_turns,
            total_tool_calls,
        }))
    }
}

impl Default for LifecycleHooks {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_noop_handler() {
        let handler = NoOpHandler;
        let result = handler.handle(&LifecycleEvent::Start(StartPayload {
            conversation_id: "test".into(),
            model: "claude".into(),
        }));
        assert_eq!(result, HookResult::Continue);
    }

    #[test]
    fn test_composite_stops_on_first_stop() {
        struct StopHandler;
        impl LifecycleHandler for StopHandler {
            fn handle(&self, _event: &LifecycleEvent) -> HookResult {
                HookResult::Stop("test stop".into())
            }
        }

        let mut composite = CompositeHandler::new();
        composite.add(Arc::new(StopHandler));

        let result = composite.handle(&LifecycleEvent::Start(StartPayload {
            conversation_id: "test".into(),
            model: "claude".into(),
        }));

        assert_eq!(result, HookResult::Stop("test stop".into()));
    }

    #[test]
    fn test_lifecycle_hooks_convenience() {
        let hooks = LifecycleHooks::new().with_handler(Arc::new(TracingHandler::new()));

        let result = hooks.on_start("conv-1", "claude-3.5");
        assert_eq!(result, HookResult::Continue);
    }

    // --- HookResult tests ---

    #[test]
    fn hook_result_equality() {
        assert_eq!(HookResult::Continue, HookResult::Continue);
        assert_eq!(
            HookResult::Stop("reason".into()),
            HookResult::Stop("reason".into())
        );
        assert_ne!(HookResult::Continue, HookResult::Stop("x".into()));
    }

    // --- Payload construction tests ---

    #[test]
    fn start_payload_fields() {
        let p = StartPayload {
            conversation_id: "c1".into(),
            model: "gpt-4".into(),
        };
        assert_eq!(p.conversation_id, "c1");
        assert_eq!(p.model, "gpt-4");
    }

    #[test]
    fn request_payload_fields() {
        let p = RequestPayload {
            conversation_id: "c1".into(),
            turn: 3,
            message_count: 10,
        };
        assert_eq!(p.turn, 3);
        assert_eq!(p.message_count, 10);
    }

    #[test]
    fn response_payload_with_usage() {
        let p = ResponsePayload {
            conversation_id: "c1".into(),
            turn: 2,
            finish_reason: Some("stop".into()),
            tool_call_count: 1,
            usage: Some(UsagePayload {
                input_tokens: Some(500),
                output_tokens: Some(200),
            }),
        };
        assert_eq!(p.finish_reason.unwrap(), "stop");
        assert_eq!(p.usage.unwrap().input_tokens.unwrap(), 500);
    }

    #[test]
    fn tool_call_start_payload_with_call_id() {
        let p = ToolCallStartPayload {
            conversation_id: "c1".into(),
            tool_name: "bash".into(),
            call_id: Some("call_123".into()),
        };
        assert_eq!(p.call_id.unwrap(), "call_123");
    }

    #[test]
    fn tool_call_end_payload_error() {
        let p = ToolCallEndPayload {
            conversation_id: "c1".into(),
            tool_name: "bash".into(),
            call_id: None,
            is_error: true,
        };
        assert!(p.is_error);
    }

    #[test]
    fn end_payload_fields() {
        let p = EndPayload {
            conversation_id: "c1".into(),
            total_turns: 5,
            total_tool_calls: 12,
        };
        assert_eq!(p.total_turns, 5);
        assert_eq!(p.total_tool_calls, 12);
    }

    // --- CompositeHandler tests ---

    #[test]
    fn composite_empty_continues() {
        let composite = CompositeHandler::new();
        let result = composite.handle(&LifecycleEvent::Start(StartPayload {
            conversation_id: "test".into(),
            model: "claude".into(),
        }));
        assert_eq!(result, HookResult::Continue);
    }

    #[test]
    fn composite_multiple_handlers_all_continue() {
        let mut composite = CompositeHandler::new();
        composite.add(Arc::new(NoOpHandler));
        composite.add(Arc::new(TracingHandler::new()));

        let result = composite.handle(&LifecycleEvent::End(EndPayload {
            conversation_id: "test".into(),
            total_turns: 1,
            total_tool_calls: 0,
        }));
        assert_eq!(result, HookResult::Continue);
    }

    #[test]
    fn composite_default_is_new() {
        let d = CompositeHandler::default();
        let n = CompositeHandler::new();
        // Both should handle identically (empty handlers → Continue)
        let event = LifecycleEvent::Start(StartPayload {
            conversation_id: "x".into(),
            model: "y".into(),
        });
        assert_eq!(d.handle(&event), n.handle(&event));
    }

    // --- LifecycleHooks convenience methods ---

    #[test]
    fn hooks_on_request() {
        let hooks = LifecycleHooks::new().with_handler(Arc::new(TracingHandler::new()));
        let result = hooks.on_request("c1", 1, 5);
        assert_eq!(result, HookResult::Continue);
    }

    #[test]
    fn hooks_on_response() {
        let hooks = LifecycleHooks::new().with_handler(Arc::new(TracingHandler::new()));
        let result = hooks.on_response("c1", 1, Some("stop"), 2);
        assert_eq!(result, HookResult::Continue);
    }

    #[test]
    fn hooks_on_tool_start_and_end() {
        let hooks = LifecycleHooks::new().with_handler(Arc::new(TracingHandler::new()));
        assert_eq!(
            hooks.on_tool_start("c1", "bash", Some("call_1")),
            HookResult::Continue
        );
        assert_eq!(
            hooks.on_tool_end("c1", "bash", Some("call_1"), false),
            HookResult::Continue
        );
    }

    #[test]
    fn hooks_on_end() {
        let hooks = LifecycleHooks::new().with_handler(Arc::new(TracingHandler::new()));
        assert_eq!(hooks.on_end("c1", 10, 3), HookResult::Continue);
    }

    #[test]
    fn hooks_default_matches_new() {
        let h1 = LifecycleHooks::new();
        let h2 = LifecycleHooks::default();
        let event = LifecycleEvent::Start(StartPayload {
            conversation_id: "x".into(),
            model: "y".into(),
        });
        assert_eq!(h1.emit(event.clone()), h2.emit(event));
    }

    #[test]
    fn hooks_emit_direct() {
        let hooks = LifecycleHooks::new().with_handler(Arc::new(NoOpHandler));
        let result = hooks.emit(LifecycleEvent::ToolCallEnd(ToolCallEndPayload {
            conversation_id: "c1".into(),
            tool_name: "edit".into(),
            call_id: None,
            is_error: false,
        }));
        assert_eq!(result, HookResult::Continue);
    }

    // --- TracingHandler ---

    #[test]
    fn tracing_handler_verbose_construction() {
        let h = TracingHandler::verbose();
        assert!(h.verbose);
    }

    #[test]
    fn tracing_handler_default_non_verbose() {
        let h = TracingHandler::default();
        assert!(!h.verbose);
    }

    #[test]
    fn tracing_handler_handles_all_event_types() {
        let h = TracingHandler::new();
        let events = [
            LifecycleEvent::Start(StartPayload {
                conversation_id: "c".into(),
                model: "m".into(),
            }),
            LifecycleEvent::Request(RequestPayload {
                conversation_id: "c".into(),
                turn: 1,
                message_count: 5,
            }),
            LifecycleEvent::Response(ResponsePayload {
                conversation_id: "c".into(),
                turn: 1,
                finish_reason: None,
                tool_call_count: 0,
                usage: None,
            }),
            LifecycleEvent::ToolCallStart(ToolCallStartPayload {
                conversation_id: "c".into(),
                tool_name: "bash".into(),
                call_id: None,
            }),
            LifecycleEvent::ToolCallEnd(ToolCallEndPayload {
                conversation_id: "c".into(),
                tool_name: "bash".into(),
                call_id: None,
                is_error: false,
            }),
            LifecycleEvent::End(EndPayload {
                conversation_id: "c".into(),
                total_turns: 1,
                total_tool_calls: 1,
            }),
        ];
        for event in &events {
            assert_eq!(h.handle(event), HookResult::Continue);
        }
    }
}
