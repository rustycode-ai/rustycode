use crate::executor::manager::ToolCallInfo;

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

impl crate::executor::manager::ToolInspector for RepetitionInspector {
    fn name(&self) -> &'static str {
        "repetition"
    }

    fn inspect(
        &self,
        call: &ToolCallInfo,
        history: &[ToolCallInfo],
        _ctx: &crate::ToolContext,
    ) -> crate::executor::manager::InspectionResult {
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
                return crate::executor::manager::InspectionResult {
                    request_id: call.id.clone(),
                    action: crate::executor::manager::InspectionAction::Deny,
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
                return crate::executor::manager::InspectionResult {
                    request_id: call.id.clone(),
                    action: crate::executor::manager::InspectionAction::RequireApproval(Some(
                        format!(
                            "Tool '{}' is repeating ({}x of {} limit)",
                            call.name, consecutive, max
                        ),
                    )),
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

        crate::executor::manager::InspectionResult {
            request_id: call.id.clone(),
            action: crate::executor::manager::InspectionAction::Allow,
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
