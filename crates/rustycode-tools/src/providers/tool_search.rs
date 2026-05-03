use crate::{Tool, ToolContext, ToolOutput, ToolPermission, ToolTag};
use anyhow::{anyhow, Result};
use serde_json::{json, Value};

/// Deferred tool loading — fetches full schema definitions on demand.
///
/// Tools are initially announced by name only. When the model needs to call
/// a tool, it uses this to load the full schema. Token optimization pattern.
pub struct ToolSearchTool;

impl Tool for ToolSearchTool {
    fn name(&self) -> &'static str {
        "tool_search"
    }

    fn description(&self) -> &'static str {
        r#"Fetches full schema definitions for deferred tools so they can be called.

When you see a tool name in the available tools list but don't have its full schema, use this tool to load it. Pass the tool name and get back the complete parameter schema and description."#
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::None
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["query"],
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Tool name to search for and load full schema"
                }
            }
        })
    }

    fn tags(&self) -> &[ToolTag] {
        &[ToolTag::Explore]
    }

    fn execute(&self, params: Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        let query = params
            .get("query")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("missing query"))?;

        if query.trim().is_empty() {
            return Err(anyhow!("query must not be empty"));
        }

        // Placeholder: actual tool lookup requires registry integration
        Ok(ToolOutput::with_structured(
            format!("Tool search: {query}"),
            json!({
                "query": query,
                "found": false,
                "note": "Tool search requires runtime registry integration",
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
    fn test_tool_search_metadata() {
        let tool = ToolSearchTool;
        assert_eq!(tool.name(), "tool_search");
        assert_eq!(tool.permission(), ToolPermission::None);
    }

    #[test]
    fn test_tool_search_requires_query() {
        let tool = ToolSearchTool;
        let result = tool.execute(json!({}), &test_ctx());
        assert!(result.is_err());
    }

    #[test]
    fn test_tool_search_rejects_empty() {
        let tool = ToolSearchTool;
        let result = tool.execute(json!({"query": "  "}), &test_ctx());
        assert!(result.is_err());
    }

    #[test]
    fn test_tool_search_returns_result() {
        let tool = ToolSearchTool;
        let result = tool.execute(json!({"query": "bash"}), &test_ctx());
        assert!(result.is_ok());
        assert!(result.unwrap().text.contains("bash"));
    }
}
