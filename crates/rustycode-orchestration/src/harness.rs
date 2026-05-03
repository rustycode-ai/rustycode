//! Tiered execution harness for orchestration.
//!
//! Provides automated escalation pathways from Tier 2 (Musician) → Tier 3 (Editor) → Tier 4 (Composer).

use crate::config::{
    BudgetConfig, DryRunConfig, FailureStoreConfig, HallucinationConfig, OrchestrationConfig,
};
use crate::pipeline::{OrchestrationPipeline, TaskResult};
use crate::task_context::TaskComplexity;

/// Result of a tiered execution attempt.
#[derive(Debug)]
pub struct TieredExecutionResult {
    pub success: bool,
    pub output: String,
    pub total_cost_usd: f64,
    pub tier_used: u8,
    pub steps_completed: usize,
    pub escalation_occurred: bool,
}

impl From<TaskResult> for TieredExecutionResult {
    fn from(result: TaskResult) -> Self {
        match result {
            TaskResult::Success {
                output,
                total_cost,
                tier_used,
                steps_completed,
                ..
            } => Self {
                success: true,
                output,
                total_cost_usd: total_cost,
                tier_used,
                steps_completed,
                escalation_occurred: tier_used > 2,
            },
            TaskResult::Failed {
                reason,
                total_cost,
                steps_completed,
            } => Self {
                success: false,
                output: reason,
                total_cost_usd: total_cost,
                tier_used: 0,
                steps_completed,
                escalation_occurred: true,
            },
        }
    }
}

pub struct TieredHarness {
    pub budget_usd: f64,
    provider: Option<std::sync::Arc<dyn rustycode_llm::provider::LLMProvider>>,
    model: Option<String>,
}

impl TieredHarness {
    pub const fn new(budget_usd: f64) -> Self {
        Self {
            budget_usd,
            provider: None,
            model: None,
        }
    }

    pub fn with_provider(
        mut self,
        provider: std::sync::Arc<dyn rustycode_llm::provider::LLMProvider>,
        model: impl Into<String>,
    ) -> Self {
        self.provider = Some(provider);
        self.model = Some(model.into());
        self
    }

    pub fn execute(
        &self,
        task_id: String,
        task: String,
        _complexity: TaskComplexity,
    ) -> anyhow::Result<TieredExecutionResult> {
        let config = OrchestrationConfig {
            models: std::collections::HashMap::new(),
            escalation: std::collections::HashMap::new(),
            budget: BudgetConfig {
                total_max_usd: self.budget_usd,
                tier_2_max_usd: self.budget_usd * 0.3,
                tier_3_max_usd: self.budget_usd * 0.4,
                tier_4_max_usd: self.budget_usd * 0.3,
                warn_threshold_pct: 0.8,
                burst_enabled_for: vec![],
                burst_multiplier: 1.0,
            },
            hallucination: HallucinationConfig {
                detection_window: 3,
                action: "escalate".into(),
            },
            failure_store: FailureStoreConfig {
                backend: "memory".into(),
                path: ":memory:".into(),
                retention_days: 30,
                promotion_threshold: 5,
            },
            dry_run: DryRunConfig::default(),
            autonomy: crate::autonomy::AutonomyConfig::default(),
            parallel_execution: crate::config::ParallelExecutionConfig::default(),
            prompt_caching: crate::config::PromptCachingConfig::default(),
            streaming_results: true,
        };

        let pipeline = match (&self.provider, &self.model) {
            (Some(provider), Some(model)) => {
                OrchestrationPipeline::with_provider_and_model(config, provider.clone(), model)
            }
            _ => OrchestrationPipeline::new(config),
        };

        let result: TaskResult =
            run_blocking(async move { pipeline.conduct(task_id, task).await })?;
        Ok(result.into())
    }
}

fn run_blocking<F, T, E>(fut: F) -> std::result::Result<T, E>
where
    F: std::future::Future<Output = std::result::Result<T, E>>,
{
    let make_run = || -> std::result::Result<T, E> {
        #[allow(clippy::expect_used)]
        let rt = tokio::runtime::Runtime::new()
            .expect("failed to create tokio runtime for tiered execution");
        rt.block_on(fut)
    };

    if tokio::runtime::Handle::try_current().is_ok() {
        tokio::task::block_in_place(make_run)
    } else {
        make_run()
    }
}
