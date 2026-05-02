//! Shared test fixtures for `rustycode-executable` integration tests
#![allow(dead_code, clippy::missing_const_for_fn, clippy::doc_markdown)]

use rustycode_executable::{
    AdvancedToolMetadata, Callable, ExecutionInput, ExecutionMetadata, ExecutionMode,
    ExecutionContext, ExecutableError, ExecutableUnit, ToolSchema, UnitCapabilities, UnitSource,
};
use rustycode_executable::router::{DirectExecutor, SkillBundler, AgentExecutor};
use std::sync::Arc;
use async_trait::async_trait;

/// Simple callable that echoes input data back
pub struct EchoCallable;

#[async_trait]
impl Callable for EchoCallable {
    async fn execute(
        &self,
        input: ExecutionInput,
        _context: ExecutionContext,
    ) -> Result<rustycode_executable::ExecutionOutput, ExecutableError> {
        Ok(rustycode_executable::ExecutionOutput {
            data: input.data,
            metadata: ExecutionMetadata {
                duration_ms: 1,
                tokens_used: None,
                was_cached: false,
                trace: None,
            },
        })
    }

    fn get_runtime_capabilities(&self) -> UnitCapabilities {
        UnitCapabilities {
            can_execute_directly: true,
            can_bundle_knowledge: false,
            can_reason_autonomously: false,
        }
    }
}

/// Callable that returns a fixed value
pub struct FixedCallable {
    pub value: serde_json::Value,
}

#[async_trait]
impl Callable for FixedCallable {
    async fn execute(
        &self,
        _input: ExecutionInput,
        _context: ExecutionContext,
    ) -> Result<rustycode_executable::ExecutionOutput, ExecutableError> {
        Ok(rustycode_executable::ExecutionOutput {
            data: self.value.clone(),
            metadata: ExecutionMetadata {
                duration_ms: 0,
                tokens_used: None,
                was_cached: false,
                trace: None,
            },
        })
    }

    fn get_runtime_capabilities(&self) -> UnitCapabilities {
        UnitCapabilities {
            can_execute_directly: true,
            can_bundle_knowledge: false,
            can_reason_autonomously: false,
        }
    }
}

/// Create a basic tool ExecutableUnit for testing
pub fn make_tool_unit(id: &str) -> ExecutableUnit {
    ExecutableUnit {
        id: id.to_string(),
        name: id.to_string(),
        description: format!("Test tool: {id}"),
        capabilities: UnitCapabilities {
            can_execute_directly: true,
            can_bundle_knowledge: false,
            can_reason_autonomously: false,
        },
        advanced_metadata: AdvancedToolMetadata {
            examples: vec![],
            defer_loading: false,
            search_hints: vec![id.to_string()],
            execution_strategy: ExecutionMode::Direct,
            result_processor: None,
        },
        handler: Arc::new(EchoCallable),
        source: UnitSource::NativeTool {
            path: format!("tools/{id}"),
        },
        schema: None,
        tags: vec![],
        version: None,
    }
}

/// Create a tool unit with a schema
pub fn make_tool_unit_with_schema(id: &str) -> ExecutableUnit {
    let mut unit = make_tool_unit(id);
    unit.schema = Some(ToolSchema {
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "path": {"type": "string"}
            },
            "required": ["path"]
        }),
        returns: Some(serde_json::json!({"type": "string"})),
    });
    unit
}

/// Create a skill ExecutableUnit for testing
pub fn make_skill_unit(id: &str) -> ExecutableUnit {
    ExecutableUnit {
        id: id.to_string(),
        name: id.to_string(),
        description: format!("Test skill: {id}"),
        capabilities: UnitCapabilities {
            can_execute_directly: true,
            can_bundle_knowledge: true,
            can_reason_autonomously: false,
        },
        advanced_metadata: AdvancedToolMetadata {
            examples: vec![],
            defer_loading: true,
            search_hints: vec![id.to_string(), "skill".to_string()],
            execution_strategy: ExecutionMode::Bundled,
            result_processor: None,
        },
        handler: Arc::new(EchoCallable),
        source: UnitSource::InstalledSkill {
            path: format!("skills/{id}"),
            version: Some("1.0".to_string()),
        },
        schema: None,
        tags: vec!["skill".to_string()],
        version: Some("1.0".to_string()),
    }
}

/// Create an agent ExecutableUnit for testing
pub fn make_agent_unit(id: &str) -> ExecutableUnit {
    ExecutableUnit {
        id: id.to_string(),
        name: id.to_string(),
        description: format!("Test agent: {id}"),
        capabilities: UnitCapabilities {
            can_execute_directly: true,
            can_bundle_knowledge: true,
            can_reason_autonomously: true,
        },
        advanced_metadata: AdvancedToolMetadata {
            examples: vec![],
            defer_loading: false,
            search_hints: vec![id.to_string(), "agent".to_string()],
            execution_strategy: ExecutionMode::Autonomous,
            result_processor: None,
        },
        handler: Arc::new(EchoCallable),
        source: UnitSource::BundledAgent {
            path: format!("agents/{id}"),
        },
        schema: None,
        tags: vec!["agent".to_string()],
        version: None,
    }
}

/// Create a basic ExecutionInput for testing
pub fn make_input(data: serde_json::Value) -> ExecutionInput {
    ExecutionInput {
        data,
        caller_info: None,
        session_context: None,
    }
}

// ---------------------------------------------------------------------------
// Custom executor implementations for verifying routing behavior.
//
// Each handler returns an output with a `"handler"` field identifying which
// path was selected, allowing tests to assert on the routing decision rather
// than just checking for errors from the default stubs.
// ---------------------------------------------------------------------------

/// Direct executor that tags its output as `"direct"`.
pub struct TagDirectExecutor;

#[async_trait]
impl DirectExecutor for TagDirectExecutor {
    async fn execute(
        &self,
        _unit: &ExecutableUnit,
        _input: ExecutionInput,
    ) -> Result<rustycode_executable::ExecutionOutput, ExecutableError> {
        Ok(rustycode_executable::ExecutionOutput {
            data: serde_json::json!({"handler": "direct", "ok": true}),
            metadata: ExecutionMetadata {
                duration_ms: 0,
                tokens_used: None,
                was_cached: false,
                trace: None,
            },
        })
    }
}

/// Skill bundler that tags its output as `"skill"`.
pub struct TagSkillBundler;

#[async_trait]
impl SkillBundler for TagSkillBundler {
    async fn bundle(
        &self,
        _unit: &ExecutableUnit,
        _input: ExecutionInput,
    ) -> Result<rustycode_executable::ExecutionOutput, ExecutableError> {
        Ok(rustycode_executable::ExecutionOutput {
            data: serde_json::json!({"handler": "skill", "ok": true}),
            metadata: ExecutionMetadata {
                duration_ms: 0,
                tokens_used: None,
                was_cached: false,
                trace: None,
            },
        })
    }
}

/// Agent executor that tags its output as `"agent"`.
pub struct TagAgentExecutor;

#[async_trait]
impl AgentExecutor for TagAgentExecutor {
    async fn execute(
        &self,
        _unit: &ExecutableUnit,
        _input: ExecutionInput,
    ) -> Result<rustycode_executable::ExecutionOutput, ExecutableError> {
        Ok(rustycode_executable::ExecutionOutput {
            data: serde_json::json!({"handler": "agent", "ok": true}),
            metadata: ExecutionMetadata {
                duration_ms: 0,
                tokens_used: None,
                was_cached: false,
                trace: None,
            },
        })
    }
}

/// Create a tool unit that has ONLY `can_execute_directly` (no knowledge, no reasoning).
pub fn make_direct_only_unit(id: &str) -> ExecutableUnit {
    ExecutableUnit {
        id: id.to_string(),
        name: id.to_string(),
        description: format!("Direct-only unit: {id}"),
        capabilities: UnitCapabilities {
            can_execute_directly: true,
            can_bundle_knowledge: false,
            can_reason_autonomously: false,
        },
        advanced_metadata: AdvancedToolMetadata {
            examples: vec![],
            defer_loading: false,
            search_hints: vec![id.to_string()],
            execution_strategy: ExecutionMode::Direct,
            result_processor: None,
        },
        handler: Arc::new(EchoCallable),
        source: UnitSource::NativeTool {
            path: format!("tools/{id}"),
        },
        schema: None,
        tags: vec![],
        version: None,
    }
}

/// Create a unit that has ONLY `can_bundle_knowledge` (no direct, no reasoning).
pub fn make_knowledge_only_unit(id: &str) -> ExecutableUnit {
    ExecutableUnit {
        id: id.to_string(),
        name: id.to_string(),
        description: format!("Knowledge-only unit: {id}"),
        capabilities: UnitCapabilities {
            can_execute_directly: false,
            can_bundle_knowledge: true,
            can_reason_autonomously: false,
        },
        advanced_metadata: AdvancedToolMetadata {
            examples: vec![],
            defer_loading: false,
            search_hints: vec![id.to_string()],
            execution_strategy: ExecutionMode::Bundled,
            result_processor: None,
        },
        handler: Arc::new(EchoCallable),
        source: UnitSource::InstalledSkill {
            path: format!("skills/{id}"),
            version: None,
        },
        schema: None,
        tags: vec!["skill".to_string()],
        version: None,
    }
}
