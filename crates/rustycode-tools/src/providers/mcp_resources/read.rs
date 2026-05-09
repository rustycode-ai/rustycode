use super::*;
use crate::{ToolOutput, ToolPermission, ToolTag};
use serde_json::json;

rustycode_tools_api::define_tool! {
    pub struct ReadMcpResourceTool;

    name: "ReadMcpResource",
    description: r#"Reads a specific resource from an MCP server, identified by server name and resource URI.

Parameters:
- server (required): The name of the MCP server to read from
- uri (required): The URI of the resource to read"#,
    permission: ToolPermission::Read,
    tags: [ToolTag::Explore],

    execute(params: ReadMcpResourceParams, ctx) {
        let server = &params.server;
        let uri = &params.uri;

        // Placeholder: actual MCP resource reading requires rustycode-mcp integration
        Ok(ToolOutput::text(format!("MCP resource {uri} from {server} — requires MCP runtime")).with_metadata(ctx, || json!({
                "server": server,
                "uri": uri,
            })))
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests_common::*;
    use super::*;
    use crate::Tool;

    #[test]
    fn test_read_mcp_resource_metadata() {
        let tool = ReadMcpResourceTool;
        assert_eq!(tool.name(), "ReadMcpResource");
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
