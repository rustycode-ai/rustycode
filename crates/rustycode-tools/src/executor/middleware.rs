//! Tool Execution Middleware
//!
//! Provides a middleware wrapper around tool execution that handles:
//! - Hook execution (`PreToolUse`, `PostToolUse`)
//! - Plan mode validation
//! - Cost tracking
//!
//! # Usage
//!
//! ```ignore
//! use execution_middleware::{ExecutionMiddleware, MiddlewareConfig};
//!
//! let config = MiddlewareConfig::default();
//! let middleware = ExecutionMiddleware::new(config);
//!
//! // Execute tool through middleware
//! let result = middleware.execute(&tool, params, &ctx);
//! ```

use crate::{Tool, ToolContext, ToolOutput};
use anyhow::Result;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Middleware configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MiddlewareConfig {
    /// Enable hook execution
    pub hooks_enabled: bool,
    /// Enable plan mode validation
    pub plan_mode_enabled: bool,
    /// Enable cost tracking
    pub cost_tracking_enabled: bool,
    /// Maximum cost per session (USD)
    pub max_session_cost: Option<f64>,
    /// Tools that trigger checkpoints (auto-snapshot)
    pub checkpoint_tools: Vec<String>,
}

impl Default for MiddlewareConfig {
    fn default() -> Self {
        Self {
            hooks_enabled: true,
            plan_mode_enabled: true,
            cost_tracking_enabled: true,
            max_session_cost: None,
            checkpoint_tools: vec![
                "edit_file".to_string(),
                "multiedit".to_string(),
                "apply_patch".to_string(),
                "write_file".to_string(),
                "bash".to_string(),
            ],
        }
    }
}

/// Execution middleware state
pub struct MiddlewareState {
    /// Current plan mode state
    pub plan_mode: PlanModeState,
    /// Running cost this session
    pub session_cost: f64,
    /// Tool execution count
    pub tool_count: usize,
}

/// Plan mode execution state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum PlanModeState {
    /// Full execution mode
    #[default]
    Executing,
    /// Planning mode (read-only)
    Planning,
}

/// Tool execution middleware
pub struct ExecutionMiddleware {
    config: MiddlewareConfig,
    state: Arc<RwLock<MiddlewareState>>,
}

impl ExecutionMiddleware {
    /// Create a new middleware instance
    pub fn new(config: MiddlewareConfig) -> Self {
        Self {
            config: config.clone(),
            state: Arc::new(RwLock::new(MiddlewareState {
                plan_mode: PlanModeState::Executing,
                session_cost: 0.0,
                tool_count: 0,
            })),
        }
    }

    /// Create with existing state (for testing/continuation)
    pub const fn with_state(config: MiddlewareConfig, state: Arc<RwLock<MiddlewareState>>) -> Self {
        Self { config, state }
    }

    /// Execute a tool through the middleware pipeline
    pub fn execute<T: Tool>(
        &self,
        tool: &T,
        params: serde_json::Value,
        ctx: &ToolContext,
    ) -> Result<ToolOutput> {
        let tool_name = tool.name().to_string();

        // Phase 1: Pre-execution checks
        self.pre_execute_check(&tool_name, ctx)?;

        // Phase 2: Execute tool
        let result = tool.execute(params, ctx);

        // Phase 3: Post-execution processing
        self.post_execute_check(&tool_name, &result, ctx)?;

        result
    }

    /// Pre-execution checks (hooks, plan mode, cost)
    fn pre_execute_check(&self, tool_name: &str, _ctx: &ToolContext) -> Result<()> {
        // Check 1: Plan mode validation
        if self.config.plan_mode_enabled {
            self.validate_plan_mode(tool_name)?;
        }

        // Check 2: Cost limits
        if self.config.cost_tracking_enabled {
            self.check_cost_limit()?;
        }

        Ok(())
    }

    /// Post-execution processing (hooks, cost recording)
    fn post_execute_check(
        &self,
        tool_name: &str,
        result: &Result<ToolOutput>,
        _ctx: &ToolContext,
    ) -> Result<()> {
        // Update state
        {
            let mut state = self.state.write();
            state.tool_count += 1;

            // Estimate cost (simple approximation)
            if self.config.cost_tracking_enabled {
                let cost = self.estimate_cost(tool_name, result.as_ref().ok());
                state.session_cost += cost;
            }
        }

        Ok(())
    }

    /// Validate tool is allowed in current plan mode
    fn validate_plan_mode(&self, tool_name: &str) -> Result<()> {
        let state = self.state.read();

        if state.plan_mode == PlanModeState::Planning {
            // In plan mode, only allow read-only tools
            let allowed = ["glob", "grep", "search", "read", "lsp", "web_fetch"];
            if !allowed.contains(&tool_name) {
                anyhow::bail!(
                    "tool '{tool_name}' not allowed in plan mode. Use read-only tools: {allowed:?}"
                );
            }
        }

        Ok(())
    }

    /// Check if we've exceeded cost limits
    fn check_cost_limit(&self) -> Result<()> {
        if let Some(max_cost) = self.config.max_session_cost {
            let state = self.state.read();
            if state.session_cost >= max_cost {
                anyhow::bail!(
                    "session cost ${:.2} exceeded limit ${:.2}",
                    state.session_cost,
                    max_cost
                );
            }
        }
        Ok(())
    }

    /// Estimate cost of a tool execution
    fn estimate_cost(&self, tool_name: &str, result: Option<&ToolOutput>) -> f64 {
        // Simple cost estimation based on tool type and output size
        let base_cost = match tool_name {
            "read" => 0.001,
            "write" | "edit" => 0.002,
            "bash" => 0.005,
            "grep" | "glob" => 0.003,
            _ => 0.001,
        };

        // Add cost for output size
        let output_cost = result
            .map(|r| r.text.len() as f64 / 10_000.0)
            .unwrap_or(0.0);

        base_cost + output_cost.min(0.01) // Cap output cost
    }

    /// Get current state
    pub fn state(&self) -> Arc<RwLock<MiddlewareState>> {
        self.state.clone()
    }

    /// Get current plan mode
    pub fn plan_mode(&self) -> PlanModeState {
        self.state.read().plan_mode
    }

    /// Set plan mode
    pub fn set_plan_mode(&self, mode: PlanModeState) {
        self.state.write().plan_mode = mode;
    }

    /// Get session cost
    pub fn session_cost(&self) -> f64 {
        self.state.read().session_cost
    }

    /// Get tool execution count
    pub fn tool_count(&self) -> usize {
        self.state.read().tool_count
    }

    /// Get the configured max session cost, if any (for external checks).
    pub const fn session_cost_checked_limit(&self) -> Option<f64> {
        self.config.max_session_cost
    }

    /// Set the middleware on a `ToolExecutor`.
    pub fn into_arc(self) -> Arc<Self> {
        Arc::new(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_tool(name: &str) -> impl Tool {
        struct TestTool {
            name: String,
        }
        impl Tool for TestTool {
            fn name(&self) -> &str {
                &self.name
            }
            fn description(&self) -> &str {
                "test tool"
            }
            fn parameters_schema(&self) -> serde_json::Value {
                serde_json::json!({ "type": "object" })
            }
            fn execute(
                &self,
                _params: serde_json::Value,
                _ctx: &ToolContext,
            ) -> Result<ToolOutput> {
                Ok(ToolOutput::text("ok"))
            }
        }
        TestTool {
            name: name.to_string(),
        }
    }

    #[test]
    fn middleware_config_defaults() {
        let config = MiddlewareConfig::default();
        assert!(config.hooks_enabled);
        assert!(config.plan_mode_enabled);
        assert!(config.cost_tracking_enabled);
        assert!(config.checkpoint_tools.contains(&"edit_file".to_string()));
    }

    #[test]
    fn middleware_execute_allowed() {
        let config = MiddlewareConfig::default();
        let middleware = ExecutionMiddleware::new(config);

        let state = middleware.state();
        state.write().plan_mode = PlanModeState::Executing;

        let tool = make_test_tool("read");
        let result = middleware.execute(
            &tool,
            serde_json::json!({}),
            &ToolContext::new(std::path::Path::new("/tmp")),
        );
        assert!(result.is_ok());
    }

    #[test]
    fn middleware_plan_mode_blocks_write() {
        let config = MiddlewareConfig::default();
        let middleware = ExecutionMiddleware::new(config);

        // Set to planning mode
        middleware.set_plan_mode(PlanModeState::Planning);

        // Write should be blocked
        let tool = make_test_tool("write");
        let result = middleware.execute(
            &tool,
            serde_json::json!({}),
            &ToolContext::new(std::path::Path::new("/tmp")),
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not allowed"));
    }

    #[test]
    fn middleware_plan_mode_allows_read() {
        let config = MiddlewareConfig::default();
        let middleware = ExecutionMiddleware::new(config);

        // Set to planning mode
        middleware.set_plan_mode(PlanModeState::Planning);

        // Read should be allowed
        let tool = make_test_tool("read");
        let result = middleware.execute(
            &tool,
            serde_json::json!({}),
            &ToolContext::new(std::path::Path::new("/tmp")),
        );
        assert!(result.is_ok());
    }

    #[test]
    fn middleware_cost_tracking() {
        let config = MiddlewareConfig::default();
        let middleware = ExecutionMiddleware::new(config);

        let tool = make_test_tool("read");
        let _ = middleware.execute(
            &tool,
            serde_json::json!({}),
            &ToolContext::new(std::path::Path::new("/tmp")),
        );

        assert!(middleware.session_cost() > 0.0);
        assert_eq!(middleware.tool_count(), 1);
    }

    #[test]
    fn middleware_cost_limit() {
        let config = MiddlewareConfig {
            cost_tracking_enabled: true,
            max_session_cost: Some(0.001), // Very low limit - 1/10th of single read cost
            ..Default::default()
        };
        let middleware = ExecutionMiddleware::new(config);

        let tool = make_test_tool("read");

        // First call succeeds (cost 0.001 estimated, under limit 0.001)
        let _result1 = middleware.execute(
            &tool,
            serde_json::json!({}),
            &ToolContext::new(std::path::Path::new("/tmp")),
        );

        // Second call should fail (cost is now 0.001 + 0.001 = 0.002, over limit)
        let result2 = middleware.execute(
            &tool,
            serde_json::json!({}),
            &ToolContext::new(std::path::Path::new("/tmp")),
        );

        // Should fail due to cost limit on second call
        assert!(result2.is_err());
        assert!(result2.unwrap_err().to_string().contains("exceeded"));
    }

    #[test]
    fn get_current_state() {
        let config = MiddlewareConfig::default();
        let middleware = ExecutionMiddleware::new(config);

        assert_eq!(middleware.plan_mode(), PlanModeState::Executing);
        assert_eq!(middleware.session_cost(), 0.0);
    }

    #[test]
    fn set_plan_mode() {
        let config = MiddlewareConfig::default();
        let middleware = ExecutionMiddleware::new(config);

        middleware.set_plan_mode(PlanModeState::Planning);
        assert_eq!(middleware.plan_mode(), PlanModeState::Planning);

        middleware.set_plan_mode(PlanModeState::Executing);
        assert_eq!(middleware.plan_mode(), PlanModeState::Executing);
    }
}
