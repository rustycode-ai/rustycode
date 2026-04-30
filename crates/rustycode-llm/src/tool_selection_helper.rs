//! Shared tool selection utilities for LLM providers
//!
//! This module provides common functionality for intelligent tool selection
//! across all LLM providers (Anthropic, OpenAI, Gemini, etc.)

use crate::provider::{ChatMessage, MessageRole};
#[cfg(feature = "vector-memory")]
use rustycode_tools_api::{route_query, SearchStrategy};
use rustycode_tools_api::{ToolInfo, ToolMetadataProvider};
use rustycode_tools_api::{ToolProfile, ToolSelector};
use std::sync::Arc;

/// Shared tool selection state - providers can embed this
pub struct ToolSelectionState {
    pub provider: Arc<dyn ToolMetadataProvider>,
    pub selector: ToolSelector,
}

impl ToolSelectionState {
    pub fn new(provider: Arc<dyn ToolMetadataProvider>) -> Self {
        Self {
            provider,
            selector: ToolSelector::new(),
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

            // Get ranked tools for this profile
            let tools = selector.select_tools();

            // AUTO-ROUTING: Use route_query() to further filter based on search intent
            let filtered_tools = Self::apply_auto_routing(&tools, &prompt);

            // Get actual tool objects via provider
            let tool_infos: Vec<ToolInfo> = filtered_tools
                .iter()
                .filter_map(|name| self.provider.get_tool_info(name))
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
                .filter(|t| t.starts_with("lsp_") || *t == "read_file")
                .cloned()
                .collect(),
            SearchStrategy::Grep => {
                if tools.contains(&"grep".to_string()) {
                    vec!["grep".to_string()]
                } else {
                    tools.to_vec()
                }
            }
            SearchStrategy::Glob => {
                if tools.contains(&"glob".to_string()) {
                    vec!["glob".to_string()]
                } else {
                    tools.to_vec()
                }
            }
            SearchStrategy::Semantic => {
                if tools.contains(&"semantic_search".to_string()) {
                    vec!["semantic_search".to_string()]
                } else if tools.contains(&"grep".to_string()) {
                    vec!["grep".to_string()]
                } else {
                    tools.to_vec()
                }
            }
            SearchStrategy::GrepThenSemantic => {
                let mut filtered = Vec::new();
                if tools.contains(&"grep".to_string()) {
                    filtered.push("grep".to_string());
                }
                if tools.contains(&"semantic_search".to_string()) {
                    filtered.push("semantic_search".to_string());
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
            "grep".to_string(),
            "semantic_search".to_string(),
            "read_file".to_string(),
        ];
        let prompt = "how do we validate JWT tokens?";
        let filtered = ToolSelectionState::apply_auto_routing(&tools, prompt);
        assert_eq!(filtered, vec!["semantic_search".to_string()]);
    }

    #[cfg(feature = "vector-memory")]
    #[test]
    fn test_auto_routing_grep() {
        let tools = vec![
            "grep".to_string(),
            "semantic_search".to_string(),
            "read_file".to_string(),
        ];
        let prompt = "\"Unauthorized\"";
        let filtered = ToolSelectionState::apply_auto_routing(&tools, prompt);
        assert_eq!(filtered, vec!["grep".to_string()]);
    }

    #[test]
    fn test_auto_routing_lsp() {
        let tools = vec![
            "lsp_definition".to_string(),
            "lsp_hover".to_string(),
            "grep".to_string(),
        ];
        let prompt = "`validate_jwt`";
        let filtered = ToolSelectionState::apply_auto_routing(&tools, prompt);
        #[cfg(feature = "vector-memory")]
        {
            assert!(filtered.contains(&"lsp_definition".to_string()));
            assert!(filtered.contains(&"lsp_hover".to_string()));
        }
        #[cfg(not(feature = "vector-memory"))]
        {
            assert_eq!(filtered.len(), 3);
        }
    }

    #[cfg(feature = "vector-memory")]
    #[test]
    fn test_auto_routing_glob() {
        let tools = vec![
            "glob".to_string(),
            "grep".to_string(),
            "read_file".to_string(),
        ];
        let prompt = "src/**/*.rs";
        let filtered = ToolSelectionState::apply_auto_routing(&tools, prompt);
        assert_eq!(filtered, vec!["glob".to_string()]);
    }

    #[cfg(feature = "vector-memory")]
    #[test]
    fn test_auto_routing_grep_then_semantic() {
        let tools = vec!["grep".to_string(), "semantic_search".to_string()];
        let prompt = "auth";
        let filtered = ToolSelectionState::apply_auto_routing(&tools, prompt);
        assert_eq!(filtered.len(), 2);
        assert!(filtered.contains(&"grep".to_string()));
        assert!(filtered.contains(&"semantic_search".to_string()));
    }

    #[cfg(not(feature = "vector-memory"))]
    #[test]
    fn test_auto_routing_noop_without_feature() {
        let tools = vec!["grep".to_string(), "read_file".to_string()];
        let filtered = ToolSelectionState::apply_auto_routing(&tools, "any prompt");
        assert_eq!(filtered, tools);
    }
}

/// Provider-specific tool formatters
pub mod formatters {
    use crate::tool_annotations::anthropic_annotations_for_tool_info;
    use rustycode_tools_api::ToolInfo;

    /// Format tools for Anthropic API
    pub fn format_for_anthropic(tools: &[ToolInfo]) -> Vec<serde_json::Value> {
        tools
            .iter()
            .map(|tool| {
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
            })
            .collect()
    }

    /// Format tools for OpenAI function calling API
    pub fn format_for_openai(tools: &[ToolInfo]) -> Vec<serde_json::Value> {
        tools
            .iter()
            .map(|tool| {
                serde_json::json!({
                    "type": "function",
                    "function": {
                        "name": tool.name,
                        "description": tool.description,
                        "parameters": tool.parameters_schema
                    }
                })
            })
            .collect()
    }

    /// Format tools for Gemini function declaration API
    pub fn format_for_gemini(tools: &[ToolInfo]) -> Vec<serde_json::Value> {
        tools
            .iter()
            .map(|tool| {
                serde_json::json!({
                    "name": tool.name,
                    "description": tool.description,
                    "parameters": tool.parameters_schema
                })
            })
            .collect()
    }
}
