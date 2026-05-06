use crate::app::pipeline::manifest::{Manifest, StepDefinition};
use crate::app::pipeline::tool_registry::ToolRegistry;
use anyhow::{anyhow, Result};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use super::artifact_registry::ArtifactRegistry;
use super::types::{Artifact, ArtifactQuery};
use tokio::sync::Mutex;

/// Represents a signal that a step needs or provides.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct Signal(pub String);

/// Defines how a dependency blocks the pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum BlockingType {
    Hard, // Halts the entire pipeline
    Soft, // Proceeds with degraded mode
}

/// A requirement for a pipeline step.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Dependency {
    pub signal: Signal,
    pub blocking: BlockingType,
}

use async_trait::async_trait;
use rustycode_agent_runtime::AgentConfig;
use rustycode_llm::provider::LLMProvider;

#[async_trait]
pub trait PipelineStep: Send + Sync {
    fn name(&self) -> String;
    fn dependencies(&self) -> Vec<Dependency>;
    fn provides(&self) -> Vec<Signal>;
    async fn execute(&self, ctx: &mut PipelineContext) -> Result<()>;
}

/// Shared context for the pipeline execution.
#[non_exhaustive]
pub struct PipelineContext {
    pub signals: HashSet<Signal>,
    pub artifact_registry: Arc<Mutex<ArtifactRegistry>>,
    pub provider: Arc<dyn LLMProvider>,
    pub agent_config: AgentConfig,
    pub current_model: String,
    pub agent_tool_registry: ToolRegistry,
}

impl PipelineContext {
    pub fn new(
        provider: Arc<dyn LLMProvider>,
        agent_config: AgentConfig,
        current_model: String,
        agent_tool_registry: ToolRegistry,
    ) -> Self {
        Self {
            signals: HashSet::new(),
            artifact_registry: Arc::new(Mutex::new(ArtifactRegistry::new())),
            provider,
            agent_config,
            current_model,
            agent_tool_registry,
        }
    }

    pub async fn query_artifacts(&self, q: &ArtifactQuery) -> Result<Vec<Artifact>> {
        let registry = self.artifact_registry.lock().await;
        registry.query(q).await
    }

    pub async fn register_artifact(&self, artifact: Artifact) -> Result<()> {
        let registry = self.artifact_registry.lock().await;
        registry.register(artifact).await
    }
}

/// A factory for creating pipeline steps from strings.
pub trait StepFactory: Send + Sync {
    fn create(&self, step: &StepDefinition) -> Arc<dyn PipelineStep>;
}

#[non_exhaustive]
pub struct PipelineRegistry {
    steps: Vec<Arc<dyn PipelineStep>>,
    factories: HashMap<String, Box<dyn StepFactory>>,
    pub tool_registry: ToolRegistry,
}

impl Default for PipelineRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl PipelineRegistry {
    pub fn new() -> Self {
        Self {
            steps: Vec::new(),
            factories: HashMap::new(),
            tool_registry: ToolRegistry::new(),
        }
    }

    pub fn register_factory(&mut self, name: &str, factory: Box<dyn StepFactory>) {
        self.factories.insert(name.to_string(), factory);
    }

    pub fn load_from_manifest(&mut self, manifest: &Manifest) -> Result<()> {
        for phase in &manifest.phases {
            if let Some(steps) = &phase.steps {
                for step_def in steps {
                    if let Some(factory) = self.factories.get(&step_def.implementation) {
                        let step = factory.create(step_def);
                        self.steps.push(step);
                    } else {
                        return Err(anyhow!(
                            "Factory not found for: {}",
                            step_def.implementation
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    /// Attempts to execute all possible steps based on current signal availability.
    pub async fn run_available(&mut self, ctx: &mut PipelineContext) -> Result<usize> {
        let mut executed_count = 0;
        let mut steps_to_run: Vec<Arc<dyn PipelineStep>> = Vec::new();

        for step in &self.steps {
            let deps = step.dependencies();
            let can_run = deps
                .iter()
                .all(|d| ctx.signals.contains(&d.signal) || d.blocking == BlockingType::Soft);

            if can_run {
                steps_to_run.push(step.clone());
            }
        }

        for step in steps_to_run {
            tracing::info!("Executing pipeline step: {}", step.name());
            step.execute(ctx).await?;
            for sig in step.provides() {
                ctx.signals.insert(sig);
            }
            executed_count += 1;
        }

        Ok(executed_count)
    }
}
