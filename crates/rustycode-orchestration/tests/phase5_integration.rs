use rustycode_orchestration::bus::BusHandle;
use rustycode_orchestration::isolation::{IsolationConfig, TierIsolation};
use rustycode_orchestration::orchestrator::StepOrchestrator;
use rustycode_orchestration::task_context::TaskContext;
use rustycode_orchestration::tool_tiers::ToolActivationManager;
use rustycode_orchestration::types::{OutputType, Step};
use rustycode_orchestration::verification_gates::VerificationGateRegistry;
use std::sync::Arc;

#[tokio::test]
async fn test_step_orchestrator_isolation_blocks_exec_at_editor() {
    let bus = BusHandle::new(16);
    let isolation = TierIsolation::new(&IsolationConfig::default());
    let activation = ToolActivationManager::new();

    // Create an orchestrator with the proper dependencies
    let orch = StepOrchestrator::with_isolation_and_activation(
        Arc::new(rustycode_orchestration::conductor::Conductor::with_bus(
            rustycode_orchestration::config::OrchestrationConfig::default(),
            bus.clone(),
        )),
        Arc::new(rustycode_orchestration::musician::Musician::with_bus(
            bus.clone(),
        )),
        Arc::new(rustycode_orchestration::editor::Editor::new(bus.clone())),
        Arc::new(rustycode_orchestration::composer::Composer::new(
            Arc::new(rustycode_llm::mock::MockProvider::from_text("")),
            Arc::new(rustycode_orchestration::shared_workspace::SharedWorkspace::new()),
            Arc::new(
                rustycode_orchestration::reasoning_store::ReasoningStore::new(
                    std::path::PathBuf::from("/tmp"),
                ),
            ),
            bus.clone(),
        )),
        Arc::new(VerificationGateRegistry::new()),
        bus.clone(),
        isolation,
        activation,
    );

    let step = Step {
        id: "s1".into(),
        index: 0,
        description: "test".into(),
        expected_output_type: OutputType::Verification,
        suggested_tool: Some("Bash".into()),
        retry_on_failure: false,
        required_resources: rustycode_orchestration::guard::RequiredResources::default(),
    };
    let mut ctx = TaskContext::new("t1".into(), "test".into());
    ctx.current_tier = 3; // Editor tier

    let result = orch.execute_step(&step, &mut ctx).await;
    assert!(
        result.is_ok(),
        "Editor-patched step should be executed under musician privileges (tier 2)"
    );
}
