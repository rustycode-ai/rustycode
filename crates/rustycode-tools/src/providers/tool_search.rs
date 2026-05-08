use crate::{ToolOutput, ToolPermission, ToolTag};
use anyhow::anyhow;
use schemars::JsonSchema;
use serde_json::{json, Value};

#[derive(serde::Deserialize, JsonSchema)]
pub struct ToolSearchParams {
    /// Tool name to search for and load full schema
    query: String,
}

rustycode_tools_api::define_tool! {
    pub struct ToolSearchTool;

    name: "tool_search",
    description: r#"Fetches full schema definitions for deferred tools so they can be called.

When you see a tool name in the available tools list but don't have its full schema, use this tool to load it. Pass the tool name and get back the complete parameter schema and description."#,
    permission: ToolPermission::None,
    tags: [ToolTag::Explore],

    execute(params: ToolSearchParams, ctx) {
        let query = params.query.trim();

        if query.is_empty() {
            return Err(anyhow!("query must not be empty"));
        }

        let registry = ctx
            .registry
            .as_ref()
            .ok_or_else(|| anyhow!("tool registry not available in this context"))?;

        // Exact name match — return full schema
        if let Some(tool) = registry.get(query) {
            let desc = tool.description();
            let first_line = desc.lines().next().unwrap_or(desc);
            return Ok(ToolOutput::with_structured(
                format!("Loaded schema for: {query}"),
                json!({
                    "query": query,
                    "found": true,
                    "match_type": "exact",
                    "name": tool.name(),
                    "description": first_line,
                    "full_description": desc,
                    "parameters_schema": tool.parameters_schema(),
                    "tags": tool.tags().iter().map(|t| t.as_str()).collect::<Vec<_>>(),
                }),
            ));
        }

        // Fuzzy match — name contains query, return up to 5 matches
        let query_lower = query.to_lowercase();
        let matches: Vec<Value> = registry
            .list()
            .iter()
            .filter(|t| t.name.to_lowercase().contains(&query_lower))
            .take(5)
            .map(|t| {
                let desc = &t.description;
                let first_line = desc.lines().next().unwrap_or(desc);
                json!({
                    "name": t.name,
                    "description": first_line,
                    "deferred": t.defer_loading == Some(true),
                })
            })
            .collect();

        if matches.is_empty() {
            return Ok(ToolOutput::with_structured(
                format!("No tools matching: {query}"),
                json!({
                    "query": query,
                    "found": false,
                    "suggestion": "Try a different search term or use a partial tool name",
                }),
            ));
        }

        Ok(ToolOutput::with_structured(
            format!("Found {} tools matching: {query}", matches.len()),
            json!({
                "query": query,
                "found": true,
                "match_type": "fuzzy",
                "results": matches,
            }),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Tool, ToolContext, ToolRegistry};
    use std::sync::Arc;

    fn test_ctx() -> ToolContext {
        ToolContext::new("/tmp")
    }

    fn test_ctx_with_registry() -> ToolContext {
        let mut registry = ToolRegistry::new();
        registry.register(ToolSearchTool);
        test_ctx().with_registry(Arc::new(registry))
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
        let result = tool.execute(json!({}), &test_ctx_with_registry());
        assert!(result.is_err());
    }

    #[test]
    fn test_tool_search_rejects_empty() {
        let tool = ToolSearchTool;
        let result = tool.execute(json!({"query": "  "}), &test_ctx_with_registry());
        assert!(result.is_err());
    }

    #[test]
    fn test_tool_search_requires_registry() {
        let tool = ToolSearchTool;
        let result = tool.execute(json!({"query": "bash"}), &test_ctx());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("registry"));
    }

    #[test]
    fn test_tool_search_exact_match() {
        let tool = ToolSearchTool;
        let ctx = test_ctx_with_registry();
        let result = tool.execute(json!({"query": "tool_search"}), &ctx).unwrap();
        let data = result.structured.unwrap();
        assert_eq!(data["found"], true);
        assert_eq!(data["match_type"], "exact");
        assert_eq!(data["name"], "tool_search");
    }

    #[test]
    fn test_tool_search_fuzzy_match() {
        let tool = ToolSearchTool;
        let ctx = test_ctx_with_registry();
        let result = tool.execute(json!({"query": "tool"}), &ctx).unwrap();
        let data = result.structured.unwrap();
        assert_eq!(data["found"], true);
        assert_eq!(data["match_type"], "fuzzy");
    }

    #[test]
    fn test_tool_search_no_match() {
        let tool = ToolSearchTool;
        let ctx = test_ctx_with_registry();
        let result = tool
            .execute(json!({"query": "xyznonexistent"}), &ctx)
            .unwrap();
        let data = result.structured.unwrap();
        assert_eq!(data["found"], false);
    }
}
