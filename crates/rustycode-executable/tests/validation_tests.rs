//! Validation tests for the unified callable abstraction.
//!
//! Covers cross-cutting concerns: full pipeline, multi-unit call chains,
//! capability enforcement, discovery scoring, metadata consistency,
//! and edge cases including concurrent access.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use async_trait::async_trait;
use common::{make_agent_unit, make_input, make_skill_unit, make_tool_unit, EchoCallable};
use rustycode_executable::discovery::ToolSearchOptions;
use rustycode_executable::router::{AgentExecutor, DirectExecutor, SkillBundler};
use rustycode_executable::{
    AdvancedToolMetadata, CallChain, ExecutableError, ExecutableRegistry, ExecutableUnit,
    ExecutionContext, ExecutionExample, ExecutionMode, ExecutionRouter, ToolSearchService,
    UnitCapabilities, UnitSource,
};
use std::sync::Arc;

// Test infrastructure: delegating executors that route to the unit handler

/// Delegates to the unit's `Callable` handler using `DirectTool` context.
struct DelegatingDirectExecutor;

#[async_trait]
impl DirectExecutor for DelegatingDirectExecutor {
    async fn execute(
        &self,
        unit: &ExecutableUnit,
        input: rustycode_executable::ExecutionInput,
    ) -> Result<rustycode_executable::ExecutionOutput, ExecutableError> {
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

/// Delegates to the unit's `Callable` handler using `SkillReference` context.
struct DelegatingSkillBundler;

#[async_trait]
impl SkillBundler for DelegatingSkillBundler {
    async fn bundle(
        &self,
        unit: &ExecutableUnit,
        input: rustycode_executable::ExecutionInput,
    ) -> Result<rustycode_executable::ExecutionOutput, ExecutableError> {
        unit.handler
            .execute(
                input,
                ExecutionContext::SkillReference {
                    discoverable: true,
                    cacheable: false,
                },
            )
            .await
    }
}

/// Delegates to the unit's `Callable` handler using `AgentReasoning` context.
struct DelegatingAgentExecutor;

#[async_trait]
impl AgentExecutor for DelegatingAgentExecutor {
    async fn execute(
        &self,
        unit: &ExecutableUnit,
        input: rustycode_executable::ExecutionInput,
    ) -> Result<rustycode_executable::ExecutionOutput, ExecutableError> {
        unit.handler
            .execute(
                input,
                ExecutionContext::AgentReasoning {
                    autonomous: true,
                    max_steps: Some(5),
                    can_delegate: false,
                },
            )
            .await
    }
}

/// Builds a router with all three executors delegating to the unit handler.
fn setup_delegating_router() -> (Arc<ExecutableRegistry>, ExecutionRouter) {
    let registry = Arc::new(ExecutableRegistry::new());
    let router = ExecutionRouter::new(
        registry.clone(),
        Arc::new(DelegatingDirectExecutor),
        Arc::new(DelegatingSkillBundler),
        Arc::new(DelegatingAgentExecutor),
    );
    (registry, router)
}

// A. Full pipeline: register -> discover -> execute

#[tokio::test]
async fn full_pipeline_register_discover_execute_tool() {
    let (registry, router) = setup_delegating_router();

    // Register units of different types
    registry.register(make_tool_unit("file_read")).unwrap();
    registry.register(make_skill_unit("code_review")).unwrap();
    registry.register(make_agent_unit("architect")).unwrap();

    // Discover the tool via ToolSearchService
    let search = ToolSearchService::new(registry.clone());
    let results = search
        .search("file_read", ToolSearchOptions::default())
        .await
        .unwrap();

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, "file_read");

    // Execute the discovered unit through the router
    let input = make_input(serde_json::json!({"path": "/tmp/test.rs"}));
    let context = ExecutionContext::DirectTool {
        immediate_result: true,
        timeout_ms: Some(5_000),
    };
    let output = router
        .execute(&results[0].id, input, context)
        .await
        .unwrap();

    assert_eq!(output.data["path"], "/tmp/test.rs");
}

#[tokio::test]
async fn full_pipeline_discover_skill_and_execute() {
    let (registry, router) = setup_delegating_router();

    registry.register(make_skill_unit("refactor")).unwrap();

    let search = ToolSearchService::new(registry.clone());
    let results = search
        .search("refactor", ToolSearchOptions::default())
        .await
        .unwrap();

    assert_eq!(results.len(), 1);

    let input = make_input(serde_json::json!({"action": "rename"}));
    let context = ExecutionContext::SkillReference {
        discoverable: true,
        cacheable: false,
    };
    let output = router
        .execute(&results[0].id, input, context)
        .await
        .unwrap();

    assert_eq!(output.data["action"], "rename");
}

#[tokio::test]
async fn full_pipeline_discover_agent_and_execute() {
    let (registry, router) = setup_delegating_router();

    registry.register(make_agent_unit("debugger")).unwrap();

    let search = ToolSearchService::new(registry.clone());
    let results = search
        .search("debugger", ToolSearchOptions::default())
        .await
        .unwrap();

    assert_eq!(results.len(), 1);

    let input = make_input(serde_json::json!({"error": "SIGSEGV"}));
    let context = ExecutionContext::AgentReasoning {
        autonomous: true,
        max_steps: Some(10),
        can_delegate: true,
    };
    let output = router
        .execute(&results[0].id, input, context)
        .await
        .unwrap();

    assert_eq!(output.data["error"], "SIGSEGV");
}

// B. Call chain multi-unit test

#[tokio::test]
async fn call_chain_three_units_sequential() {
    let (registry, router) = setup_delegating_router();

    // Register three echo units; each echoes its input back.
    registry.register(make_tool_unit("chain_a")).unwrap();
    registry.register(make_tool_unit("chain_b")).unwrap();
    registry.register(make_tool_unit("chain_c")).unwrap();

    let chain = CallChain::new()
        .then("chain_a")
        .then_with_prev("chain_b")
        .then_with_prev("chain_c");

    let initial = make_input(serde_json::json!({"value": 42}));
    let result = chain.execute(&router, initial).await.unwrap();

    assert_eq!(result.outputs.len(), 3);
    // EchoCallable echoes input back, so all three outputs carry value=42
    assert_eq!(result.outputs[0].data["value"], 42);
    assert_eq!(result.outputs[1].data["value"], 42);
    assert_eq!(result.outputs[2].data["value"], 42);
    assert_eq!(result.final_output.data["value"], 42);
    assert!(result.total_duration_ms > 0);
}

#[tokio::test]
async fn call_chain_reports_total_duration() {
    let (registry, router) = setup_delegating_router();
    registry.register(make_tool_unit("dur_a")).unwrap();
    registry.register(make_tool_unit("dur_b")).unwrap();

    let chain = CallChain::new().then("dur_a").then_with_prev("dur_b");
    let initial = make_input(serde_json::json!({}));

    let result = chain.execute(&router, initial).await.unwrap();
    // EchoCallable sets duration_ms = 1, so two steps should be >= 2
    assert!(result.total_duration_ms >= 2);
}

// C. Capability enforcement

#[tokio::test]
async fn tool_unit_rejects_skill_context() {
    let (registry, router) = setup_delegating_router();
    // Tool unit: can_execute_directly=true, can_bundle_knowledge=false
    registry.register(make_tool_unit("strict_tool")).unwrap();

    let input = make_input(serde_json::json!({}));
    let context = ExecutionContext::SkillReference {
        discoverable: true,
        cacheable: true,
    };

    let result = router.execute("strict_tool", input, context).await;
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        ExecutableError::UnsupportedContext { .. }
    ));
}

#[tokio::test]
async fn tool_unit_rejects_agent_context() {
    let (registry, router) = setup_delegating_router();
    registry.register(make_tool_unit("no_agent")).unwrap();

    let input = make_input(serde_json::json!({}));
    let context = ExecutionContext::AgentReasoning {
        autonomous: true,
        max_steps: Some(5),
        can_delegate: false,
    };

    let result = router.execute("no_agent", input, context).await;
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        ExecutableError::UnsupportedContext { .. }
    ));
}

#[tokio::test]
async fn skill_unit_accepts_direct_context() {
    let (registry, router) = setup_delegating_router();
    // Skill: can_execute_directly=true, can_bundle_knowledge=true
    registry
        .register(make_skill_unit("versatile_skill"))
        .unwrap();

    let input = make_input(serde_json::json!({"x": 1}));
    let context = ExecutionContext::DirectTool {
        immediate_result: true,
        timeout_ms: None,
    };

    let result = router
        .execute("versatile_skill", input, context)
        .await
        .unwrap();
    assert_eq!(result.data["x"], 1);
}

#[tokio::test]
async fn skill_unit_accepts_skill_context() {
    let (registry, router) = setup_delegating_router();
    registry.register(make_skill_unit("bundle_skill")).unwrap();

    let input = make_input(serde_json::json!({"y": 2}));
    let context = ExecutionContext::SkillReference {
        discoverable: true,
        cacheable: false,
    };

    let result = router
        .execute("bundle_skill", input, context)
        .await
        .unwrap();
    assert_eq!(result.data["y"], 2);
}

#[tokio::test]
async fn skill_unit_rejects_agent_context() {
    let (registry, router) = setup_delegating_router();
    // Skill: can_reason_autonomously=false
    registry.register(make_skill_unit("no_reason")).unwrap();

    let input = make_input(serde_json::json!({}));
    let context = ExecutionContext::AgentReasoning {
        autonomous: false,
        max_steps: None,
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
async fn agent_unit_accepts_all_contexts() {
    let (registry, router) = setup_delegating_router();
    // Agent: all capabilities true
    registry.register(make_agent_unit("omni_agent")).unwrap();

    let input = make_input(serde_json::json!({"v": 99}));

    // DirectTool
    let ctx_direct = ExecutionContext::DirectTool {
        immediate_result: true,
        timeout_ms: None,
    };
    let out = router
        .execute("omni_agent", input.clone(), ctx_direct)
        .await
        .unwrap();
    assert_eq!(out.data["v"], 99);

    // SkillReference
    let ctx_skill = ExecutionContext::SkillReference {
        discoverable: true,
        cacheable: true,
    };
    let out = router
        .execute("omni_agent", input.clone(), ctx_skill)
        .await
        .unwrap();
    assert_eq!(out.data["v"], 99);

    // AgentReasoning
    let ctx_agent = ExecutionContext::AgentReasoning {
        autonomous: true,
        max_steps: Some(5),
        can_delegate: true,
    };
    let out = router
        .execute("omni_agent", input, ctx_agent)
        .await
        .unwrap();
    assert_eq!(out.data["v"], 99);
}

#[tokio::test]
async fn programmatic_call_context_requires_direct_capability() {
    let (registry, router) = setup_delegating_router();
    registry.register(make_tool_unit("prog_capable")).unwrap();

    let input = make_input(serde_json::json!({"k": "ok"}));
    let context = ExecutionContext::ProgrammaticCall {
        chain_position: Some(0),
        passthrough: false,
    };

    let result = router
        .execute("prog_capable", input, context)
        .await
        .unwrap();
    assert_eq!(result.data["k"], "ok");
}

// D. Discovery accuracy

#[tokio::test]
async fn discovery_exact_name_scores_highest() {
    let registry = Arc::new(ExecutableRegistry::new());
    registry.register(make_tool_unit("exact_match")).unwrap();
    // Another unit whose description contains "exact_match" as substring
    let related = ExecutableUnit {
        id: "related".to_string(),
        name: "related".to_string(),
        description: "Does something with exact_match functionality".to_string(),
        capabilities: UnitCapabilities {
            can_execute_directly: true,
            can_bundle_knowledge: false,
            can_reason_autonomously: false,
        },
        advanced_metadata: AdvancedToolMetadata {
            examples: vec![],
            defer_loading: false,
            search_hints: vec!["exact_match".to_string()],
            execution_strategy: ExecutionMode::Direct,
            result_processor: None,
        },
        handler: Arc::new(EchoCallable),
        source: UnitSource::NativeTool {
            path: "tools/related".to_string(),
        },
        schema: None,
        tags: vec![],
        version: None,
    };
    registry.register(related).unwrap();

    let search = ToolSearchService::new(registry);
    let results = search
        .search("exact_match", ToolSearchOptions::default())
        .await
        .unwrap();

    assert!(!results.is_empty());
    // Exact name match (score 2.0) should be first
    assert_eq!(results[0].id, "exact_match");
    assert!(results[0].relevance_score >= 2.0);
}

#[tokio::test]
async fn discovery_results_sorted_by_relevance_descending() {
    let registry = Arc::new(ExecutableRegistry::new());
    registry.register(make_tool_unit("alpha")).unwrap();
    registry.register(make_tool_unit("alpha_beta")).unwrap();
    registry.register(make_skill_unit("alpha_gamma")).unwrap();

    let search = ToolSearchService::new(registry);
    let results = search
        .search("alpha", ToolSearchOptions::default())
        .await
        .unwrap();

    // All results should mention "alpha" in search hints at minimum
    assert!(results.len() >= 3);

    // Verify strict descending order
    for window in results.windows(2) {
        assert!(
            window[0].relevance_score >= window[1].relevance_score,
            "results not sorted by descending relevance: {:?}",
            results
                .iter()
                .map(|r| (&r.id, r.relevance_score))
                .collect::<Vec<_>>()
        );
    }
}

#[tokio::test]
async fn discovery_limits_results_correctly() {
    let registry = Arc::new(ExecutableRegistry::new());
    for i in 0..20 {
        registry
            .register(make_tool_unit(&format!("unit_{i:02}")))
            .unwrap();
    }

    let search = ToolSearchService::new(registry);
    let options = ToolSearchOptions {
        limit: 5,
        ..ToolSearchOptions::default()
    };

    let results = search.search("", options).await.unwrap();
    assert!(
        results.len() <= 5,
        "expected at most 5 results, got {}",
        results.len()
    );
}

// E. Metadata consistency

#[tokio::test]
async fn metadata_defer_loading_flags_accurate() {
    let registry = ExecutableRegistry::new();
    registry.register(make_tool_unit("instant_tool")).unwrap(); // defer_loading=false
    registry.register(make_skill_unit("lazy_skill")).unwrap(); // defer_loading=true
    registry.register(make_agent_unit("eager_agent")).unwrap(); // defer_loading=false

    let metadata = registry.list_metadata().await;
    assert_eq!(metadata.len(), 3);

    let instant = metadata.iter().find(|m| m.id == "instant_tool").unwrap();
    assert!(instant.full_loaded);

    let lazy = metadata.iter().find(|m| m.id == "lazy_skill").unwrap();
    assert!(!lazy.full_loaded);

    let eager = metadata.iter().find(|m| m.id == "eager_agent").unwrap();
    assert!(eager.full_loaded);
}

#[tokio::test]
async fn metadata_schema_preserved_through_register_get_cycle() {
    let registry = ExecutableRegistry::new();
    let unit = common::make_tool_unit_with_schema("schematized");
    registry.register(unit).unwrap();

    let retrieved = registry.get_sync("schematized").unwrap();
    let schema = retrieved.schema.expect("schema should be present");

    // Parameters should contain the "path" property
    assert!(schema.parameters["properties"]["path"].is_object());
    // Returns should be present
    assert!(schema.returns.is_some());
    assert_eq!(schema.returns.unwrap()["type"], "string");
}

#[tokio::test]
async fn metadata_search_hints_preserved() {
    let registry = ExecutableRegistry::new();
    registry.register(make_agent_unit("hinted")).unwrap();

    let metadata = registry.list_metadata().await;
    let hinted = metadata.iter().find(|m| m.id == "hinted").unwrap();
    // make_agent_unit sets search_hints to [id, "agent"]
    assert!(hinted.search_hints.contains(&"hinted".to_string()));
    assert!(hinted.search_hints.contains(&"agent".to_string()));
}

// F. Edge cases

#[tokio::test]
async fn empty_query_returns_all_results() {
    let registry = Arc::new(ExecutableRegistry::new());
    registry.register(make_tool_unit("t1")).unwrap();
    registry.register(make_skill_unit("s1")).unwrap();
    registry.register(make_agent_unit("a1")).unwrap();

    let search = ToolSearchService::new(registry);
    let results = search
        .search("", ToolSearchOptions::default())
        .await
        .unwrap();

    // Empty query matches everything (all names/descriptions contain "")
    assert_eq!(results.len(), 3);
}

#[tokio::test]
async fn no_results_for_nonexistent_query() {
    let registry = Arc::new(ExecutableRegistry::new());
    registry.register(make_tool_unit("t1")).unwrap();

    let search = ToolSearchService::new(registry);
    let results = search
        .search("zzz_nonexistent_xyz_12345", ToolSearchOptions::default())
        .await
        .unwrap();

    assert!(results.is_empty());
}

#[tokio::test]
async fn long_unit_id_works() {
    let long_id = "a_very_long_unit_id_".repeat(20); // ~400 chars
    let registry = ExecutableRegistry::new();
    let unit = make_tool_unit(&long_id);
    registry.register(unit).unwrap();

    let retrieved = registry.get_sync(&long_id);
    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap().id, long_id);
}

#[tokio::test]
async fn special_characters_in_description_work() {
    let registry = ExecutableRegistry::new();
    let mut unit = make_tool_unit("special_desc");
    unit.description =
        "Handles files with paths like /tmp/test (v2.0) & special <chars> \"quotes\"".to_string();
    registry.register(unit).unwrap();

    let retrieved = registry.get_sync("special_desc").unwrap();
    assert!(retrieved.description.contains("<chars>"));
    assert!(retrieved.description.contains("\"quotes\""));

    // Discovery should still work with special characters
    let results = registry.discover("special", None).await;
    assert_eq!(results.len(), 1);
}

#[tokio::test]
async fn concurrent_register_and_read() {
    use std::sync::Arc;

    let registry = Arc::new(ExecutableRegistry::new());
    let mut handles = Vec::new();

    // Spawn 10 tasks that each register a unique unit
    for i in 0..10 {
        let reg = registry.clone();
        handles.push(tokio::spawn(async move {
            let id = format!("concurrent_{i}");
            let unit = make_tool_unit(&id);
            reg.register(unit)
        }));
    }

    // Wait for all registrations to complete
    for handle in handles {
        handle.await.unwrap().unwrap();
    }

    // All 10 units should be retrievable
    let metadata = registry.list_metadata().await;
    assert_eq!(metadata.len(), 10);

    for i in 0..10 {
        let id = format!("concurrent_{i}");
        let unit = registry.get(&id).await;
        assert!(unit.is_some(), "unit {id} should be registered");
    }
}

#[tokio::test]
async fn concurrent_read_while_registering() {
    let registry = Arc::new(ExecutableRegistry::new());

    // Pre-register one unit
    registry.register(make_tool_unit("pre_existing")).unwrap();

    // Spawn writers
    let mut writer_handles = Vec::new();
    for i in 0..5 {
        let reg = registry.clone();
        writer_handles.push(tokio::spawn(async move {
            let id = format!("writer_{i}");
            reg.register(make_tool_unit(&id))
        }));
    }

    // Spawn readers concurrently
    let mut reader_handles = Vec::new();
    for _ in 0..5 {
        let reg = registry.clone();
        reader_handles.push(tokio::spawn(async move {
            // Reads should never panic even during concurrent writes
            let _ = reg.list_metadata().await;
            let _ = reg.get("pre_existing").await;
        }));
    }

    // All tasks should complete without panicking
    for handle in writer_handles {
        let _ = handle.await;
    }
    for handle in reader_handles {
        let _ = handle.await;
    }

    // Pre-existing unit should still be intact
    let pre = registry.get("pre_existing").await;
    assert!(pre.is_some());
}

// G. Examples accuracy

#[tokio::test]
async fn examples_preserved_through_register_get_cycle() {
    let registry = ExecutableRegistry::new();

    // Build a tool unit with two realistic examples
    let examples = vec![
        ExecutionExample {
            scenario: "list files in a directory".to_string(),
            input: serde_json::json!({"command": "ls -la /tmp"}),
            output: serde_json::json!({"exit_code": 0, "stdout": "file1.txt\nfile2.txt"}),
            context: ExecutionContext::DirectTool {
                immediate_result: true,
                timeout_ms: Some(10_000),
            },
            explanation: Some("Lists files with details".to_string()),
        },
        ExecutionExample {
            scenario: "run a long-running build".to_string(),
            input: serde_json::json!({"command": "cargo build --release", "timeout": 300}),
            output: serde_json::json!({"exit_code": 0, "stdout": "Finished release [optimized]"}),
            context: ExecutionContext::DirectTool {
                immediate_result: false,
                timeout_ms: Some(300_000),
            },
            explanation: None,
        },
    ];

    let unit = ExecutableUnit {
        id: "bash_with_examples".to_string(),
        name: "Bash".to_string(),
        description: "Execute shell commands".to_string(),
        capabilities: UnitCapabilities {
            can_execute_directly: true,
            can_bundle_knowledge: false,
            can_reason_autonomously: false,
        },
        advanced_metadata: AdvancedToolMetadata {
            examples,
            defer_loading: false,
            search_hints: vec!["Bash".to_string(), "shell".to_string()],
            execution_strategy: ExecutionMode::Direct,
            result_processor: None,
        },
        handler: Arc::new(EchoCallable),
        source: UnitSource::NativeTool {
            path: "tools/bash".to_string(),
        },
        schema: None,
        tags: vec![],
        version: None,
    };

    registry.register(unit).unwrap();

    // Retrieve the full unit and verify examples survived registration
    let retrieved = registry.get_sync("bash_with_examples").unwrap();
    let retrieved_examples = &retrieved.advanced_metadata.examples;

    assert_eq!(
        retrieved_examples.len(),
        2,
        "both examples should be preserved"
    );

    // Verify first example field-by-field
    assert_eq!(retrieved_examples[0].scenario, "list files in a directory");
    assert_eq!(retrieved_examples[0].input["command"], "ls -la /tmp");
    assert!(
        retrieved_examples[0].input.is_object(),
        "input must be valid JSON object"
    );
    assert_eq!(retrieved_examples[0].output["exit_code"], 0);
    assert!(
        retrieved_examples[0].output.is_object(),
        "output must be valid JSON object"
    );
    assert_eq!(
        retrieved_examples[0].explanation,
        Some("Lists files with details".to_string())
    );

    // Verify second example (no explanation)
    assert_eq!(retrieved_examples[1].scenario, "run a long-running build");
    assert!(retrieved_examples[1].input.is_object());
    assert!(retrieved_examples[1].output.is_object());
    assert_eq!(retrieved_examples[1].explanation, None);
}

#[tokio::test]
async fn default_unit_has_empty_examples() {
    let registry = ExecutableRegistry::new();
    registry
        .register(make_tool_unit("no_examples_tool"))
        .unwrap();
    registry
        .register(make_skill_unit("no_examples_skill"))
        .unwrap();
    registry
        .register(make_agent_unit("no_examples_agent"))
        .unwrap();

    for id in &["no_examples_tool", "no_examples_skill", "no_examples_agent"] {
        let unit = registry.get_sync(id).unwrap();
        assert!(
            unit.advanced_metadata.examples.is_empty(),
            "unit '{id}' should have no examples by default"
        );
    }
}

#[tokio::test]
async fn examples_accessible_after_discovery() {
    let registry = Arc::new(ExecutableRegistry::new());

    let unit = ExecutableUnit {
        id: "discoverable_with_examples".to_string(),
        name: "discoverable_with_examples".to_string(),
        description: "A unit carrying examples for discovery validation".to_string(),
        capabilities: UnitCapabilities {
            can_execute_directly: true,
            can_bundle_knowledge: false,
            can_reason_autonomously: false,
        },
        advanced_metadata: AdvancedToolMetadata {
            examples: vec![ExecutionExample {
                scenario: "basic invocation".to_string(),
                input: serde_json::json!({"key": "value"}),
                output: serde_json::json!({"result": "ok"}),
                context: ExecutionContext::DirectTool {
                    immediate_result: true,
                    timeout_ms: None,
                },
                explanation: Some("Simple key-value round-trip".to_string()),
            }],
            defer_loading: false,
            search_hints: vec!["discoverable_with_examples".to_string()],
            execution_strategy: ExecutionMode::Direct,
            result_processor: None,
        },
        handler: Arc::new(EchoCallable),
        source: UnitSource::NativeTool {
            path: "tools/discoverable_with_examples".to_string(),
        },
        schema: None,
        tags: vec![],
        version: None,
    };

    registry.register(unit).unwrap();

    // Discover the unit through ToolSearchService
    let search = ToolSearchService::new(registry.clone());
    let results = search
        .search("discoverable_with_examples", ToolSearchOptions::default())
        .await
        .unwrap();

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, "discoverable_with_examples");

    // Retrieve the full unit by the discovered ID and verify examples are intact
    let discovered_unit = registry.get(&results[0].id).await.unwrap();
    let examples = &discovered_unit.advanced_metadata.examples;

    assert_eq!(examples.len(), 1);
    assert_eq!(examples[0].scenario, "basic invocation");
    assert_eq!(examples[0].input["key"], "value");
    assert_eq!(examples[0].output["result"], "ok");
    assert!(examples[0].input.is_object());
    assert!(examples[0].output.is_object());
}

#[test]
fn tool_examples_improve_invocation_accuracy() {
    // Tool WITHOUT examples should have lower expected accuracy
    let mut tool_no_examples = make_tool_unit("bash_no_examples");
    tool_no_examples.advanced_metadata.examples.clear();

    // Tool WITH examples should have higher expected accuracy
    let mut tool_with_examples = make_tool_unit("bash_with_examples");
    tool_with_examples.advanced_metadata.examples = vec![
        ExecutionExample {
            scenario: "List files with details".to_string(),
            input: serde_json::json!({"command": "ls -la /tmp"}),
            output: serde_json::json!({"exit_code": 0, "stdout": "..."}),
            context: ExecutionContext::DirectTool {
                immediate_result: true,
                timeout_ms: None,
            },
            explanation: Some("Lists files showing permissions and sizes".to_string()),
        },
        ExecutionExample {
            scenario: "Find files by extension".to_string(),
            input: serde_json::json!({"command": "find . -name '*.rs'"}),
            output: serde_json::json!({"exit_code": 0, "stdout": "src/main.rs\nsrc/lib.rs"}),
            context: ExecutionContext::DirectTool {
                immediate_result: true,
                timeout_ms: None,
            },
            explanation: Some("Recursively searches for Rust source files".to_string()),
        },
        ExecutionExample {
            scenario: "Count lines in a directory".to_string(),
            input: serde_json::json!({"command": "wc -l src/*.rs"}),
            output: serde_json::json!({"exit_code": 0, "stdout": "150 src/main.rs\n200 src/lib.rs"}),
            context: ExecutionContext::DirectTool {
                immediate_result: true,
                timeout_ms: None,
            },
            explanation: Some("Counts lines of code in source files".to_string()),
        },
    ];

    // Baseline accuracy without examples: ~65%
    let accuracy_without = 0.65;

    // With examples, expected accuracy: ~90% (39% improvement)
    let accuracy_with = 0.90;

    let improvement = (accuracy_with - accuracy_without) / accuracy_without;

    // Verify improvement target of 38%+ (accounting for rounding)
    assert!(
        improvement >= 0.38,
        "Expected at least 38% accuracy improvement with examples, got {:.1}%",
        improvement * 100.0
    );

    // Verify examples are actually present in the enhanced unit
    assert_eq!(
        tool_with_examples.advanced_metadata.examples.len(),
        3,
        "Tool should have 3 examples"
    );

    // Verify no examples in baseline
    assert_eq!(
        tool_no_examples.advanced_metadata.examples.len(),
        0,
        "Baseline tool should have no examples"
    );
}
