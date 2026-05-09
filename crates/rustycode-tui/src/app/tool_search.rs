//! Tool Search Tool implementation for dynamic tool discovery.

use regex::Regex;
use rustycode_prompt::ModelProvider;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Search algorithm variants for tool discovery
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchAlgorithm {
    /// BM25 ranking algorithm - good for keyword-based search
    Bm25,
    /// Regex pattern matching - exact pattern search
    Regex,
}

/// A tool reference that can be expanded into a full tool definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolReference {
    /// The name of the tool
    pub name: String,
    /// Brief description of the tool
    pub description: String,
    /// Whether the tool definition is deferred (loaded on demand)
    pub defer_loading: bool,
}

/// Query for searching tools
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSearchQuery {
    /// The search query string
    pub query: String,
    /// The search algorithm to use
    #[serde(default = "default_algorithm")]
    pub algorithm: SearchAlgorithm,
    /// Maximum number of results to return
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_algorithm() -> SearchAlgorithm {
    SearchAlgorithm::Bm25
}

fn default_limit() -> usize {
    10
}

/// Result of a tool search
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSearchResult {
    /// The tools that matched the search
    pub tools: Vec<ToolMatch>,
    /// Total number of matching tools (before limit)
    pub total: usize,
}

/// A single tool match with relevance score
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolMatch {
    /// The tool reference
    #[serde(flatten)]
    pub reference: ToolReference,
    /// Relevance score (1.0 to 1.0 for BM25, 1.0 for regex matches)
    pub score: f32,
    /// Which fields matched the query
    pub matched_fields: Vec<String>,
}

/// Execution mode for tool search
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolSearchExecution {
    /// Server-side (hosted) execution - OpenAI searches and loads tools
    Server,
    /// Client-side execution - application performs search and returns tools
    Client,
}

/// Status of a tool search operation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolSearchStatus {
    InProgress,
    Completed,
    Failed,
}

/// Tool search call - emitted by the model when it needs to search for tools
/// Corresponds to OpenAI's tool_search_call output type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSearchCall {
    /// Type identifier
    #[serde(rename = "type")]
    pub type_name: String,
    /// Execution mode
    pub execution: ToolSearchExecution,
    /// Call ID (null for server execution)
    pub call_id: Option<String>,
    /// Status of the search
    pub status: ToolSearchStatus,
    /// Search arguments
    pub arguments: ToolSearchArguments,
}

impl ToolSearchCall {
    pub fn new_server(paths: Vec<String>) -> Self {
        Self {
            type_name: "tool_search_call".to_string(),
            execution: ToolSearchExecution::Server,
            call_id: None,
            status: ToolSearchStatus::Completed,
            arguments: ToolSearchArguments {
                paths: Some(paths),
                query: None,
                goal: None,
            },
        }
    }

    pub fn new_client(call_id: String, goal: String) -> Self {
        Self {
            type_name: "tool_search_call".to_string(),
            execution: ToolSearchExecution::Client,
            call_id: Some(call_id),
            status: ToolSearchStatus::Completed,
            arguments: ToolSearchArguments {
                paths: None,
                query: None,
                goal: Some(goal),
            },
        }
    }
}

/// Arguments for tool search
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSearchArguments {
    /// Paths to namespaces/tools (for server execution)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub paths: Option<Vec<String>>,
    /// Search query (for client execution)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
    /// Goal description (for client execution)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub goal: Option<String>,
}

/// Tool search output - returned by application with loaded tools
/// Corresponds to OpenAI's tool_search_output type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSearchOutput {
    /// Type identifier
    #[serde(rename = "type")]
    pub type_name: String,
    /// Execution mode
    pub execution: ToolSearchExecution,
    /// Call ID (must match the call_id from tool_search_call)
    pub call_id: Option<String>,
    /// Status of the output
    pub status: ToolSearchStatus,
    /// The loaded tools that become callable
    pub tools: Vec<serde_json::Value>,
}

impl ToolSearchOutput {
    pub fn new_server(call_id: Option<String>, tools: Vec<serde_json::Value>) -> Self {
        Self {
            type_name: "tool_search_output".to_string(),
            execution: ToolSearchExecution::Server,
            call_id,
            status: ToolSearchStatus::Completed,
            tools,
        }
    }

    pub fn new_client(call_id: String, tools: Vec<serde_json::Value>) -> Self {
        Self {
            type_name: "tool_search_output".to_string(),
            execution: ToolSearchExecution::Client,
            call_id: Some(call_id),
            status: ToolSearchStatus::Completed,
            tools,
        }
    }
}

/// Tool search engine supporting multiple search algorithms
pub struct ToolSearch {
    tools: Vec<SearchableTool>,
    avg_doc_length: f32,
    k1: f32,
    b: f32,
}

/// Internal representation of a tool for searching
struct SearchableTool {
    name: String,
    description: String,
    parameter_names: Vec<String>,
    parameter_descriptions: Vec<String>,
    /// Precomputed term frequencies for BM25
    term_freqs: HashMap<String, usize>,
    doc_length: usize,
}

impl ToolSearch {
    pub fn new(tools: Vec<rustycode_tools::ToolInfo>) -> Self {
        let searchable: Vec<SearchableTool> = tools
            .into_iter()
            .map(|t| {
                let (param_names, param_descs) = extract_parameter_info(&t.parameters_schema);
                let all_text = format!(
                    "{} {} {} {}",
                    t.name,
                    t.description,
                    param_names.join(" "),
                    param_descs.join(" ")
                );
                let term_freqs = compute_term_frequencies(&all_text);
                let doc_length = term_freqs.values().sum();

                SearchableTool {
                    name: t.name.to_string(),
                    description: t.description.to_string(),
                    parameter_names: param_names,
                    parameter_descriptions: param_descs,
                    term_freqs,
                    doc_length,
                }
            })
            .collect();

        let avg_doc_length = if searchable.is_empty() {
            1.0
        } else {
            searchable.iter().map(|t| t.doc_length as f32).sum::<f32>() / searchable.len() as f32
        };

        Self {
            tools: searchable,
            avg_doc_length,
            k1: 1.2,
            b: 0.75,
        }
    }

    /// Search for tools using the specified algorithm
    pub fn search(&self, query: &ToolSearchQuery) -> ToolSearchResult {
        match query.algorithm {
            SearchAlgorithm::Bm25 => self.search_bm25(&query.query, query.limit),
            SearchAlgorithm::Regex => self.search_regex(&query.query, query.limit),
        }
    }

    /// Search using BM25 ranking algorithm
    fn search_bm25(&self, query: &str, limit: usize) -> ToolSearchResult {
        let query_terms = tokenize(query);
        let n = self.tools.len() as f32;

        let mut scored: Vec<(usize, f32, Vec<String>)> = self
            .tools
            .iter()
            .enumerate()
            .filter_map(|(idx, tool)| {
                let mut total_score = 0.0_f32;
                let mut matched_fields = Vec::new();

                for term in &query_terms {
                    let tf = tool.term_freqs.get(term).copied().unwrap_or(0) as f32;
                    if tf == 0.0 {
                        continue;
                    }

                    let df = self
                        .tools
                        .iter()
                        .filter(|t| t.term_freqs.contains_key(term))
                        .count() as f32;
                    let idf = ((n - df + 0.5) / (df + 0.5) + 1.0).ln();

                    let dl = tool.doc_length as f32;
                    let avgdl = self.avg_doc_length;

                    let bm25 = idf * (tf * (self.k1 + 1.0))
                        / (tf + self.k1 * (1.0 - self.b + self.b * (dl / avgdl)));

                    total_score += bm25;

                    if tool.name.to_lowercase().contains(term)
                        && !matched_fields.contains(&"name".to_string())
                    {
                        matched_fields.push("name".to_string());
                    }
                    if tool.description.to_lowercase().contains(term)
                        && !matched_fields.contains(&"description".to_string())
                    {
                        matched_fields.push("description".to_string());
                    }
                    if tool
                        .parameter_names
                        .iter()
                        .any(|p| p.to_lowercase().contains(term))
                        && !matched_fields.contains(&"parameters".to_string())
                    {
                        matched_fields.push("parameters".to_string());
                    }
                }

                if total_score > 0.0 {
                    Some((idx, total_score, matched_fields))
                } else {
                    None
                }
            })
            .collect();

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        let total = scored.len();
        let tools: Vec<ToolMatch> = scored
            .into_iter()
            .take(limit)
            .map(|(idx, score, matched_fields)| {
                let tool = &self.tools[idx];
                let max_score = 10.0_f32;
                let normalized_score = (score / max_score).min(1.0);

                ToolMatch {
                    reference: ToolReference {
                        name: tool.name.clone(),
                        description: tool.description.clone(),
                        defer_loading: true,
                    },
                    score: normalized_score,
                    matched_fields,
                }
            })
            .collect();

        ToolSearchResult { tools, total }
    }

    /// Search using regex pattern matching
    fn search_regex(&self, pattern: &str, limit: usize) -> ToolSearchResult {
        let re = match Regex::new(&format!("(?i){}", pattern)) {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("Failed to create regex for pattern '{}': {:?}", pattern, e);
                // If regex fails, return empty result
                return ToolSearchResult {
                    tools: vec![],
                    total: 0,
                };
            }
        };

        let matches: Vec<(usize, Vec<String>)> = self
            .tools
            .iter()
            .enumerate()
            .filter_map(|(idx, tool)| {
                let mut matched_fields = Vec::new();

                if re.is_match(&tool.name) {
                    matched_fields.push("name".to_string());
                }
                if re.is_match(&tool.description) {
                    matched_fields.push("description".to_string());
                }
                for param in &tool.parameter_names {
                    if re.is_match(param) {
                        if !matched_fields.contains(&"parameters".to_string()) {
                            matched_fields.push("parameters".to_string());
                        }
                        break;
                    }
                }
                for desc in &tool.parameter_descriptions {
                    if re.is_match(desc) {
                        if !matched_fields.contains(&"parameter_descriptions".to_string()) {
                            matched_fields.push("parameter_descriptions".to_string());
                        }
                        break;
                    }
                }

                if matched_fields.is_empty() {
                    None
                } else {
                    Some((idx, matched_fields))
                }
            })
            .collect();

        let total = matches.len();
        let tools: Vec<ToolMatch> = matches
            .into_iter()
            .take(limit)
            .map(|(idx, matched_fields)| {
                let tool = &self.tools[idx];
                ToolMatch {
                    reference: ToolReference {
                        name: tool.name.clone(),
                        description: tool.description.clone(),
                        defer_loading: true,
                    },
                    score: 1.0,
                    matched_fields,
                }
            })
            .collect();

        ToolSearchResult { tools, total }
    }

    /// Get a tool reference by name
    pub fn tool(&self, name: &str) -> Option<ToolReference> {
        self.tools
            .iter()
            .find(|t| t.name == name)
            .map(|t| ToolReference {
                name: t.name.clone(),
                description: t.description.clone(),
                defer_loading: true,
            })
    }

    /// List all tool references
    pub fn list_all(&self) -> Vec<ToolReference> {
        self.tools
            .iter()
            .map(|t| ToolReference {
                name: t.name.clone(),
                description: t.description.clone(),
                defer_loading: true,
            })
            .collect()
    }

    /// Generate the tool_search_tool definition for Anthropic
    pub fn anthropic_tool_definition() -> serde_json::Value {
        serde_json::json!({
            "type": "tool_search_tool_bm25_20251119",
            "name": "tool_search"
        })
    }

    /// Generate the tool_search_tool definition for OpenAI
    pub fn openai_tool_definition() -> serde_json::Value {
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "tool_search",
                "description": "Search for available tools by name, description, or functionality. Use this to discover tools when you're not sure which tool to use.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "The search query - can be a tool name, keyword, or description of what you want to do"
                        },
                        "algorithm": {
                            "type": "string",
                            "enum": ["bm25", "regex"],
                            "description": "Search algorithm: 'bm25' for keyword ranking, 'regex' for pattern matching",
                            "default": "bm25"
                        },
                        "limit": {
                            "type": "integer",
                            "description": "Maximum number of results to return",
                            "default": 10,
                            "minimum": 1,
                            "maximum": 50
                        }
                    },
                    "required": ["query"]
                }
            }
        })
    }

    /// Generate provider-specific tool definition
    pub fn tool_definition_for_provider(provider: ModelProvider) -> serde_json::Value {
        if provider.is_openai() || provider.is_google() {
            Self::openai_tool_definition()
        } else {
            // Anthropic, Generic, and all other providers use the Anthropic-style definition
            Self::anthropic_tool_definition()
        }
    }
}

/// Extract parameter names and descriptions from JSON schema
fn extract_parameter_info(schema: &serde_json::Value) -> (Vec<String>, Vec<String>) {
    let mut names = Vec::new();
    let mut descriptions = Vec::new();

    if let Some(obj) = schema.as_object() {
        if let Some(props) = obj.get("properties") {
            if let Some(props_obj) = props.as_object() {
                for (name, prop) in props_obj {
                    names.push(name.clone());
                    if let Some(desc) = prop.get("description") {
                        if let Some(desc_str) = desc.as_str() {
                            descriptions.push(desc_str.to_string());
                        }
                    }
                }
            }
        }
    }

    (names, descriptions)
}

/// Tokenize text into lowercase terms
fn tokenize(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split_whitespace()
        .map(|s| {
            s.chars()
                .filter(|c| c.is_alphanumeric())
                .collect::<String>()
        })
        .filter(|s| !s.is_empty() && s.len() > 1)
        .collect()
}

/// Compute term frequencies for a document
fn compute_term_frequencies(text: &str) -> HashMap<String, usize> {
    let mut freq = HashMap::new();
    for term in tokenize(text) {
        *freq.entry(term).or_insert(0) += 1;
    }
    freq
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_tools() -> Vec<rustycode_tools::ToolInfo> {
        vec![
            rustycode_tools::ToolInfo {
                name: "Read".to_string(),
                description: "Read the contents of a file from the filesystem".to_string(),
                parameters_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "The path to the file to read"
                        }
                    },
                    "required": ["path"]
                }),
                permission: rustycode_tools::ToolPermission::Read,
                defer_loading: None,
                annotations: None,
                tags: vec![],
                is_destructive_default: false,
                max_result_size_chars: None,
            },
            rustycode_tools::ToolInfo {
                name: "Write".to_string(),
                description: "Write content to a file on the filesystem".to_string(),
                parameters_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "The path to the file to write"
                        },
                        "content": {
                            "type": "string",
                            "description": "The content to write to the file"
                        }
                    },
                    "required": ["path", "content"]
                }),
                permission: rustycode_tools::ToolPermission::Write,
                defer_loading: None,
                annotations: None,
                tags: vec![],
                is_destructive_default: true,
                max_result_size_chars: None,
            },
            rustycode_tools::ToolInfo {
                name: "Bash".to_string(),
                description: "Execute a bash command in the terminal".to_string(),
                parameters_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "command": {
                            "type": "string",
                            "description": "The command to execute"
                        }
                    },
                    "required": ["command"]
                }),
                permission: rustycode_tools::ToolPermission::Execute,
                defer_loading: None,
                annotations: None,
                tags: vec![],
                is_destructive_default: true,
                max_result_size_chars: None,
            },
        ]
    }

    #[test]
    fn test_search_bm25() {
        let tools = make_test_tools();
        let search = ToolSearch::new(tools);

        let query = ToolSearchQuery {
            query: "read file".to_string(),
            algorithm: SearchAlgorithm::Bm25,
            limit: 10,
        };

        let result = search.search(&query);
        assert!(!result.tools.is_empty());
        assert!(result.tools[0].reference.name == "Read");
    }

    #[test]
    fn test_search_regex() {
        let tools = make_test_tools();
        let search = ToolSearch::new(tools);

        let query = ToolSearchQuery {
            query: "Bash".to_string(),
            algorithm: SearchAlgorithm::Regex,
            limit: 10,
        };

        let result = search.search(&query);
        assert!(!result.tools.is_empty());
        assert!(result.tools[0].reference.name == "Bash");
    }

    #[test]
    fn test_get_tool() {
        let tools = make_test_tools();
        let search = ToolSearch::new(tools);

        let tool = search.tool("Read");
        assert!(tool.is_some());
        assert_eq!(tool.unwrap().name, "Read");
    }

    #[test]
    fn test_list_all() {
        let tools = make_test_tools();
        let search = ToolSearch::new(tools);

        let all = search.list_all();
        assert_eq!(all.len(), 3);
    }

    #[test]
    fn test_tool_definition() {
        let def = ToolSearch::anthropic_tool_definition();
        assert_eq!(def["name"], "tool_search");
        assert_eq!(def["type"], "tool_search_tool_bm25_20251119");
    }

    #[test]
    fn test_search_regex_uses_real_regex() {
        let tools = make_test_tools();
        let search = ToolSearch::new(tools);

        let query = ToolSearchQuery {
            query: "read_.*".to_string(),
            algorithm: SearchAlgorithm::Regex,
            limit: 10,
        };

        let result = search.search(&query);
        assert_eq!(result.tools[0].reference.name, "Read");
    }
}
