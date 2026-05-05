//! Integration tests verifying the orchestration pipeline properly receives
//! external tools, system prompt, and conversation history.
//!
//! These tests exercise the **desired API** that doesn't fully exist yet.
//! The TDD red-green cycle:
//!   1. These tests call methods that don't exist → compile errors (RED)
//!   2. Implement the missing methods → tests compile and run (GREEN)
//!   3. Assert the correct behaviour once the API exists.

#![allow(clippy::unwrap_used)]

use std::sync::Arc;

use rustycode_orchestration::config::OrchestrationConfig;
use rustycode_orchestration::pipeline::OrchestrationPipeline;
use rustycode_tools_api::{Tool, ToolContext, ToolOutput, ToolPermission, ToolRegistry};
use serde_json::Value;

// ---------------------------------------------------------------------------
// Stub tool for registration tests
// ---------------------------------------------------------------------------

/// Minimal tool implementation used only in these tests.
struct StubTool;

impl Tool for StubTool {
    fn name(&self) -> &'static str {
        "test_stub"
    }
    fn description(&self) -> &'static str {
        "A stub tool for pipeline integration tests"
    }
    fn permission(&self) -> ToolPermission {
        ToolPermission::Read
    }
    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "input": { "type": "string" }
            }
        })
    }
    fn execute(&self, _params: Value, _ctx: &ToolContext) -> anyhow::Result<ToolOutput> {
        Ok(ToolOutput::text("stub response"))
    }
}

// ---------------------------------------------------------------------------
// Test 1: Pipeline constructed with an external tool registry exposes tools
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_pipeline_with_external_tool_registry_has_tools() {
    let mut registry = ToolRegistry::new();
    registry.register(StubTool);

    let mock_provider = Arc::new(rustycode_llm::mock::MockProvider::from_text("ok"));
    let config = OrchestrationConfig::default();

    // `with_provider_model_and_tools` does not exist yet — that's the point.
    // Once implemented, it should accept an Arc<ToolRegistry> and wire it
    // through to AgentSessionExecutor.
    let pipeline = OrchestrationPipeline::with_provider_model_and_tools(
        config,
        mock_provider,
        "test-model",
        Arc::new(registry),
    );

    // The pipeline should report at least one registered tool.
    assert!(
        pipeline.tool_count() > 0,
        "pipeline built with an external registry should report >0 tools"
    );
}

// ---------------------------------------------------------------------------
// Test 2: Default pipeline has zero tools (regression guard)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_pipeline_default_has_no_tools() {
    let mock_provider = Arc::new(rustycode_llm::mock::MockProvider::from_text("ok"));
    let config = OrchestrationConfig::default();

    let pipeline =
        OrchestrationPipeline::with_provider_and_model(config, mock_provider, "test-model");

    // `tool_count()` does not exist yet.  When implemented, the default
    // path (no external registry) creates ToolRegistry::new() which is empty.
    assert_eq!(
        pipeline.tool_count(),
        0,
        "default pipeline should start with zero tools"
    );
}

// ---------------------------------------------------------------------------
// Test 3: Pipeline accepts a custom system prompt
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_pipeline_with_custom_system_prompt() {
    let mock_provider = Arc::new(rustycode_llm::mock::MockProvider::from_text("done"));
    let config = OrchestrationConfig::default();
    let custom_prompt = "You are a specialised code reviewer. Only review code.";

    // `with_provider_model_and_prompt` does not exist yet.
    // It should pass the system prompt through to AgentSessionExecutor
    // instead of the hardcoded default.
    let pipeline = OrchestrationPipeline::with_provider_model_and_prompt(
        config,
        mock_provider,
        "test-model",
        custom_prompt,
    );

    let result = pipeline
        .conduct("prompt-test".into(), "review this code".into())
        .await;

    // The conduct itself should succeed — we're verifying the constructor
    // accepts and stores a custom prompt without panicking.
    assert!(
        result.is_ok(),
        "pipeline with custom system prompt should conduct successfully"
    );
}

// ---------------------------------------------------------------------------
// Test 4: conduct accepts conversation history
// ---------------------------------------------------------------------------

/// A single previous message in the conversation.
/// Mirrors the shape that rustycode-protocol uses for messages.
struct HistoryEntry {
    role: String,
    content: String,
}

#[tokio::test]
async fn test_conduct_accepts_conversation_history() {
    let mock_provider = Arc::new(rustycode_llm::mock::MockProvider::from_text("done"));
    let config = OrchestrationConfig::default();

    let pipeline =
        OrchestrationPipeline::with_provider_and_model(config, mock_provider, "test-model");

    let history = vec![
        HistoryEntry {
            role: "user".into(),
            content: "Create a file called hello.txt".into(),
        },
        HistoryEntry {
            role: "assistant".into(),
            content: "Created hello.txt".into(),
        },
    ];

    let system_prompt = "You are an autonomous developer.";

    // `conduct_with_history` does not exist yet.  The current `conduct`
    // signature is `conduct(task_id, task) -> Result<TaskResult>`.
    // The desired signature accepts prior turns and a system prompt so the
    // agent loop can continue an existing conversation.
    let result = pipeline
        .conduct_with_history(
            "history-test".into(),
            "Now add a goodbye message".into(),
            history
                .into_iter()
                .map(|e| rustycode_protocol::Message {
                    role: rustycode_protocol::MessageRole::from(e.role),
                    content: rustycode_protocol::MessageContent::simple(e.content),
                    timestamp: chrono::Utc::now(),
                    metadata: rustycode_protocol::MessageMetadata::default(),
                })
                .collect(),
            system_prompt,
        )
        .await;

    assert!(
        result.is_ok(),
        "conduct_with_history should accept conversation history and succeed"
    );
}
