use crate::autonomy::AutonomyConfig;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Dry-run execution modes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DryRunMode {
    /// Full execution (no dry-run)
    Disabled,
    /// Log intended tool calls without executing
    LogOnly,
    /// Execute locally available tools, skip external APIs
    LocalOnly,
    /// Validate plan structure without execution
    ValidationOnly,
}

impl DryRunMode {
    /// Returns true if tool execution should be skipped
    pub const fn skip_execution(&self) -> bool {
        !matches!(self, Self::Disabled)
    }

    /// Returns true if external API calls should be skipped
    pub const fn skip_external_apis(&self) -> bool {
        matches!(self, Self::LocalOnly | Self::ValidationOnly)
    }

    /// Returns true if only validation should be performed
    pub const fn validation_only(&self) -> bool {
        matches!(self, Self::ValidationOnly)
    }
}

/// Model configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    pub name: String,
    pub provider: String,
    pub cost_per_1m_tokens_input: f64,
    pub cost_per_1m_tokens_output: f64,
    pub context_window: usize,
    pub supports_extended_thinking: Option<bool>,
    pub max_thinking_tokens: Option<usize>,
}

/// Tier configuration for escalation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TierConfig {
    pub max_attempts: u8,
    pub critical_errors: Vec<crate::error_signal::SignalCategory>,
    pub recoverable_errors: Vec<crate::error_signal::SignalCategory>,
}

impl Default for TierConfig {
    fn default() -> Self {
        Self {
            max_attempts: 2,
            critical_errors: vec![],
            recoverable_errors: vec![],
        }
    }
}

/// Budget configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetConfig {
    pub total_max_usd: f64,
    pub tier_2_max_usd: f64,
    pub tier_3_max_usd: f64,
    pub tier_4_max_usd: f64,
    pub warn_threshold_pct: f64,
    pub burst_enabled_for: Vec<String>,
    pub burst_multiplier: f64,
}

/// Hallucination detection configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HallucinationConfig {
    pub detection_window: usize,
    pub action: String,
}

/// Failure store configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailureStoreConfig {
    pub backend: String,
    pub path: String,
    pub retention_days: u32,
    pub promotion_threshold: u32,
}

/// Parallel tool execution configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParallelExecutionConfig {
    pub enabled: bool,
    pub max_concurrent: usize,
}

impl Default for ParallelExecutionConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_concurrent: 3,
        }
    }
}

/// Prompt caching configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptCachingConfig {
    pub enabled: bool,
    pub cache_system_prompt: bool,
    pub cache_tool_definitions: bool,
}

impl Default for PromptCachingConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            cache_system_prompt: true,
            cache_tool_definitions: true,
        }
    }
}

/// Dry-run configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DryRunConfig {
    pub default_mode: DryRunMode,
    pub enabled_for_tasks: Vec<String>,
    pub log_level: String,
    pub collect_metrics: bool,
}

/// Main orchestration configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestrationConfig {
    pub models: HashMap<String, Vec<ModelConfig>>,
    pub escalation: HashMap<String, TierConfig>,
    pub budget: BudgetConfig,
    pub hallucination: HallucinationConfig,
    pub failure_store: FailureStoreConfig,
    pub dry_run: DryRunConfig,
    pub autonomy: AutonomyConfig,
    pub parallel_execution: ParallelExecutionConfig,
    pub prompt_caching: PromptCachingConfig,
    pub streaming_results: bool,
}

impl Default for OrchestrationConfig {
    fn default() -> Self {
        Self {
            models: HashMap::new(),
            escalation: HashMap::new(),
            budget: BudgetConfig::default(),
            hallucination: HallucinationConfig::default(),
            failure_store: FailureStoreConfig::default(),
            dry_run: DryRunConfig::default(),
            autonomy: AutonomyConfig::default(),
            parallel_execution: ParallelExecutionConfig::default(),
            prompt_caching: PromptCachingConfig::default(),
            streaming_results: true,
        }
    }
}

impl OrchestrationConfig {
    /// Load configuration from YAML file
    pub fn load_from_yaml(path: &str) -> Result<Self, crate::OrchestrationError> {
        let content = std::fs::read_to_string(path).map_err(|e| {
            crate::OrchestrationError::Configuration {
                message: e.to_string(),
            }
        })?;
        serde_yaml::from_str(&content).map_err(|e| crate::OrchestrationError::Configuration {
            message: e.to_string(),
        })
    }

    /// Check if dry-run mode should be enabled for a specific task type
    pub fn should_use_dry_run(&self, task_type: &str) -> DryRunMode {
        if self
            .dry_run
            .enabled_for_tasks
            .iter()
            .any(|t| t == task_type)
        {
            self.dry_run.default_mode
        } else {
            DryRunMode::Disabled
        }
    }
}

impl Default for DryRunConfig {
    fn default() -> Self {
        Self {
            default_mode: DryRunMode::Disabled,
            enabled_for_tasks: Vec::new(),
            log_level: "info".to_string(),
            collect_metrics: false,
        }
    }
}

impl Default for BudgetConfig {
    fn default() -> Self {
        Self {
            total_max_usd: 10.0,
            tier_2_max_usd: 2.0,
            tier_3_max_usd: 5.0,
            tier_4_max_usd: 10.0,
            warn_threshold_pct: 0.8,
            burst_enabled_for: Vec::new(),
            burst_multiplier: 1.5,
        }
    }
}

impl Default for HallucinationConfig {
    fn default() -> Self {
        Self {
            detection_window: 5,
            action: "escalate".to_string(),
        }
    }
}

impl Default for FailureStoreConfig {
    fn default() -> Self {
        Self {
            backend: "memory".to_string(),
            path: ":memory:".to_string(),
            retention_days: 30,
            promotion_threshold: 5,
        }
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_dry_run_mode_skip_execution() {
        assert!(!DryRunMode::Disabled.skip_execution());
        assert!(DryRunMode::LogOnly.skip_execution());
        assert!(DryRunMode::LocalOnly.skip_execution());
        assert!(DryRunMode::ValidationOnly.skip_execution());
    }

    #[test]
    fn test_default_config() {
        let config = OrchestrationConfig::default();
        assert_eq!(config.budget.total_max_usd, 10.0);
        assert_eq!(config.failure_store.backend, "memory");
        assert!(config.models.is_empty());
    }

    #[test]
    fn test_dry_run_enabled_for_task() {
        let mut config = OrchestrationConfig::default();
        config
            .dry_run
            .enabled_for_tasks
            .push("test-task".to_string());
        config.dry_run.default_mode = DryRunMode::LogOnly;

        assert_eq!(config.should_use_dry_run("test-task"), DryRunMode::LogOnly);
        assert_eq!(
            config.should_use_dry_run("other-task"),
            DryRunMode::Disabled
        );
    }

    #[test]
    fn test_dry_run_mode_flags() {
        assert!(DryRunMode::LogOnly.skip_execution());
        assert!(!DryRunMode::LogOnly.skip_external_apis());
        assert!(DryRunMode::LocalOnly.skip_execution());
        assert!(DryRunMode::LocalOnly.skip_external_apis());
        assert!(DryRunMode::ValidationOnly.validation_only());
        assert!(!DryRunMode::LogOnly.validation_only());
    }

    #[test]
    fn test_budget_defaults() {
        let budget = BudgetConfig::default();
        assert_eq!(budget.total_max_usd, 10.0);
        assert_eq!(budget.tier_2_max_usd, 2.0);
        assert_eq!(budget.tier_3_max_usd, 5.0);
        assert_eq!(budget.tier_4_max_usd, 10.0);
        assert_eq!(budget.warn_threshold_pct, 0.8);
        assert_eq!(budget.burst_multiplier, 1.5);
    }

    #[test]
    fn test_hallucination_defaults() {
        let cfg = HallucinationConfig::default();
        assert_eq!(cfg.detection_window, 5);
        assert_eq!(cfg.action, "escalate");
    }

    #[test]
    fn test_failure_store_defaults() {
        let cfg = FailureStoreConfig::default();
        assert_eq!(cfg.backend, "memory");
        assert_eq!(cfg.path, ":memory:");
        assert_eq!(cfg.retention_days, 30);
        assert_eq!(cfg.promotion_threshold, 5);
    }

    #[test]
    fn test_model_config_serialization() {
        let model = ModelConfig {
            name: "claude-sonnet".into(),
            provider: "anthropic".into(),
            cost_per_1m_tokens_input: 3.0,
            cost_per_1m_tokens_output: 15.0,
            context_window: 200_000,
            supports_extended_thinking: Some(true),
            max_thinking_tokens: Some(10_000),
        };
        let json = serde_json::to_string(&model).unwrap();
        let back: ModelConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.name, "claude-sonnet");
        assert_eq!(back.context_window, 200_000);
    }

    #[test]
    fn test_tier_config_default() {
        let tier = TierConfig::default();
        assert_eq!(tier.max_attempts, 2);
        assert!(tier.critical_errors.is_empty());
        assert!(tier.recoverable_errors.is_empty());
    }

    #[test]
    fn test_tier_config_serialization() {
        let tier = TierConfig {
            max_attempts: 3,
            critical_errors: vec![crate::error_signal::SignalCategory::LogicError],
            recoverable_errors: vec![crate::error_signal::SignalCategory::ToolTimeout],
        };
        let json = serde_json::to_string(&tier).unwrap();
        let back: TierConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.max_attempts, 3);
        assert_eq!(back.critical_errors.len(), 1);
    }

    #[test]
    fn test_dry_run_config_defaults() {
        let cfg = DryRunConfig::default();
        assert_eq!(cfg.default_mode, DryRunMode::Disabled);
        assert!(cfg.enabled_for_tasks.is_empty());
        assert_eq!(cfg.log_level, "info");
        assert!(!cfg.collect_metrics);
    }

    #[test]
    fn test_dry_run_mode_serialization_roundtrip() {
        for mode in [
            DryRunMode::Disabled,
            DryRunMode::LogOnly,
            DryRunMode::LocalOnly,
            DryRunMode::ValidationOnly,
        ] {
            let json = serde_json::to_string(&mode).unwrap();
            let back: DryRunMode = serde_json::from_str(&json).unwrap();
            assert_eq!(mode, back);
        }
    }

    #[test]
    fn test_orchestration_config_serialization() {
        let config = OrchestrationConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let back: OrchestrationConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.budget.total_max_usd, config.budget.total_max_usd);
        assert_eq!(back.failure_store.backend, config.failure_store.backend);
    }

    #[test]
    fn test_should_use_dry_run_empty_list() {
        let config = OrchestrationConfig::default();
        assert_eq!(config.should_use_dry_run("any-task"), DryRunMode::Disabled);
    }

    #[test]
    fn test_model_config_optional_fields_none() {
        let model = ModelConfig {
            name: "test".into(),
            provider: "test".into(),
            cost_per_1m_tokens_input: 0.0,
            cost_per_1m_tokens_output: 0.0,
            context_window: 4096,
            supports_extended_thinking: None,
            max_thinking_tokens: None,
        };
        assert!(model.supports_extended_thinking.is_none());
        assert!(model.max_thinking_tokens.is_none());
    }

    #[test]
    fn test_budget_config_serialization() {
        let budget = BudgetConfig::default();
        let json = serde_json::to_string(&budget).unwrap();
        let back: BudgetConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.total_max_usd, budget.total_max_usd);
        assert_eq!(back.burst_multiplier, budget.burst_multiplier);
    }

    #[test]
    fn test_load_from_yaml_nonexistent_file() {
        let result = OrchestrationConfig::load_from_yaml("/nonexistent/path.yaml");
        assert!(result.is_err());
    }

    #[test]
    fn test_config_has_default_autonomy() {
        let config = OrchestrationConfig::default();
        assert_eq!(
            config.autonomy.default_level,
            crate::autonomy::AutonomyLevel::L1,
            "Default autonomy should be L1 (ask permission)"
        );
    }

    #[test]
    fn test_parallel_execution_config_default() {
        let cfg = ParallelExecutionConfig::default();
        assert!(cfg.enabled);
        assert_eq!(cfg.max_concurrent, 3);
    }

    #[test]
    fn test_prompt_caching_config_default() {
        let cfg = PromptCachingConfig::default();
        assert!(cfg.enabled);
        assert!(cfg.cache_system_prompt);
        assert!(cfg.cache_tool_definitions);
    }

    #[test]
    fn test_optimization_config_defaults() {
        let config = OrchestrationConfig::default();
        assert!(config.parallel_execution.enabled);
        assert_eq!(config.parallel_execution.max_concurrent, 3);
        assert!(config.prompt_caching.enabled);
        assert!(config.prompt_caching.cache_system_prompt);
        assert!(config.prompt_caching.cache_tool_definitions);
        assert!(config.streaming_results);
    }
}
