//! Integration tests for `ExecutionRouter`
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::doc_markdown)]

mod common;

use common::{
    make_agent_unit, make_direct_only_unit, make_input, make_knowledge_only_unit, make_skill_unit,
    make_tool_unit, TagAgentExecutor, TagDirectExecutor, TagSkillBundler,
};
use rustycode_executable::{
    ExecutableError, ExecutableRegistry, ExecutionContext, ExecutionRouter,
};
use std::sync::Arc;

fn setup_router() -> (Arc<ExecutableRegistry>, ExecutionRouter) {
    let registry = Arc::new(ExecutableRegistry::new());
    let router = ExecutionRouter::new_with_defaults(registry.clone());
    (registry, router)
}

/// Build a router with tagged executors so we can verify which handler was selected.
fn setup_tagged_router() -> (Arc<ExecutableRegistry>, ExecutionRouter) {
    let registry = Arc::new(ExecutableRegistry::new());
    let router = ExecutionRouter::new(
        registry.clone(),
        Arc::new(TagDirectExecutor),
        Arc::new(TagSkillBundler),
        Arc::new(TagAgentExecutor),
    );
    (registry, router)
}

#[tokio::test]
async fn execute_unregistered_unit_returns_not_found() {
    let (_registry, router) = setup_router();
    let input = make_input(serde_json::json!({"cmd": "ls"}));
    let context = ExecutionContext::DirectTool {
        immediate_result: true,
        timeout_ms: Some(30_000),
    };

    let result = router.execute("nonexistent", input, context).await;
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), ExecutableError::NotFound(_)));
}

#[tokio::test]
async fn execute_skill_context_requires_knowledge_capability() {
    let (registry, router) = setup_router();
    registry.register(make_tool_unit("tool_only")).unwrap();

    let input = make_input(serde_json::json!({}));
    let context = ExecutionContext::SkillReference {
        discoverable: true,
        cacheable: true,
    };

    let result = router.execute("tool_only", input, context).await;
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        ExecutableError::UnsupportedContext { .. }
    ));
}

#[tokio::test]
async fn execute_agent_context_requires_reasoning_capability() {
    let (registry, router) = setup_router();
    registry.register(make_tool_unit("no_reason")).unwrap();

    let input = make_input(serde_json::json!({}));
    let context = ExecutionContext::AgentReasoning {
        autonomous: true,
        max_steps: Some(5),
        can_delegate: false,
    };

    let result = router.execute("no_reason", input, context).await;
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        ExecutableError::UnsupportedContext { .. }
    ));
}

#[tokio::test]
async fn skill_unit_accepts_skill_context() {
    let (registry, router) = setup_router();
    registry.register(make_skill_unit("my_skill")).unwrap();

    let input = make_input(serde_json::json!({}));
    let context = ExecutionContext::SkillReference {
        discoverable: true,
        cacheable: false,
    };

    // Unit has can_bundle_knowledge = true, context is supported
    // Default stub executor returns NotFound
    let result = router.execute("my_skill", input, context).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn execute_hybrid_selects_direct_for_tool() {
    let (registry, router) = setup_router();
    registry.register(make_tool_unit("direct_tool")).unwrap();

    let input = make_input(serde_json::json!({}));
    let result = router.execute_hybrid("direct_tool", input).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn execute_hybrid_selects_agent_for_autonomous_unit() {
    let (registry, router) = setup_router();
    registry
        .register(make_agent_unit("autonomous_agent"))
        .unwrap();

    let input = make_input(serde_json::json!({}));
    let result = router.execute_hybrid("autonomous_agent", input).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn programmatic_call_context_uses_direct_executor() {
    let (registry, router) = setup_router();
    registry.register(make_tool_unit("prog_tool")).unwrap();

    let input = make_input(serde_json::json!({"val": 42}));
    let context = ExecutionContext::ProgrammaticCall {
        chain_position: Some(0),
        passthrough: true,
    };

    let result = router.execute("prog_tool", input, context).await;
    assert!(result.is_err());
}

// Phase 5: Hybrid mode tests for ExecutionRouter
//
// These tests verify that `execute_hybrid()` selects the correct execution
// path based on unit capabilities and execution_strategy, and that context
// validation properly gates access based on declared capabilities.

/// Verify that a tool-only unit (Direct strategy, can_execute_directly only)
/// routes to the DirectExecutor in hybrid mode.
#[tokio::test]
async fn hybrid_selects_direct_for_tool_only_unit() {
    let (registry, router) = setup_tagged_router();
    registry
        .register(make_direct_only_unit("bash_tool"))
        .unwrap();

    let input = make_input(serde_json::json!({"cmd": "ls"}));
    let result = router.execute_hybrid("bash_tool", input).await;

    let output = result.expect("tool-only unit should succeed via direct executor");
    assert_eq!(output.data["handler"], "direct");
}

/// Verify that an autonomous unit (Autonomous strategy, can_reason_autonomously)
/// routes to the AgentExecutor in hybrid mode.
#[tokio::test]
async fn hybrid_selects_agent_for_autonomous_unit() {
    let (registry, router) = setup_tagged_router();
    registry.register(make_agent_unit("code_reviewer")).unwrap();

    let input = make_input(serde_json::json!({"task": "review"}));
    let result = router.execute_hybrid("code_reviewer", input).await;

    let output = result.expect("autonomous unit should succeed via agent executor");
    assert_eq!(output.data["handler"], "agent");
}

/// Verify that a knowledge-only unit (no direct, no reasoning, only knowledge)
/// routes to the SkillBundler in hybrid mode.
#[tokio::test]
async fn hybrid_selects_skill_for_knowledge_only_unit() {
    let (registry, router) = setup_tagged_router();
    registry
        .register(make_knowledge_only_unit("doc_skill"))
        .unwrap();

    let input = make_input(serde_json::json!({"topic": "rust"}));
    let result = router.execute_hybrid("doc_skill", input).await;

    let output = result.expect("knowledge-only unit should succeed via skill bundler");
    assert_eq!(output.data["handler"], "skill");
}

/// Verify that the standard skill unit (can_execute_directly=true,
/// can_bundle_knowledge=true, strategy=Bundled) routes to the direct
/// executor because can_execute_directly takes precedence in select_context
/// when the unit is NOT autonomous.
#[tokio::test]
async fn hybrid_skill_unit_prefers_direct_over_skill() {
    let (registry, router) = setup_tagged_router();
    // make_skill_unit has can_execute_directly=true AND can_bundle_knowledge=true
    // but execution_strategy is Bundled (not Autonomous).
    // select_context checks autonomous first (false), then can_execute_directly (true).
    registry.register(make_skill_unit("hybrid_skill")).unwrap();

    let input = make_input(serde_json::json!({}));
    let result = router.execute_hybrid("hybrid_skill", input).await;

    let output = result.expect("skill unit with can_execute_directly should succeed");
    // Goes to direct because can_execute_directly=true and strategy is not Autonomous
    assert_eq!(output.data["handler"], "direct");
}

/// Verify that execute_hybrid returns NotFound for an unregistered unit id.
#[tokio::test]
async fn hybrid_returns_not_found_for_unregistered_unit() {
    let (registry, router) = setup_tagged_router();
    // Register something else so the registry is not empty
    registry.register(make_tool_unit("other")).unwrap();

    let input = make_input(serde_json::json!({}));
    let result = router.execute_hybrid("ghost_unit", input).await;

    assert!(result.is_err());
    match result.unwrap_err() {
        ExecutableError::NotFound(id) => assert_eq!(id, "ghost_unit"),
        other => panic!("expected NotFound, got: {other}"),
    }
}

/// Verify that a tool-only unit (can_execute_directly only) rejects
/// AgentReasoning context because it lacks can_reason_autonomously.
#[tokio::test]
async fn tool_unit_rejects_agent_reasoning_context() {
    let (registry, router) = setup_tagged_router();
    registry
        .register(make_direct_only_unit("strict_tool"))
        .unwrap();

    let input = make_input(serde_json::json!({}));
    let context = ExecutionContext::AgentReasoning {
        autonomous: true,
        max_steps: Some(5),
        can_delegate: false,
    };

    let result = router.execute("strict_tool", input, context).await;
    assert!(result.is_err());
    match result.unwrap_err() {
        ExecutableError::UnsupportedContext { unit, .. } => {
            assert_eq!(unit, "strict_tool");
        }
        other => panic!("expected UnsupportedContext, got: {other}"),
    }
}

/// Verify that a tool-only unit rejects SkillReference context
/// because it lacks can_bundle_knowledge.
#[tokio::test]
async fn tool_only_unit_rejects_skill_context() {
    let (registry, router) = setup_tagged_router();
    registry
        .register(make_direct_only_unit("no_knowledge_tool"))
        .unwrap();

    let input = make_input(serde_json::json!({}));
    let context = ExecutionContext::SkillReference {
        discoverable: true,
        cacheable: true,
    };

    let result = router.execute("no_knowledge_tool", input, context).await;
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        ExecutableError::UnsupportedContext { .. }
    ));
}

/// Verify that a knowledge-only unit rejects DirectTool context
/// because it lacks can_execute_directly.
#[tokio::test]
async fn knowledge_only_unit_rejects_direct_context() {
    let (registry, router) = setup_tagged_router();
    registry
        .register(make_knowledge_only_unit("pure_skill"))
        .unwrap();

    let input = make_input(serde_json::json!({}));
    let context = ExecutionContext::DirectTool {
        immediate_result: true,
        timeout_ms: Some(5_000),
    };

    let result = router.execute("pure_skill", input, context).await;
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        ExecutableError::UnsupportedContext { .. }
    ));
}

/// Verify that a knowledge-only unit rejects AgentReasoning context
/// because it lacks can_reason_autonomously.
#[tokio::test]
async fn knowledge_only_unit_rejects_agent_context() {
    let (registry, router) = setup_tagged_router();
    registry
        .register(make_knowledge_only_unit("no_reason_skill"))
        .unwrap();

    let input = make_input(serde_json::json!({}));
    let context = ExecutionContext::AgentReasoning {
        autonomous: false,
        max_steps: None,
        can_delegate: false,
    };

    let result = router.execute("no_reason_skill", input, context).await;
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        ExecutableError::UnsupportedContext { .. }
    ));
}

/// Verify that an agent unit accepts DirectTool context because
/// can_execute_directly is true for agent units.
#[tokio::test]
async fn agent_unit_accepts_direct_context() {
    let (registry, router) = setup_tagged_router();
    registry
        .register(make_agent_unit("flexible_agent"))
        .unwrap();

    let input = make_input(serde_json::json!({"action": "run"}));
    let context = ExecutionContext::DirectTool {
        immediate_result: true,
        timeout_ms: None,
    };

    let result = router.execute("flexible_agent", input, context).await;
    let output = result.expect("agent unit should accept DirectTool context");
    assert_eq!(output.data["handler"], "direct");
}

/// Verify that an agent unit accepts SkillReference context because
/// can_bundle_knowledge is true for agent units.
#[tokio::test]
async fn agent_unit_accepts_skill_context() {
    let (registry, router) = setup_tagged_router();
    registry
        .register(make_agent_unit("knowledgeable_agent"))
        .unwrap();

    let input = make_input(serde_json::json!({}));
    let context = ExecutionContext::SkillReference {
        discoverable: true,
        cacheable: false,
    };

    let result = router.execute("knowledgeable_agent", input, context).await;
    let output = result.expect("agent unit should accept SkillReference context");
    assert_eq!(output.data["handler"], "skill");
}

/// Verify that an agent unit accepts AgentReasoning context because
/// can_reason_autonomously is true for agent units.
#[tokio::test]
async fn agent_unit_accepts_agent_context() {
    let (registry, router) = setup_tagged_router();
    registry.register(make_agent_unit("full_agent")).unwrap();

    let input = make_input(serde_json::json!({}));
    let context = ExecutionContext::AgentReasoning {
        autonomous: true,
        max_steps: Some(10),
        can_delegate: true,
    };

    let result = router.execute("full_agent", input, context).await;
    let output = result.expect("agent unit should accept AgentReasoning context");
    assert_eq!(output.data["handler"], "agent");
}

/// Verify that ProgrammaticCall context requires can_execute_directly
/// and is rejected by a knowledge-only unit.
#[tokio::test]
async fn programmatic_call_rejected_by_knowledge_only_unit() {
    let (registry, router) = setup_tagged_router();
    registry
        .register(make_knowledge_only_unit("doc_only"))
        .unwrap();

    let input = make_input(serde_json::json!({}));
    let context = ExecutionContext::ProgrammaticCall {
        chain_position: None,
        passthrough: false,
    };

    let result = router.execute("doc_only", input, context).await;
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        ExecutableError::UnsupportedContext { .. }
    ));
}

/// Verify that ProgrammaticCall context is accepted by a unit with
/// can_execute_directly capability.
#[tokio::test]
async fn programmatic_call_accepted_by_direct_unit() {
    let (registry, router) = setup_tagged_router();
    registry
        .register(make_direct_only_unit("callable_tool"))
        .unwrap();

    let input = make_input(serde_json::json!({"x": 1}));
    let context = ExecutionContext::ProgrammaticCall {
        chain_position: Some(2),
        passthrough: true,
    };

    let result = router.execute("callable_tool", input, context).await;
    let output = result.expect("direct unit should accept ProgrammaticCall");
    assert_eq!(output.data["handler"], "direct");
}
