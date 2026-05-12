//! Runtime tool execution operations.

use std::path::Path;

use anyhow::{bail, Result};
use chrono::Utc;
use rustycode_protocol::{EventKind, Session, SessionEvent, SessionId, ToolCall, ToolResult};
use rustycode_tools::ToolContext;
use tracing::info;

use super::{Runtime, ToolCallReport};

impl Runtime {
    /// Execute a tool call within a session.
    ///
    /// Checks the cache before executing the tool. If a cached result exists
    /// and is not expired, returns the cached result. Otherwise, executes the
    /// tool and caches the result.
    pub fn execute_tool(
        &self,
        session_id: &SessionId,
        call: ToolCall,
        cwd: &Path,
    ) -> Result<ToolResult> {
        self.check_tool_permission_and_publish(session_id, &call)?;

        // Check cache before executing
        // Serialize arguments for cache key computation
        let args_json = serde_json::to_value(&call.arguments).unwrap_or(serde_json::Value::Null);

        // Check cache (need to hold lock briefly)
        let cached_result = {
            let mut cache = self.tool_cache.lock().unwrap_or_else(|e| e.into_inner());
            cache.get(&call.name, &args_json).cloned()
        };

        if let Some(cached) = cached_result {
            info!(
                tool = %call.name,
                tokens_saved = cached.token_count,
                "Tool result cache hit"
            );
            // Return cached result - reconstruct ToolResult from cached content
            return Ok(ToolResult {
                call_id: call.call_id.clone(),
                output: cached.content.clone(),
                error: None,
                success: true,
                exit_code: None,
                data: None,
                new_cwd: None,
            });
        }

        let ctx = ToolContext::new(cwd).with_registry(self.tools.clone());
        let result = self.tools.execute(&call, &ctx);

        if let Ok(mut guard) = self.skill_manager.lock() {
            if let Some(ref mut mgr) = *guard {
                let args_str = serde_json::to_string(&call.arguments).unwrap_or_default();
                mgr.observe_tool_use(&call.name, &args_str);
            }
        }

        // Cache the result if successful and large enough
        if result.success {
            let content = result.output.as_str();
            let mut cache = self.tool_cache.lock().unwrap_or_else(|e| e.into_inner());
            if cache.insert(&call.name, &args_json, content) {
                info!(
                    tool = %call.name,
                    content_len = content.len(),
                    "Tool result cached"
                );
            }
        }

        Ok(result)
    }

    /// Run a single tool by name and arguments, returning a full report.
    pub fn run_tool(
        &self,
        cwd: &Path,
        name: String,
        arguments: serde_json::Value,
    ) -> Result<ToolCallReport> {
        let call_id = SessionId::new().to_string();
        let session = Session::builder().task(format!("tool={}", name)).build();
        self.storage.insert_session(&session)?;

        let call = ToolCall {
            call_id,
            name: name.clone(),
            arguments,
        };
        let ctx = ToolContext::new(cwd).with_registry(self.tools.clone());
        let result = self.tools.execute(&call, &ctx);

        Ok(ToolCallReport { session, result })
    }

    /// Check tool permissions based on session mode and publish blocked event if not permitted
    pub fn check_tool_permission_and_publish(
        &self,
        session_id: &SessionId,
        call: &ToolCall,
    ) -> Result<()> {
        // Check permissions based on session mode
        if let Ok(Some(session)) = self.storage.load_session(session_id) {
            if !rustycode_tools::check_tool_permission(&call.name, session.mode) {
                tracing::warn!(
                    tool = %call.name,
                    mode = ?session.mode,
                    "tool not permitted in current session mode"
                );

                // Publish tool blocked event
                self.publish_tool_blocked(
                    session_id.clone(),
                    call.name.clone(),
                    call.arguments.clone(),
                    format!("{:?}", session.mode),
                    format!(
                        "Tool '{}' is not permitted in {:?} mode",
                        call.name, session.mode
                    ),
                );

                // Record blocked event in storage
                self.storage.insert_event(&SessionEvent {
                    session_id: session_id.clone(),
                    at: Utc::now(),
                    kind: EventKind::ToolBlockedInPlanningMode,
                    detail: format!("tool={} mode={:?}", call.name, session.mode),
                })?;

                bail!(
                    "tool '{}' is not permitted in {:?} mode",
                    call.name,
                    session.mode
                );
            }
        }
        Ok(())
    }
}
