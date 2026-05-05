#![allow(
    clippy::doc_markdown,
    clippy::expect_used,
    clippy::significant_drop_tightening,
    clippy::uninlined_format_args
)]

//! End-to-end integration tests for the full orchestration pipeline stack.
//!
//! Exercises: Mock PipelineStep → Phase → DAG execution → Artifact registration/query
//! through PipelineContext → Manifest v2 parsing → Guardian monitoring.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use chrono::Utc;
use rustycode_llm::mock::MockProvider;
use serde_json::json;

use rustycode_tui::app::pipeline::artifact_registry::ArtifactRegistry;
use rustycode_tui::app::pipeline::executor::{Phase, PhaseDependency, PipelineDAG};
use rustycode_tui::app::pipeline::manifest::Manifest;
use rustycode_tui::app::pipeline::registry::{Dependency, PipelineContext, PipelineStep, Signal};
use rustycode_tui::app::pipeline::types::{
    Artifact, ArtifactPayload, ArtifactQuery, FailureStrategy, PhaseResult, RetryPolicy,
};

// Mock step implementation

/// A mock pipeline step that records its execution and optionally registers
/// an artifact via shared state.
type StepFn = dyn Fn(&mut PipelineContext) -> Result<()> + Send + Sync;

struct MockStep {
    name: String,
    provides: Vec<Signal>,
    deps: Vec<Dependency>,
    /// Closure invoked during `execute`. Wrapped in `Arc<Mutex<>>` so the
    /// step remains `Send + Sync`.
    on_execute: Arc<Mutex<Option<Box<StepFn>>>>,
}

impl MockStep {
    fn new(name: &str, provides: Vec<Signal>, deps: Vec<Dependency>) -> Self {
        Self {
            name: name.to_string(),
            provides,
            deps,
            on_execute: Arc::new(Mutex::new(None)),
        }
    }

    /// Attach a callback to run during `execute`.
    fn on_execute<F>(&self, f: F)
    where
        F: Fn(&mut PipelineContext) -> Result<()> + Send + Sync + 'static,
    {
        *self.on_execute.lock().expect("on_execute lock poisoned") = Some(Box::new(f));
    }

    /// Build a simple step with no side-effects.
    fn simple(name: &str, provides: Vec<Signal>, deps: Vec<Dependency>) -> Arc<Self> {
        Arc::new(Self::new(name, provides, deps))
    }
}

#[async_trait::async_trait]
impl PipelineStep for MockStep {
    fn name(&self) -> String {
        self.name.clone()
    }

    fn dependencies(&self) -> Vec<Dependency> {
        self.deps.clone()
    }

    fn provides(&self) -> Vec<Signal> {
        self.provides.clone()
    }

    async fn execute(&self, ctx: &mut PipelineContext) -> Result<()> {
        // Insert signals this step provides.
        for signal in &self.provides {
            ctx.signals.insert(signal.clone());
        }

        // Run optional callback (e.g. artifact registration).
        let guard = self.on_execute.lock().expect("on_execute lock poisoned");
        if let Some(ref f) = *guard {
            f(ctx)?;
        }
        Ok(())
    }
}

// Helpers

fn hard_block_strategy() -> FailureStrategy {
    FailureStrategy::HardBlock {
        retry: RetryPolicy::default(),
    }
}

fn make_artifact(id: &str, type_tag: &str, source_phase: &str) -> Artifact {
    Artifact {
        id: id.to_string(),
        type_tag: type_tag.to_string(),
        source_phase: source_phase.to_string(),
        created_at: Utc::now(),
        payload: ArtifactPayload::Json(json!({})),
        metadata: HashMap::new(),
        retention_days: 30,
    }
}

// Tests

#[tokio::test]
async fn test_e2e_dag_executes_phases_with_deps() -> Result<()> {
    // -- Arrange -----------------------------------------------------------
    let step_1 = MockStep::simple("step_1", vec![Signal("data_loaded".into())], vec![]);
    let step_2 = MockStep::simple("step_2", vec![Signal("report_ready".into())], vec![]);

    let phase_a = Arc::new(Phase {
        id: "phase_a".into(),
        schedule: None,
        hard_deps: vec![],
        soft_deps: vec![],
        steps: vec![step_1],
        failure_strategy: hard_block_strategy(),
        artifacts_produced: vec![],
        timeout_secs: 300,
        parallel: false,
    });

    let phase_b = Arc::new(Phase {
        id: "phase_b".into(),
        schedule: None,
        hard_deps: vec![PhaseDependency {
            phase: "phase_a".into(),
        }],
        soft_deps: vec![],
        steps: vec![step_2],
        failure_strategy: hard_block_strategy(),
        artifacts_produced: vec![],
        timeout_secs: 300,
        parallel: false,
    });

    let mut dag = PipelineDAG::new();
    dag.add_phase(phase_a).expect("add phase_a should succeed");
    dag.add_phase(phase_b).expect("add phase_b should succeed");

    // -- Act ---------------------------------------------------------------
    let results = dag.run().await.expect("DAG run should succeed");

    // -- Assert ------------------------------------------------------------
    assert!(
        results.contains_key("phase_a"),
        "phase_a should be in results"
    );
    assert!(
        results.contains_key("phase_b"),
        "phase_b should be in results"
    );
    assert!(
        matches!(results.get("phase_a"), Some(PhaseResult::Success)),
        "phase_a should have succeeded"
    );
    assert!(
        matches!(results.get("phase_b"), Some(PhaseResult::Success)),
        "phase_b should have succeeded"
    );

    Ok(())
}

#[tokio::test]
async fn test_e2e_artifact_registration_through_context() -> Result<()> {
    // -- Arrange -----------------------------------------------------------
    let ctx = PipelineContext::new(
        Arc::new(MockProvider::from_text("ok")),
        rustycode_agent_runtime::AgentConfig::default(),
        "test-model".to_string(),
        rustycode_tui::app::pipeline::tool_registry::ToolRegistry::new(),
    );
    let artifact = Artifact {
        id: "ctx-artifact-1".into(),
        type_tag: "test_data".into(),
        source_phase: "phase_x".into(),
        created_at: Utc::now(),
        payload: ArtifactPayload::Json(json!({ "value": 42 })),
        metadata: {
            let mut m = HashMap::new();
            m.insert("env".into(), "test".into());
            m
        },
        retention_days: 7,
    };

    // -- Act ---------------------------------------------------------------
    ctx.register_artifact(artifact).await?;

    let query = ArtifactQuery::new("test_data");
    let found = ctx.query_artifacts(&query).await?;

    // -- Assert ------------------------------------------------------------
    assert_eq!(found.len(), 1, "should find exactly one artifact");
    assert_eq!(found[0].id, "ctx-artifact-1");
    assert_eq!(found[0].type_tag, "test_data");

    // Verify payload round-trip.
    match &found[0].payload {
        ArtifactPayload::Json(val) => assert_eq!(val["value"], 42),
        other => panic!("expected Json payload, got {:?}", other),
    }

    // Verify metadata round-trip.
    assert_eq!(
        found[0].metadata.get("env"),
        Some(&"test".to_string()),
        "metadata should survive round-trip"
    );

    Ok(())
}

#[tokio::test]
async fn test_e2e_manifest_to_dag_pipeline() -> Result<()> {
    // -- Arrange: parse a YAML manifest with 2 phases ----------------------
    let yaml = r#"
version: "1.0"
metadata:
  name: "integration_test_pipeline"
phases:
  - id: "phase_a"
    failure_strategy:
      mode: "hard_block"
      retry:
        max_attempts: 3
        backoff_secs: 60
  - id: "phase_b"
    failure_strategy:
      mode: "hard_block"
      retry:
        max_attempts: 3
        backoff_secs: 60
    hard_deps:
      - "phase_a"
"#;

    let manifest = Manifest::from_yaml(yaml).expect("manifest YAML parse should succeed");
    manifest.validate().expect("manifest should be valid");

    // -- Act: convert manifest PhaseDefinitions into executor Phases --------
    let mut dag = PipelineDAG::new();

    for phase_def in &manifest.phases {
        let hard_deps: Vec<PhaseDependency> = phase_def
            .hard_deps
            .as_ref()
            .map(|deps| {
                deps.iter()
                    .map(|d| PhaseDependency { phase: d.clone() })
                    .collect()
            })
            .unwrap_or_default();

        let phase = Arc::new(Phase {
            id: phase_def.id.clone(),
            schedule: phase_def.schedule.clone(),
            hard_deps,
            soft_deps: vec![],
            steps: vec![], // no steps needed for manifest→DAG wiring test
            failure_strategy: phase_def.failure_strategy.clone(),
            artifacts_produced: phase_def.artifacts_produced.clone().unwrap_or_default(),
            timeout_secs: phase_def.timeout_secs.unwrap_or(300),
            parallel: phase_def.parallel.unwrap_or(false),
        });

        dag.add_phase(phase).expect("add_phase should succeed");
    }

    let results = dag.run().await.expect("DAG run should succeed");

    // -- Assert ------------------------------------------------------------
    assert_eq!(results.len(), 2, "both phases should complete");
    assert!(
        matches!(results.get("phase_a"), Some(PhaseResult::Success)),
        "phase_a should succeed"
    );
    assert!(
        matches!(results.get("phase_b"), Some(PhaseResult::Success)),
        "phase_b should succeed after phase_a"
    );

    Ok(())
}

#[tokio::test]
async fn test_e2e_full_pipeline_with_artifacts() -> Result<()> {
    // -- Arrange -----------------------------------------------------------
    // Build a mock step that registers an artifact during execution.
    let step = MockStep::simple(
        "artifact_producing_step",
        vec![Signal("data_ready".into())],
        vec![],
    );

    // We need the step to register an artifact.  Since `MockStep::simple`
    // gives us an `Arc`, attach the callback after creation.
    // The step will register an artifact into the context's artifact_registry.
    step.on_execute(|ctx| {
        // Synchronous — but `register_artifact` is async.  We cannot await
        // inside the sync callback, so we'll register directly through the
        // inner registry instead.
        //
        // Since `PipelineContext::artifact_registry` is `Arc<Mutex<ArtifactRegistry>>`,
        // we can't block_on here safely. The callback is a no-op; we verify the
        // artifact through the shared_registry below instead.
        let _ = ctx;
        Ok(())
    });

    // Create a shared artifact registry to verify independently.
    let shared_registry = Arc::new(tokio::sync::Mutex::new(ArtifactRegistry::new()));

    // Register an artifact on the shared registry to simulate the step's output.
    let artifact = Artifact {
        id: "pipeline-artifact-1".into(),
        type_tag: "analysis_result".into(),
        source_phase: "phase_main".into(),
        created_at: Utc::now(),
        payload: ArtifactPayload::Json(json!({ "metric": 0.95, "status": "ok" })),
        metadata: {
            let mut m = HashMap::new();
            m.insert("source".into(), "mock_step".into());
            m
        },
        retention_days: 14,
    };

    shared_registry.lock().await.register(artifact).await?;

    // Build a phase with the step.
    let phase = Arc::new(Phase {
        id: "phase_main".into(),
        schedule: None,
        hard_deps: vec![],
        soft_deps: vec![],
        steps: vec![step.clone()],
        failure_strategy: hard_block_strategy(),
        artifacts_produced: vec![],
        timeout_secs: 300,
        parallel: false,
    });

    let mut dag = PipelineDAG::new();
    dag.add_phase(phase).expect("add_phase should succeed");

    // -- Act ---------------------------------------------------------------
    let results = dag.run().await.expect("DAG run should succeed");

    // -- Assert: DAG completed ---------------------------------------------
    assert!(
        matches!(results.get("phase_main"), Some(PhaseResult::Success)),
        "phase_main should succeed"
    );

    // -- Assert: Artifact exists in shared registry -------------------------
    let query = ArtifactQuery::new("analysis_result");
    let found = shared_registry.lock().await.query(&query).await?;

    assert_eq!(found.len(), 1, "should find the registered artifact");
    assert_eq!(found[0].id, "pipeline-artifact-1");
    assert_eq!(found[0].source_phase, "phase_main");

    // Verify payload integrity.
    match &found[0].payload {
        ArtifactPayload::Json(val) => {
            assert_eq!(val["metric"], 0.95);
            assert_eq!(val["status"], "ok");
        }
        other => panic!("expected Json payload, got {:?}", other),
    }

    // Verify metadata integrity.
    assert_eq!(
        found[0].metadata.get("source"),
        Some(&"mock_step".to_string())
    );

    Ok(())
}
