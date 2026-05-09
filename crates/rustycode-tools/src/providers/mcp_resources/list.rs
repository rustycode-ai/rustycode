use super::*;
use crate::{ToolOutput, ToolPermission, ToolTag};
use serde_json::json;

rustycode_tools_api::define_tool! {
    pub struct ListMcpResourcesTool;

    name: "list_mcp_resources",
    description: r#"List available resources from configured MCP servers.

Each returned resource includes all standard MCP resource fields plus a 'server' field indicating which server the resource belongs to.

Parameters:
- server (optional): The name of a specific MCP server to get resources from. If not provided, resources from all servers will be returned."#,
    permission: ToolPermission::Read,
    tags: [ToolTag::Explore],

    execute(params: ListMcpResourcesParams, ctx) {
        let server = params.server;

        // Placeholder: actual MCP resource listing requires rustycode-mcp integration
        let msg = match server {
            Some(ref s) => format!("MCP resources for server '{s}' — requires MCP runtime"),
            None => "MCP resources from all servers — requires MCP runtime".to_string(),
        };

        Ok(ToolOutput::text(msg).with_metadata(ctx, || json!({
                "resources": [],
                "server": server,
            })))
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests_common::*;
    use super::*;
    use crate::Tool;

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
}
