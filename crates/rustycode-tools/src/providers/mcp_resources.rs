use crate::{ToolOutput, ToolPermission, ToolTag};
use schemars::JsonSchema;
use serde_json::json;

#[derive(serde::Deserialize, JsonSchema)]
pub struct ListMcpResourcesParams {
    /// Optional server name to filter resources by
    server: Option<String>,
}

rustycode_tools_api::define_tool! {
    pub struct ListMcpResourcesTool;

    name: "list_mcp_resources",
    description: r#"List available resources from configured MCP servers.

Each returned resource includes all standard MCP resource fields plus a 'server' field indicating which server the resource belongs to.

Parameters:
- server (optional): The name of a specific MCP server to get resources from. If not provided, resources from all servers will be returned."#,
    permission: ToolPermission::Read,
    tags: [ToolTag::Explore],

    execute(params: ListMcpResourcesParams, _ctx) {
        let server = params.server;

        // Placeholder: actual MCP resource listing requires rustycode-mcp integration
        let msg = match server {
            Some(ref s) => format!("MCP resources for server '{s}' — requires MCP runtime"),
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

#[derive(serde::Deserialize, JsonSchema)]
pub struct ReadMcpResourceParams {
    /// The MCP server name
    server: String,
    /// The URI of the resource to read
    uri: String,
}

rustycode_tools_api::define_tool! {
    pub struct ReadMcpResourceTool;

    name: "read_mcp_resource",
    description: r#"Reads a specific resource from an MCP server, identified by server name and resource URI.

Parameters:
- server (required): The name of the MCP server to read from
- uri (required): The URI of the resource to read"#,
    permission: ToolPermission::Read,
    tags: [ToolTag::Explore],

    execute(params: ReadMcpResourceParams, _ctx) {
        let server = &params.server;
        let uri = &params.uri;

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
    use crate::Tool;
    use crate::ToolContext;

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
