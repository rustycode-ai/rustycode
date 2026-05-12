//! Conversion from StreamEvent to EventMsg for broadcast emission.
//!
//! `StreamEvent` is the raw event type emitted by `AgentSession`.
//! `EventMsg` is the unified broadcast type consumed by frontends.
//! This module provides a lossless conversion between the two.

use rustycode_protocol::{EventMsg, PlanStepInfo, StreamEvent, StreamPlanStep};

/// Convert StreamPlanStep to PlanStepInfo.
fn convert_plan_step(step: StreamPlanStep) -> PlanStepInfo {
    PlanStepInfo {
        name: step.name,
        description: step.description,
    }
}

/// Convert a StreamEvent to an EventMsg for broadcast emission.
///
/// Returns None for events that don't have a direct EventMsg equivalent.
/// All mappable events are converted losslessly.
///
/// # Mapping Rules
///
/// - `TextDelta { content }` → `EventMsg::TextDelta { delta: content }`
/// - `ThinkingDelta { content }` → `EventMsg::ThinkingDelta { delta: content }`
/// - `ThinkingBlockCompleted { .. }` → `EventMsg::ThinkingBlockCompleted { .. }` (fields match 1:1)
/// - `ToolCallStarted { id, name }` → `EventMsg::ToolCallStarted { tool_id: id, tool_name: name, input: serde_json::Value::Null }`
/// - `ToolInputDelta { id, chunk }` → `EventMsg::ToolInputDelta { tool_id: id, delta: chunk }`
/// - `ToolExecStarted { id, name }` → `EventMsg::ToolExecStarted { tool_id: id, tool_name: name }`
/// - `ToolExecCompleted { id, name, output, is_error }` → `EventMsg::ToolExecCompleted { tool_id: id, tool_name: name, success: !is_error, output, output_size: output.len(), duration_ms: 0 }`
/// - `TurnStarted { turn }` → `EventMsg::TurnStarted { turn }`
/// - `TokenUsage { input_tokens, output_tokens }` → `EventMsg::TokenUsage { input_tokens, output_tokens, cache_read_tokens: 0, cache_creation_tokens: 0 }`
/// - `TurnCompleted { stop_reason }` → `EventMsg::TurnCompleted { stop_reason }`
/// - `CacheUsage { cache_read_tokens, cache_creation_tokens }` → `EventMsg::TokenUsage { input_tokens: 0, output_tokens: 0, cache_read_tokens, cache_creation_tokens }`
/// - `Done` → `EventMsg::Done`
/// - `PlanCreated { id, title, steps }` → `EventMsg::PlanCreated { plan_id: id, title, steps: converted_steps }`
/// - `PlanStepStarted { plan_id, step_index }` → `EventMsg::PlanStepStarted { plan_id, step_index }`
/// - `PlanStepCompleted { plan_id, step_index, success, message }` → `EventMsg::PlanStepCompleted { plan_id, step_index, success, message }`
/// - `PlanCompleted { plan_id, success, summary }` → `EventMsg::PlanCompleted { plan_id, success, summary }`
/// - `PlanApprovalRequested { plan_id, title, steps }` → `EventMsg::PlanApprovalRequested { plan_id, title, steps: converted_steps }`
///
/// # Examples
///
/// ```
/// use rustycode_protocol::{StreamEvent, EventMsg};
/// use rustycode_agent_runtime::event_convert::stream_event_to_event_msg;
///
/// let event = StreamEvent::TextDelta { content: "Hello".into() };
/// let msg = stream_event_to_event_msg(event).expect("conversion should return Some");
/// assert!(matches!(msg, EventMsg::TextDelta { delta: _ }));
/// ```
#[allow(clippy::too_many_lines)]
pub fn stream_event_to_event_msg(event: StreamEvent) -> Option<EventMsg> {
    match event {
        StreamEvent::TextDelta { content } => Some(EventMsg::TextDelta { delta: content }),

        StreamEvent::ThinkingDelta { content } => Some(EventMsg::ThinkingDelta { delta: content }),

        StreamEvent::ThinkingBlockCompleted {
            block_type,
            signature,
            data,
        } => Some(EventMsg::ThinkingBlockCompleted {
            block_type,
            signature,
            data,
        }),

        StreamEvent::ToolCallStarted { id, name } => Some(EventMsg::ToolCallStarted {
            tool_id: id,
            tool_name: name,
            input: serde_json::Value::Null,
        }),

        StreamEvent::ToolInputDelta { id, chunk } => Some(EventMsg::ToolInputDelta {
            tool_id: id,
            delta: chunk,
        }),

        StreamEvent::ToolExecStarted { id, name } => Some(EventMsg::ToolExecStarted {
            tool_id: id,
            tool_name: name,
        }),

        StreamEvent::ToolExecCompleted {
            id,
            name,
            output,
            is_error,
        } => {
            let output_size = output.len();
            Some(EventMsg::ToolExecCompleted {
                tool_id: id,
                tool_name: name,
                success: !is_error,
                output,
                output_size,
                duration_ms: 0,
            })
        }

        StreamEvent::TurnStarted { turn } => Some(EventMsg::TurnStarted { turn }),

        StreamEvent::TokenUsage {
            input_tokens,
            output_tokens,
        } => Some(EventMsg::TokenUsage {
            input_tokens,
            output_tokens,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
        }),

        StreamEvent::TurnCompleted { stop_reason } => Some(EventMsg::TurnCompleted { stop_reason }),

        StreamEvent::CacheUsage {
            cache_read_tokens,
            cache_creation_tokens,
        } => Some(EventMsg::TokenUsage {
            input_tokens: 0,
            output_tokens: 0,
            cache_read_tokens,
            cache_creation_tokens,
        }),

        StreamEvent::Done => Some(EventMsg::Done),

        StreamEvent::PlanCreated { id, title, steps } => Some(EventMsg::PlanCreated {
            plan_id: id,
            title,
            steps: steps.into_iter().map(convert_plan_step).collect(),
        }),

        StreamEvent::PlanStepStarted {
            plan_id,
            step_index,
        } => Some(EventMsg::PlanStepStarted {
            plan_id,
            step_index,
        }),

        StreamEvent::PlanStepCompleted {
            plan_id,
            step_index,
            success,
            message,
        } => Some(EventMsg::PlanStepCompleted {
            plan_id,
            step_index,
            success,
            message,
        }),

        StreamEvent::PlanCompleted {
            plan_id,
            success,
            summary,
        } => Some(EventMsg::PlanCompleted {
            plan_id,
            success,
            summary,
        }),

        StreamEvent::PlanApprovalRequested {
            plan_id,
            title,
            steps,
        } => Some(EventMsg::PlanApprovalRequested {
            plan_id,
            title,
            steps: steps.into_iter().map(convert_plan_step).collect(),
        }),

        // Future StreamEvent variants that don't have EventMsg equivalents
        // are ignored for now. Add mappings as needed.
        _ => None,
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use rustycode_protocol::StreamPlanStep;

    #[test]
    fn text_delta_conversion() {
        let event = StreamEvent::TextDelta {
            content: "Hello, world!".into(),
        };
        let msg = stream_event_to_event_msg(event).expect("conversion should return Some");
        assert!(matches!(msg, EventMsg::TextDelta { delta } if delta == "Hello, world!"));
    }

    #[test]
    fn thinking_delta_conversion() {
        let event = StreamEvent::ThinkingDelta {
            content: "Let me think...".into(),
        };
        let msg = stream_event_to_event_msg(event).expect("conversion should return Some");
        assert!(matches!(msg, EventMsg::ThinkingDelta { delta } if delta == "Let me think..."));
    }

    #[test]
    fn thinking_block_completed_conversion() {
        let event = StreamEvent::ThinkingBlockCompleted {
            block_type: "redacted_thinking".into(),
            signature: "sig123".into(),
            data: "encrypted".into(),
        };
        let msg = stream_event_to_event_msg(event).expect("conversion should return Some");
        assert!(matches!(
            msg,
            EventMsg::ThinkingBlockCompleted {
                block_type,
                signature,
                data
            } if block_type == "redacted_thinking" && signature == "sig123" && data == "encrypted"
        ));
    }

    #[test]
    fn tool_call_started_conversion() {
        let event = StreamEvent::ToolCallStarted {
            id: "tool_1".into(),
            name: "Bash".into(),
        };
        let msg = stream_event_to_event_msg(event).expect("conversion should return Some");
        assert!(matches!(
            msg,
            EventMsg::ToolCallStarted {
                tool_id,
                tool_name,
                input
            } if tool_id == "tool_1" && tool_name == "Bash" && input.is_null()
        ));
    }

    #[test]
    fn tool_input_delta_conversion() {
        let event = StreamEvent::ToolInputDelta {
            id: "tool_1".into(),
            chunk: r#"{"cmd":"echo"#.into(),
        };
        let msg = stream_event_to_event_msg(event).expect("conversion should return Some");
        assert!(matches!(
            msg,
            EventMsg::ToolInputDelta { tool_id, delta } if tool_id == "tool_1" && delta == r#"{"cmd":"echo"#
        ));
    }

    #[test]
    fn tool_exec_started_conversion() {
        let event = StreamEvent::ToolExecStarted {
            id: "tool_1".into(),
            name: "Bash".into(),
        };
        let msg = stream_event_to_event_msg(event).expect("conversion should return Some");
        assert!(matches!(
            msg,
            EventMsg::ToolExecStarted { tool_id, tool_name } if tool_id == "tool_1" && tool_name == "Bash"
        ));
    }

    #[test]
    fn tool_exec_completed_success_conversion() {
        let event = StreamEvent::ToolExecCompleted {
            id: "tool_1".into(),
            name: "Bash".into(),
            output: "success".into(),
            is_error: false,
        };
        let msg = stream_event_to_event_msg(event).expect("conversion should return Some");
        assert!(matches!(
            msg,
            EventMsg::ToolExecCompleted {
                tool_id,
                tool_name,
                success,
                output,
                output_size,
                duration_ms
            } if tool_id == "tool_1" && tool_name == "Bash" && success && output == "success" && output_size == 7 && duration_ms == 0
        ));
    }

    #[test]
    fn tool_exec_completed_error_conversion() {
        let event = StreamEvent::ToolExecCompleted {
            id: "tool_1".into(),
            name: "Bash".into(),
            output: "error".into(),
            is_error: true,
        };
        let msg = stream_event_to_event_msg(event).expect("conversion should return Some");
        assert!(matches!(
            msg,
            EventMsg::ToolExecCompleted { success, .. } if !success
        ));
    }

    #[test]
    fn turn_started_conversion() {
        let event = StreamEvent::TurnStarted { turn: 5 };
        let msg = stream_event_to_event_msg(event).expect("conversion should return Some");
        assert!(matches!(msg, EventMsg::TurnStarted { turn } if turn == 5));
    }

    #[test]
    fn token_usage_conversion() {
        let event = StreamEvent::TokenUsage {
            input_tokens: 1000,
            output_tokens: 500,
        };
        let msg = stream_event_to_event_msg(event).expect("conversion should return Some");
        assert!(matches!(
            msg,
            EventMsg::TokenUsage {
                input_tokens,
                output_tokens,
                cache_read_tokens,
                cache_creation_tokens
            } if input_tokens == 1000 && output_tokens == 500 && cache_read_tokens == 0 && cache_creation_tokens == 0
        ));
    }

    #[test]
    fn turn_completed_conversion() {
        let event = StreamEvent::TurnCompleted {
            stop_reason: "end_turn".into(),
        };
        let msg = stream_event_to_event_msg(event).expect("conversion should return Some");
        assert!(matches!(
            msg,
            EventMsg::TurnCompleted { stop_reason } if stop_reason == "end_turn"
        ));
    }

    #[test]
    fn cache_usage_conversion() {
        let event = StreamEvent::CacheUsage {
            cache_read_tokens: 2048,
            cache_creation_tokens: 512,
        };
        let msg = stream_event_to_event_msg(event).expect("conversion should return Some");
        assert!(matches!(
            msg,
            EventMsg::TokenUsage {
                input_tokens,
                output_tokens,
                cache_read_tokens,
                cache_creation_tokens
            } if input_tokens == 0 && output_tokens == 0 && cache_read_tokens == 2048 && cache_creation_tokens == 512
        ));
    }

    #[test]
    fn done_conversion() {
        let event = StreamEvent::Done;
        let msg = stream_event_to_event_msg(event).expect("conversion should return Some");
        assert!(matches!(msg, EventMsg::Done));
    }

    #[test]
    fn plan_created_conversion() {
        let event = StreamEvent::PlanCreated {
            id: "plan_1".into(),
            title: "Test Plan".into(),
            steps: vec![StreamPlanStep {
                name: "Step 1".into(),
                description: "Do something".into(),
            }],
        };
        let msg = stream_event_to_event_msg(event).expect("conversion should return Some");
        assert!(matches!(
            msg,
            EventMsg::PlanCreated { plan_id, title, steps } if plan_id == "plan_1" && title == "Test Plan" && steps.len() == 1 && steps[0].name == "Step 1"
        ));
    }

    #[test]
    fn plan_step_started_conversion() {
        let event = StreamEvent::PlanStepStarted {
            plan_id: "plan_1".into(),
            step_index: 0,
        };
        let msg = stream_event_to_event_msg(event).expect("conversion should return Some");
        assert!(matches!(
            msg,
            EventMsg::PlanStepStarted { plan_id, step_index } if plan_id == "plan_1" && step_index == 0
        ));
    }

    #[test]
    fn plan_step_completed_conversion() {
        let event = StreamEvent::PlanStepCompleted {
            plan_id: "plan_1".into(),
            step_index: 0,
            success: true,
            message: "Complete".into(),
        };
        let msg = stream_event_to_event_msg(event).expect("conversion should return Some");
        assert!(matches!(
            msg,
            EventMsg::PlanStepCompleted {
                plan_id,
                step_index,
                success,
                message
            } if plan_id == "plan_1" && step_index == 0 && success && message == "Complete"
        ));
    }

    #[test]
    fn plan_completed_conversion() {
        let event = StreamEvent::PlanCompleted {
            plan_id: "plan_1".into(),
            success: true,
            summary: "All done".into(),
        };
        let msg = stream_event_to_event_msg(event).expect("conversion should return Some");
        assert!(matches!(
            msg,
            EventMsg::PlanCompleted {
                plan_id,
                success,
                summary
            } if plan_id == "plan_1" && success && summary == "All done"
        ));
    }

    #[test]
    fn plan_approval_requested_conversion() {
        let event = StreamEvent::PlanApprovalRequested {
            plan_id: "plan_1".into(),
            title: "Test Plan".into(),
            steps: vec![StreamPlanStep {
                name: "Step 1".into(),
                description: "Do something".into(),
            }],
        };
        let msg = stream_event_to_event_msg(event).expect("conversion should return Some");
        assert!(matches!(
            msg,
            EventMsg::PlanApprovalRequested { plan_id, title, steps } if plan_id == "plan_1" && title == "Test Plan" && steps.len() == 1 && steps[0].name == "Step 1"
        ));
    }

    #[test]
    fn tool_exec_completed_empty_output() {
        let event = StreamEvent::ToolExecCompleted {
            id: "tool_1".into(),
            name: "Bash".into(),
            output: String::new(),
            is_error: false,
        };
        let msg = stream_event_to_event_msg(event).expect("conversion should return Some");
        assert!(matches!(
            msg,
            EventMsg::ToolExecCompleted { output_size, .. } if output_size == 0
        ));
    }
}
