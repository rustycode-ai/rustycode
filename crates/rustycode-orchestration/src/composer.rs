//! Composer orchestrates reasoning strategies and persists thinking processes.
//!
//! Integrates deep thinking, strategy selection, and structured reasoning
//! to solve complex, multi-phase tasks.

use crate::bus::BusHandle;
use crate::isolation::TierIsolation;
use crate::reasoning_store::ReasoningStore;
use crate::shared_workspace::SharedWorkspace;
use crate::task_context::TaskContext;
use crate::thinking::executor::RealExecutor;
use crate::thinking::ThinkingExecutor;
use crate::types::{OutputType, Step};
use anyhow::Result;
use std::sync::Arc;

pub struct Composer {
    executor: Arc<dyn ThinkingExecutor>,
    #[allow(dead_code)]
    workspace: Arc<SharedWorkspace>,
    store: Arc<ReasoningStore>,
    #[allow(dead_code)]
    bus: BusHandle,
    isolation: Arc<tokio::sync::RwLock<TierIsolation>>,
}

impl Composer {
    pub fn new(
        llm_provider: Arc<dyn rustycode_llm::provider::LLMProvider>,
        workspace: Arc<SharedWorkspace>,
        store: Arc<ReasoningStore>,
        bus: BusHandle,
    ) -> Self {
        Self {
            executor: Arc::new(RealExecutor::new(llm_provider)),
            workspace,
            store,
            bus,
            isolation: Arc::new(tokio::sync::RwLock::new(TierIsolation::with_defaults())),
        }
    }

    pub fn with_model(
        llm_provider: Arc<dyn rustycode_llm::provider::LLMProvider>,
        workspace: Arc<SharedWorkspace>,
        store: Arc<ReasoningStore>,
        bus: BusHandle,
        model: &str,
    ) -> Self {
        Self {
            executor: Arc::new(RealExecutor::new(llm_provider).with_model(model)),
            workspace,
            store,
            bus,
            isolation: Arc::new(tokio::sync::RwLock::new(TierIsolation::with_defaults())),
        }
    }

    pub fn with_isolation(mut self, isolation: Arc<tokio::sync::RwLock<TierIsolation>>) -> Self {
        self.isolation = isolation;
        self
    }

    pub async fn compose_new_score(&self, ctx: &mut TaskContext) -> Result<Vec<Step>> {
        tracing::info!(task_id = %ctx.task_id, "Composer starting deep re-composition");

        let result = self.executor.think_with_context(ctx).await?;

        // Persist thoughts from the graph into the store
        if let Some(graph) = &ctx.reasoning_graph {
            for thought in graph.thoughts() {
                let structured = crate::types::StructuredThought {
                    thought: thought.content.clone(),
                    phase: u32::from(ctx.current_tier),
                    thought_type: crate::types::ThoughtType::Hypothesis,
                    references: vec![],
                    confidence: (thought.metadata.confidence * 100.0) as u32,
                    next_thought_needed: false,
                    branch_id: None,
                    metadata: crate::types::ThoughtMetadata::default(),
                };
                if let Err(e) =
                    self.store
                        .store_thought(&ctx.task_id, u32::from(ctx.current_tier), &structured)
                {
                    tracing::warn!(error = %e, "Failed to store composed thought");
                }
            }
        }

        Ok(vec![Step {
            id: format!("composed-{}", uuid::Uuid::new_v4()),
            index: 0,
            description: result,
            expected_output_type: OutputType::Code,
            suggested_tool: Some("bash".into()),
            retry_on_failure: true,
            required_resources: crate::guard::RequiredResources::default(),
        }])
    }
}
