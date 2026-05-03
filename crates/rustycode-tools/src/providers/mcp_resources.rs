use crate::{Tool, ToolContext, ToolOutput, ToolPermission, ToolTag};
use anyhow::{anyhow, Result};
use serde_json::{json, Value};

/// List available resources from configured MCP servers.
pub struct ListMcpResourcesTool;

impl Tool for ListMcpResourcesTool {
    fn name(&self) -> &'static str {
        "list_mcp_resources"
    }

    fn description(&self) -> &'static str {
        r#"List available resources from configured MCP servers.

Each returned resource includes all standard MCP resource fields plus a 'server' field indicating which server the resource belongs to.

Parameters:
- server (optional): The name of a specific MCP server to get resources from. If not provided, resources from all servers will be returned."#
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::Read
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "server": {
                    "type": "string",
                    "description": "Optional server name to filter resources by"
                }
            }
        })
    }

    fn tags(&self) -> &[ToolTag] {
        &[ToolTag::Explore]
    }

    fn execute(&self, params: Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        let server = params.get("server").and_then(Value::as_str);

        // Placeholder: actual MCP resource listing requires rustycode-mcp integration
        let msg = match server {
            Some(s) => format!("MCP resources for server '{s}' — requires MCP runtime"),
            None => "MCP resources from all servers — requires MCP runtime".to_string(),
        };

        Ok(ToolOutput::with_structured(
            msg,
            json!({
                "resources": [],
                "server": server,
            }),
        ))
    }
}

/// Read a specific resource from an MCP server by URI.
pub struct ReadMcpResourceTool;

impl Tool for ReadMcpResourceTool {
    fn name(&self) -> &'static str {
        "read_mcp_resource"
    }

    fn description(&self) -> &'static str {
        r#"Reads a specific resource from an MCP server, identified by server name and resource URI.

Parameters:
- server (required): The name of the MCP server to read from
- uri (required): The URI of the resource to read"#
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::Read
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["server", "uri"],
            "properties": {
                "server": {
                    "type": "string",
                    "description": "The MCP server name"
                },
                "uri": {
                    "type": "string",
                    "description": "The URI of the resource to read"
                }
            }
        })
    }

    fn tags(&self) -> &[ToolTag] {
        &[ToolTag::Explore]
    }

    fn execute(&self, params: Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        let server = params
            .get("server")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("missing server name"))?;

        let uri = params
            .get("uri")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("missing resource URI"))?;

        // Placeholder: actual MCP resource reading requires rustycode-mcp integration
        Ok(ToolOutput::with_structured(
            format!("MCP resource {uri} from {server} — requires MCP runtime"),
            json!({
                "server": server,
                "uri": uri,
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
    fn test_list_mcp_resources_metadata() {
        let tool = ListMcpResourcesTool;
        assert_eq!(tool.name(), "list_mcp_resources");
        assert_eq!(tool.permission(), ToolPermission::Read);
    }

    #[test]
    fn test_list_mcp_resources_no_filter() {
        let tool = ListMcpResourcesTool;
        let result = tool.execute(json!({}), &test_ctx());
        assert!(result.is_ok());
    }

    #[test]
    fn test_list_mcp_resources_with_server() {
        let tool = ListMcpResourcesTool;
        let result = tool.execute(json!({"server": "context7"}), &test_ctx());
        assert!(result.is_ok());
        assert!(result.unwrap().text.contains("context7"));
    }

    #[test]
    fn test_read_mcp_resource_metadata() {
        let tool = ReadMcpResourceTool;
        assert_eq!(tool.name(), "read_mcp_resource");
        assert_eq!(tool.permission(), ToolPermission::Read);
    }

    #[test]
    fn test_read_mcp_resource_requires_server() {
        let tool = ReadMcpResourceTool;
        let result = tool.execute(json!({"uri": "test://res"}), &test_ctx());
        assert!(result.is_err());
    }

    #[test]
    fn test_read_mcp_resource_requires_uri() {
        let tool = ReadMcpResourceTool;
        let result = tool.execute(json!({"server": "test"}), &test_ctx());
        assert!(result.is_err());
    }

    #[test]
    fn test_read_mcp_resource_returns_result() {
        let tool = ReadMcpResourceTool;
        let result = tool.execute(
            json!({"server": "context7", "uri": "docs://react"}),
            &test_ctx(),
        );
        assert!(result.is_ok());
    }
}
