use schemars::JsonSchema;

// Re-export all tools
pub use list::*;
pub use read::*;

pub mod list;
pub mod read;

#[derive(serde::Deserialize, JsonSchema)]
pub struct ListMcpResourcesParams {
    /// Optional server name to filter resources by
    pub server: Option<String>,
}

#[derive(serde::Deserialize, JsonSchema)]
pub struct ReadMcpResourceParams {
    /// The MCP server name
    pub server: String,
    /// The URI of the resource to read
    pub uri: String,
}

#[cfg(test)]
pub(crate) mod tests_common {
    use crate::ToolContext;

    pub fn test_ctx() -> ToolContext {
        ToolContext::new("/tmp")
    }
}
