use crate::executor::inspector::{InspectionAction, InspectionResult, ToolCallInfo};
use std::time::Instant;

/// Prevents rapid-fire tool execution.
///
/// Tracks the time between tool calls and blocks if they come too fast.
pub struct RateLimitInspector {
    /// Minimum interval between calls in milliseconds
    min_interval_ms: u64,
    /// Last call time per tool
    last_call: std::sync::Mutex<std::collections::HashMap<String, Instant>>,
}

impl RateLimitInspector {
    pub fn new(min_interval_ms: u64) -> Self {
        Self {
            min_interval_ms,
            last_call: std::sync::Mutex::new(std::collections::HashMap::new()),
        }
    }
}

impl crate::executor::inspector::ToolInspector for RateLimitInspector {
    fn name(&self) -> &'static str {
        "rate_limit"
    }

    fn inspect(
        &self,
        call: &ToolCallInfo,
        _history: &[ToolCallInfo],
        _ctx: &crate::ToolContext,
    ) -> InspectionResult {
        let mut last_calls = self
            .last_call
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let now = Instant::now();

        if let Some(last_time) = last_calls.get(&call.name) {
            let elapsed = now.duration_since(*last_time).as_millis() as u64;
            if elapsed < self.min_interval_ms {
                return InspectionResult {
                    request_id: call.id.clone(),
                    action: InspectionAction::Deny,
                    reason: format!(
                        "Tool '{}' called too quickly ({}ms < {}ms minimum)",
                        call.name, elapsed, self.min_interval_ms
                    ),
                    confidence: 1.0,
                    inspector_name: "rate_limit".to_string(),
                    finding_id: Some("RATE-001".to_string()),
                };
            }
        }

        last_calls.insert(call.name.clone(), now);

        InspectionResult {
            request_id: call.id.clone(),
            action: InspectionAction::Allow,
            reason: format!("Rate limit OK for tool '{}'", call.name),
            confidence: 1.0,
            inspector_name: "rate_limit".to_string(),
            finding_id: None,
        }
    }

    fn reset(&self) {
        self.last_call
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::executor::inspector::ToolInspector;
    use serde_json::json;

    fn test_ctx() -> crate::ToolContext {
        crate::ToolContext::new(std::env::temp_dir())
    }

    fn make_call(name: &str, args: serde_json::Value) -> crate::executor::manager::ToolCallInfo {
        crate::executor::manager::ToolCallInfo::new("test-id", name, args)
    }

    #[test]
    fn test_rate_limit_inspector() {
        let inspector = RateLimitInspector::new(1000); // 1 second
        let ctx = test_ctx();

        let call = make_call("Read", json!({"path": "/tmp/test.txt"}));

        // First call should be allowed
        let result1 = inspector.inspect(&call, &[], &ctx);
        assert_eq!(
            result1.action,
            crate::executor::manager::InspectionAction::Allow
        );

        // Immediate second call should be denied
        let result2 = inspector.inspect(&call, &[], &ctx);
        assert_eq!(
            result2.action,
            crate::executor::manager::InspectionAction::Deny
        );
        assert!(result2.reason.contains("too quickly"));
    }
}
