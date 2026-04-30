use crate::app::pipeline::manifest::StepDefinition;
use crate::app::pipeline::registry::{PipelineStep, StepFactory};
use crate::app::pipeline::steps::agent_step::AgentStep;
use std::sync::Arc;

pub struct AgentStepFactory;

impl StepFactory for AgentStepFactory {
    fn create(&self, step: &StepDefinition) -> Arc<dyn PipelineStep> {
        Arc::new(AgentStep::from_definition(step))
    }
}
