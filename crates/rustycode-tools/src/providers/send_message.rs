use crate::{ToolOutput, ToolPermission, ToolTag};
use anyhow::anyhow;
use schemars::JsonSchema;
use serde_json::json;

// ── Params structs ──────────────────────────────────────────────────────────

#[derive(serde::Deserialize, JsonSchema)]
pub struct SendMessageParams {
    /// Recipient: teammate name, or '*' for broadcast
    to: String,
    /// Plain text message content, or structured JSON for protocol responses
    message: String,
    /// A 5-10 word summary shown as a preview in the UI
    summary: Option<String>,
}

// ── Tool definition ─────────────────────────────────────────────────────────

rustycode_tools_api::define_tool! {
    pub struct SendMessageTool;

    name: "send_message",
    description: r#"Send a message to another agent.

```json
{"to": "researcher", "summary": "assign task 1", "message": "start on task #1"}
```

| `to` | |
|---|---|
| `"researcher"` | Teammate by name |
| `"*"` | Broadcast to all teammates |

Your plain text output is NOT visible to other agents — to communicate, you MUST call this tool. Messages from teammates are delivered automatically; you don't check an inbox. Refer to teammates by name, never by UUID.

## Protocol responses (legacy)

If you receive a JSON message with `type: "shutdown_request"` or `type: "plan_approval_request"`, respond with the matching `_response` type — echo the `request_id`, set `approve` true/false."#,
    permission: ToolPermission::None,
    tags: [ToolTag::Ops],

    execute(params: SendMessageParams, ctx) {
        let to = params.to;
        let message = params.message;

        if message.trim().is_empty() {
            return Err(anyhow!("message must not be empty"));
        }

        let summary = params.summary.as_deref().unwrap_or("");

        let is_broadcast = to == "*";

        if let Some(ref sender) = ctx.message_sender {
            if is_broadcast {
                sender
                    .broadcast(&message, summary)
                    .map_err(|e| anyhow!("broadcast failed: {e}"))?;
            } else {
                sender
                    .send(&to, &message, summary)
                    .map_err(|e| anyhow!("send failed: {e}"))?;
            }
            return Ok(ToolOutput::text(if is_broadcast {
                    "Broadcast delivered".into()
                } else {
                    format!("Message delivered to {to}")
                }).with_metadata(ctx, || json!({
                    "to": to,
                    "message": message,
                    "summary": summary,
                    "broadcast": is_broadcast,
                    "delivered": true,
                })));
        }

        Ok(ToolOutput::text(format!("Message sent to {to}")).with_metadata(ctx, || json!({
                "to": to,
                "message": message,
                "summary": summary,
                "broadcast": is_broadcast,
            })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MessageSender;
    use crate::Tool;
    use crate::ToolContext;
    use std::sync::{Arc, Mutex};

    fn test_ctx() -> ToolContext {
        ToolContext::new("/tmp").with_structured_output(true)
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

    #[derive(Debug)]
    struct MockMessageSender {
        sent: Arc<Mutex<Vec<(String, String, String)>>>,
        broadcasts: Arc<Mutex<Vec<(String, String)>>>,
    }

    impl MockMessageSender {
        fn new() -> Self {
            Self {
                sent: Arc::new(Mutex::new(Vec::new())),
                broadcasts: Arc::new(Mutex::new(Vec::new())),
            }
        }
    }

    impl MessageSender for MockMessageSender {
        fn send(&self, to: &str, message: &str, summary: &str) -> Result<(), String> {
            self.sent.lock().expect("sent lock").push((
                to.to_string(),
                message.to_string(),
                summary.to_string(),
            ));
            Ok(())
        }

        fn broadcast(&self, message: &str, summary: &str) -> Result<(), String> {
            self.broadcasts
                .lock()
                .expect("broadcasts lock")
                .push((message.to_string(), summary.to_string()));
            Ok(())
        }
    }

    #[test]
    fn test_send_message_with_real_sender() {
        let mock = Arc::new(MockMessageSender::new());
        let sent_clone = Arc::clone(&mock.sent);
        let ctx = ToolContext::new("/tmp")
            .with_message_sender(mock)
            .with_structured_output(true);

        let tool = SendMessageTool;
        let result = tool.execute(
            json!({"to": "researcher", "message": "start on task #1", "summary": "task assignment"}),
            &ctx,
        );

        assert!(result.is_ok());
        let output = result.unwrap();
        assert_eq!(output.text, "Message delivered to researcher");
        let structured = output.structured.expect("missing structured output");
        assert_eq!(structured["delivered"], true);
        assert_eq!(structured["to"], "researcher");

        let sent = sent_clone.lock().expect("sent lock");
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].0, "researcher");
        assert_eq!(sent[0].1, "start on task #1");
        assert_eq!(sent[0].2, "task assignment");
    }

    #[test]
    fn test_send_message_broadcast_with_real_sender() {
        let mock = Arc::new(MockMessageSender::new());
        let broadcasts_clone = Arc::clone(&mock.broadcasts);
        let ctx = ToolContext::new("/tmp")
            .with_message_sender(mock)
            .with_structured_output(true);

        let tool = SendMessageTool;
        let result = tool.execute(
            json!({"to": "*", "message": "standup time", "summary": "standup"}),
            &ctx,
        );

        assert!(result.is_ok());
        let output = result.unwrap();
        assert_eq!(output.text, "Broadcast delivered");
        let structured = output.structured.expect("missing structured output");
        assert_eq!(structured["delivered"], true);
        assert_eq!(structured["broadcast"], true);

        let broadcasts = broadcasts_clone.lock().expect("broadcasts lock");
        assert_eq!(broadcasts.len(), 1);
        assert_eq!(broadcasts[0].0, "standup time");
        assert_eq!(broadcasts[0].1, "standup");
    }

    #[test]
    fn test_send_message_sender_failure() {
        #[derive(Debug)]
        struct FailingSender;

        impl MessageSender for FailingSender {
            fn send(&self, _to: &str, _message: &str, _summary: &str) -> Result<(), String> {
                Err("mailbox full".to_string())
            }
            fn broadcast(&self, _message: &str, _summary: &str) -> Result<(), String> {
                Err("network down".to_string())
            }
        }

        let ctx = ToolContext::new("/tmp")
            .with_message_sender(Arc::new(FailingSender))
            .with_structured_output(true);
        let tool = SendMessageTool;
        let result = tool.execute(json!({"to": "researcher", "message": "hello"}), &ctx);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("send failed"),
            "expected send failure, got: {err}"
        );
        assert!(err.contains("mailbox full"));
    }

    #[test]
    fn test_send_message_fallback_without_sender() {
        let tool = SendMessageTool;
        let result = tool.execute(json!({"to": "researcher", "message": "hello"}), &test_ctx());
        assert!(result.is_ok());
        let output = result.unwrap();
        assert_eq!(output.text, "Message sent to researcher");
        let structured = output.structured.expect("missing structured output");
        assert!(structured.get("delivered").is_none());
    }
}
