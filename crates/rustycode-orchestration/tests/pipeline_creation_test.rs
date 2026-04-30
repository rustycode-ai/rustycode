//! Integration tests for orchestration pipeline creation and execution.

use rustycode_orchestration::{config::OrchestrationConfig, pipeline::OrchestrationPipeline};

#[test]
fn test_orchestration_pipeline_creation() {
    let config = OrchestrationConfig::default();
    let pipeline = OrchestrationPipeline::new(config);

    // Verify core components are accessible
    let _orchestrator = pipeline.orchestrator();
    let _workspace = pipeline.workspace();
    let _bus = pipeline.bus_handle();
}
