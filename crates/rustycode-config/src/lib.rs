#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::float_cmp,
        clippy::format_push_string,
        clippy::items_after_statements,
        clippy::bool_to_int_with_if,
        clippy::uninlined_format_args
    )
)]
// RustyCode Configuration Library
//
// This library provides a hierarchical configuration system with support for:
// - JSON/JSONC parsing (with comments and trailing commas)
// - Environment variable substitution ({env:VAR_NAME})
// - File reference resolution ({file:path})
// - Hierarchical merging (global → workspace → project)
// - Schema validation
// - Well-known configuration templates

pub mod backup;
pub mod domain;
pub mod jsonc;
pub mod loader;
pub mod parser;
pub mod paths;
#[cfg(feature = "schema-validation")]
pub mod schema;
pub mod substitutions;
pub mod wellknown;

pub use backup::ConfigBackup;
pub use domain::{AutonomyLevel, DomainContext, DomainFormatterConfig, DomainLintConfig};
pub use jsonc::JsoncParser;
pub use loader::ConfigLoader;
#[cfg(feature = "schema-validation")]
pub use schema::SchemaValidator;
pub use substitutions::SubstitutionEngine;
pub use wellknown::WellKnownTemplates;

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

// Re-export LspConfig from rustycode-lsp for convenience
pub use rustycode_lsp::LspConfig;

pub use parser::{api_key_env_name, default_model_for_provider};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(rename = "$schema")]
    pub schema: Option<String>,

    // Core settings
    pub model: String,
    /// Active provider name (e.g., "openai", "anthropic")
    #[serde(default = "default_provider")]
    pub provider: String,
    pub temperature: Option<f32>,
    pub max_tokens: Option<usize>,
    #[serde(default = "default_timeout_seconds")]
    pub timeout_seconds: Option<u64>,

    // Provider configuration
    #[serde(default)]
    pub providers: ProvidersConfig,

    // Workspace settings
    pub workspace: Option<WorkspaceConfig>,

    // Features
    #[serde(default)]
    pub features: FeaturesConfig,

    // Advanced
    #[serde(default)]
    pub advanced: AdvancedConfig,

    // Model routing — maps intent categories to models
    #[serde(default)]
    pub model_routing: ModelRoutingConfig,

    // Task routing — controls confidence thresholds and fallback behavior
    #[serde(default)]
    pub task_routing: TaskRoutingConfig,

    // Directory configuration
    #[serde(default = "default_data_dir")]
    pub data_dir: PathBuf,

    #[serde(default)]
    pub lsp_servers: Vec<String>,

    #[serde(default = "default_memory_dir")]
    pub memory_dir: PathBuf,

    #[serde(default = "default_skills_dir")]
    pub skills_dir: PathBuf,
}

fn default_provider() -> String {
    "anthropic".to_string()
}

#[allow(clippy::unnecessary_wraps)]
const fn default_timeout_seconds() -> Option<u64> {
    Some(120)
}

fn default_data_dir() -> PathBuf {
    if let Ok(codey) = std::env::var("CODEX_HOME") {
        return PathBuf::from(codey).join("rustycode");
    }

    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".rustycode")
        .join("data")
}

fn default_memory_dir() -> PathBuf {
    if let Ok(codey) = std::env::var("CODEX_HOME") {
        return PathBuf::from(codey).join("memory");
    }

    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".rustycode")
        .join("memory")
}

fn default_skills_dir() -> PathBuf {
    if let Ok(codey) = std::env::var("CODEX_HOME") {
        return PathBuf::from(codey).join("skills");
    }

    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".rustycode")
        .join("skills")
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProvidersConfig {
    pub anthropic: Option<ProviderConfig>,
    pub openai: Option<ProviderConfig>,
    pub openrouter: Option<ProviderConfig>,
    pub nvidia: Option<ProviderConfig>,
    #[serde(flatten)]
    pub custom: std::collections::HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub models: Option<Vec<String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers: Option<std::collections::HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceConfig {
    pub name: Option<String>,
    pub root: Option<PathBuf>,
    #[serde(default)]
    pub features: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FeaturesConfig {
    #[serde(default)]
    pub git_integration: bool,

    #[serde(default)]
    pub file_watcher: bool,

    #[serde(default)]
    pub mcp_servers: Vec<String>,

    #[serde(default)]
    pub agents: Vec<String>,
}

/// Task routing configuration — controls when the assistant should proceed,
/// ask clarifying questions, or switch into research-first behavior.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRoutingConfig {
    /// Enable task routing guidance in prompts.
    #[serde(default = "default_task_routing_enabled")]
    pub enabled: bool,

    /// Confidence level required before the agent proceeds without extra checks.
    #[serde(default = "default_task_routing_confidence_threshold")]
    pub confidence_threshold: f64,

    /// Maximum clarifying questions to ask in interactive mode.
    #[serde(default = "default_task_routing_max_clarifying_questions")]
    pub max_clarifying_questions: usize,

    /// Maximum research passes to run in headless mode before falling back.
    #[serde(default = "default_task_routing_max_research_passes")]
    pub max_research_passes: usize,

    /// Optional per-intent routing overrides.
    #[serde(default)]
    pub intent_overrides: BTreeMap<String, TaskRoutingOverride>,

    /// Optional per-workflow routing overrides.
    #[serde(default)]
    pub workflow_overrides: BTreeMap<String, TaskRoutingOverride>,
}

impl Default for TaskRoutingConfig {
    fn default() -> Self {
        Self {
            enabled: default_task_routing_enabled(),
            confidence_threshold: default_task_routing_confidence_threshold(),
            max_clarifying_questions: default_task_routing_max_clarifying_questions(),
            max_research_passes: default_task_routing_max_research_passes(),
            intent_overrides: BTreeMap::new(),
            workflow_overrides: BTreeMap::new(),
        }
    }
}

/// Optional overrides for a task routing path.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TaskRoutingOverride {
    /// Override the confidence threshold for this routing key.
    #[serde(default)]
    pub confidence_threshold: Option<f64>,

    /// Override the clarifying-question budget.
    #[serde(default)]
    pub max_clarifying_questions: Option<usize>,

    /// Override the research-pass budget.
    #[serde(default)]
    pub max_research_passes: Option<usize>,

    /// Force a different workflow label.
    #[serde(default)]
    pub workflow: Option<String>,

    /// Force a different team role label.
    #[serde(default)]
    pub team: Option<String>,

    /// Force a different agent role label.
    #[serde(default)]
    pub agent: Option<String>,

    /// Override skill hints.
    #[serde(default)]
    pub skills: Vec<String>,
}

const fn default_task_routing_enabled() -> bool {
    true
}

const fn default_task_routing_confidence_threshold() -> f64 {
    0.72
}

const fn default_task_routing_max_clarifying_questions() -> usize {
    3
}

const fn default_task_routing_max_research_passes() -> usize {
    2
}

/// MCP transport type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum McpTransportType {
    Stdio,
    Http,
    Sse,
}

/// OAuth configuration for remote MCP servers
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpOAuthConfig {
    pub client_id: String,
    #[serde(default)]
    pub scopes: Option<String>,
    #[serde(default)]
    pub callback_port: Option<u16>,
}

/// MCP server configuration with detailed settings
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MCPServerConfig {
    /// Server name/ID
    pub name: String,

    /// Transport type — auto-detected if omitted (command → stdio, url → http)
    #[serde(default, rename = "type")]
    pub transport_type: Option<McpTransportType>,

    /// Command to start the MCP server (stdio)
    #[serde(default)]
    pub command: Option<String>,

    /// Arguments to pass to the command (stdio)
    #[serde(default)]
    pub args: Vec<String>,

    /// Environment variables for the server (stdio)
    #[serde(default)]
    pub env: std::collections::HashMap<String, String>,

    /// Remote server URL (http/sse)
    #[serde(default)]
    pub url: Option<String>,

    /// HTTP headers for remote servers
    #[serde(default)]
    pub headers: Option<std::collections::HashMap<String, String>>,

    /// Path to script that outputs dynamic headers as JSON
    #[serde(default)]
    pub headers_helper: Option<String>,

    /// Server description
    #[serde(default)]
    pub description: Option<String>,

    /// OAuth configuration for remote servers
    #[serde(default)]
    pub oauth: Option<McpOAuthConfig>,

    /// Whether the server is enabled
    #[serde(default = "default_mcp_enabled")]
    pub enabled: bool,

    /// Transport type (legacy field, superseded by `transport_type`)
    #[serde(default)]
    pub transport: Option<String>,
}

const fn default_mcp_enabled() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AdvancedConfig {
    #[serde(default)]
    pub log_level: String,

    #[serde(default)]
    pub cache_enabled: bool,

    #[serde(default)]
    pub telemetry_enabled: bool,

    /// Automatic context compaction configuration
    #[serde(default)]
    pub compaction: Option<CompactionConfig>,

    /// Prompt caching configuration for cost optimization
    #[serde(default)]
    pub prompt_caching: Option<PromptCachingConfig>,

    /// MCP server configurations (detailed)
    #[serde(default)]
    pub mcp_servers_map: std::collections::HashMap<String, MCPServerConfig>,

    #[serde(default)]
    pub experimental: std::collections::HashMap<String, serde_json::Value>,

    /// LSP server configurations (per-language overrides)
    #[serde(default)]
    pub lsp_config: Option<LspConfig>,

    /// Per-project tool configuration
    #[serde(default)]
    pub project_tools: Option<ProjectTools>,
}

/// Build system detection
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[non_exhaustive]
pub enum BuildSystem {
    Cargo,
    Maven,
    Gradle,
    Bazel,
    Npm,
    Pip,
    Yarn,
    Pnpm,
    Go,
    CargoMake,
    Make,
    CMake,
    Composer,
}

/// Per-project tool configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProjectTools {
    /// Detected or configured build system
    #[serde(default)]
    pub build_system: Option<BuildSystem>,

    /// Linters configured for this project
    #[serde(default)]
    pub linters: Vec<String>,

    /// Formatters configured for this project
    #[serde(default)]
    pub formatters: Vec<String>,

    /// LSP server overrides for this project (merged with global `lsp_config`)
    #[serde(default)]
    pub lsp_config: Option<LspConfig>,
}

/// Automatic context compaction configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionConfig {
    /// Enable automatic compaction (default: true)
    #[serde(default = "default_compaction_enabled")]
    pub enabled: bool,

    /// Trigger at percentage of context window (default: 0.85)
    #[serde(default = "default_compaction_threshold")]
    pub trigger_threshold: f64,

    /// Target percentage after compaction (default: 0.60)
    #[serde(default = "default_compaction_target")]
    pub target_ratio: f64,

    /// Minimum messages to preserve (default: 10)
    #[serde(default = "default_compaction_preserve")]
    pub min_preserve_messages: usize,
}

const fn default_compaction_enabled() -> bool {
    true
}

const fn default_compaction_threshold() -> f64 {
    0.85
}

const fn default_compaction_target() -> f64 {
    0.60
}

const fn default_compaction_preserve() -> usize {
    10
}

/// Prompt caching configuration for cost optimization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptCachingConfig {
    /// Enable prompt caching (default: true)
    #[serde(default = "default_prompt_caching_enabled")]
    pub enabled: bool,

    /// Caching strategy to use (default: "`system_prompts`")
    ///
    /// Options: "none", "`system_prompts`", "`tool_definitions`", "`large_context`", "aggressive"
    #[serde(default = "default_prompt_caching_strategy")]
    pub strategy: String,

    /// Minimum size for large content caching (default: 5000)
    #[serde(default = "default_prompt_caching_min_size")]
    pub min_size: usize,
}

const fn default_prompt_caching_enabled() -> bool {
    true
}

fn default_prompt_caching_strategy() -> String {
    "system_prompts".to_string()
}

const fn default_prompt_caching_min_size() -> usize {
    5000
}

/// Model routing configuration — maps task intents to specific models.
///
/// When `--mode auto` is used, `IntentGate` classifies the prompt into an
/// intent category, and this config determines which model to use for each.
///
/// Example config.json:
/// ```json
/// {
///   "model": "claude-sonnet-4-20250514",
///   "model_routing": {
///     "enabled": true,
///     "models": {
///       "explanation": "claude-haiku-4-20250414",
///       "investigation": "claude-sonnet-4-20250514",
///       "implementation": "claude-sonnet-4-20250514",
///       "refactoring": "claude-sonnet-4-20250514",
///       "planning": "claude-opus-4-20250514",
///       "testing": "claude-sonnet-4-20250514"
///     }
///   }
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelRoutingConfig {
    /// Enable intent-based model routing (default: true)
    #[serde(default = "default_model_routing_enabled")]
    pub enabled: bool,

    /// Map of intent category name to model ID.
    /// Keys: explanation, investigation, implementation, refactoring, planning, testing
    /// Values: any valid model string for the configured provider
    #[serde(default = "default_model_routing_models")]
    pub models: std::collections::HashMap<String, String>,
}

impl Default for ModelRoutingConfig {
    fn default() -> Self {
        Self {
            enabled: default_model_routing_enabled(),
            models: default_model_routing_models(),
        }
    }
}

const fn default_model_routing_enabled() -> bool {
    true
}

fn default_model_routing_models() -> std::collections::HashMap<String, String> {
    let mut m = std::collections::HashMap::new();
    // Anthropic Claude model defaults — override in config for other providers
    m.insert("explanation".into(), "claude-haiku-4-5-20251001".into());
    m.insert("investigation".into(), "claude-sonnet-4-6".into());
    m.insert("implementation".into(), "claude-sonnet-4-6".into());
    m.insert("refactoring".into(), "claude-sonnet-4-6".into());
    m.insert("planning".into(), "claude-opus-4-6".into());
    m.insert("testing".into(), "claude-sonnet-4-6".into());
    m
}

impl ModelRoutingConfig {
    /// Get the model for a given intent category name.
    /// Returns None if routing is disabled or the intent isn't configured.
    pub fn model_for_intent(&self, intent_name: &str) -> Option<String> {
        if !self.enabled {
            return None;
        }
        self.models.get(intent_name).cloned()
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            schema: None,
            model: "claude-sonnet-4-6".to_string(),
            provider: default_provider(),
            temperature: Some(0.1),
            max_tokens: Some(4096),
            timeout_seconds: default_timeout_seconds(),
            providers: ProvidersConfig::default(),
            workspace: None,
            features: FeaturesConfig::default(),
            advanced: AdvancedConfig::default(),
            model_routing: ModelRoutingConfig::default(),
            task_routing: TaskRoutingConfig::default(),
            data_dir: default_data_dir(),
            lsp_servers: Vec::new(),
            memory_dir: default_memory_dir(),
            skills_dir: default_skills_dir(),
        }
    }
}

impl Config {
    /// Load configuration from a directory, searching for .rustycode/config files
    pub fn load(project_dir: &Path) -> Result<Self, ConfigError> {
        fn ensure_dir(dir: &Path) -> Result<(), ConfigError> {
            std::fs::create_dir_all(dir)
                .map_err(|e| ConfigError::FileReadError(dir.to_path_buf(), e.to_string()))
        }

        let mut loader = ConfigLoader::new();
        let config_value = loader
            .load(project_dir)
            .map_err(|e| ConfigError::FileReadError(project_dir.to_path_buf(), e))?;

        // Deserialize the merged config
        let config: Self =
            serde_json::from_value(config_value).map_err(ConfigError::DeserializeError)?;

        // Validate critical values
        if let Some(timeout) = config.timeout_seconds {
            if timeout == 0 {
                return Err(ConfigError::ValidationError(
                    "timeout_seconds must be greater than 0".into(),
                ));
            }
        }
        if let Some(max_tokens) = config.max_tokens {
            if max_tokens == 0 {
                return Err(ConfigError::ValidationError(
                    "max_tokens must be greater than 0".into(),
                ));
            }
        }

        ensure_dir(&config.data_dir)?;
        ensure_dir(&config.skills_dir)?;
        ensure_dir(&config.memory_dir)?;

        // Auto-add .rustycode/ to .gitignore if in a git repo
        ensure_gitignore(project_dir);

        Ok(config)
    }

    /// Save configuration to a file, creating a backup of the existing file first.
    pub fn save(&self, path: &Path) -> Result<(), ConfigError> {
        let path_buf = path.to_path_buf();

        // Create backup before overwriting (non-fatal if it fails)
        let backup = ConfigBackup::new(path);
        if let Err(_e) = backup.create_backup() {
            // Backup failure is non-fatal — the save proceeds regardless
        }

        let json = serde_json::to_string_pretty(self)
            .map_err(|e| ConfigError::ParseError(path_buf.clone(), e.to_string()))?;

        let tmp_path = path.with_extension("json.tmp");
        std::fs::write(&tmp_path, &json)
            .map_err(|e| ConfigError::FileReadError(path_buf.clone(), e.to_string()))?;
        std::fs::rename(&tmp_path, path)
            .map_err(|e| ConfigError::FileReadError(path_buf, e.to_string()))?;

        Ok(())
    }

    /// Return a JSON-safe representation with API keys redacted.
    ///
    /// Keys longer than 8 characters show first 4 and last 4 chars.
    /// Keys 4-8 characters show first 2 and last 2.
    /// Shorter keys are fully masked.
    pub fn redacted_for_display(&self) -> serde_json::Value {
        let mut json = serde_json::to_value(self).unwrap_or_else(|_| serde_json::json!({}));

        // Redact provider API keys
        if let Some(providers) = json.get_mut("providers").and_then(|p| p.as_object_mut()) {
            for (_name, provider_val) in providers.iter_mut() {
                if let Some(obj) = provider_val.as_object_mut() {
                    if let Some(api_key) = obj.get_mut("api_key") {
                        if let Some(key_str) = api_key.as_str() {
                            *api_key = serde_json::Value::String(redact_key(key_str));
                        }
                    }
                }
            }
        }

        // Round f32 temperature to avoid floating-point display artifacts
        // (e.g., 0.1f32 renders as 0.100000001490116)
        if let Some(temp) = json.get_mut("temperature").and_then(|t| t.as_f64()) {
            let rounded = format!("{temp:.2}").parse::<f64>().unwrap_or(temp);
            if let Some(temp_val) = json.get_mut("temperature") {
                *temp_val = serde_json::json!(rounded);
            }
        }

        json
    }
}

fn redact_key(key: &str) -> String {
    if key.len() > 8 {
        let start = 4.min(key.floor_char_boundary(4));
        let end = key.len().saturating_sub(4);
        let end = key.floor_char_boundary(end);
        if start >= end {
            return "***".to_string();
        }
        format!("{}...{}", &key[..start], &key[end..])
    } else if key.len() > 4 {
        let start = 2.min(key.floor_char_boundary(2));
        let end = key.len().saturating_sub(2);
        let end = key.floor_char_boundary(end);
        if start >= end {
            return "***".to_string();
        }
        format!("{}...{}", &key[..start], &key[end..])
    } else {
        "***".to_string()
    }
}

/// Ensure `.rustycode/` is in the project's `.gitignore` if it's a git repo.
///
/// This prevents session data, cache, and local configuration from being
/// accidentally committed. Non-fatal if it fails.
fn ensure_gitignore(project_dir: &Path) {
    let gitignore_path = project_dir.join(".gitignore");

    // Only proceed if this is a git repo (has .git directory)
    if !project_dir.join(".git").exists() {
        return;
    }

    // Read existing .gitignore content
    let existing = std::fs::read_to_string(&gitignore_path).unwrap_or_default();

    // Check if .rustycode/ is already listed
    for line in existing.lines() {
        let trimmed = line.trim();
        if trimmed == ".rustycode/" || trimmed == ".rustycode" {
            return; // Already listed
        }
    }

    // Append .rustycode/ to .gitignore
    let entry = if existing.is_empty() || existing.ends_with('\n') {
        ".rustycode/\n".to_string()
    } else {
        "\n.rustycode/\n".to_string()
    };

    if let Err(e) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&gitignore_path)
        .and_then(|mut f| {
            use std::io::Write;
            write!(f, "{entry}")
        })
    {
        tracing::debug!("Could not update .gitignore: {}", e);
    }
}

// Errors
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ConfigError {
    #[error("Failed to read configuration file {0}: {1}")]
    FileReadError(PathBuf, String),

    #[error("Failed to parse configuration file {0}: {1}")]
    ParseError(PathBuf, String),

    #[error("Configuration validation failed: {0}")]
    ValidationError(String),

    #[error("Failed to deserialize configuration: {0}")]
    DeserializeError(#[from] serde_json::Error),

    #[error("Required environment variable '{0}' is not set. Set it with: export {0}=your_value")]
    EnvVarNotFound(String),

    #[error("Substitution error: {0}")]
    SubstitutionError(#[from] substitutions::SubstitutionError),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default_model() {
        let config = Config::default();
        assert_eq!(config.model, "claude-sonnet-4-6");
        assert_eq!(config.temperature, Some(0.1));
        assert_eq!(config.max_tokens, Some(4096));
        assert!(config.schema.is_none());
        assert!(config.workspace.is_none());
    }

    #[test]
    fn test_config_default_directories() {
        let config = Config::default();
        assert!(config.data_dir.to_string_lossy().contains(".rustycode"));
        assert!(config.memory_dir.to_string_lossy().contains(".rustycode"));
        assert!(config.skills_dir.to_string_lossy().contains(".rustycode"));
    }

    #[test]
    fn test_config_serialization_roundtrip() {
        let config = Config::default();
        let json = serde_json::to_string_pretty(&config).unwrap();
        let decoded: Config = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.model, config.model);
        assert_eq!(decoded.temperature, config.temperature);
    }

    #[test]
    fn test_providers_config_default() {
        let providers = ProvidersConfig::default();
        assert!(providers.anthropic.is_none());
        assert!(providers.openai.is_none());
        assert!(providers.openrouter.is_none());
        assert!(providers.custom.is_empty());
    }

    #[test]
    fn test_features_config_default() {
        let features = FeaturesConfig::default();
        assert!(!features.git_integration);
        assert!(!features.file_watcher);
        assert!(features.mcp_servers.is_empty());
        assert!(features.agents.is_empty());
    }

    #[test]
    fn test_task_routing_config_default() {
        let routing = TaskRoutingConfig::default();
        assert!(routing.enabled);
        assert!((routing.confidence_threshold - 0.72).abs() < f64::EPSILON);
        assert_eq!(routing.max_clarifying_questions, 3);
        assert_eq!(routing.max_research_passes, 2);
        assert!(routing.intent_overrides.is_empty());
        assert!(routing.workflow_overrides.is_empty());
    }

    #[test]
    fn test_task_routing_override_roundtrip() {
        let mut routing = TaskRoutingConfig::default();
        routing.intent_overrides.insert(
            "planning".to_string(),
            TaskRoutingOverride {
                confidence_threshold: Some(0.9),
                max_clarifying_questions: Some(1),
                max_research_passes: Some(4),
                workflow: Some("plan".to_string()),
                team: Some("architect".to_string()),
                agent: Some("planner".to_string()),
                skills: vec!["research".to_string(), "design".to_string()],
            },
        );
        let json = serde_json::to_string(&routing).unwrap();
        let decoded: TaskRoutingConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded.intent_overrides.len(), 1);
        assert_eq!(
            decoded
                .intent_overrides
                .get("planning")
                .unwrap()
                .workflow
                .as_deref(),
            Some("plan")
        );
    }

    #[test]
    fn test_advanced_config_default() {
        let advanced = AdvancedConfig::default();
        assert!(advanced.log_level.is_empty());
        assert!(!advanced.cache_enabled);
        assert!(!advanced.telemetry_enabled);
        assert!(advanced.compaction.is_none());
        assert!(advanced.prompt_caching.is_none());
        assert!(advanced.mcp_servers_map.is_empty());
        assert!(advanced.experimental.is_empty());
    }

    #[test]
    fn test_model_routing_config_default() {
        let routing = ModelRoutingConfig::default();
        assert!(routing.enabled);
        assert!(routing.models.contains_key("explanation"));
        assert!(routing.models.contains_key("implementation"));
        assert!(routing.models.contains_key("planning"));
        assert_eq!(routing.models.len(), 6);
    }

    #[test]
    fn test_model_routing_model_for_intent() {
        let routing = ModelRoutingConfig::default();
        let model = routing.model_for_intent("planning").unwrap();
        assert!(model.contains("opus"));
    }

    #[test]
    fn test_model_routing_disabled_returns_none() {
        let routing = ModelRoutingConfig {
            enabled: false,
            ..Default::default()
        };
        assert!(routing.model_for_intent("planning").is_none());
    }

    #[test]
    fn test_model_routing_unknown_intent() {
        let routing = ModelRoutingConfig::default();
        assert!(routing.model_for_intent("unknown_intent").is_none());
    }

    #[test]
    fn test_compaction_config_defaults() {
        let json = r"{}";
        let config: CompactionConfig = serde_json::from_str(json).unwrap();
        assert!(config.enabled);
        assert!((config.trigger_threshold - 0.85).abs() < f64::EPSILON);
        assert!((config.target_ratio - 0.60).abs() < f64::EPSILON);
        assert_eq!(config.min_preserve_messages, 10);
    }

    #[test]
    fn test_prompt_caching_config_defaults() {
        let json = r"{}";
        let config: PromptCachingConfig = serde_json::from_str(json).unwrap();
        assert!(config.enabled);
        assert_eq!(config.strategy, "system_prompts");
        assert_eq!(config.min_size, 5000);
    }

    #[test]
    fn test_provider_config_serialization() {
        let provider = ProviderConfig {
            api_key: Some("sk-test".to_string()),
            base_url: Some("https://api.example.com".to_string()),
            models: Some(vec!["model-1".to_string(), "model-2".to_string()]),
            headers: None,
        };
        let json = serde_json::to_string(&provider).unwrap();
        assert!(json.contains("sk-test"));
        assert!(json.contains("model-1"));
        assert!(!json.contains("headers"));
    }

    #[test]
    fn test_workspace_config_serialization() {
        let ws = WorkspaceConfig {
            name: Some("my-project".to_string()),
            root: Some(PathBuf::from("/home/user/project")),
            features: vec!["git".to_string(), "watcher".to_string()],
        };
        let json = serde_json::to_string(&ws).unwrap();
        let decoded: WorkspaceConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.name, Some("my-project".to_string()));
        assert_eq!(decoded.features.len(), 2);
    }

    #[test]
    fn test_mcp_server_config_defaults() {
        let json = r#"{"name":"test","command":"npx"}"#;
        let config: MCPServerConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.name, "test");
        assert_eq!(config.command.unwrap(), "npx");
        assert!(config.args.is_empty());
        assert!(config.env.is_empty());
        assert!(config.enabled); // default true
        assert!(config.transport.is_none());
        assert!(config.url.is_none());
        assert!(config.headers.is_none());
        assert!(config.description.is_none());
    }

    #[test]
    fn test_config_error_display() {
        let err =
            ConfigError::FileReadError(PathBuf::from("/tmp/config.json"), "not found".to_string());
        let msg = err.to_string();
        assert!(msg.contains("/tmp/config.json"));
        assert!(msg.contains("not found"));

        let err2 = ConfigError::ValidationError("invalid field".to_string());
        assert!(err2.to_string().contains("invalid field"));

        let err3 = ConfigError::EnvVarNotFound("API_KEY".to_string());
        assert!(err3.to_string().contains("API_KEY"));
    }

    #[test]
    fn test_model_routing_config_serialization() {
        let routing = ModelRoutingConfig::default();
        let json = serde_json::to_string(&routing).unwrap();
        let decoded: ModelRoutingConfig = serde_json::from_str(&json).unwrap();
        assert!(decoded.enabled);
        assert_eq!(decoded.models.len(), 6);
    }

    #[test]
    fn test_config_clone() {
        let config = Config::default();
        let cloned = config.clone();
        assert_eq!(cloned.model, config.model);
        assert_eq!(cloned.temperature, config.temperature);
        assert_eq!(cloned.max_tokens, config.max_tokens);
    }

    #[test]
    fn test_config_debug() {
        let config = Config::default();
        let debug = format!("{:?}", config);
        assert!(debug.contains("claude-sonnet-4-6"));
        assert!(debug.contains("model"));
        assert!(debug.contains("temperature"));
    }

    #[test]
    fn test_config_error_parse_error() {
        let err =
            ConfigError::ParseError(PathBuf::from("/bad/config.json"), "unexpected token".into());
        let msg = err.to_string();
        assert!(msg.contains("/bad/config.json"));
        assert!(msg.contains("unexpected token"));
    }

    #[test]
    fn test_config_error_substitution_error() {
        let err = ConfigError::SubstitutionError(substitutions::SubstitutionError::InvalidFormat(
            "bad format".into(),
        ));
        let msg = err.to_string();
        assert!(msg.contains("bad format") || msg.contains("substitution"));
    }

    #[test]
    fn test_providers_config_with_custom() {
        let json = r#"{
            "anthropic": {"api_key": "sk-test"},
            "custom_provider": {"api_key": "custom-key"}
        }"#;
        let providers: ProvidersConfig = serde_json::from_str(json).unwrap();
        assert!(providers.anthropic.is_some());
        assert_eq!(providers.custom.len(), 1);
        assert!(providers.custom.contains_key("custom_provider"));
    }

    #[test]
    fn test_providers_config_serialization_roundtrip() {
        let providers = ProvidersConfig::default();
        let json = serde_json::to_string(&providers).unwrap();
        let decoded: ProvidersConfig = serde_json::from_str(&json).unwrap();
        assert!(decoded.anthropic.is_none());
        assert!(decoded.openai.is_none());
        assert!(decoded.custom.is_empty());
    }

    #[test]
    fn test_provider_config_with_headers() {
        let provider = ProviderConfig {
            api_key: Some("sk-test".into()),
            base_url: None,
            models: None,
            headers: Some({
                let mut h = std::collections::HashMap::new();
                h.insert("X-Custom".into(), "value".into());
                h
            }),
        };
        let json = serde_json::to_string(&provider).unwrap();
        let decoded: ProviderConfig = serde_json::from_str(&json).unwrap();
        assert!(decoded.headers.is_some());
        assert_eq!(decoded.headers.unwrap().get("X-Custom").unwrap(), "value");
    }

    #[test]
    fn test_provider_config_empty_optional_fields() {
        let provider = ProviderConfig {
            api_key: None,
            base_url: None,
            models: None,
            headers: None,
        };
        let json = serde_json::to_string(&provider).unwrap();
        // skip_serializing_if means these should not appear
        assert!(!json.contains("api_key"));
        assert!(!json.contains("base_url"));
        assert!(!json.contains("models"));
        assert!(!json.contains("headers"));
    }

    #[test]
    fn test_redact_key_masks_long_keys() {
        assert_eq!(redact_key("sk-1234567890abcdef"), "sk-1...cdef");
        assert_eq!(redact_key("short"), "sh...rt");
        assert_eq!(redact_key("abc"), "***");
    }

    #[test]
    fn test_redacted_for_display_hides_api_keys() {
        let config = Config::default();
        let display = config.redacted_for_display();

        // No providers configured in default, but the structure should exist
        assert!(display.get("providers").is_some());
    }

    #[test]
    fn test_mcp_server_config_full() {
        let config = MCPServerConfig {
            name: "my-server".into(),
            command: Some("npx".into()),
            args: vec!["-y".into(), "some-mcp-server".into()],
            env: {
                let mut e = std::collections::HashMap::new();
                e.insert("API_KEY".into(), "secret".into());
                e
            },
            enabled: false,
            transport: Some("sse".into()),
            transport_type: Some(McpTransportType::Sse),
            url: None,
            headers: None,
            headers_helper: None,
            description: None,
            oauth: None,
        };
        let json = serde_json::to_string(&config).unwrap();
        let decoded: MCPServerConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.name, "my-server");
        assert_eq!(decoded.args.len(), 2);
        assert_eq!(decoded.env.get("API_KEY").unwrap(), "secret");
        assert!(!decoded.enabled);
        assert_eq!(decoded.transport.unwrap(), "sse");
    }

    #[test]
    fn test_mcp_server_config_claude_stdio_format() {
        let json = r#"{
            "name": "filesystem",
            "type": "stdio",
            "command": "npx",
            "args": ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"],
            "env": {"API_KEY": "secret"},
            "description": "File system access"
        }"#;
        let config: MCPServerConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.name, "filesystem");
        assert_eq!(config.transport_type, Some(McpTransportType::Stdio));
        assert_eq!(config.command.as_deref(), Some("npx"));
        assert_eq!(config.args.len(), 3);
        assert_eq!(config.description.as_deref(), Some("File system access"));
    }

    #[test]
    fn test_mcp_server_config_claude_http_format() {
        let json = r#"{
            "name": "vercel",
            "type": "http",
            "url": "https://mcp.vercel.com",
            "headers": {"Authorization": "Bearer token123"},
            "description": "Vercel deployments"
        }"#;
        let config: MCPServerConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.name, "vercel");
        assert_eq!(config.transport_type, Some(McpTransportType::Http));
        assert_eq!(config.url.as_deref(), Some("https://mcp.vercel.com"));
        assert_eq!(
            config
                .headers
                .as_ref()
                .unwrap()
                .get("Authorization")
                .unwrap(),
            "Bearer token123"
        );
        assert!(config.command.is_none());
    }

    #[test]
    fn test_mcp_server_config_claude_oauth_format() {
        let json = r#"{
            "name": "slack",
            "type": "http",
            "url": "https://mcp.slack.com/mcp",
            "oauth": {
                "clientId": "123.456",
                "scopes": "channels:read chat:write",
                "callbackPort": 8080
            }
        }"#;
        let config: MCPServerConfig = serde_json::from_str(json).unwrap();
        let oauth = config.oauth.unwrap();
        assert_eq!(oauth.client_id, "123.456");
        assert_eq!(oauth.scopes.as_deref(), Some("channels:read chat:write"));
        assert_eq!(oauth.callback_port, Some(8080));
    }

    #[test]
    fn test_mcp_server_config_auto_detect_stdio() {
        let json = r#"{"name":"test","command":"npx","args":["server"]}"#;
        let config: MCPServerConfig = serde_json::from_str(json).unwrap();
        assert!(config.transport_type.is_none());
        assert!(config.command.is_some());
    }

    #[test]
    fn test_mcp_server_config_auto_detect_http() {
        let json = r#"{"name":"test","url":"https://api.example.com/mcp"}"#;
        let config: MCPServerConfig = serde_json::from_str(json).unwrap();
        assert!(config.transport_type.is_none());
        assert!(config.url.is_some());
        assert!(config.command.is_none());
    }

    #[test]
    fn test_compaction_config_explicit_values() {
        let config = CompactionConfig {
            enabled: false,
            trigger_threshold: 0.95,
            target_ratio: 0.40,
            min_preserve_messages: 20,
        };
        let json = serde_json::to_string(&config).unwrap();
        let decoded: CompactionConfig = serde_json::from_str(&json).unwrap();
        assert!(!decoded.enabled);
        assert!((decoded.trigger_threshold - 0.95).abs() < f64::EPSILON);
        assert!((decoded.target_ratio - 0.40).abs() < f64::EPSILON);
        assert_eq!(decoded.min_preserve_messages, 20);
    }

    #[test]
    fn test_prompt_caching_config_explicit_values() {
        let config = PromptCachingConfig {
            enabled: false,
            strategy: "aggressive".into(),
            min_size: 10000,
        };
        let json = serde_json::to_string(&config).unwrap();
        let decoded: PromptCachingConfig = serde_json::from_str(&json).unwrap();
        assert!(!decoded.enabled);
        assert_eq!(decoded.strategy, "aggressive");
        assert_eq!(decoded.min_size, 10000);
    }

    #[test]
    fn test_model_routing_all_intents() {
        let routing = ModelRoutingConfig::default();
        for intent in &[
            "explanation",
            "investigation",
            "implementation",
            "refactoring",
            "planning",
            "testing",
        ] {
            let model = routing.model_for_intent(intent);
            assert!(model.is_some(), "Missing model for intent: {}", intent);
        }
    }

    #[test]
    fn test_model_routing_custom_override() {
        let mut routing = ModelRoutingConfig::default();
        routing
            .models
            .insert("explanation".into(), "my-custom-model".into());
        let model = routing.model_for_intent("explanation").unwrap();
        assert_eq!(model, "my-custom-model");
    }

    #[test]
    fn test_features_config_with_values() {
        let json = r#"{
            "git_integration": true,
            "file_watcher": true,
            "mcp_servers": ["server1", "server2"],
            "agents": ["planner", "coder"]
        }"#;
        let features: FeaturesConfig = serde_json::from_str(json).unwrap();
        assert!(features.git_integration);
        assert!(features.file_watcher);
        assert_eq!(features.mcp_servers.len(), 2);
        assert_eq!(features.agents.len(), 2);
    }

    #[test]
    fn test_advanced_config_with_values() {
        let json = r#"{
            "log_level": "debug",
            "cache_enabled": true,
            "telemetry_enabled": true
        }"#;
        let advanced: AdvancedConfig = serde_json::from_str(json).unwrap();
        assert_eq!(advanced.log_level, "debug");
        assert!(advanced.cache_enabled);
        assert!(advanced.telemetry_enabled);
    }

    #[test]
    fn test_advanced_config_with_compaction() {
        let json = r#"{
            "compaction": {
                "enabled": true,
                "trigger_threshold": 0.9,
                "target_ratio": 0.5,
                "min_preserve_messages": 5
            }
        }"#;
        let advanced: AdvancedConfig = serde_json::from_str(json).unwrap();
        let compaction = advanced.compaction.unwrap();
        assert!(compaction.enabled);
        assert!((compaction.trigger_threshold - 0.9).abs() < f64::EPSILON);
        assert!((compaction.target_ratio - 0.5).abs() < f64::EPSILON);
        assert_eq!(compaction.min_preserve_messages, 5);
    }

    #[test]
    fn test_advanced_config_with_mcp_servers_map() {
        let json = r#"{
            "mcp_servers_map": {
                "my-server": {
                    "name": "my-server",
                    "command": "node",
                    "args": ["server.js"],
                    "enabled": true
                }
            }
        }"#;
        let advanced: AdvancedConfig = serde_json::from_str(json).unwrap();
        assert_eq!(advanced.mcp_servers_map.len(), 1);
        let server = advanced.mcp_servers_map.get("my-server").unwrap();
        assert_eq!(server.command.as_deref(), Some("node"));
        assert_eq!(server.args.len(), 1);
    }

    #[test]
    fn test_workspace_config_with_all_fields() {
        let ws = WorkspaceConfig {
            name: Some("test-project".into()),
            root: Some(PathBuf::from("/home/user/project")),
            features: vec!["git".into(), "watcher".into(), "lsp".into()],
        };
        let json = serde_json::to_string(&ws).unwrap();
        let decoded: WorkspaceConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.name, Some("test-project".into()));
        assert_eq!(decoded.root, Some(PathBuf::from("/home/user/project")));
        assert_eq!(decoded.features.len(), 3);
    }

    #[test]
    fn test_workspace_config_empty_features() {
        let ws = WorkspaceConfig {
            name: None,
            root: None,
            features: vec![],
        };
        let json = serde_json::to_string(&ws).unwrap();
        let decoded: WorkspaceConfig = serde_json::from_str(&json).unwrap();
        assert!(decoded.name.is_none());
        assert!(decoded.root.is_none());
        assert!(decoded.features.is_empty());
    }

    #[test]
    fn test_config_with_lsp_servers() {
        let json = r#"{
            "model": "gpt-4",
            "lsp_servers": ["rust-analyzer", "typescript-language-server"]
        }"#;
        let config: Config = serde_json::from_str(json).unwrap();
        assert_eq!(config.model, "gpt-4");
        assert_eq!(config.lsp_servers.len(), 2);
        assert!(config.lsp_servers.contains(&"rust-analyzer".to_string()));
    }

    #[test]
    fn test_config_default_lsp_servers_empty() {
        let config = Config::default();
        assert!(config.lsp_servers.is_empty());
    }

    #[test]
    fn test_config_error_deserialize_error() {
        let json_str = r"{invalid json";
        let result: Result<serde_json::Value, _> = serde_json::from_str(json_str);
        assert!(result.is_err());
        // Wrap into ConfigError::DeserializeError
        let err = result.unwrap_err();
        let config_err = ConfigError::DeserializeError(err);
        // Just check it produces a non-empty error message
        assert!(!config_err.to_string().is_empty());
    }

    #[test]
    fn test_config_non_exhaustive() {
        // Extra fields should be ignored by serde's default behavior
        let json = r#"{
            "model": "claude-3",
            "unknown_field": "some_value",
            "another_unknown": 42
        }"#;
        let config: Config = serde_json::from_str(json).unwrap();
        assert_eq!(config.model, "claude-3");
    }

    #[test]
    fn test_model_routing_config_debug() {
        let routing = ModelRoutingConfig::default();
        let debug = format!("{:?}", routing);
        assert!(debug.contains("enabled"));
        assert!(debug.contains("models"));
    }

    #[test]
    fn test_model_routing_config_clone() {
        let routing = ModelRoutingConfig::default();
        let cloned = routing.clone();
        assert_eq!(cloned.enabled, routing.enabled);
        assert_eq!(cloned.models.len(), routing.models.len());
    }

    #[test]
    fn test_provider_config_debug() {
        let provider = ProviderConfig {
            api_key: Some("sk-test123".into()),
            base_url: Some("https://api.test.com".into()),
            models: Some(vec!["gpt-4".into()]),
            headers: None,
        };
        let debug = format!("{:?}", provider);
        assert!(debug.contains("ProviderConfig"));
    }

    #[test]
    fn test_provider_config_clone() {
        let provider = ProviderConfig {
            api_key: Some("sk-test".into()),
            base_url: None,
            models: Some(vec!["model-a".into()]),
            headers: None,
        };
        let cloned = provider.clone();
        assert_eq!(cloned.api_key, provider.api_key);
        assert_eq!(cloned.models.unwrap().len(), 1);
    }

    #[test]
    fn test_config_model_override() {
        let json = r#"{"model": "gpt-4-turbo"}"#;
        let config: Config = serde_json::from_str(json).unwrap();
        assert_eq!(config.model, "gpt-4-turbo");
    }

    #[test]
    fn test_config_temperature_override() {
        let json = r#"{"model": "test", "temperature": 0.5}"#;
        let config: Config = serde_json::from_str(json).unwrap();
        assert_eq!(config.temperature.unwrap(), 0.5);
    }

    #[test]
    fn test_config_max_tokens_override() {
        let json = r#"{"model": "test", "max_tokens": 8192}"#;
        let config: Config = serde_json::from_str(json).unwrap();
        assert_eq!(config.max_tokens.unwrap(), 8192);
    }

    #[test]
    fn test_config_null_optionals() {
        let json =
            r#"{"model": "test", "temperature": null, "max_tokens": null, "workspace": null}"#;
        let config: Config = serde_json::from_str(json).unwrap();
        assert!(config.temperature.is_none());
        assert!(config.max_tokens.is_none());
        assert!(config.workspace.is_none());
    }

    #[test]
    fn test_model_routing_intent_disabled_returns_none_for_all() {
        let routing = ModelRoutingConfig {
            enabled: false,
            ..Default::default()
        };
        for intent in &[
            "explanation",
            "investigation",
            "implementation",
            "refactoring",
            "planning",
            "testing",
        ] {
            assert!(routing.model_for_intent(intent).is_none());
        }
    }

    #[test]
    fn test_mcp_server_config_disabled() {
        let json = r#"{"name": "test", "command": "cmd", "enabled": false}"#;
        let config: MCPServerConfig = serde_json::from_str(json).unwrap();
        assert!(!config.enabled);
    }

    #[test]
    fn test_mcp_server_config_with_args_and_env() {
        let json = r#"{
            "name": "test",
            "command": "node",
            "args": ["--verbose", "server.js"],
            "env": {"NODE_ENV": "production", "PORT": "3000"},
            "transport": "stdio"
        }"#;
        let config: MCPServerConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.args.len(), 2);
        assert_eq!(config.env.len(), 2);
        assert_eq!(config.transport.unwrap(), "stdio");
    }

    #[test]
    fn test_build_system_serialization() {
        for bs in [
            BuildSystem::Cargo,
            BuildSystem::Maven,
            BuildSystem::Gradle,
            BuildSystem::Bazel,
            BuildSystem::Npm,
            BuildSystem::Pip,
            BuildSystem::Yarn,
            BuildSystem::Pnpm,
            BuildSystem::Go,
            BuildSystem::CargoMake,
            BuildSystem::Make,
            BuildSystem::CMake,
            BuildSystem::Composer,
        ] {
            let json = serde_json::to_string(&bs).unwrap();
            let decoded: BuildSystem = serde_json::from_str(&json).unwrap();
            assert_eq!(decoded, bs);
        }
    }

    #[test]
    fn test_build_system_from_str() {
        assert_eq!(
            serde_json::from_str::<BuildSystem>(r#""Cargo""#).unwrap(),
            BuildSystem::Cargo
        );
        assert_eq!(
            serde_json::from_str::<BuildSystem>(r#""Npm""#).unwrap(),
            BuildSystem::Npm
        );
        assert_eq!(
            serde_json::from_str::<BuildSystem>(r#""Go""#).unwrap(),
            BuildSystem::Go
        );
    }

    #[test]
    fn test_project_tools_serialization_roundtrip() {
        let tools = ProjectTools {
            build_system: Some(BuildSystem::Cargo),
            linters: vec!["clippy".to_string(), "rustfmt".to_string()],
            formatters: vec!["rustfmt".to_string()],
            lsp_config: None,
        };
        let json = serde_json::to_string(&tools).unwrap();
        let decoded: ProjectTools = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.build_system, Some(BuildSystem::Cargo));
        assert_eq!(decoded.linters.len(), 2);
        assert_eq!(decoded.formatters, vec!["rustfmt"]);
    }

    #[test]
    fn test_project_tools_minimal_json() {
        let json = r"{}";
        let tools: ProjectTools = serde_json::from_str(json).unwrap();
        assert!(tools.build_system.is_none());
        assert!(tools.linters.is_empty());
        assert!(tools.formatters.is_empty());
        assert!(tools.lsp_config.is_none());
    }

    #[test]
    fn test_project_tools_with_lsp_config() {
        let json = r#"{
            "build_system": "Npm",
            "linters": ["eslint"],
            "formatters": ["prettier"],
            "lsp_config": {
                "servers": {
                    "typescript": {
                        "command": "ts-language-server",
                        "args": ["--stdio"],
                        "enabled": true
                    }
                }
            }
        }"#;
        let tools: ProjectTools = serde_json::from_str(json).unwrap();
        assert_eq!(tools.build_system, Some(BuildSystem::Npm));
        assert_eq!(tools.linters, vec!["eslint"]);
        assert!(tools.lsp_config.is_some());
        let servers = &tools.lsp_config.as_ref().unwrap().servers;
        assert!(servers.contains_key("typescript"));
    }

    #[test]
    fn test_advanced_config_with_project_tools() {
        let json = r#"{
            "log_level": "debug",
            "cache_enabled": true,
            "project_tools": {
                "build_system": "Cargo",
                "linters": ["clippy"]
            }
        }"#;
        let advanced: AdvancedConfig = serde_json::from_str(json).unwrap();
        assert_eq!(advanced.log_level, "debug");
        assert!(advanced.cache_enabled);
        let pt = advanced.project_tools.unwrap();
        assert_eq!(pt.build_system, Some(BuildSystem::Cargo));
        assert_eq!(pt.linters, vec!["clippy"]);
    }

    #[test]
    fn test_lsp_config_roundtrip() {
        let config = LspConfig {
            servers: std::collections::HashMap::from_iter([
                (
                    "rust".to_string(),
                    rustycode_lsp::LspServerConfig {
                        command: "rust-analyzer".to_string(),
                        args: vec![],
                        env: std::collections::HashMap::new(),
                        enabled: true,
                    },
                ),
                (
                    "typescript".to_string(),
                    rustycode_lsp::LspServerConfig {
                        command: "typescript-language-server".to_string(),
                        args: vec!["--stdio".to_string()],
                        env: std::collections::HashMap::from_iter([(
                            "TSS_LOG".to_string(),
                            "error".to_string(),
                        )]),
                        enabled: false,
                    },
                ),
            ]),
        };
        let json = serde_json::to_string(&config).unwrap();
        let decoded: LspConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.servers.len(), 2);
        let rust_cfg = decoded.servers.get("rust").unwrap();
        assert_eq!(rust_cfg.command, "rust-analyzer");
        assert!(rust_cfg.enabled);
        let ts_cfg = decoded.servers.get("typescript").unwrap();
        assert_eq!(ts_cfg.command, "typescript-language-server");
        assert!(!ts_cfg.enabled);
        assert_eq!(ts_cfg.env.get("TSS_LOG").unwrap(), "error");
    }

    #[test]
    fn test_lsp_config_empty() {
        let json = r"{}";
        let config: LspConfig = serde_json::from_str(json).unwrap();
        assert!(config.servers.is_empty());
    }

    #[test]
    fn test_lsp_config_minimal_server() {
        let json = r#"{"servers": {"rust": {"command": "ra-lsp"}}}"#;
        let config: LspConfig = serde_json::from_str(json).unwrap();
        let rust = config.servers.get("rust").unwrap();
        assert_eq!(rust.command, "ra-lsp");
        assert!(rust.args.is_empty());
        assert!(rust.env.is_empty());
        assert!(rust.enabled);
    }

    #[test]
    fn test_config_with_advanced_lsp_and_project_tools() {
        let json = r#"{
            "model": "claude-3-5-sonnet",
            "advanced": {
                "log_level": "info",
                "lsp_config": {
                    "servers": {
                        "rust": {"command": "rust-analyzer"}
                    }
                },
                "project_tools": {
                    "build_system": "Cargo",
                    "linters": ["clippy"]
                }
            }
        }"#;
        let config: Config = serde_json::from_str(json).unwrap();
        assert_eq!(config.model, "claude-3-5-sonnet");
        assert_eq!(config.advanced.log_level, "info");
        assert!(config.advanced.lsp_config.is_some());
        assert!(config.advanced.project_tools.is_some());
        let pt = config.advanced.project_tools.unwrap();
        assert_eq!(pt.build_system, Some(BuildSystem::Cargo));
    }

    #[test]
    fn test_ensure_gitignore_creates_entry() {
        let dir = tempfile::tempdir().unwrap();
        // Create a .git directory to simulate a git repo
        std::fs::create_dir(dir.path().join(".git")).unwrap();

        ensure_gitignore(dir.path());

        let gitignore = std::fs::read_to_string(dir.path().join(".gitignore")).unwrap();
        assert!(gitignore.contains(".rustycode/"));
    }

    #[test]
    fn test_ensure_gitignore_no_duplicate() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        std::fs::write(dir.path().join(".gitignore"), ".rustycode/\n").unwrap();

        ensure_gitignore(dir.path());

        let gitignore = std::fs::read_to_string(dir.path().join(".gitignore")).unwrap();
        let count = gitignore.matches(".rustycode/").count();
        assert_eq!(count, 1, "Should not duplicate .rustycode/ entry");
    }

    #[test]
    fn test_ensure_gitignore_skips_non_git_repo() {
        let dir = tempfile::tempdir().unwrap();
        // No .git directory — should not create .gitignore

        ensure_gitignore(dir.path());

        assert!(!dir.path().join(".gitignore").exists());
    }
}

#[cfg(test)]
mod integration_test {
    use super::*;

    #[test]
    fn test_loads_user_config_from_home_dir() {
        let home = std::env::var("HOME").unwrap();
        let config_path = Path::new(&home).join(".rustycode/config.json");
        if !config_path.exists() {
            eprintln!("Skipping: no ~/.rustycode/config.json");
            return;
        }

        let project_dir = Path::new("/Users/nat/dev/rustycode");
        let mut loader = ConfigLoader::new();
        let value = loader.load(project_dir).unwrap();

        eprintln!("Merged model: {:?}", value.get("model"));
        eprintln!("Merged provider: {:?}", value.get("provider"));

        let config = Config::load(project_dir).unwrap();
        eprintln!("Config.model: {}", config.model);
        eprintln!("Config.provider: {}", config.provider);

        // Verify config loaded successfully with non-empty values
        assert!(
            !config.model.is_empty(),
            "model should be populated from config"
        );
        assert!(
            !config.provider.is_empty(),
            "provider should be populated from config"
        );
    }
}
