//! Enhanced hooks system for Runtime.
//!
//! This module provides a comprehensive hooks system based on Claude SDK patterns,
//! enabling control over tool execution, session events, and subagent lifecycle.
//!
//! ## Hook Types
//!
//! - **PreToolUseHook**: Before tool execution (can allow/deny/modify)
//! - **PostToolUseHook**: After tool execution (observability)
//! - **UserPromptSubmitHook**: Before sending user message (can sanitize/validate)
//! - **StopHook**: When session stops (cleanup, logging)
//! - **SubagentStartHook**: When subagent starts (tracking)
//! - **SubagentStopHook**: When subagent stops (cleanup)
//!
//! ## Usage
//!
//! ```ignore
//! use rustycode_core::integration::*;
//! use rustycode_protocol::SessionId;
//! use std::sync::Arc;
//!
//! // Create a hook registry
//! let mut registry = HookRegistry::new();
//!
//! // Register a pre-tool-use hook
//! registry.register_pre_tool_use(Box::new(MyPermissionHook));
//!
//! // Register a post-tool-use hook
//! registry.register_post_tool_use(Box::new(MyLoggingHook));
//! ```

#![allow(unused_imports)]

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

/// Hook context containing metadata about the current operation
#[derive(Debug, Clone)]
pub struct HookContext {
    /// Agent identifier
    pub agent_id: String,
    /// Session identifier
    pub session_id: String,
    /// Additional metadata
    pub metadata: HashMap<String, String>,
}

impl HookContext {
    /// Create a new hook context
    pub fn new(agent_id: String, session_id: String) -> Self {
        Self {
            agent_id,
            session_id,
            metadata: HashMap::new(),
        }
    }

    /// Add metadata to the context
    pub fn with_metadata(mut self, key: String, value: String) -> Self {
        self.metadata.insert(key, value);
        self
    }
}

/// Action result from a hook that can control execution flow
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq)]
pub enum HookAction {
    /// Allow the operation to proceed
    Allow,
    /// Deny the operation with a reason
    Deny(String),
    /// Modify the arguments/prompt before proceeding
    Modify(Value),
}

/// Pre-tool-use hook - called before tool execution
///
/// This hook can:
/// - Allow the tool to execute
/// - Deny execution with a reason
/// - Modify the tool arguments before execution
#[async_trait]
pub trait PreToolUseHook: Send + Sync + std::fmt::Debug {
    /// Execute the hook before tool use
    async fn execute(
        &self,
        ctx: &HookContext,
        tool_name: &str,
        arguments: &Value,
    ) -> Result<HookAction>;
}

/// Post-tool-use hook - called after tool execution
///
/// This hook is for observability and cannot control execution flow.
/// Use it for logging, metrics, or side effects.
#[async_trait]
pub trait PostToolUseHook: Send + Sync + std::fmt::Debug {
    /// Execute the hook after tool use
    async fn execute(
        &self,
        ctx: &HookContext,
        tool_name: &str,
        result: &Result<String>,
    ) -> Result<()>;
}

/// User prompt submit hook - called before sending user message to LLM
///
/// This hook can:
/// - Allow the message to be sent
/// - Deny sending with a reason
/// - Modify the prompt before sending
#[async_trait]
pub trait UserPromptSubmitHook: Send + Sync + std::fmt::Debug {
    /// Execute the hook before user prompt submission
    async fn execute(&self, ctx: &HookContext, prompt: &str) -> Result<HookAction>;
}

/// Stop hook - called when session stops
///
/// This hook is for cleanup and finalization.
#[async_trait]
pub trait StopHook: Send + Sync + std::fmt::Debug {
    /// Execute the hook when session stops
    async fn execute(&self, ctx: &HookContext, summary: &str) -> Result<()>;
}

/// Subagent start hook - called when a subagent starts
///
/// This hook is for tracking and coordination.
#[async_trait]
pub trait SubagentStartHook: Send + Sync + std::fmt::Debug {
    /// Execute the hook when subagent starts
    async fn execute(&self, ctx: &HookContext, subagent_id: &str) -> Result<()>;
}

/// Subagent stop hook - called when a subagent stops
///
/// This hook is for cleanup and result processing.
#[async_trait]
pub trait SubagentStopHook: Send + Sync + std::fmt::Debug {
    /// Execute the hook when subagent stops
    async fn execute(
        &self,
        ctx: &HookContext,
        subagent_id: &str,
        result: &Result<String>,
    ) -> Result<()>;
}

/// File change event kind.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileChangeKind {
    /// File was created.
    Created,
    /// File was modified.
    Modified,
    /// File was deleted.
    Deleted,
}

/// Information about a file change event.
#[derive(Debug, Clone)]
pub struct FileChangeEvent {
    /// Path of the changed file.
    pub path: std::path::PathBuf,
    /// Kind of change.
    pub kind: FileChangeKind,
}

/// File changed hook - called when a watched file changes during a session.
///
/// This enables reactive workflows: auto-reformat after saves, re-run tests
/// when source changes, reload config when settings change, etc.
#[async_trait]
pub trait FileChangedHook: Send + Sync + std::fmt::Debug {
    /// Execute the hook when a file change is detected.
    async fn execute(&self, ctx: &HookContext, event: &FileChangeEvent) -> Result<()>;
}

/// Hook registry for managing all hook types
#[derive(Debug)]
pub struct HookRegistry {
    pre_tool_use_hooks: Vec<Box<dyn PreToolUseHook>>,
    post_tool_use_hooks: Vec<Box<dyn PostToolUseHook>>,
    user_prompt_submit_hooks: Vec<Box<dyn UserPromptSubmitHook>>,
    stop_hooks: Vec<Box<dyn StopHook>>,
    subagent_start_hooks: Vec<Box<dyn SubagentStartHook>>,
    subagent_stop_hooks: Vec<Box<dyn SubagentStopHook>>,
    file_changed_hooks: Vec<Box<dyn FileChangedHook>>,
}

impl HookRegistry {
    /// Create a new empty hook registry
    pub fn new() -> Self {
        Self {
            pre_tool_use_hooks: Vec::new(),
            post_tool_use_hooks: Vec::new(),
            user_prompt_submit_hooks: Vec::new(),
            stop_hooks: Vec::new(),
            subagent_start_hooks: Vec::new(),
            subagent_stop_hooks: Vec::new(),
            file_changed_hooks: Vec::new(),
        }
    }

    /// Register a pre-tool-use hook
    pub fn register_pre_tool_use(&mut self, hook: Box<dyn PreToolUseHook>) {
        self.pre_tool_use_hooks.push(hook);
    }

    /// Register a post-tool-use hook
    pub fn register_post_tool_use(&mut self, hook: Box<dyn PostToolUseHook>) {
        self.post_tool_use_hooks.push(hook);
    }

    /// Register a user prompt submit hook
    pub fn register_user_prompt_submit(&mut self, hook: Box<dyn UserPromptSubmitHook>) {
        self.user_prompt_submit_hooks.push(hook);
    }

    /// Register a stop hook
    pub fn register_stop(&mut self, hook: Box<dyn StopHook>) {
        self.stop_hooks.push(hook);
    }

    /// Register a subagent start hook
    pub fn register_subagent_start(&mut self, hook: Box<dyn SubagentStartHook>) {
        self.subagent_start_hooks.push(hook);
    }

    /// Register a subagent stop hook
    pub fn register_subagent_stop(&mut self, hook: Box<dyn SubagentStopHook>) {
        self.subagent_stop_hooks.push(hook);
    }

    /// Execute all pre-tool-use hooks
    ///
    /// Returns `Ok(action)` with the final action (Allow/Deny/Modify).
    /// If any hook denies, returns Deny immediately.
    /// If any hook modifies, returns Modify with modified arguments.
    /// Otherwise returns Allow.
    pub async fn execute_pre_tool_use(
        &self,
        ctx: &HookContext,
        tool_name: &str,
        arguments: &Value,
    ) -> Result<HookAction> {
        for hook in &self.pre_tool_use_hooks {
            let action = hook.execute(ctx, tool_name, arguments).await?;
            match action {
                HookAction::Allow => continue,
                deny @ (HookAction::Deny(_) | HookAction::Modify(_)) => return Ok(deny),
            }
        }
        Ok(HookAction::Allow)
    }

    /// Execute all post-tool-use hooks
    ///
    /// Executes all hooks in order, collecting any errors.
    pub async fn execute_post_tool_use(
        &self,
        ctx: &HookContext,
        tool_name: &str,
        result: &Result<String>,
    ) -> Result<()> {
        for hook in &self.post_tool_use_hooks {
            hook.execute(ctx, tool_name, result).await?;
        }
        Ok(())
    }

    /// Execute all user prompt submit hooks
    ///
    /// Returns `Ok(action)` with the final action (Allow/Deny/Modify).
    pub async fn execute_user_prompt_submit(
        &self,
        ctx: &HookContext,
        prompt: &str,
    ) -> Result<HookAction> {
        for hook in &self.user_prompt_submit_hooks {
            let action = hook.execute(ctx, prompt).await?;
            match action {
                HookAction::Allow => continue,
                deny @ (HookAction::Deny(_) | HookAction::Modify(_)) => return Ok(deny),
            }
        }
        Ok(HookAction::Allow)
    }

    /// Execute all stop hooks
    pub async fn execute_stop(&self, ctx: &HookContext, summary: &str) -> Result<()> {
        for hook in &self.stop_hooks {
            hook.execute(ctx, summary).await?;
        }
        Ok(())
    }

    /// Execute all subagent start hooks
    pub async fn execute_subagent_start(&self, ctx: &HookContext, subagent_id: &str) -> Result<()> {
        for hook in &self.subagent_start_hooks {
            hook.execute(ctx, subagent_id).await?;
        }
        Ok(())
    }

    /// Execute all subagent stop hooks
    pub async fn execute_subagent_stop(
        &self,
        ctx: &HookContext,
        subagent_id: &str,
        result: &Result<String>,
    ) -> Result<()> {
        for hook in &self.subagent_stop_hooks {
            hook.execute(ctx, subagent_id, result).await?;
        }
        Ok(())
    }

    /// Register a file changed hook
    pub fn register_file_changed(&mut self, hook: Box<dyn FileChangedHook>) {
        self.file_changed_hooks.push(hook);
    }

    /// Execute all file changed hooks
    pub async fn execute_file_changed(
        &self,
        ctx: &HookContext,
        event: &FileChangeEvent,
    ) -> Result<()> {
        for hook in &self.file_changed_hooks {
            hook.execute(ctx, event).await?;
        }
        Ok(())
    }
}

impl Default for HookRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Protocol type conversions
// ---------------------------------------------------------------------------

impl From<HookAction> for rustycode_protocol::HookOutput {
    fn from(action: HookAction) -> Self {
        match action {
            HookAction::Allow => rustycode_protocol::HookOutput::default(),
            HookAction::Deny(reason) => rustycode_protocol::HookOutput {
                r#continue: false,
                stop_reason: Some(reason),
                decision: Some(rustycode_protocol::HookDecision::Deny),
                ..Default::default()
            },
            HookAction::Modify(value) => {
                let ctx = value.as_str().map(|s| s.to_string());
                rustycode_protocol::HookOutput {
                    hook_specific_output: Some(rustycode_protocol::HookSpecificOutput {
                        additional_context: ctx,
                    }),
                    ..Default::default()
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct TestPreToolHook;

    #[async_trait]
    impl PreToolUseHook for TestPreToolHook {
        async fn execute(
            &self,
            _ctx: &HookContext,
            tool_name: &str,
            _arguments: &Value,
        ) -> Result<HookAction> {
            if tool_name == "dangerous_tool" {
                Ok(HookAction::Deny("Tool not allowed".to_string()))
            } else {
                Ok(HookAction::Allow)
            }
        }
    }

    struct TestPostToolHook {
        pub calls: Arc<std::sync::Mutex<Vec<String>>>,
    }

    impl std::fmt::Debug for TestPostToolHook {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("TestPostToolHook")
                .field("calls", &"<Mutex<Vec<String>>>")
                .finish()
        }
    }

    #[async_trait]
    impl PostToolUseHook for TestPostToolHook {
        async fn execute(
            &self,
            _ctx: &HookContext,
            tool_name: &str,
            _result: &Result<String>,
        ) -> Result<()> {
            self.calls
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .push(tool_name.to_string());
            Ok(())
        }
    }

    #[test]
    fn test_hook_context_creation() {
        let ctx = HookContext::new("agent1".to_string(), "session1".to_string())
            .with_metadata("key".to_string(), "value".to_string());

        assert_eq!(ctx.agent_id, "agent1");
        assert_eq!(ctx.session_id, "session1");
        assert_eq!(ctx.metadata.get("key"), Some(&"value".to_string()));
    }

    #[tokio::test]
    async fn test_pre_tool_hook_deny() {
        let mut registry = HookRegistry::new();
        registry.register_pre_tool_use(Box::new(TestPreToolHook));

        let ctx = HookContext::new("agent1".to_string(), "session1".to_string());
        let args = serde_json::json!({});

        // Test allowed tool
        let result = registry
            .execute_pre_tool_use(&ctx, "safe_tool", &args)
            .await
            .unwrap();
        assert_eq!(result, HookAction::Allow);

        // Test denied tool
        let result = registry
            .execute_pre_tool_use(&ctx, "dangerous_tool", &args)
            .await
            .unwrap();
        assert!(matches!(result, HookAction::Deny(_)));
    }

    #[tokio::test]
    async fn test_post_tool_hook() {
        let calls = Arc::new(std::sync::Mutex::new(Vec::new()));
        let hook = TestPostToolHook {
            calls: Arc::clone(&calls),
        };

        let mut registry = HookRegistry::new();
        registry.register_post_tool_use(Box::new(hook));

        let ctx = HookContext::new("agent1".to_string(), "session1".to_string());
        let result = Ok("success".to_string());

        registry
            .execute_post_tool_use(&ctx, "test_tool", &result)
            .await
            .unwrap();

        let executed_calls = calls.lock().unwrap();
        assert_eq!(executed_calls.len(), 1);
        assert_eq!(executed_calls[0], "test_tool");
    }

    #[derive(Debug)]
    struct TestFileChangedHook {
        pub changes: Arc<std::sync::Mutex<Vec<FileChangeKind>>>,
    }

    #[async_trait]
    impl FileChangedHook for TestFileChangedHook {
        async fn execute(&self, _ctx: &HookContext, event: &FileChangeEvent) -> Result<()> {
            self.changes
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .push(event.kind.clone());
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_file_changed_hook() {
        let changes = Arc::new(std::sync::Mutex::new(Vec::new()));
        let mut registry = HookRegistry::new();

        registry.register_file_changed(Box::new(TestFileChangedHook {
            changes: changes.clone(),
        }));

        let ctx = HookContext::new("agent1".to_string(), "session1".to_string());
        let event = FileChangeEvent {
            path: std::path::PathBuf::from("src/main.rs"),
            kind: FileChangeKind::Modified,
        };

        registry.execute_file_changed(&ctx, &event).await.unwrap();

        let recorded = changes.lock().unwrap();
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0], FileChangeKind::Modified);
    }
}
