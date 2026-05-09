use crate::{ToolOutput, ToolPermission, ToolTag};
use anyhow::anyhow;
use schemars::JsonSchema;
use serde_json::json;

#[derive(serde::Deserialize, JsonSchema)]
pub struct BriefParams {
    /// The message to send to the user. Supports markdown formatting.
    message: String,
    /// Intent label. 'normal' when replying; 'proactive' when initiating (background task, blocker, unsolicited input).
    #[serde(default)]
    status: Option<String>,
    /// File paths (absolute or cwd-relative) for images, diffs, logs to attach.
    #[serde(default)]
    attachments: Option<Vec<String>>,
}

// Primary user communication channel.
rustycode_tools_api::define_tool! {
    pub struct BriefTool;

    name: "brief",
    description: r#"Send a message the user will read. Text outside this tool is visible in the detail view, but most won't open it — the answer lives here.

`message` supports markdown. `attachments` takes file paths (absolute or cwd-relative) for images, diffs, logs.

`status` labels intent: "normal" when replying to what they just asked; "proactive" when initiating — a scheduled task finished, a blocker surfaced during background work, you need input on something they haven't asked about. Set it honestly; downstream routing uses it."#,
    permission: ToolPermission::None,
    tags: [ToolTag::Explore],

    execute(params: BriefParams, ctx) {
        let message = &params.message;

        if message.trim().is_empty() {
            return Err(anyhow!("message must not be empty"));
        }

        let status = params.status.as_deref().unwrap_or("normal");

        let attachments = params.attachments.unwrap_or_default();

        // Resolve relative attachment paths against cwd
        let resolved: Vec<String> = attachments
            .into_iter()
            .map(|p| {
                if p.starts_with('/') {
                    p
                } else {
                    format!("{}/{}", ctx.cwd.display(), p)
                }
            })
            .collect();

        Ok(ToolOutput::text(message.to_string()).with_metadata(ctx, || json!({
                "status": status,
                "attachments": resolved,
            })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Tool;
    use crate::ToolContext;

    fn test_ctx() -> ToolContext {
        ToolContext::new("/tmp")
    }

    #[test]
    fn test_brief_metadata() {
        let tool = BriefTool;
        assert_eq!(tool.name(), "brief");
        assert_eq!(tool.permission(), ToolPermission::None);
        assert!(tool.description().contains("detail view"));
    }

    #[test]
    fn test_brief_sends_message() {
        let tool = BriefTool;
        let result = tool.execute(
            json!({"message": "Fixed the bug in src/main.rs:42"}),
            &test_ctx(),
        );
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.text.contains("Fixed the bug"));
    }

    #[test]
    fn test_brief_with_status() {
        let tool = BriefTool;
        let result = tool.execute(
            json!({"message": "Build complete", "status": "proactive"}),
            &test_ctx(),
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_brief_rejects_empty_message() {
        let tool = BriefTool;
        let result = tool.execute(json!({"message": "  "}), &test_ctx());
        assert!(result.is_err());
    }

    #[test]
    fn test_brief_resolves_relative_attachments() {
        let tool = BriefTool;
        let result = tool.execute(
            json!({
                "message": "See attached",
                "attachments": ["diff.patch", "/absolute/log.txt"]
            }),
            &test_ctx(),
        );
        assert!(result.is_ok());
        // Verify resolved paths are in structured output
        let output = result.unwrap();
        let structured = output.structured.unwrap();
        let attachments = structured["attachments"].as_array().unwrap();
        assert_eq!(attachments.len(), 2);
        assert_eq!(attachments[0], "/tmp/diff.patch");
        assert_eq!(attachments[1], "/absolute/log.txt");
    }
}
