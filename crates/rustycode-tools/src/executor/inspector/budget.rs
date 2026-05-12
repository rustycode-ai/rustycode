use super::{InspectionAction, InspectionResult, ToolCallInfo, ToolInspector};
use crate::ToolContext;

/// Tracks estimated token costs of tool calls and warns when approaching a
/// session budget limit.
///
/// This inspector monitors the cumulative token usage of tool calls
/// throughout a session. When usage approaches or exceeds the configured
/// budget, it escalates the action from Allow to `RequireApproval` to Deny.
///
/// Token estimates are based on argument size (rough approximation of
/// how much context each tool call consumes).
pub struct BudgetInspector {
    /// Maximum estimated tokens before denying calls
    max_tokens: usize,
    /// Running token count
    used_tokens: std::sync::Mutex<usize>,
}

impl BudgetInspector {
    pub const fn new(max_tokens: usize) -> Self {
        Self {
            max_tokens,
            used_tokens: std::sync::Mutex::new(0),
        }
    }

    /// Rough estimate of token cost for a tool call.
    /// Uses ~4 chars per token as approximation.
    fn estimate_tokens(call: &ToolCallInfo) -> usize {
        let args_size = call.arguments.to_string().len();
        let name_size = call.name.len();
        (args_size + name_size) / 4
    }

    /// Get current token usage
    pub fn used_tokens(&self) -> usize {
        *self
            .used_tokens
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Get the token budget
    pub const fn budget(&self) -> usize {
        self.max_tokens
    }

    /// Get remaining tokens
    pub fn remaining(&self) -> usize {
        self.max_tokens.saturating_sub(self.used_tokens())
    }
}

impl ToolInspector for BudgetInspector {
    fn name(&self) -> &'static str {
        "budget"
    }

    fn inspect(
        &self,
        call: &ToolCallInfo,
        _history: &[ToolCallInfo],
        _ctx: &ToolContext,
    ) -> InspectionResult {
        let call_tokens = Self::estimate_tokens(call);
        let mut used = self
            .used_tokens
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *used += call_tokens;
        let total = *used;
        drop(used);

        let usage_pct = (total as f64 / self.max_tokens as f64) * 100.0;

        if total > self.max_tokens {
            return InspectionResult {
                request_id: call.id.clone(),
                action: InspectionAction::Deny,
                reason: format!(
                    "Token budget exceeded: {} tokens used of {} budget ({:.0}%)",
                    total, self.max_tokens, usage_pct
                ),
                confidence: 0.9,
                inspector_name: "budget".to_string(),
                finding_id: Some("BUDGET-001".to_string()),
            };
        }

        // Warn at 80% usage
        if usage_pct >= 80.0 {
            return InspectionResult {
                request_id: call.id.clone(),
                action: InspectionAction::RequireApproval(Some(format!(
                    "Approaching token budget: {:.0}% used ({} of {})",
                    usage_pct, total, self.max_tokens
                ))),
                reason: format!(
                    "Token budget warning: {} tokens used of {} ({:.0}%)",
                    total, self.max_tokens, usage_pct
                ),
                confidence: 0.7,
                inspector_name: "budget".to_string(),
                finding_id: Some("BUDGET-002".to_string()),
            };
        }

        InspectionResult {
            request_id: call.id.clone(),
            action: InspectionAction::Allow,
            reason: format!(
                "Token budget OK: {} used of {} ({:.0}%)",
                total, self.max_tokens, usage_pct
            ),
            confidence: 1.0,
            inspector_name: "budget".to_string(),
            finding_id: None,
        }
    }

    fn reset(&self) {
        *self
            .used_tokens
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::executor::inspector::ToolInspector;
    use serde_json::json;

    fn test_ctx() -> ToolContext {
        ToolContext::new(std::env::temp_dir())
    }

    fn make_call(name: &str, args: serde_json::Value) -> ToolCallInfo {
        ToolCallInfo::new("test-id", name, args)
    }

    #[test]
    fn test_budget_inspector_allows_within_budget() {
        let inspector = BudgetInspector::new(100_000);
        let ctx = test_ctx();

        let call = make_call("Read", json!({"path": "/tmp/test.txt"}));
        let result = inspector.inspect(&call, &[], &ctx);

        assert_eq!(result.action, InspectionAction::Allow);
        assert_eq!(result.inspector_name, "budget");
        assert!(inspector.used_tokens() > 0);
    }

    #[test]
    fn test_budget_inspector_warns_at_80_percent() {
        // Budget where a single call hits 80-99%
        let inspector = BudgetInspector::new(500);
        let ctx = test_ctx();

        // Make a call that uses ~85% of budget (425 tokens = ~1700 chars)
        let big_args = "x".repeat(1696); // ~424 tokens
        let call = make_call("Bash", json!({"command": big_args}));
        let result = inspector.inspect(&call, &[], &ctx);

        assert!(matches!(
            result.action,
            InspectionAction::RequireApproval(_)
        ));
        assert!(result.reason.contains("budget warning"));
    }

    #[test]
    fn test_budget_inspector_denies_over_budget() {
        let inspector = BudgetInspector::new(10); // Very small budget
        let ctx = test_ctx();

        // First call uses budget
        let call1 = make_call("Bash", json!({"command": "some long command here"}));
        let _ = inspector.inspect(&call1, &[], &ctx);

        // Second call should push over
        let call2 = make_call("Bash", json!({"command": "another long command"}));
        let result = inspector.inspect(&call2, &[], &ctx);

        assert_eq!(result.action, InspectionAction::Deny);
        assert!(result.reason.contains("budget exceeded"));
        assert_eq!(result.finding_id, Some("BUDGET-001".to_string()));
    }

    #[test]
    fn test_budget_inspector_tracks_usage() {
        let inspector = BudgetInspector::new(100_000);
        let ctx = test_ctx();

        assert_eq!(inspector.used_tokens(), 0);
        assert_eq!(inspector.remaining(), 100_000);
        assert_eq!(inspector.budget(), 100_000);

        let call = make_call("Read", json!({"path": "/tmp/test.txt"}));
        let _ = inspector.inspect(&call, &[], &ctx);

        assert!(inspector.used_tokens() > 0);
        assert!(inspector.remaining() < 100_000);
    }

    #[test]
    fn test_budget_inspector_reset() {
        let inspector = BudgetInspector::new(100_000);
        let ctx = test_ctx();

        let call = make_call("Read", json!({"path": "/tmp/test.txt"}));
        let _ = inspector.inspect(&call, &[], &ctx);
        assert!(inspector.used_tokens() > 0);

        inspector.reset();
        assert_eq!(inspector.used_tokens(), 0);
    }
}
