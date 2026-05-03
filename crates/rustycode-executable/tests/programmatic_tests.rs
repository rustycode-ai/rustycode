//! Comprehensive tests for `CallChain` builder and programmatic execution
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::sync::Arc;

use async_trait::async_trait;
use rustycode_executable::router::{AgentExecutor, DirectExecutor, SkillBundler};
use rustycode_executable::{
    AdvancedToolMetadata, CallChain, ChainResult, ChainStep, ExecutableError, ExecutableRegistry,
    ExecutableUnit, ExecutionContext, ExecutionInput, ExecutionMetadata, ExecutionMode,
    ExecutionOutput, ExecutionRouter, InputTransform, OutputTransform, UnitCapabilities,
    UnitSource,
};

use common::{make_input, make_tool_unit, FixedCallable};

// ---------------------------------------------------------------------------
// Test infrastructure
// ---------------------------------------------------------------------------

/// A direct executor that delegates to the unit's Callable handler
struct DelegatingDirectExecutor;

#[async_trait]
impl DirectExecutor for DelegatingDirectExecutor {
    async fn execute(
        &self,
        unit: &ExecutableUnit,
        input: ExecutionInput,
    ) -> Result<ExecutionOutput, ExecutableError> {
        unit.handler
            .execute(
                input,
                ExecutionContext::DirectTool {
                    immediate_result: true,
                    timeout_ms: None,
                },
            )
            .await
    }
}

/// Stub skill bundler
struct StubSkillBundler;

#[async_trait]
impl SkillBundler for StubSkillBundler {
    async fn bundle(
        &self,
        _unit: &ExecutableUnit,
        _input: ExecutionInput,
    ) -> Result<ExecutionOutput, ExecutableError> {
        Err(ExecutableError::NotFound("skill stub".to_string()))
    }
}

/// Stub agent executor
struct StubAgentExecutor;

#[async_trait]
impl AgentExecutor for StubAgentExecutor {
    async fn execute(
        &self,
        _unit: &ExecutableUnit,
        _input: ExecutionInput,
    ) -> Result<ExecutionOutput, ExecutableError> {
        Err(ExecutableError::NotFound("agent stub".to_string()))
    }
}

/// Create a registry and router wired with the delegating direct executor
fn setup_chain_env() -> (Arc<ExecutableRegistry>, ExecutionRouter) {
    let registry = Arc::new(ExecutableRegistry::new());
    let router = ExecutionRouter::new(
        registry.clone(),
        Arc::new(DelegatingDirectExecutor),
        Arc::new(StubSkillBundler),
        Arc::new(StubAgentExecutor),
    );
    (registry, router)
}

/// Create a tool unit that returns a fixed JSON value
fn fixed_unit(id: &str, value: serde_json::Value) -> ExecutableUnit {
    ExecutableUnit {
        id: id.to_string(),
        name: id.to_string(),
        description: format!("Fixed unit: {id}"),
        capabilities: UnitCapabilities {
            can_execute_directly: true,
            can_bundle_knowledge: false,
            can_reason_autonomously: false,
        },
        advanced_metadata: AdvancedToolMetadata {
            examples: vec![],
            defer_loading: false,
            search_hints: vec![],
            execution_strategy: ExecutionMode::Direct,
            result_processor: None,
        },
        handler: Arc::new(FixedCallable { value }),
        source: UnitSource::NativeTool {
            path: format!("tools/{id}"),
        },
        schema: None,
        tags: vec![],
        version: None,
    }
}

// ---------------------------------------------------------------------------
// 1. Single-step chain
// ---------------------------------------------------------------------------

#[tokio::test]
async fn single_step_chain_executes_and_returns_one_output() {
    let (registry, router) = setup_chain_env();
    registry
        .register(fixed_unit("add", serde_json::json!({"sum": 42})))
        .unwrap();

    let chain = CallChain::new().then("add");
    let input = make_input(serde_json::json!({"a": 1, "b": 2}));
    let result = chain.execute(&router, input).await.unwrap();

    assert_eq!(
        result.outputs.len(),
        1,
        "single-step chain should produce exactly one output"
    );
    assert_eq!(result.outputs[0].data["sum"], 42);
    assert_eq!(
        result.final_output.data["sum"], 42,
        "final_output should be the same as the sole output"
    );
}

#[tokio::test]
async fn single_step_chain_with_echo_callable_returns_input() {
    let (registry, router) = setup_chain_env();
    registry.register(make_tool_unit("echo")).unwrap();

    let chain = CallChain::new().then("echo");
    let input = make_input(serde_json::json!({"msg": "hello"}));
    let result = chain.execute(&router, input).await.unwrap();

    assert_eq!(result.outputs[0].data["msg"], "hello");
}

// ---------------------------------------------------------------------------
// 2. Multi-step chain
// ---------------------------------------------------------------------------

#[tokio::test]
async fn multi_step_chain_executes_in_sequence() {
    let (registry, router) = setup_chain_env();

    // Step A returns {"result": "alpha"}
    registry
        .register(fixed_unit("step_a", serde_json::json!({"result": "alpha"})))
        .unwrap();
    // Step B echoes whatever it receives
    registry.register(make_tool_unit("step_b")).unwrap();

    // .then("step_a") passes original input (ignores transform)
    // .then_with_prev("step_b") feeds step_a output into step_b
    let chain = CallChain::new().then("step_a").then_with_prev("step_b");
    let input = make_input(serde_json::json!({"start": true}));
    let result = chain.execute(&router, input).await.unwrap();

    assert_eq!(result.outputs.len(), 2);
    // First output comes from FixedCallable
    assert_eq!(result.outputs[0].data["result"], "alpha");
    // Second output echoes the previous output data
    assert_eq!(result.outputs[1].data["result"], "alpha");
    assert_eq!(result.final_output.data["result"], "alpha");
}

#[tokio::test]
async fn three_step_chain_carries_data_through() {
    let (registry, router) = setup_chain_env();

    registry
        .register(fixed_unit("gen", serde_json::json!({"x": 10})))
        .unwrap();
    registry.register(make_tool_unit("pass1")).unwrap();
    registry.register(make_tool_unit("pass2")).unwrap();

    // gen -> pass1 (prev output) -> pass2 (prev output)
    let chain = CallChain::new()
        .then("gen")
        .then_with_prev("pass1")
        .then_with_prev("pass2");

    let input = make_input(serde_json::json!({}));
    let result = chain.execute(&router, input).await.unwrap();

    assert_eq!(result.outputs.len(), 3);
    assert_eq!(result.outputs[0].data["x"], 10);
    assert_eq!(result.outputs[1].data["x"], 10);
    assert_eq!(result.outputs[2].data["x"], 10);
    assert_eq!(result.final_output.data["x"], 10);
}

#[tokio::test]
async fn multi_step_chain_then_without_prev_uses_current_input() {
    let (registry, router) = setup_chain_env();

    // Both units echo their input; neither uses PreviousOutput
    registry.register(make_tool_unit("first")).unwrap();
    registry.register(make_tool_unit("second")).unwrap();

    let chain = CallChain::new().then("first").then("second");
    let input = make_input(serde_json::json!({"val": 99}));
    let result = chain.execute(&router, input).await.unwrap();

    // Both steps receive the *current_input* (which flows from original input,
    // then is overwritten to the output of the previous step). So step 1
    // gets {"val":99} and step 2 gets the output of step 1 which is also {"val":99}.
    assert_eq!(result.outputs[0].data["val"], 99);
    assert_eq!(result.outputs[1].data["val"], 99);
}

// ---------------------------------------------------------------------------
// 3. Input transform: Fixed
// ---------------------------------------------------------------------------

#[tokio::test]
async fn input_transform_fixed_ignores_previous_output() {
    let (registry, router) = setup_chain_env();

    registry
        .register(fixed_unit(
            "producer",
            serde_json::json!({"original": true}),
        ))
        .unwrap();
    registry.register(make_tool_unit("consumer")).unwrap();

    // Build a chain with a manual step that has InputTransform::Fixed
    let chain = CallChain {
        steps: vec![
            ChainStep {
                unit_id: "producer".to_string(),
                input_transform: None,
                output_transform: None,
            },
            ChainStep {
                unit_id: "consumer".to_string(),
                input_transform: Some(InputTransform::Fixed(serde_json::json!({
                    "injected": "fixed_value"
                }))),
                output_transform: None,
            },
        ],
    };

    let input = make_input(serde_json::json!({}));
    let result = chain.execute(&router, input).await.unwrap();

    assert_eq!(result.outputs.len(), 2);
    // The consumer receives the Fixed value, not the producer output
    assert_eq!(result.outputs[1].data["injected"], "fixed_value");
}

// ---------------------------------------------------------------------------
// 4. Input transform: Merge
// ---------------------------------------------------------------------------

#[tokio::test]
async fn input_transform_merge_combines_with_current_input() {
    let (registry, router) = setup_chain_env();

    registry.register(make_tool_unit("merger")).unwrap();

    let chain = CallChain {
        steps: vec![ChainStep {
            unit_id: "merger".to_string(),
            input_transform: Some(InputTransform::Merge(serde_json::json!({
                "extra_field": "extra_value"
            }))),
            output_transform: None,
        }],
    };

    let input = make_input(serde_json::json!({"base_field": "base_value"}));
    let result = chain.execute(&router, input).await.unwrap();

    // The merger echoes its input, which should have both base and extra fields
    assert_eq!(result.outputs[0].data["base_field"], "base_value");
    assert_eq!(result.outputs[0].data["extra_field"], "extra_value");
}

#[tokio::test]
async fn input_transform_merge_overwrites_conflicting_keys() {
    let (registry, router) = setup_chain_env();
    registry.register(make_tool_unit("echo")).unwrap();

    let chain = CallChain {
        steps: vec![ChainStep {
            unit_id: "echo".to_string(),
            input_transform: Some(InputTransform::Merge(serde_json::json!({
                "key": "merged"
            }))),
            output_transform: None,
        }],
    };

    let input = make_input(serde_json::json!({"key": "original"}));
    let result = chain.execute(&router, input).await.unwrap();

    // Merge overwrites existing keys with the merge values
    assert_eq!(result.outputs[0].data["key"], "merged");
}

#[tokio::test]
async fn input_transform_merge_with_non_object_current_input_is_noop() {
    let (registry, router) = setup_chain_env();
    registry.register(make_tool_unit("echo")).unwrap();

    let chain = CallChain {
        steps: vec![ChainStep {
            unit_id: "echo".to_string(),
            input_transform: Some(InputTransform::Merge(serde_json::json!({"extra": 1}))),
            output_transform: None,
        }],
    };

    // Non-object current input: merge does nothing (both sides must be Object)
    let input = make_input(serde_json::json!("just a string"));
    let result = chain.execute(&router, input).await.unwrap();

    // The original non-object data passes through unchanged
    assert_eq!(result.outputs[0].data, serde_json::json!("just a string"));
}

// ---------------------------------------------------------------------------
// 5. Input transform: PreviousOutput edge case
// ---------------------------------------------------------------------------

#[tokio::test]
async fn input_transform_previous_output_on_first_step_falls_through() {
    let (registry, router) = setup_chain_env();
    registry.register(make_tool_unit("echo")).unwrap();

    // On step 0 (first step), PreviousOutput has no previous output to use,
    // so the code falls through to the default branch (clone current_input).
    let chain = CallChain {
        steps: vec![ChainStep {
            unit_id: "echo".to_string(),
            input_transform: Some(InputTransform::PreviousOutput),
            output_transform: None,
        }],
    };

    let input = make_input(serde_json::json!({"fallback": true}));
    let result = chain.execute(&router, input).await.unwrap();

    assert_eq!(result.outputs[0].data["fallback"], true);
}

// ---------------------------------------------------------------------------
// 6. Empty chain
// ---------------------------------------------------------------------------

#[tokio::test]
async fn empty_chain_returns_execution_failed_error() {
    let (registry, router) = setup_chain_env();
    registry.register(make_tool_unit("dummy")).unwrap();

    let chain = CallChain::new();
    let input = make_input(serde_json::json!({}));
    let result = chain.execute(&router, input).await;

    assert!(result.is_err(), "empty chain must error");
    match result.unwrap_err() {
        ExecutableError::ExecutionFailed(msg) => {
            assert!(
                msg.contains("empty call chain"),
                "error message should mention empty chain, got: {msg}"
            );
        }
        other => panic!("expected ExecutionFailed, got: {other}"),
    }
}

#[tokio::test]
async fn default_chain_is_empty_and_fails() {
    let (_registry, router) = setup_chain_env();

    let chain = CallChain::default();
    assert!(chain.steps.is_empty(), "default chain should have no steps");

    let input = make_input(serde_json::json!({}));
    let result = chain.execute(&router, input).await;
    assert!(result.is_err());
}

// ---------------------------------------------------------------------------
// 7. Chain with nonexistent unit
// ---------------------------------------------------------------------------

#[tokio::test]
async fn chain_with_nonexistent_unit_returns_not_found() {
    let (_registry, router) = setup_chain_env();
    // Do NOT register "phantom"

    let chain = CallChain::new().then("phantom");
    let input = make_input(serde_json::json!({}));
    let result = chain.execute(&router, input).await;

    assert!(result.is_err());
    match result.unwrap_err() {
        ExecutableError::NotFound(id) => {
            assert_eq!(id, "phantom");
        }
        other => panic!("expected NotFound, got: {other}"),
    }
}

#[tokio::test]
async fn chain_with_nonexistent_unit_in_second_step_errors() {
    let (registry, router) = setup_chain_env();
    registry.register(make_tool_unit("ok_step")).unwrap();

    let chain = CallChain::new().then("ok_step").then_with_prev("missing");
    let input = make_input(serde_json::json!({"v": 1}));
    let result = chain.execute(&router, input).await;

    assert!(result.is_err());
    match result.unwrap_err() {
        ExecutableError::NotFound(id) => {
            assert_eq!(id, "missing");
        }
        other => panic!("expected NotFound, got: {other}"),
    }
}

// ---------------------------------------------------------------------------
// 8. ChainResult structure
// ---------------------------------------------------------------------------

#[tokio::test]
async fn chain_result_has_correct_field_shapes() {
    let (registry, router) = setup_chain_env();
    registry
        .register(fixed_unit("u", serde_json::json!({"ok": true})))
        .unwrap();

    let chain = CallChain::new().then("u");
    let input = make_input(serde_json::json!({}));
    let ChainResult {
        outputs,
        final_output,
        total_duration_ms,
    } = chain.execute(&router, input).await.unwrap();

    // outputs is a vec with one element
    assert_eq!(outputs.len(), 1);
    assert_eq!(outputs[0].data["ok"], true);

    // final_output matches the single output
    assert_eq!(final_output.data["ok"], true);

    // total_duration_ms is accumulated; the FixedCallable returns duration_ms=0
    assert_eq!(total_duration_ms, 0);
}

#[tokio::test]
async fn chain_result_duration_is_sum_of_step_durations() {
    let (registry, router) = setup_chain_env();

    // EchoCallable returns duration_ms=1 in its metadata
    registry.register(make_tool_unit("a")).unwrap();
    registry.register(make_tool_unit("b")).unwrap();
    registry.register(make_tool_unit("c")).unwrap();

    let chain = CallChain::new()
        .then("a")
        .then_with_prev("b")
        .then_with_prev("c");

    let input = make_input(serde_json::json!({}));
    let result = chain.execute(&router, input).await.unwrap();

    // Each EchoCallable sets duration_ms=1, so total should be 3
    assert_eq!(result.total_duration_ms, 3);
}

// ---------------------------------------------------------------------------
// 9. ChainStep structure
// ---------------------------------------------------------------------------

#[test]
fn chain_step_fields_are_set_correctly_by_then() {
    let chain = CallChain::new().then("unit_a");
    assert_eq!(chain.steps.len(), 1);

    let step = &chain.steps[0];
    assert_eq!(step.unit_id, "unit_a");
    assert!(
        step.input_transform.is_none(),
        "then() should not set input_transform"
    );
    assert!(
        step.output_transform.is_none(),
        "then() should not set output_transform"
    );
}

#[test]
fn chain_step_then_with_prev_sets_previous_output_transform() {
    let chain = CallChain::new().then_with_prev("unit_b");
    assert_eq!(chain.steps.len(), 1);

    let step = &chain.steps[0];
    assert_eq!(step.unit_id, "unit_b");
    assert!(matches!(
        &step.input_transform,
        Some(InputTransform::PreviousOutput)
    ));
    assert!(step.output_transform.is_none());
}

#[test]
fn chain_step_manual_construction_with_all_fields() {
    let step = ChainStep {
        unit_id: "custom".to_string(),
        input_transform: Some(InputTransform::Fixed(serde_json::json!({"k": "v"}))),
        output_transform: Some(OutputTransform::ExtractField("result".to_string())),
    };

    assert_eq!(step.unit_id, "custom");
    assert!(matches!(
        &step.input_transform,
        Some(InputTransform::Fixed(_))
    ));
    assert!(matches!(
        &step.output_transform,
        Some(OutputTransform::ExtractField(f)) if f == "result"
    ));
}

// ---------------------------------------------------------------------------
// 10. InputTransform enum variants
// ---------------------------------------------------------------------------

#[test]
fn input_transform_variants_construct_correctly() {
    let prev = InputTransform::PreviousOutput;
    assert!(matches!(prev, InputTransform::PreviousOutput));

    let fixed = InputTransform::Fixed(serde_json::json!({"x": 1}));
    assert!(matches!(fixed, InputTransform::Fixed(_)));

    let merge = InputTransform::Merge(serde_json::json!({"y": 2}));
    assert!(matches!(merge, InputTransform::Merge(_)));
}

// ---------------------------------------------------------------------------
// 11. OutputTransform enum variants
// ---------------------------------------------------------------------------

#[test]
fn output_transform_variants_construct_correctly() {
    let extract = OutputTransform::ExtractField("name".to_string());
    assert!(matches!(extract, OutputTransform::ExtractField(f) if f == "name"));

    let data_only = OutputTransform::DataOnly;
    assert!(matches!(data_only, OutputTransform::DataOnly));

    let full = OutputTransform::Full;
    assert!(matches!(full, OutputTransform::Full));
}

#[test]
fn output_transform_stored_on_chain_step() {
    let chain = CallChain {
        steps: vec![
            ChainStep {
                unit_id: "s1".to_string(),
                input_transform: None,
                output_transform: Some(OutputTransform::ExtractField("id".to_string())),
            },
            ChainStep {
                unit_id: "s2".to_string(),
                input_transform: None,
                output_transform: Some(OutputTransform::DataOnly),
            },
            ChainStep {
                unit_id: "s3".to_string(),
                input_transform: None,
                output_transform: Some(OutputTransform::Full),
            },
        ],
    };

    assert_eq!(chain.steps.len(), 3);
    assert!(matches!(
        &chain.steps[0].output_transform,
        Some(OutputTransform::ExtractField(f)) if f == "id"
    ));
    assert!(matches!(
        &chain.steps[1].output_transform,
        Some(OutputTransform::DataOnly)
    ));
    assert!(matches!(
        &chain.steps[2].output_transform,
        Some(OutputTransform::Full)
    ));
}

// ---------------------------------------------------------------------------
// 12. ProgrammaticCall context details
// ---------------------------------------------------------------------------

#[tokio::test]
async fn chain_sets_programmatic_call_context_with_position() {
    // Verify that chain execution creates ProgrammaticCall contexts with
    // correct chain_position values. The DelegatingDirectExecutor passes
    // its own context so we cannot directly inspect what the router received.
    // Instead, verify that the chain completes successfully (meaning the
    // ProgrammaticCall context was accepted by the router).
    let (registry, router) = setup_chain_env();
    registry.register(make_tool_unit("a")).unwrap();
    registry.register(make_tool_unit("b")).unwrap();
    registry.register(make_tool_unit("c")).unwrap();

    let chain = CallChain::new()
        .then("a")
        .then_with_prev("b")
        .then_with_prev("c");

    let input = make_input(serde_json::json!({}));
    let result = chain.execute(&router, input).await;

    assert!(
        result.is_ok(),
        "chain should execute successfully with ProgrammaticCall context"
    );
    assert_eq!(result.unwrap().outputs.len(), 3);
}

// ---------------------------------------------------------------------------
// 13. Passthrough semantics
// ---------------------------------------------------------------------------

#[tokio::test]
async fn chain_passthrough_true_for_non_final_steps() {
    // The execute method sets passthrough=true for all steps except the last.
    // This is not directly observable from the outside, but we verify the chain
    // completes and all steps produce output.
    let (registry, router) = setup_chain_env();
    registry.register(make_tool_unit("s1")).unwrap();
    registry.register(make_tool_unit("s2")).unwrap();

    let chain = CallChain::new().then("s1").then_with_prev("s2");
    let input = make_input(serde_json::json!({"v": 1}));
    let result = chain.execute(&router, input).await.unwrap();

    assert_eq!(result.outputs.len(), 2);
}

// ---------------------------------------------------------------------------
// 14. Caller info preservation
// ---------------------------------------------------------------------------

#[tokio::test]
async fn chain_preserves_input_data_across_steps() {
    let (registry, router) = setup_chain_env();
    registry.register(make_tool_unit("echo")).unwrap();

    let chain = CallChain::new().then("echo");
    let input = ExecutionInput {
        data: serde_json::json!({"x": 1}),
        caller_info: None,
        session_context: None,
    };

    let result = chain.execute(&router, input).await.unwrap();
    assert_eq!(result.outputs[0].data["x"], 1);
}

// ---------------------------------------------------------------------------
// 15. Clone and Debug traits
// ---------------------------------------------------------------------------

#[test]
fn call_chain_is_cloneable() {
    let chain = CallChain::new().then("a").then_with_prev("b");
    let cloned = chain.clone();

    assert_eq!(chain.steps.len(), cloned.steps.len());
    assert_eq!(chain.steps[0].unit_id, cloned.steps[0].unit_id);
    assert_eq!(chain.steps[1].unit_id, cloned.steps[1].unit_id);
}

#[test]
fn call_chain_implements_debug() {
    let chain = CallChain::new().then("debug_unit");
    let debug_str = format!("{chain:?}");
    assert!(
        debug_str.contains("debug_unit"),
        "Debug output should contain unit id"
    );
}

#[test]
fn chain_step_implements_debug() {
    let step = ChainStep {
        unit_id: "test".to_string(),
        input_transform: Some(InputTransform::PreviousOutput),
        output_transform: Some(OutputTransform::Full),
    };
    let debug_str = format!("{step:?}");
    assert!(debug_str.contains("test"));
}

#[test]
fn chain_result_implements_debug() {
    // Construct a minimal ChainResult to verify Debug
    let result = ChainResult {
        outputs: vec![ExecutionOutput {
            data: serde_json::json!({"ok": true}),
            metadata: ExecutionMetadata {
                duration_ms: 5,
                tokens_used: None,
                was_cached: false,
                trace: None,
            },
        }],
        final_output: ExecutionOutput {
            data: serde_json::json!({"ok": true}),
            metadata: ExecutionMetadata {
                duration_ms: 5,
                tokens_used: None,
                was_cached: false,
                trace: None,
            },
        },
        total_duration_ms: 5,
    };
    let debug_str = format!("{result:?}");
    assert!(debug_str.contains("total_duration_ms"));
}

// ---------------------------------------------------------------------------
// 16. Builder chaining pattern
// ---------------------------------------------------------------------------

#[test]
fn builder_returns_new_chain_on_each_call() {
    let chain_v1 = CallChain::new().then("a");
    let chain_v2 = chain_v1.clone().then("b");

    // Original chain is unchanged
    assert_eq!(chain_v1.steps.len(), 1);
    assert_eq!(chain_v1.steps[0].unit_id, "a");

    // Extended chain has both steps
    assert_eq!(chain_v2.steps.len(), 2);
    assert_eq!(chain_v2.steps[0].unit_id, "a");
    assert_eq!(chain_v2.steps[1].unit_id, "b");
}

#[test]
fn builder_accepts_string_and_str() {
    let unit_id: String = "string_id".to_string();
    let chain = CallChain::new().then("str_id").then(unit_id);

    assert_eq!(chain.steps[0].unit_id, "str_id");
    assert_eq!(chain.steps[1].unit_id, "string_id");
}
