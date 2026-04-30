use crate::app::pipeline::registry::{Dependency, PipelineContext, PipelineStep, Signal};
use anyhow::Result;
use async_trait::async_trait;

pub struct LoadWorkflowStep;

#[async_trait]
impl PipelineStep for LoadWorkflowStep {
    fn name(&self) -> String {
        "Phase 1: Load Workflow".to_string()
    }

    fn dependencies(&self) -> Vec<Dependency> {
        vec![]
    }

    fn provides(&self) -> Vec<Signal> {
        vec![Signal("WORKFLOW_LOADED".to_string())]
    }

    async fn execute(&self, _ctx: &mut PipelineContext) -> Result<()> {
        tracing::info!("Phase 1: Workflow loaded.");
        Ok(())
    }
}
