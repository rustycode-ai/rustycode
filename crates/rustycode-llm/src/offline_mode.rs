//! Offline Mode Support
//!
//! Enables RustyCode to operate in degraded mode when API services are unavailable,
//! providing local-only functionality without LLM API calls.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::debug;

// ─── Offline Mode ──────────────────────────────────────────────────────────

/// Configuration for offline mode
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OfflineModeConfig {
    /// Whether offline mode is enabled
    pub enabled: bool,
    /// Which services should have offline fallback
    pub fallback_services: Vec<String>,
    /// Local search enabled
    pub local_search_enabled: bool,
    /// Static tool descriptions enabled
    pub static_tools_enabled: bool,
    /// Configuration editing enabled
    pub config_edit_enabled: bool,
}

impl Default for OfflineModeConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            fallback_services: vec![
                "code_analysis".to_string(),
                "message_search".to_string(),
                "tool_descriptions".to_string(),
                "configuration".to_string(),
            ],
            local_search_enabled: true,
            static_tools_enabled: true,
            config_edit_enabled: true,
        }
    }
}

impl OfflineModeConfig {
    /// Create with all offline services enabled
    pub fn all_services() -> Self {
        Self {
            enabled: true,
            fallback_services: vec![
                "code_analysis".to_string(),
                "message_search".to_string(),
                "tool_descriptions".to_string(),
                "configuration".to_string(),
            ],
            local_search_enabled: true,
            static_tools_enabled: true,
            config_edit_enabled: true,
        }
    }

    /// Add a service to fallback
    pub fn with_service(mut self, service: impl Into<String>) -> Self {
        self.fallback_services.push(service.into());
        self
    }

    /// Check if a service has offline fallback
    pub fn has_service(&self, service: &str) -> bool {
        self.fallback_services.iter().any(|s| s == service)
    }
}

/// Offline mode manager
pub struct OfflineMode {
    config: OfflineModeConfig,
    available_services: HashMap<String, bool>,
}

impl OfflineMode {
    /// Create a new offline mode manager
    pub fn new(config: OfflineModeConfig) -> Self {
        let available_services = config
            .fallback_services
            .iter()
            .map(|s| (s.clone(), true))
            .collect();

        Self {
            config,
            available_services,
        }
    }

    /// Check if offline mode is active
    pub fn is_offline(&self) -> bool {
        self.config.enabled
    }

    /// Enable offline mode
    pub fn enable(&mut self) {
        debug!("Enabling offline mode");
        self.config.enabled = true;
    }

    /// Disable offline mode
    pub fn disable(&mut self) {
        debug!("Disabling offline mode");
        self.config.enabled = false;
    }

    /// Check if a service is available in offline mode
    pub fn service_available(&self, service: &str) -> bool {
        if !self.is_offline() {
            return true;
        }
        self.available_services
            .get(service)
            .copied()
            .unwrap_or(false)
    }

    /// Mark a service as unavailable
    pub fn mark_unavailable(&mut self, service: &str) {
        debug!(
            "Marking service as unavailable in offline mode: {}",
            service
        );
        self.available_services.insert(service.to_string(), false);
    }

    /// Mark a service as available
    pub fn mark_available(&mut self, service: &str) {
        debug!("Marking service as available in offline mode: {}", service);
        self.available_services.insert(service.to_string(), true);
    }

    /// Get configuration
    pub fn config(&self) -> &OfflineModeConfig {
        &self.config
    }

    /// Get mutable configuration
    pub fn config_mut(&mut self) -> &mut OfflineModeConfig {
        &mut self.config
    }
}

impl Default for OfflineMode {
    fn default() -> Self {
        Self::new(OfflineModeConfig::default())
    }
}

// ─── Offline Fallback Implementations ──────────────────────────────────────

/// Local code analysis without API calls
pub struct LocalCodeAnalyzer;

impl LocalCodeAnalyzer {
    /// Analyze code structure locally
    pub fn analyze_structure(code: &str) -> LocalCodeAnalysisResult {
        debug!("Performing local code structure analysis");

        let lines = code.lines().count();
        let functions = code.matches("fn ").count();
        let structs = code.matches("struct ").count();
        let modules = code.matches("mod ").count();

        let has_errors = code.contains("TODO") || code.contains("FIXME");
        let has_documentation = code.contains("///") || code.contains("//!");

        LocalCodeAnalysisResult {
            line_count: lines,
            function_count: functions,
            struct_count: structs,
            module_count: modules,
            has_error_markers: has_errors,
            has_documentation,
        }
    }

    /// Perform syntax validation (basic)
    pub fn validate_syntax(code: &str) -> SyntaxValidationResult {
        debug!("Performing local syntax validation");

        let mut issues = Vec::new();

        // Check for common syntax issues
        if code.chars().filter(|c| *c == '{').count() != code.chars().filter(|c| *c == '}').count()
        {
            issues.push("Unmatched braces detected".to_string());
        }

        if code.chars().filter(|c| *c == '(').count() != code.chars().filter(|c| *c == ')').count()
        {
            issues.push("Unmatched parentheses detected".to_string());
        }

        if code.chars().filter(|c| *c == '[').count() != code.chars().filter(|c| *c == ']').count()
        {
            issues.push("Unmatched brackets detected".to_string());
        }

        SyntaxValidationResult { issues }
    }

    /// Extract metadata about code
    pub fn extract_metadata(code: &str, file_name: &str) -> CodeMetadata {
        let mut exports = Vec::new();
        for line in code.lines() {
            if line.contains("pub fn") || line.contains("pub struct") || line.contains("pub enum") {
                if let Some(name) = line.split_whitespace().nth(2) {
                    exports.push(name.to_string());
                }
            }
        }

        CodeMetadata {
            file_name: file_name.to_string(),
            language: infer_language(file_name),
            exports,
            size_bytes: code.len(),
            last_modified: chrono::Local::now().to_rfc3339(),
        }
    }
}

/// Local code analysis result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalCodeAnalysisResult {
    pub line_count: usize,
    pub function_count: usize,
    pub struct_count: usize,
    pub module_count: usize,
    pub has_error_markers: bool,
    pub has_documentation: bool,
}

/// Syntax validation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyntaxValidationResult {
    pub issues: Vec<String>,
}

/// Code metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeMetadata {
    pub file_name: String,
    pub language: String,
    pub exports: Vec<String>,
    pub size_bytes: usize,
    pub last_modified: String,
}

/// Infer programming language from file name
fn infer_language(file_name: &str) -> String {
    if file_name.ends_with(".rs") {
        "Rust".to_string()
    } else if file_name.ends_with(".py") {
        "Python".to_string()
    } else if file_name.ends_with(".js") || file_name.ends_with(".ts") {
        "JavaScript/TypeScript".to_string()
    } else if file_name.ends_with(".go") {
        "Go".to_string()
    } else if file_name.ends_with(".java") {
        "Java".to_string()
    } else if file_name.ends_with(".cpp") || file_name.ends_with(".c") {
        "C/C++".to_string()
    } else if file_name.ends_with(".md") {
        "Markdown".to_string()
    } else {
        "Unknown".to_string()
    }
}

// ─── Local Search ──────────────────────────────────────────────────────────

/// Local text-based search without semantic analysis
pub struct LocalSearchEngine;

impl LocalSearchEngine {
    /// Search text without semantic analysis
    pub fn search(text: &str, query: &str, max_results: usize) -> Vec<SearchResult> {
        debug!("Performing local text search for: {}", query);

        let query_lower = query.to_lowercase();
        let mut results = Vec::new();

        for (line_num, line) in text.lines().enumerate() {
            if line.to_lowercase().contains(&query_lower) {
                results.push(SearchResult {
                    line_number: line_num + 1,
                    content: line.trim().to_string(),
                    relevance_score: calculate_relevance(&line.to_lowercase(), &query_lower),
                });
            }

            if results.len() >= max_results {
                break;
            }
        }

        // Sort by relevance
        results.sort_by(|a, b| b.relevance_score.partial_cmp(&a.relevance_score).unwrap());
        results
    }

    /// Get search statistics
    pub fn search_stats(text: &str, query: &str) -> SearchStats {
        let query_lower = query.to_lowercase();
        let matches = text
            .lines()
            .filter(|l| l.to_lowercase().contains(&query_lower))
            .count();

        SearchStats {
            total_lines: text.lines().count(),
            matching_lines: matches,
            match_percentage: if text.lines().count() > 0 {
                (matches as f64 / text.lines().count() as f64) * 100.0
            } else {
                0.0
            },
        }
    }
}

/// Search result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub line_number: usize,
    pub content: String,
    pub relevance_score: f64,
}

/// Search statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchStats {
    pub total_lines: usize,
    pub matching_lines: usize,
    pub match_percentage: f64,
}

fn calculate_relevance(text: &str, query: &str) -> f64 {
    let query_words: Vec<&str> = query.split_whitespace().collect();
    let text_words: Vec<&str> = text.split_whitespace().collect();

    let mut score = 0.0;
    for qword in &query_words {
        if text_words.iter().any(|w| w == qword) {
            score += 1.0;
        }
    }

    // Exact match gets highest score
    if text == query {
        score += 100.0;
    } else if text.starts_with(query) {
        score += 50.0;
    }

    score
}

// ─── Static Tool Descriptions ──────────────────────────────────────────────

/// Static tool descriptions for offline mode
pub struct StaticToolDescriptions;

impl StaticToolDescriptions {
    /// Get all available static tool descriptions
    pub fn all() -> HashMap<String, ToolDescription> {
        let mut tools = HashMap::new();

        tools.insert(
            "code_analysis".to_string(),
            ToolDescription {
                name: "Code Analysis".to_string(),
                description: "Analyze code structure, functions, and modules locally".to_string(),
                capabilities: vec![
                    "Count functions and structs".to_string(),
                    "Detect error markers".to_string(),
                    "Check documentation".to_string(),
                ],
                offline: true,
            },
        );

        tools.insert(
            "text_search".to_string(),
            ToolDescription {
                name: "Text Search".to_string(),
                description: "Search text content locally without semantic analysis".to_string(),
                capabilities: vec![
                    "Case-insensitive search".to_string(),
                    "Line-based matching".to_string(),
                    "Relevance scoring".to_string(),
                ],
                offline: true,
            },
        );

        tools.insert(
            "file_metadata".to_string(),
            ToolDescription {
                name: "File Metadata".to_string(),
                description: "Extract file metadata without API calls".to_string(),
                capabilities: vec![
                    "File size".to_string(),
                    "Language detection".to_string(),
                    "Line count".to_string(),
                    "Modification time".to_string(),
                ],
                offline: true,
            },
        );

        tools.insert(
            "syntax_check".to_string(),
            ToolDescription {
                name: "Syntax Checker".to_string(),
                description: "Basic syntax validation for common issues".to_string(),
                capabilities: vec![
                    "Bracket matching".to_string(),
                    "Parenthesis matching".to_string(),
                    "Basic structure validation".to_string(),
                ],
                offline: true,
            },
        );

        tools
    }

    /// Get a specific tool description
    pub fn get(tool_name: &str) -> Option<ToolDescription> {
        Self::all().remove(tool_name)
    }

    /// Check if a tool is available offline
    pub fn is_offline_available(tool_name: &str) -> bool {
        Self::get(tool_name).is_some_and(|t| t.offline)
    }
}

/// Tool description
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDescription {
    pub name: String,
    pub description: String,
    pub capabilities: Vec<String>,
    pub offline: bool,
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // Offline Mode Config Tests

    #[test]
    fn test_config_default() {
        let config = OfflineModeConfig::default();
        assert!(!config.enabled);
        assert!(config.local_search_enabled);
        assert!(config.static_tools_enabled);
        assert!(config.config_edit_enabled);
    }

    #[test]
    fn test_config_all_services() {
        let config = OfflineModeConfig::all_services();
        assert!(config.enabled);
        assert_eq!(config.fallback_services.len(), 4);
    }

    #[test]
    fn test_config_with_service() {
        let config = OfflineModeConfig::default().with_service("custom_service");
        assert!(config.has_service("custom_service"));
    }

    #[test]
    fn test_config_has_service() {
        let config = OfflineModeConfig::all_services();
        assert!(config.has_service("code_analysis"));
        assert!(config.has_service("message_search"));
        assert!(!config.has_service("nonexistent"));
    }

    // Offline Mode Tests

    #[test]
    fn test_offline_mode_new() {
        let mode = OfflineMode::new(OfflineModeConfig::default());
        assert!(!mode.is_offline());
    }

    #[test]
    fn test_offline_mode_enable() {
        let mut mode = OfflineMode::new(OfflineModeConfig::default());
        mode.enable();
        assert!(mode.is_offline());
    }

    #[test]
    fn test_offline_mode_disable() {
        let mut mode = OfflineMode::new(OfflineModeConfig::all_services());
        mode.disable();
        assert!(!mode.is_offline());
    }

    #[test]
    fn test_offline_mode_service_available_online() {
        let mode = OfflineMode::new(OfflineModeConfig::all_services());
        assert!(mode.service_available("code_analysis"));
    }

    #[test]
    fn test_offline_mode_service_available_offline() {
        let mut mode = OfflineMode::new(OfflineModeConfig::all_services());
        mode.enable();
        assert!(mode.service_available("code_analysis"));
        mode.mark_unavailable("code_analysis");
        assert!(!mode.service_available("code_analysis"));
    }

    // Local Code Analyzer Tests

    #[test]
    fn test_analyze_structure_empty() {
        let result = LocalCodeAnalyzer::analyze_structure("");
        assert_eq!(result.line_count, 0);
        assert_eq!(result.function_count, 0);
        assert_eq!(result.struct_count, 0);
    }

    #[test]
    fn test_analyze_structure_with_functions() {
        let code = "fn foo() {}\nfn bar() {}";
        let result = LocalCodeAnalyzer::analyze_structure(code);
        assert_eq!(result.line_count, 2);
        assert_eq!(result.function_count, 2);
    }

    #[test]
    fn test_analyze_structure_with_todos() {
        let code = "// TODO: fix this\nfn foo() {}";
        let result = LocalCodeAnalyzer::analyze_structure(code);
        assert!(result.has_error_markers);
    }

    #[test]
    fn test_validate_syntax_balanced() {
        let result = LocalCodeAnalyzer::validate_syntax("{ foo() }");
        assert!(result.issues.is_empty());
    }

    #[test]
    fn test_validate_syntax_unmatched_braces() {
        let result = LocalCodeAnalyzer::validate_syntax("{ foo()");
        assert!(!result.issues.is_empty());
    }

    #[test]
    fn test_extract_metadata() {
        let code = "pub fn foo() {}";
        let meta = LocalCodeAnalyzer::extract_metadata(code, "test.rs");
        assert_eq!(meta.file_name, "test.rs");
        assert_eq!(meta.language, "Rust");
    }

    // Local Search Tests

    #[test]
    fn test_search_empty() {
        let results = LocalSearchEngine::search("", "query", 10);
        assert!(results.is_empty());
    }

    #[test]
    fn test_search_single_match() {
        let text = "line one\nline two\nline three";
        let results = LocalSearchEngine::search(text, "two", 10);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].line_number, 2);
    }

    #[test]
    fn test_search_case_insensitive() {
        let text = "Line One\nline two";
        let results = LocalSearchEngine::search(text, "line", 10);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_search_max_results() {
        let text = "line\nline\nline\nline\nline";
        let results = LocalSearchEngine::search(text, "line", 2);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_search_stats() {
        let text = "foo\nbar\nfoo\nbaz";
        let stats = LocalSearchEngine::search_stats(text, "foo");
        assert_eq!(stats.total_lines, 4);
        assert_eq!(stats.matching_lines, 2);
        assert!((stats.match_percentage - 50.0).abs() < 0.01);
    }

    // Static Tool Descriptions Tests

    #[test]
    fn test_static_tools_all() {
        let tools = StaticToolDescriptions::all();
        assert!(!tools.is_empty());
        assert!(tools.contains_key("code_analysis"));
    }

    #[test]
    fn test_static_tools_get() {
        let tool = StaticToolDescriptions::get("code_analysis");
        assert!(tool.is_some());
        assert!(tool.unwrap().offline);
    }

    #[test]
    fn test_static_tools_offline_available() {
        assert!(StaticToolDescriptions::is_offline_available(
            "code_analysis"
        ));
        assert!(!StaticToolDescriptions::is_offline_available("nonexistent"));
    }

    // Language Detection Tests

    #[test]
    fn test_infer_language_rust() {
        assert_eq!(infer_language("test.rs"), "Rust");
    }

    #[test]
    fn test_infer_language_python() {
        assert_eq!(infer_language("test.py"), "Python");
    }

    #[test]
    fn test_infer_language_go() {
        assert_eq!(infer_language("test.go"), "Go");
    }

    #[test]
    fn test_infer_language_unknown() {
        assert_eq!(infer_language("test.unknown"), "Unknown");
    }
}
