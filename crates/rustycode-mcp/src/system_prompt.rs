//! System prompt integration for MCP servers
//!
//! Generates system prompts that inform the AI about available MCP servers,
//! their tools, resources, and capabilities.

use crate::manager::McpServer;
use crate::types::{McpResource, McpTool};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::debug;

/// System prompt configuration for an MCP server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpSystemPrompt {
    pub server_name: String,
    pub server_description: String,
    /// Available tools with descriptions
    pub tools: Vec<ToolDescription>,
    /// Available resources with descriptions
    pub resources: Vec<ResourceDescription>,
    /// Usage guidelines
    pub guidelines: Vec<String>,
}

/// Tool description for system prompts
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDescription {
    /// Tool name
    pub name: String,
    /// Tool description
    pub description: String,
    /// Example usage (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub example: Option<String>,
    /// When to use this tool
    #[serde(skip_serializing_if = "Option::is_none")]
    pub when_to_use: Option<String>,
}

/// Resource description for system prompts
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceDescription {
    /// Resource URI pattern
    pub uri_pattern: String,
    /// Resource name
    pub name: String,
    /// Resource description
    pub description: String,
    /// MIME type
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
}

/// Generate system prompts for MCP servers
pub struct McpPromptGenerator {
    /// Cached prompts per server
    prompts: HashMap<String, McpSystemPrompt>,
}

impl McpPromptGenerator {
    pub fn new() -> Self {
        Self {
            prompts: HashMap::new(),
        }
    }

    /// Generate a system prompt for a server
    pub async fn generate_prompt(
        &mut self,
        server: &McpServer,
        tools: Vec<McpTool>,
        resources: Vec<McpResource>,
    ) -> String {
        let server_id = server.id().to_string();
        debug!("Generating system prompt for server '{}'", server_id);

        // Check cache first
        if let Some(cached) = self.prompts.get(&server_id) {
            return self.format_prompt(cached);
        }

        // Build tool descriptions
        let tool_descriptions: Vec<ToolDescription> = tools
            .iter()
            .map(|tool| ToolDescription {
                name: tool.name.clone(),
                description: tool.description.clone(),
                example: self.generate_example(tool),
                when_to_use: self.generate_when_to_use(tool),
            })
            .collect();

        // Build resource descriptions
        let resource_descriptions: Vec<ResourceDescription> = resources
            .iter()
            .map(|resource| ResourceDescription {
                uri_pattern: resource.uri.clone(),
                name: resource.name.clone(),
                description: resource.description.clone(),
                mime_type: resource.mime_type.clone(),
            })
            .collect();

        // Generate guidelines
        let guidelines = self.generate_guidelines(&tool_descriptions, &resource_descriptions);

        let prompt = McpSystemPrompt {
            server_name: server_id.clone(),
            server_description: format!(
                "MCP server providing {} tools and {} resources",
                tools.len(),
                resources.len()
            ),
            tools: tool_descriptions,
            resources: resource_descriptions,
            guidelines,
        };

        // Cache the prompt
        self.prompts.insert(server_id, prompt.clone());

        self.format_prompt(&prompt)
    }

    /// Generate example usage for a tool
    fn generate_example(&self, tool: &McpTool) -> Option<String> {
        // Extract parameters from schema if available
        if let Some(schema) = tool.input_schema.as_object() {
            if let Some(properties) = schema.get("properties").and_then(|p| p.as_object()) {
                let mut params = Vec::new();
                for (param_name, param_schema) in properties.iter().take(3) {
                    if let Some(obj) = param_schema.as_object() {
                        let default = obj
                            .get("default")
                            .or(obj.get("example"))
                            .map_or_else(|| "\"value\"".to_string(), ToString::to_string);
                        params.push(format!("{param_name}: {default}"));
                    }
                }
                if !params.is_empty() {
                    return Some(format!("{}({})", tool.name, params.join(", ")));
                }
            }
        }
        Some(format!("{}()", tool.name))
    }

    /// Generate when-to-use hint for a tool
    fn generate_when_to_use(&self, tool: &McpTool) -> Option<String> {
        let name = tool.name.to_lowercase();
        let desc = tool.description.to_lowercase();

        // Name matching uses segments (split on _, -, :) to avoid false positives
        // like "thread_reader" matching "read". Description matching uses substrings
        // since descriptions are free-text prose.
        if rustycode_protocol::tool_names::has_segment(&name, &["read", "get", "fetch"])
            || desc.contains("read")
            || desc.contains("retrieve")
            || desc.contains("fetch")
        {
            Some("Use when you need to retrieve information or data".to_string())
        } else if rustycode_protocol::tool_names::has_segment(&name, &["write", "create", "save"])
            || desc.contains("write")
            || desc.contains("create")
            || desc.contains("save")
        {
            Some("Use when you need to create or modify data".to_string())
        } else if rustycode_protocol::tool_names::has_segment(&name, &["delete", "remove"])
            || desc.contains("delete")
            || desc.contains("remove")
        {
            Some("Use when you need to delete or remove something".to_string())
        } else if rustycode_protocol::tool_names::has_segment(&name, &["list", "Search", "Find"])
            || desc.contains("list")
            || desc.contains("Search")
            || desc.contains("Find")
        {
            Some("Use when you need to find or list items".to_string())
        } else if rustycode_protocol::tool_names::has_segment(&name, &["execute", "run", "call"])
            || desc.contains("execute")
            || desc.contains("run")
            || desc.contains("invoke")
        {
            Some("Use when you need to execute a command or function".to_string())
        } else {
            None
        }
    }

    /// Generate usage guidelines
    fn generate_guidelines(
        &self,
        _tools: &[ToolDescription],
        _resources: &[ResourceDescription],
    ) -> Vec<String> {
        vec![
            "Always prefer using MCP tools over manual operations when available".to_string(),
            "Check resource availability before attempting to read".to_string(),
            "Handle errors gracefully and inform the user of any issues".to_string(),
            "Use descriptive parameters when calling tools".to_string(),
        ]
    }

    /// Format a prompt for display
    fn format_prompt(&self, prompt: &McpSystemPrompt) -> String {
        let mut output = String::new();

        output.push_str(&format!("## MCP Server: {}\n\n", prompt.server_name));
        output.push_str(&format!("{}\n\n", prompt.server_description));

        if !prompt.tools.is_empty() {
            output.push_str("### Available Tools\n\n");
            for tool in &prompt.tools {
                output.push_str(&format!("**{}** - {}\n", tool.name, tool.description));
                if let Some(when) = &tool.when_to_use {
                    output.push_str(&format!("  *When*: {when}\n"));
                }
                if let Some(example) = &tool.example {
                    output.push_str(&format!("  *Example*: `{example}`\n"));
                }
            }
            output.push('\n');
        }

        if !prompt.resources.is_empty() {
            output.push_str("### Available Resources\n\n");
            for resource in &prompt.resources {
                output.push_str(&format!(
                    "**{}** ({}) - {}\n",
                    resource.name, resource.uri_pattern, resource.description
                ));
                if let Some(mime) = &resource.mime_type {
                    output.push_str(&format!("  *Type*: {mime}\n"));
                }
            }
            output.push('\n');
        }

        if !prompt.guidelines.is_empty() {
            output.push_str("### Usage Guidelines\n\n");
            for guideline in &prompt.guidelines {
                output.push_str(&format!("- {guideline}\n"));
            }
        }

        output
    }

    /// Clear cached prompts
    pub fn clear_cache(&mut self) {
        self.prompts.clear();
    }

    /// Remove a specific server from cache
    pub fn remove_server(&mut self, server_id: &str) {
        self.prompts.remove(server_id);
    }

    /// Get all cached prompts
    pub const fn get_all_prompts(&self) -> &HashMap<String, McpSystemPrompt> {
        &self.prompts
    }
}

impl Default for McpPromptGenerator {
    fn default() -> Self {
        Self::new()
    }
}

/// Combine multiple MCP server prompts into a single system prompt
pub fn combine_mcp_prompts(prompts: &[McpSystemPrompt]) -> String {
    if prompts.is_empty() {
        return String::new();
    }

    let mut output = String::from("# MCP Servers Integration\n\n");
    output.push_str(&format!(
        "This session has access to {} MCP server(s) providing various tools and resources.\n\n",
        prompts.len()
    ));

    for prompt in prompts {
        output.push_str(&format!(
            "---\n\n{}\n",
            McpPromptGenerator::new().format_prompt(prompt)
        ));
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_description_generation() {
        let tool = McpTool {
            name: "Read".to_string(),
            title: None,
            description: "Read contents of a file".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to the file"
                    }
                }
            }),
            output_schema: None,
            annotations: None,
            category: Some("files".to_string()),
        };

        let generator = McpPromptGenerator::new();
        let desc = ToolDescription {
            name: tool.name.clone(),
            description: tool.description.clone(),
            example: generator.generate_example(&tool),
            when_to_use: generator.generate_when_to_use(&tool),
        };

        assert_eq!(desc.name, "Read");
        assert!(desc.when_to_use.is_some());
        assert!(desc.example.is_some());
    }

    #[test]
    fn test_when_to_use_generation() {
        let generator = McpPromptGenerator::new();

        // Test read tool
        let read_tool = McpTool {
            name: "get_data".to_string(),
            title: None,
            description: "Get data from database".to_string(),
            input_schema: serde_json::json!({}),
            output_schema: None,
            annotations: None,
            category: None,
        };
        let when = generator.generate_when_to_use(&read_tool);
        assert!(when.is_some());
        assert!(when.unwrap().to_lowercase().contains("retrieve"));

        // Test write tool
        let write_tool = McpTool {
            name: "create_record".to_string(),
            title: None,
            description: "Create a new record".to_string(),
            input_schema: serde_json::json!({}),
            output_schema: None,
            annotations: None,
            category: None,
        };
        let when = generator.generate_when_to_use(&write_tool);
        assert!(when.is_some());
        assert!(when.unwrap().to_lowercase().contains("create"));
    }

    #[test]
    fn test_guidelines_generation() {
        let generator = McpPromptGenerator::new();
        let guidelines = generator.generate_guidelines(&[], &[]);
        assert!(!guidelines.is_empty());
        assert!(guidelines.iter().any(|g| g.contains("MCP tools")));
    }

    #[test]
    fn test_prompt_formatting() {
        let generator = McpPromptGenerator::new();
        let prompt = McpSystemPrompt {
            server_name: "test-server".to_string(),
            server_description: "Test server".to_string(),
            tools: vec![ToolDescription {
                name: "test_tool".to_string(),
                description: "A test tool".to_string(),
                example: Some("test_tool(param: value)".to_string()),
                when_to_use: Some("Use for testing".to_string()),
            }],
            resources: vec![],
            guidelines: vec!["Test guideline".to_string()],
        };

        let formatted = generator.format_prompt(&prompt);
        assert!(formatted.contains("## MCP Server: test-server"));
        assert!(formatted.contains("**test_tool**"));
        assert!(formatted.contains("A test tool"));
        assert!(formatted.contains("Use for testing"));
    }

    #[test]
    fn test_combine_prompts() {
        let prompts = vec![
            McpSystemPrompt {
                server_name: "server1".to_string(),
                server_description: "First server".to_string(),
                tools: vec![],
                resources: vec![],
                guidelines: vec![],
            },
            McpSystemPrompt {
                server_name: "server2".to_string(),
                server_description: "Second server".to_string(),
                tools: vec![],
                resources: vec![],
                guidelines: vec![],
            },
        ];

        let combined = combine_mcp_prompts(&prompts);
        assert!(combined.contains("# MCP Servers Integration"));
        assert!(combined.contains("2 MCP server"));
        assert!(combined.contains("server1"));
        assert!(combined.contains("server2"));
    }

    #[test]
    fn test_combine_prompts_empty() {
        let combined = combine_mcp_prompts(&[]);
        assert!(combined.is_empty());
    }

    #[test]
    fn test_mcp_system_prompt_serialization() {
        let prompt = McpSystemPrompt {
            server_name: "test".to_string(),
            server_description: "desc".to_string(),
            tools: vec![ToolDescription {
                name: "tool1".to_string(),
                description: "A tool".to_string(),
                example: None,
                when_to_use: None,
            }],
            resources: vec![ResourceDescription {
                uri_pattern: "file://{p}".to_string(),
                name: "files".to_string(),
                description: "File access".to_string(),
                mime_type: Some("text/plain".to_string()),
            }],
            guidelines: vec!["Be careful".to_string()],
        };
        let json = serde_json::to_string(&prompt).unwrap();
        let parsed: McpSystemPrompt = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.server_name, "test");
        assert_eq!(parsed.tools.len(), 1);
        assert_eq!(parsed.resources.len(), 1);
        assert_eq!(parsed.guidelines.len(), 1);
    }

    #[test]
    fn test_tool_description_skip_optional() {
        let desc = ToolDescription {
            name: "t".to_string(),
            description: "d".to_string(),
            example: None,
            when_to_use: None,
        };
        let json = serde_json::to_string(&desc).unwrap();
        assert!(!json.contains("example"));
        assert!(!json.contains("when_to_use"));
    }

    #[test]
    fn test_resource_description_skip_optional() {
        let desc = ResourceDescription {
            uri_pattern: "x://y".to_string(),
            name: "n".to_string(),
            description: "d".to_string(),
            mime_type: None,
        };
        let json = serde_json::to_string(&desc).unwrap();
        assert!(!json.contains("mime_type"));
    }

    #[test]
    fn test_prompt_formatting_with_resources() {
        let generator = McpPromptGenerator::new();
        let prompt = McpSystemPrompt {
            server_name: "test".to_string(),
            server_description: "desc".to_string(),
            tools: vec![],
            resources: vec![ResourceDescription {
                uri_pattern: "file://x".to_string(),
                name: "files".to_string(),
                description: "File access".to_string(),
                mime_type: Some("text/plain".to_string()),
            }],
            guidelines: vec![],
        };
        let formatted = generator.format_prompt(&prompt);
        assert!(formatted.contains("### Available Resources"));
        assert!(formatted.contains("file://x"));
        assert!(formatted.contains("text/plain"));
    }

    #[test]
    fn test_prompt_formatting_empty_server() {
        let generator = McpPromptGenerator::new();
        let prompt = McpSystemPrompt {
            server_name: "empty".to_string(),
            server_description: "No tools".to_string(),
            tools: vec![],
            resources: vec![],
            guidelines: vec![],
        };
        let formatted = generator.format_prompt(&prompt);
        assert!(formatted.contains("## MCP Server: empty"));
        assert!(!formatted.contains("### Available Tools"));
        assert!(!formatted.contains("### Available Resources"));
    }

    #[test]
    fn test_generator_clear_cache() {
        let mut generator = McpPromptGenerator::new();
        // Manually insert a prompt
        generator.prompts.insert(
            "srv".to_string(),
            McpSystemPrompt {
                server_name: "srv".to_string(),
                server_description: "d".to_string(),
                tools: vec![],
                resources: vec![],
                guidelines: vec![],
            },
        );
        assert_eq!(generator.get_all_prompts().len(), 1);
        generator.clear_cache();
        assert!(generator.get_all_prompts().is_empty());
    }

    #[test]
    fn test_generator_remove_server() {
        let mut generator = McpPromptGenerator::new();
        generator.prompts.insert(
            "srv1".to_string(),
            McpSystemPrompt {
                server_name: "srv1".to_string(),
                server_description: "d".to_string(),
                tools: vec![],
                resources: vec![],
                guidelines: vec![],
            },
        );
        generator.prompts.insert(
            "srv2".to_string(),
            McpSystemPrompt {
                server_name: "srv2".to_string(),
                server_description: "d".to_string(),
                tools: vec![],
                resources: vec![],
                guidelines: vec![],
            },
        );
        generator.remove_server("srv1");
        assert_eq!(generator.get_all_prompts().len(), 1);
        assert!(generator.get_all_prompts().contains_key("srv2"));
    }

    #[test]
    fn test_generate_example_no_schema() {
        let generator = McpPromptGenerator::new();
        let tool = McpTool {
            name: "mystery".to_string(),
            title: None,
            description: "Unknown".to_string(),
            input_schema: serde_json::json!("not an object"),
            output_schema: None,
            annotations: None,
            category: None,
        };
        let example = generator.generate_example(&tool);
        assert_eq!(example, Some("mystery()".to_string()));
    }

    #[test]
    fn test_generate_when_to_use_delete() {
        let generator = McpPromptGenerator::new();
        let tool = McpTool {
            name: "delete_item".to_string(),
            title: None,
            description: "Delete".to_string(),
            input_schema: serde_json::json!({}),
            output_schema: None,
            annotations: None,
            category: None,
        };
        let when = generator.generate_when_to_use(&tool);
        assert!(when.is_some());
        assert!(when.unwrap().to_lowercase().contains("delete"));
    }

    #[test]
    fn test_generate_when_to_use_execute() {
        let generator = McpPromptGenerator::new();
        let tool = McpTool {
            name: "run_tests".to_string(),
            title: None,
            description: "Run".to_string(),
            input_schema: serde_json::json!({}),
            output_schema: None,
            annotations: None,
            category: None,
        };
        let when = generator.generate_when_to_use(&tool);
        assert!(when.is_some());
        assert!(when.unwrap().to_lowercase().contains("execute"));
    }

    #[test]
    fn test_generate_when_to_use_list() {
        let generator = McpPromptGenerator::new();
        let tool = McpTool {
            name: "list_items".to_string(),
            title: None,
            description: "List".to_string(),
            input_schema: serde_json::json!({}),
            output_schema: None,
            annotations: None,
            category: None,
        };
        let when = generator.generate_when_to_use(&tool);
        assert!(when.is_some());
        assert!(when.unwrap().to_lowercase().contains("find"));
    }

    #[test]
    fn test_generate_when_to_use_unknown() {
        let generator = McpPromptGenerator::new();
        let tool = McpTool {
            name: "transform".to_string(),
            title: None,
            description: "Transform data".to_string(),
            input_schema: serde_json::json!({}),
            output_schema: None,
            annotations: None,
            category: None,
        };
        let when = generator.generate_when_to_use(&tool);
        assert!(when.is_none());
    }

    #[test]
    fn test_generator_default() {
        let generator = McpPromptGenerator::default();
        assert!(generator.get_all_prompts().is_empty());
    }
}
