//! Shared tool selection utilities for LLM providers
//!
//! This module provides common functionality for intelligent tool selection
//! across all LLM providers (Anthropic, OpenAI, Gemini, etc.)

use crate::provider::{ChatMessage, MessageRole};
#[cfg(feature = "vector-memory")]
use rustycode_protocol::tool_names as tn;
#[cfg(feature = "vector-memory")]
use rustycode_tools_api::{route_query, SearchStrategy};
use rustycode_tools_api::{ToolInfo, ToolMetadataProvider, ToolRegistry};
use rustycode_tools_api::{ToolProfile, ToolSelector};
use std::sync::Arc;

/// Shared tool selection state - providers can embed this
pub struct ToolSelectionState {
    pub provider: Arc<dyn ToolMetadataProvider>,
    pub selector: ToolSelector,
    pub registry: Arc<ToolRegistry>,
}

impl ToolSelectionState {
    pub fn new(provider: Arc<dyn ToolMetadataProvider>, registry: Arc<ToolRegistry>) -> Self {
        Self {
            provider,
            selector: ToolSelector::new(),
            registry,
        }
    }

    /// Detect the user's intent from their latest message and select appropriate tools
    pub fn select_tools_for_prompt(
        &self,
        messages: &[ChatMessage],
        formatter: &dyn Fn(&[ToolInfo]) -> Vec<serde_json::Value>,
    ) -> Option<Vec<serde_json::Value>> {
        // Find the last user message to detect intent
        let user_prompt = messages
            .iter()
            .rev()
            .find(|msg| matches!(msg.role, MessageRole::User))
            .map(|msg| msg.content.as_text());

        if let Some(prompt) = user_prompt {
            // Detect profile from prompt
            let profile = ToolProfile::from_prompt(&prompt);

            // Update selector with detected profile
            let selector = self.selector.clone().with_profile(profile);

            // Get ranked tools for this profile using tag-based discovery
            let tools = selector.select_tools(&self.registry);

            // AUTO-ROUTING: Use route_query() to further filter based on search intent
            let filtered_tools = Self::apply_auto_routing(&tools, &prompt);

            // Get actual tool objects via provider
            let tool_infos: Vec<ToolInfo> = filtered_tools
                .iter()
                .filter_map(|name| self.provider.tool_info(name))
                .collect();

            if tool_infos.is_empty() {
                None
            } else {
                Some(formatter(&tool_infos))
            }
        } else {
            None
        }
    }

    /// Apply auto-routing to filter tools based on query intent
    #[cfg(feature = "vector-memory")]
    pub fn apply_auto_routing(tools: &[String], prompt: &str) -> Vec<String> {
        let strategy = route_query(prompt);

        match strategy {
            SearchStrategy::Lsp => tools
                .iter()
                .filter(|t| t.starts_with("lsp_") || *t == tn::READ)
                .cloned()
                .collect(),
            SearchStrategy::Grep => {
                if tools.contains(&tn::GREP.to_string()) {
                    vec![tn::GREP.to_string()]
                } else {
                    tools.to_vec()
                }
            }
            SearchStrategy::Glob => {
                if tools.contains(&tn::GLOB.to_string()) {
                    vec![tn::GLOB.to_string()]
                } else {
                    tools.to_vec()
                }
            }
            SearchStrategy::Semantic => {
                if tools.contains(&tn::SEMANTIC_SEARCH.to_string()) {
                    vec![tn::SEMANTIC_SEARCH.to_string()]
                } else if tools.contains(&tn::GREP.to_string()) {
                    vec![tn::GREP.to_string()]
                } else {
                    tools.to_vec()
                }
            }
            SearchStrategy::GrepThenSemantic => {
                let mut filtered = Vec::new();
                if tools.contains(&tn::GREP.to_string()) {
                    filtered.push(tn::GREP.to_string());
                }
                if tools.contains(&tn::SEMANTIC_SEARCH.to_string()) {
                    filtered.push(tn::SEMANTIC_SEARCH.to_string());
                }
                if filtered.is_empty() {
                    tools.to_vec()
                } else {
                    filtered
                }
            }
            #[allow(unreachable_patterns)]
            _ => tools.to_vec(),
        }
    }

    /// No-op stub when vector-memory is disabled
    #[cfg(not(feature = "vector-memory"))]
    pub fn apply_auto_routing(tools: &[String], _prompt: &str) -> Vec<String> {
        tools.to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "vector-memory")]
    #[test]
    fn test_auto_routing_semantic() {
        let tools = vec![
            "Grep".to_string(),
            "SemanticSearch".to_string(),
            "Read".to_string(),
        ];
        let prompt = "how do we validate JWT tokens?";
        let filtered = ToolSelectionState::apply_auto_routing(&tools, prompt);
        assert_eq!(filtered, vec!["SemanticSearch".to_string()]);
    }

    #[cfg(feature = "vector-memory")]
    #[test]
    fn test_auto_routing_grep() {
        let tools = vec![
            "Grep".to_string(),
            "SemanticSearch".to_string(),
            "Read".to_string(),
        ];
        let prompt = "\"Unauthorized\"";
        let filtered = ToolSelectionState::apply_auto_routing(&tools, prompt);
        assert_eq!(filtered, vec!["Grep".to_string()]);
    }

    #[test]
    fn test_auto_routing_lsp() {
        let tools = vec![
            "LspDefinition".to_string(),
            "LspHover".to_string(),
            "Grep".to_string(),
        ];
        let prompt = "`validate_jwt`";
        let filtered = ToolSelectionState::apply_auto_routing(&tools, prompt);
        #[cfg(feature = "vector-memory")]
        {
            assert!(filtered.contains(&"LspDefinition".to_string()));
            assert!(filtered.contains(&"LspHover".to_string()));
        }
        #[cfg(not(feature = "vector-memory"))]
        {
            assert_eq!(filtered.len(), 3);
        }
    }

    #[cfg(feature = "vector-memory")]
    #[test]
    fn test_auto_routing_glob() {
        let tools = vec!["Glob".to_string(), "Grep".to_string(), "Read".to_string()];
        let prompt = "src/**/*.rs";
        let filtered = ToolSelectionState::apply_auto_routing(&tools, prompt);
        assert_eq!(filtered, vec!["Glob".to_string()]);
    }

    #[cfg(feature = "vector-memory")]
    #[test]
    fn test_auto_routing_grep_then_semantic() {
        let tools = vec!["Grep".to_string(), "SemanticSearch".to_string()];
        let prompt = "auth";
        let filtered = ToolSelectionState::apply_auto_routing(&tools, prompt);
        assert_eq!(filtered.len(), 2);
        assert!(filtered.contains(&"Grep".to_string()));
        assert!(filtered.contains(&"SemanticSearch".to_string()));
    }

    #[cfg(not(feature = "vector-memory"))]
    #[test]
    fn test_auto_routing_noop_without_feature() {
        let tools = vec!["Grep".to_string(), "Read".to_string()];
        let filtered = ToolSelectionState::apply_auto_routing(&tools, "any prompt");
        assert_eq!(filtered, tools);
    }
}

/// Provider-specific tool formatters
pub mod formatters {
    use crate::tool_annotations::anthropic_annotations_for_tool_info;
    use rustycode_tools_api::ToolInfo;

    /// Canonical stub description for a deferred tool.
    fn deferred_description(tool: &ToolInfo) -> String {
        let desc = tool
            .description
            .lines()
            .next()
            .filter(|s| !s.is_empty())
            .unwrap_or("(no description)");
        format!(
            "{} [DEFERRED: call ToolSearch with name=\"{}\" to load full schema]",
            desc, tool.name
        )
    }

    /// Canonical empty JSON Schema used for deferred tool stubs.
    pub fn deferred_stub_schema() -> serde_json::Value {
        serde_json::json!({"type": "object", "properties": {}})
    }

    /// Format tools for Anthropic API.
    /// Deferred tools emit stubs (empty schema with hint to use tool_search).
    pub fn format_for_anthropic(tools: &[ToolInfo]) -> Vec<serde_json::Value> {
        tools
            .iter()
            .map(|tool| {
                if tool.defer_loading == Some(true) {
                    serde_json::json!({
                        "name": tool.name,
                        "description": deferred_description(tool),
                        "input_schema": deferred_stub_schema()
                    })
                } else {
                    let mut tool_json = serde_json::json!({
                        "name": tool.name,
                        "description": tool.description,
                        "input_schema": tool.parameters_schema
                    });
                    if let Some(annotations) = anthropic_annotations_for_tool_info(
                        &tool.name,
                        matches!(tool.permission, rustycode_tools_api::ToolPermission::Read),
                    ) {
                        tool_json["annotations"] = annotations;
                    }
                    tool_json
                }
            })
            .collect()
    }

    /// Format tools for OpenAI function calling API.
    /// Deferred tools emit stubs (empty schema with hint to use tool_search).
    pub fn format_for_openai(tools: &[ToolInfo]) -> Vec<serde_json::Value> {
        tools
            .iter()
            .map(|tool| {
                if tool.defer_loading == Some(true) {
                    serde_json::json!({
                        "type": "function",
                        "function": {
                            "name": tool.name,
                            "description": deferred_description(tool),
                            "parameters": deferred_stub_schema()
                        }
                    })
                } else {
                    serde_json::json!({
                        "type": "function",
                        "function": {
                            "name": tool.name,
                            "description": tool.description,
                            "parameters": tool.parameters_schema
                        }
                    })
                }
            })
            .collect()
    }

    /// Format tools for Gemini function declaration API.
    /// Deferred tools emit stubs (empty schema with hint to use tool_search).
    pub fn format_for_gemini(tools: &[ToolInfo]) -> Vec<serde_json::Value> {
        tools
            .iter()
            .map(|tool| {
                if tool.defer_loading == Some(true) {
                    serde_json::json!({
                        "name": tool.name,
                        "description": deferred_description(tool),
                        "parameters": deferred_stub_schema()
                    })
                } else {
                    serde_json::json!({
                        "name": tool.name,
                        "description": tool.description,
                        "parameters": tool.parameters_schema
                    })
                }
            })
            .collect()
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use rustycode_tools_api::ToolPermission;

        fn eager_tool() -> ToolInfo {
            ToolInfo {
                name: "Bash".to_string(),
                description: "Run a shell command".to_string(),
                parameters_schema: serde_json::json!({"type": "object", "properties": {"command": {"type": "string"}}}),
                permission: ToolPermission::Execute,
                defer_loading: None,
                annotations: None,
                tags: vec![],
                max_result_size_chars: None,
                is_destructive_default: false,
            }
        }

        fn deferred_tool() -> ToolInfo {
            ToolInfo {
                name: "LspHover".to_string(),
                description: "Show hover info".to_string(),
                parameters_schema: serde_json::json!({"type": "object", "properties": {"file": {"type": "string"}}}),
                permission: ToolPermission::Read,
                defer_loading: Some(true),
                annotations: None,
                tags: vec![],
                max_result_size_chars: None,
                is_destructive_default: false,
            }
        }

        #[test]
        fn test_anthropic_eager_full_schema() {
            let tools = vec![eager_tool()];
            let result = format_for_anthropic(&tools);
            assert_eq!(result.len(), 1);
            assert_eq!(result[0]["name"], "Bash");
            assert_eq!(
                result[0]["input_schema"]["properties"]["command"]["type"],
                "string"
            );
        }

        #[test]
        fn test_anthropic_deferred_stub() {
            let tools = vec![deferred_tool()];
            let result = format_for_anthropic(&tools);
            assert_eq!(
                result[0]["input_schema"]["properties"],
                serde_json::json!({})
            );
            assert!(result[0]["description"]
                .as_str()
                .unwrap()
                .contains("DEFERRED"));
            assert!(result[0]["description"]
                .as_str()
                .unwrap()
                .contains("ToolSearch"));
        }

        #[test]
        fn test_openai_deferred_stub() {
            let tools = vec![deferred_tool()];
            let result = format_for_openai(&tools);
            assert_eq!(result[0]["type"], "function");
            assert_eq!(
                result[0]["function"]["parameters"]["properties"],
                serde_json::json!({})
            );
            assert!(result[0]["function"]["description"]
                .as_str()
                .unwrap()
                .contains("DEFERRED"));
        }

        #[test]
        fn test_openai_eager_full_schema() {
            let tools = vec![eager_tool()];
            let result = format_for_openai(&tools);
            assert_eq!(
                result[0]["function"]["parameters"]["properties"]["command"]["type"],
                "string"
            );
        }

        #[test]
        fn test_gemini_deferred_stub() {
            let tools = vec![deferred_tool()];
            let result = format_for_gemini(&tools);
            assert_eq!(result[0]["parameters"]["properties"], serde_json::json!({}));
            assert!(result[0]["description"]
                .as_str()
                .unwrap()
                .contains("DEFERRED"));
        }

        #[test]
        fn test_gemini_eager_full_schema() {
            let tools = vec![eager_tool()];
            let result = format_for_gemini(&tools);
            assert_eq!(
                result[0]["parameters"]["properties"]["command"]["type"],
                "string"
            );
        }

        #[test]
        fn test_mixed_deferred_and_eager() {
            let tools = vec![eager_tool(), deferred_tool()];
            let result = format_for_anthropic(&tools);
            assert_eq!(result.len(), 2);
            // Eager keeps full schema
            assert!(result[0]["input_schema"]["properties"]
                .get("command")
                .is_some());
            // Deferred has stub
            assert_eq!(
                result[1]["input_schema"]["properties"],
                serde_json::json!({})
            );
        }
    }
}
