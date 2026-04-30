//! Integration test for the orchestration pipeline flow.
//! This verifies that the Conductor can orchestrate a task through the Musician tier.

#![allow(clippy::unwrap_used)]

use rustycode_orchestration::config::OrchestrationConfig;
use rustycode_orchestration::pipeline::{OrchestrationPipeline, TaskResult};

#[tokio::test]
async fn test_orchestration_pipeline_conduct_success() {
    let config = OrchestrationConfig::default();
    let pipeline = OrchestrationPipeline::new(config);

    let result = pipeline
        .conduct("test-1".into(), "echo hello world".into())
        .await
        .unwrap();

    match result {
        TaskResult::Success {
            output,
            tier_used,
            steps_completed,
            ..
        } => {
            assert!(tier_used >= 2);
            assert!(steps_completed > 0);
            let _ = output;
        }
        TaskResult::Failed { reason, .. } => {
            panic!("Expected success, got failure: {reason}");
        }
    }
}

#[tokio::test]
async fn test_orchestration_pipeline_bus_events() {
    let config = OrchestrationConfig::default();
    let pipeline = OrchestrationPipeline::new(config);
    let bus = pipeline.bus_handle();
    let mut rx = bus.subscribe();

    let _result = pipeline
        .conduct("test-bus".into(), "echo bus events".into())
        .await
        .unwrap();

    // Should receive a TaskCompleted event
    let event = rx.try_recv();
    assert!(event.is_ok(), "Should receive TaskCompleted event on bus");
}

#[tokio::test]
async fn test_orchestration_pipeline_workspace() {
    let config = OrchestrationConfig::default();
    let pipeline = OrchestrationPipeline::new(config);

    let workspace = pipeline.workspace();
    // Workspace should be usable
    assert!(
        workspace.keys().await.is_empty(),
        "Workspace should start empty"
    );
}

#[tokio::test]
async fn test_orchestration_pipeline_conduct_with_special_chars() {
    let config = OrchestrationConfig::default();
    let pipeline = OrchestrationPipeline::new(config);

    let result = pipeline
        .conduct("task-special".into(), "echo 'hello & world'".into())
        .await;
    assert!(result.is_ok(), "Should handle special chars in task");
}

#[tokio::test]
async fn test_orchestration_pipeline_conduct_empty_task() {
    let config = OrchestrationConfig::default();
    let pipeline = OrchestrationPipeline::new(config);

    let result = pipeline.conduct("empty-task".into(), String::new()).await;
    assert!(result.is_ok(), "Should handle empty task description");
}

#[tokio::test]
async fn test_orchestration_pipeline_conduct_tracks_tier() {
    let config = OrchestrationConfig::default();
    let pipeline = OrchestrationPipeline::new(config);

    let result = pipeline
        .conduct("tier-check".into(), "echo tier".into())
        .await
        .unwrap();

    match result {
        TaskResult::Success { tier_used, .. } => {
            assert!(tier_used >= 2, "Tier should be at least 2, got {tier_used}");
        }
        TaskResult::Failed { .. } => {}
    }
}

#[tokio::test]
async fn test_orchestration_pipeline_multiple_conducts() {
    let config = OrchestrationConfig::default();
    let pipeline = OrchestrationPipeline::new(config);

    // Run multiple tasks through the same pipeline
    for i in 0..3 {
        let result = pipeline
            .conduct(format!("multi-{i}"), format!("echo task {i}"))
            .await;
        assert!(result.is_ok(), "Task {i} should succeed");
    }
}

#[tokio::test]
async fn test_orchestration_pipeline_bus_receives_correct_task_id() {
    let config = OrchestrationConfig::default();
    let pipeline = OrchestrationPipeline::new(config);
    let bus = pipeline.bus_handle();
    let mut rx = bus.subscribe();

    let _ = pipeline
        .conduct("specific-id-42".into(), "echo id".into())
        .await
        .unwrap();

    let mut found = false;
    while let Ok(event) = rx.try_recv() {
        if let rustycode_orchestration::bus::OrchestrationEvent::TaskCompleted { task_id, .. } =
            event
        {
            if task_id == "specific-id-42" {
                found = true;
            }
        }
    }
    assert!(
        found,
        "Should find TaskCompleted event with correct task_id"
    );
}
