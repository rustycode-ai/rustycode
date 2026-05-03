//! End-to-end integration tests for the unified callable abstraction
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::doc_markdown)]

mod common;

use async_trait::async_trait;
use common::{make_input, make_tool_unit, EchoCallable, FixedCallable};
use rustycode_executable::router::{AgentExecutor, DirectExecutor, SkillBundler};
use rustycode_executable::{
    AdvancedToolMetadata, CallChain, ExecutableError, ExecutableRegistry, ExecutableUnit,
    ExecutionCapability, ExecutionContext, ExecutionInput, ExecutionMode, ExecutionOutput,
    ExecutionRouter, UnitCapabilities, UnitSource,
};
use std::sync::Arc;

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

fn setup_full_router() -> (Arc<ExecutableRegistry>, ExecutionRouter) {
    let registry = Arc::new(ExecutableRegistry::new());
    let router = ExecutionRouter::new(
        registry.clone(),
        Arc::new(DelegatingDirectExecutor),
        Arc::new(StubSkillBundler),
        Arc::new(StubAgentExecutor),
    );
    (registry, router)
}

#[tokio::test]
async fn full_pipeline_register_route_execute() {
    let (registry, router) = setup_full_router();
    registry.register(make_tool_unit("echo")).unwrap();

    let input = make_input(serde_json::json!({"msg": "hello world"}));
    let context = ExecutionContext::DirectTool {
        immediate_result: true,
        timeout_ms: Some(5_000),
    };

    let result = router.execute("echo", input, context).await.unwrap();
    assert_eq!(result.data["msg"], "hello world");
    assert!(!result.metadata.was_cached);
}

#[tokio::test]
async fn full_pipeline_discover_then_execute() {
    let (registry, router) = setup_full_router();
    registry.register(make_tool_unit("search")).unwrap();

    let search_svc = rustycode_executable::ToolSearchService::new(registry.clone());
    let results = search_svc
        .search(
            "search",
            rustycode_executable::discovery::ToolSearchOptions::default(),
        )
        .await
        .unwrap();

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, "search");

    let input = make_input(serde_json::json!({"query": "test"}));
    let context = ExecutionContext::DirectTool {
        immediate_result: true,
        timeout_ms: None,
    };
    let result = router.execute("search", input, context).await.unwrap();
    assert_eq!(result.data["query"], "test");
}

#[tokio::test]
async fn call_chain_executes_sequence() {
    let (registry, router) = setup_full_router();

    let unit_a = ExecutableUnit {
        id: "step_a".to_string(),
        name: "Step A".to_string(),
        description: "First step".to_string(),
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
        handler: Arc::new(FixedCallable {
            value: serde_json::json!({"result": "from_a"}),
        }),
        source: UnitSource::NativeTool {
            path: "a".to_string(),
        },
        schema: None,
        tags: vec![],
        version: None,
    };

    let unit_b = ExecutableUnit {
        id: "step_b".to_string(),
        name: "Step B".to_string(),
        description: "Second step".to_string(),
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
        handler: Arc::new(EchoCallable),
        source: UnitSource::NativeTool {
            path: "b".to_string(),
        },
        schema: None,
        tags: vec![],
        version: None,
    };

    registry.register(unit_a).unwrap();
    registry.register(unit_b).unwrap();

    let chain = CallChain::new().then("step_a").then_with_prev("step_b");

    let initial_input = make_input(serde_json::json!({"start": true}));
    let result = chain.execute(&router, initial_input).await.unwrap();

    assert_eq!(result.outputs.len(), 2);
    assert_eq!(result.outputs[0].data["result"], "from_a");
    assert_eq!(result.outputs[1].data["result"], "from_a");
    assert_eq!(result.final_output.data["result"], "from_a");
}

#[tokio::test]
async fn call_chain_empty_fails() {
    let (registry, router) = setup_full_router();
    registry.register(make_tool_unit("dummy")).unwrap();

    let chain = CallChain::new();
    let input = make_input(serde_json::json!({}));
    let result = chain.execute(&router, input).await;

    assert!(result.is_err());
}

#[tokio::test]
async fn multiple_contexts_same_unit() {
    let (registry, router) = setup_full_router();

    let mut unit = make_tool_unit("multi");
    unit.capabilities.can_bundle_knowledge = true;
    unit.capabilities.can_reason_autonomously = true;
    registry.register(unit).unwrap();

    let input = make_input(serde_json::json!({"x": 1}));

    // Direct context works (delegating executor)
    let ctx_direct = ExecutionContext::DirectTool {
        immediate_result: true,
        timeout_ms: None,
    };
    let result = router.execute("multi", input.clone(), ctx_direct).await;
    assert!(result.is_ok());

    // ProgrammaticCall also routes to direct executor
    let ctx_prog = ExecutionContext::ProgrammaticCall {
        chain_position: None,
        passthrough: false,
    };
    let result = router.execute("multi", input, ctx_prog).await;
    assert!(result.is_ok());
}

#[test]
fn capability_check_enforcement() {
    // Tool unit: can_execute_directly=true, can_bundle=false, can_reason=false
    let tool_unit = make_tool_unit("basic_tool");
    assert!(tool_unit.capabilities.can_execute_directly);
    assert!(!tool_unit.capabilities.can_bundle_knowledge);
    assert!(!tool_unit.capabilities.can_reason_autonomously);

    let direct_ctx = ExecutionContext::DirectTool {
        immediate_result: true,
        timeout_ms: None,
    };
    assert_eq!(
        direct_ctx.requires_capability(),
        ExecutionCapability::DirectExecution
    );

    let skill_ctx = ExecutionContext::SkillReference {
        discoverable: true,
        cacheable: true,
    };
    assert_eq!(
        skill_ctx.requires_capability(),
        ExecutionCapability::Knowledge
    );

    let agent_ctx = ExecutionContext::AgentReasoning {
        autonomous: false,
        max_steps: None,
        can_delegate: false,
    };
    assert_eq!(
        agent_ctx.requires_capability(),
        ExecutionCapability::Reasoning
    );
}
