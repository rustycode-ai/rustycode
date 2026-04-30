use crate::{Tool, ToolContext, ToolOutput, ToolPermission};
use anyhow::{anyhow, Result};
use serde_json::{json, Value};

/// Send a message to another agent by name.
///
/// Plain text output is NOT visible to other agents — communication
/// MUST go through this tool. Supports named teammates and broadcast.
pub struct SendMessageTool;

impl Tool for SendMessageTool {
    fn name(&self) -> &'static str {
        "send_message"
    }

    fn description(&self) -> &'static str {
        r#"Send a message to another agent.

```json
{"to": "researcher", "summary": "assign task 1", "message": "start on task #1"}
```

| `to` | |
|---|---|
| `"researcher"` | Teammate by name |
| `"*"` | Broadcast to all teammates |

Your plain text output is NOT visible to other agents — to communicate, you MUST call this tool. Messages from teammates are delivered automatically; you don't check an inbox. Refer to teammates by name, never by UUID.

## Protocol responses (legacy)

If you receive a JSON message with `type: "shutdown_request"` or `type: "plan_approval_request"`, respond with the matching `_response` type — echo the `request_id`, set `approve` true/false."#
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::None
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["to", "message"],
            "properties": {
                "to": {
                    "type": "string",
                    "description": "Recipient: teammate name, or '*' for broadcast"
                },
                "message": {
                    "description": "Plain text message content, or structured JSON for protocol responses",
                    "type": "string"
                },
                "summary": {
                    "type": "string",
                    "description": "A 5-10 word summary shown as a preview in the UI"
                }
            }
        })
    }

    fn execute(&self, params: Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        let to = params
            .get("to")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("missing recipient"))?;

        let message = params
            .get("message")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("missing message"))?;

        if message.trim().is_empty() {
            return Err(anyhow!("message must not be empty"));
        }

        let summary = params.get("summary").and_then(Value::as_str).unwrap_or("");

        let is_broadcast = to == "*";

        Ok(ToolOutput::with_structured(
            format!("Message sent to {to}"),
            json!({
                "to": to,
                "message": message,
                "summary": summary,
                "broadcast": is_broadcast,
            }),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_ctx() -> ToolContext {
        ToolContext::new("/tmp")
    }

    #[test]
    fn test_send_message_metadata() {
        let tool = SendMessageTool;
        assert_eq!(tool.name(), "send_message");
        assert_eq!(tool.permission(), ToolPermission::None);
    }

    #[test]
    fn test_send_message_to_teammate() {
        let tool = SendMessageTool;
        let result = tool.execute(
            json!({"to": "researcher", "message": "start on task #1"}),
            &test_ctx(),
        );
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.text.contains("researcher"));
    }

    #[test]
    fn test_send_message_broadcast() {
        let tool = SendMessageTool;
        let result = tool.execute(json!({"to": "*", "message": "standup time"}), &test_ctx());
        assert!(result.is_ok());
        let structured = result.unwrap().structured.unwrap();
        assert_eq!(structured["broadcast"], true);
    }

    #[test]
    fn test_send_message_with_summary() {
        let tool = SendMessageTool;
        let result = tool.execute(
            json!({"to": "tester", "summary": "assign task 1", "message": "run the e2e tests"}),
            &test_ctx(),
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_send_message_missing_recipient() {
        let tool = SendMessageTool;
        let result = tool.execute(json!({"message": "hello"}), &test_ctx());
        assert!(result.is_err());
    }

    #[test]
    fn test_send_message_empty_message() {
        let tool = SendMessageTool;
        let result = tool.execute(json!({"to": "lead", "message": ""}), &test_ctx());
        assert!(result.is_err());
    }
}
