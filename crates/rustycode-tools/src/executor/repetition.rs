use crate::executor::inspector::{InspectionAction, InspectionResult, ToolCallInfo, ToolInspector};
use crate::ToolContext;

/// Detects and blocks repetitive tool calls (infinite loop prevention).
///
/// Tracks consecutive identical tool calls (same name + arguments) and
/// blocks execution after a configurable threshold.
pub struct RepetitionInspector {
    /// Maximum consecutive identical calls before blocking
    max_repetitions: Option<u32>,
    /// Total call counts per tool name
    call_counts: std::sync::Mutex<std::collections::HashMap<String, u32>>,
}

impl RepetitionInspector {
    pub fn new(max_repetitions: Option<u32>) -> Self {
        Self {
            max_repetitions,
            call_counts: std::sync::Mutex::new(std::collections::HashMap::new()),
        }
    }

    /// Count consecutive identical calls in history
    fn count_consecutive(history: &[ToolCallInfo], call: &ToolCallInfo) -> u32 {
        let mut count = 0u32;
        for past in history.iter().rev() {
            if past.matches(call) {
                count += 1;
            } else {
                break;
            }
        }
        count
    }
}

impl ToolInspector for RepetitionInspector {
    fn name(&self) -> &'static str {
        "repetition"
    }

    fn inspect(
        &self,
        call: &ToolCallInfo,
        history: &[ToolCallInfo],
        _ctx: &ToolContext,
    ) -> InspectionResult {
        // Track total calls per tool
        let mut counts = self
            .call_counts
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let total = counts.entry(call.name.clone()).or_insert(0);
        *total += 1;
        let total_calls = *total;
        drop(counts);

        // Check consecutive repetitions
        let consecutive = Self::count_consecutive(history, call);

        if let Some(max) = self.max_repetitions {
            if consecutive >= max {
                return InspectionResult {
                    request_id: call.id.clone(),
                    action: InspectionAction::Deny,
                    reason: format!(
                        "Tool '{}' repeated {} times consecutively (limit: {}). Possible infinite loop.",
                        call.name, consecutive, max
                    ),
                    confidence: 0.95,
                    inspector_name: "repetition".to_string(),
                    finding_id: Some("REP-001".to_string()),
                };
            }

            // Warn at 80% of threshold
            if consecutive >= (max * 80 / 100).max(1) {
                return InspectionResult {
                    request_id: call.id.clone(),
                    action: InspectionAction::RequireApproval(Some(format!(
                        "Tool '{}' is repeating ({}x of {} limit)",
                        call.name, consecutive, max
                    ))),
                    reason: format!(
                        "Tool '{}' approaching repetition limit ({}/{})",
                        call.name, consecutive, max
                    ),
                    confidence: 0.7,
                    inspector_name: "repetition".to_string(),
                    finding_id: Some("REP-002".to_string()),
                };
            }
        }

        InspectionResult {
            request_id: call.id.clone(),
            action: InspectionAction::Allow,
            reason: format!(
                "Tool '{}' called {} time(s) total, {} consecutive",
                call.name, total_calls, consecutive
            ),
            confidence: 1.0,
            inspector_name: "repetition".to_string(),
            finding_id: None,
        }
    }

    fn reset(&self) {
        self.call_counts
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn test_ctx() -> ToolContext {
        ToolContext::new(std::env::temp_dir())
    }

    fn make_call(name: &str, args: serde_json::Value) -> ToolCallInfo {
        ToolCallInfo::new("test-id", name, args)
    }

    #[test]
    fn test_repetition_inspector_allows_normal() {
        let inspector = RepetitionInspector::new(Some(3));
        let ctx = test_ctx();
        let history = vec![];

        let call = make_call("Read", json!({"path": "/tmp/test.txt"}));
        let result = inspector.inspect(&call, &history, &ctx);

        assert_eq!(result.action, InspectionAction::Allow);
    }

    #[test]
    fn test_repetition_inspector_blocks_loop() {
        let inspector = RepetitionInspector::new(Some(3));
        let ctx = test_ctx();

        let call = make_call("Read", json!({"path": "/tmp/test.txt"}));
        let history = vec![call.clone(), call.clone(), call.clone()];

        let result = inspector.inspect(&call, &history, &ctx);
        assert_eq!(result.action, InspectionAction::Deny);
        assert!(result.reason.contains("repeated"));
        assert_eq!(result.finding_id, Some("REP-001".to_string()));
    }

    #[test]
    fn test_repetition_inspector_warns_near_limit() {
        let inspector = RepetitionInspector::new(Some(5));
        let ctx = test_ctx();

        let call = make_call("Read", json!({"path": "/tmp/test.txt"}));
        let history = vec![call.clone(), call.clone(), call.clone(), call.clone()];

        let result = inspector.inspect(&call, &history, &ctx);
        assert!(matches!(
            result.action,
            InspectionAction::RequireApproval(_)
        ));
    }

    #[test]
    fn test_repetition_inspector_different_tools_ok() {
        let inspector = RepetitionInspector::new(Some(2));
        let ctx = test_ctx();

        let call1 = make_call("Read", json!({"path": "/a"}));
        let call2 = make_call("Read", json!({"path": "/b"}));
        let history = vec![call1];

        let result = inspector.inspect(&call2, &history, &ctx);
        assert_eq!(result.action, InspectionAction::Allow);
    }
}
