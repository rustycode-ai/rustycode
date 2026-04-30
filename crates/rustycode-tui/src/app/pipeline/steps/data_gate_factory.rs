use crate::app::pipeline::manifest::StepDefinition;
use crate::app::pipeline::registry::{PipelineStep, StepFactory};
use crate::app::pipeline::steps::data_gate::DataGateStep;
use std::sync::Arc;

pub struct DataGateFactory;

impl StepFactory for DataGateFactory {
    fn create(&self, _step: &StepDefinition) -> Arc<dyn PipelineStep> {
        Arc::new(DataGateStep)
    }
}
