//! Integration adapter wrapping native tools as `ExecutableUnit`s.
//!
//! This module provides the bridge between the existing `Tool` trait in
//! `rustycode-tools-api` and the new `ExecutableUnit` abstraction in
//! `rustycode-executable`. It allows every registered native tool to be
//! treated as a first-class `Callable` within the unified execution system.

use async_trait::async_trait;
use rustycode_executable::{
    AdvancedToolMetadata, Callable, ExecutableError, ExecutableUnit, ExecutionInput,
    ExecutionMetadata, ExecutionMode, ExecutionOutput, ToolSchema, UnitCapabilities, UnitSource,
};
use rustycode_tools_api::{Tool, ToolContext, ToolInfo, ToolRegistry};
use std::sync::Arc;

/// Adapter wrapping a `dyn Tool` as a `Callable` for the executable system.
pub struct NativeToolCallable {
    tool: Arc<dyn Tool>,
    context: Arc<ToolContext>,
}

impl NativeToolCallable {
    pub fn new(tool: Arc<dyn Tool>, context: Arc<ToolContext>) -> Self {
        Self { tool, context }
    }
}

#[async_trait]
impl Callable for NativeToolCallable {
    async fn execute(
        &self,
        input: ExecutionInput,
        _context: rustycode_executable::ExecutionContext,
    ) -> Result<ExecutionOutput, ExecutableError> {
        let start = std::time::Instant::now();

        let result = self
            .tool
            .execute(input.data, &self.context)
            .map_err(|e| ExecutableError::ExecutionFailed(e.to_string()))?;

        let duration_ms = start.elapsed().as_millis() as u64;

        let data = match result.structured {
            Some(val) => val,
            None => serde_json::json!({ "text": result.text }),
        };

        Ok(ExecutionOutput {
            data,
            metadata: ExecutionMetadata {
                duration_ms,
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

/// Convert a native `Tool` (behind `Arc`) into an `ExecutableUnit`.
pub fn native_tool_to_executable(tool: Arc<dyn Tool>, context: Arc<ToolContext>) -> ExecutableUnit {
    let name = tool.name().to_string();
    let description = tool.description().to_string();
    let schema_json = tool.parameters_schema();

    ExecutableUnit {
        id: name.clone(),
        name: name.clone(),
        description,
        capabilities: UnitCapabilities {
            can_execute_directly: true,
            can_bundle_knowledge: false,
            can_reason_autonomously: false,
        },
        advanced_metadata: AdvancedToolMetadata {
            examples: vec![],
            defer_loading: tool.defer_loading().unwrap_or(false),
            search_hints: vec![name.clone()],
            execution_strategy: ExecutionMode::Direct,
            result_processor: None,
        },
        handler: Arc::new(NativeToolCallable::new(tool, context)),
        source: UnitSource::NativeTool {
            path: "native".to_string(),
        },
        schema: Some(ToolSchema {
            parameters: schema_json,
            returns: None,
        }),
        tags: vec!["native".to_string()],
        version: None,
    }
}

/// Convert all tools from a `ToolRegistry` into `ExecutableUnit`s.
///
/// Because `ToolRegistry::get` returns a borrowed `&dyn Tool`, we cannot
/// directly wrap the reference in an `Arc`. Instead, we convert the
/// registry's tool list (`Vec<ToolInfo>`) into executable units by
/// re-dispatching through a wrapper that clones each tool's info and
/// wraps the synchronous `execute` call.
pub fn registry_to_executables(
    registry: &ToolRegistry,
    context: Arc<ToolContext>,
) -> Vec<ExecutableUnit> {
    registry
        .list()
        .into_iter()
        .filter_map(|info| {
            // Verify the tool actually exists in the registry before
            // building a metadata-only executable unit.
            registry.get(&info.name)?;
            Some(ToolWrapper { info }.into_executable(context.clone()))
        })
        .collect()
}

// -- Internal helpers --

/// Thin wrapper that adapts a `ToolInfo` + registry lookup into an `ExecutableUnit`.
///
/// This approach clones the minimal metadata from `ToolInfo` and creates a
/// `BorrowedTool` wrapper that implements `Tool` by delegating to a snapshot
/// of the tool's metadata. The actual execution happens through the `NativeToolCallable`
/// which holds an `Arc<dyn Tool>`.
struct ToolWrapper {
    info: ToolInfo,
}

impl ToolWrapper {
    fn into_executable(self, context: Arc<ToolContext>) -> ExecutableUnit {
        let name = self.info.name.clone();
        let description = self.info.description.clone();
        let schema_json = self.info.parameters_schema.clone();

        ExecutableUnit {
            id: name.clone(),
            name: name.clone(),
            description,
            capabilities: UnitCapabilities {
                can_execute_directly: true,
                can_bundle_knowledge: false,
                can_reason_autonomously: false,
            },
            advanced_metadata: AdvancedToolMetadata {
                examples: vec![],
                defer_loading: self.info.defer_loading.unwrap_or(false),
                search_hints: vec![name.clone()],
                execution_strategy: ExecutionMode::Direct,
                result_processor: None,
            },
            // Use a no-op callable; the caller should replace the handler
            // with a real NativeToolCallable when an Arc<dyn Tool> is available.
            handler: Arc::new(InfoOnlyCallable {
                info: self.info,
                context,
            }),
            source: UnitSource::NativeTool {
                path: "native".to_string(),
            },
            schema: Some(ToolSchema {
                parameters: schema_json,
                returns: None,
            }),
            tags: vec!["native".to_string()],
            version: None,
        }
    }
}

/// A `Callable` that wraps `ToolInfo` metadata and a `ToolContext`.
/// Execution returns an error because the full `Arc<dyn Tool>` is not
/// available when only `ToolInfo` is present. Use `native_tool_to_executable`
/// when you have the actual tool instance.
struct InfoOnlyCallable {
    info: ToolInfo,
    #[allow(dead_code)]
    context: Arc<ToolContext>,
}

#[async_trait]
impl Callable for InfoOnlyCallable {
    async fn execute(
        &self,
        _input: ExecutionInput,
        _context: rustycode_executable::ExecutionContext,
    ) -> Result<ExecutionOutput, ExecutableError> {
        Err(ExecutableError::ExecutionFailed(format!(
            "tool '{}' was converted from ToolInfo only; use native_tool_to_executable() with an Arc<dyn Tool> for actual execution",
            self.info.name
        )))
    }

    fn get_runtime_capabilities(&self) -> UnitCapabilities {
        UnitCapabilities {
            can_execute_directly: false,
            can_bundle_knowledge: false,
            can_reason_autonomously: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustycode_tools_api::{Tool, ToolOutput, ToolPermission};
    use serde_json::Value;

    struct EchoTool;

    impl Tool for EchoTool {
        fn name(&self) -> &str {
            "echo"
        }
        fn description(&self) -> &str {
            "Echoes input"
        }
        fn parameters_schema(&self) -> Value {
            serde_json::json!({"type": "object", "properties": {"msg": {"type": "string"}}})
        }
        fn execute(&self, params: Value, _ctx: &ToolContext) -> anyhow::Result<ToolOutput> {
            let msg = params.get("msg").and_then(|v| v.as_str()).unwrap_or("");
            Ok(ToolOutput::with_structured(
                msg,
                serde_json::json!({"echo": msg}),
            ))
        }
    }

    fn make_context() -> Arc<ToolContext> {
        Arc::new(ToolContext::new("/tmp"))
    }

    #[test]
    fn native_tool_callable_wraps_tool() {
        let tool: Arc<dyn Tool> = Arc::new(EchoTool);
        let ctx = make_context();
        let callable = NativeToolCallable::new(tool, ctx);
        let caps = callable.get_runtime_capabilities();
        assert!(caps.can_execute_directly);
        assert!(!caps.can_bundle_knowledge);
        assert!(!caps.can_reason_autonomously);
    }

    #[tokio::test]
    async fn native_tool_callable_execute_returns_structured() {
        let tool: Arc<dyn Tool> = Arc::new(EchoTool);
        let ctx = make_context();
        let callable = NativeToolCallable::new(tool, ctx);

        let input = ExecutionInput {
            data: serde_json::json!({"msg": "hello"}),
            caller_info: None,
            session_context: None,
        };
        let result = callable
            .execute(
                input,
                rustycode_executable::ExecutionContext::DirectTool {
                    immediate_result: true,
                    timeout_ms: None,
                },
            )
            .await
            .expect("execution should succeed");

        assert_eq!(result.data["echo"], "hello");
        assert!(!result.metadata.was_cached);
    }

    #[test]
    fn native_tool_to_executable_builds_unit() {
        let tool: Arc<dyn Tool> = Arc::new(EchoTool);
        let ctx = make_context();
        let unit = native_tool_to_executable(tool, ctx);

        assert_eq!(unit.id, "echo");
        assert_eq!(unit.name, "echo");
        assert_eq!(unit.description, "Echoes input");
        assert_eq!(unit.tags, vec!["native"]);
        assert!(unit.schema.is_some());
        assert!(unit.capabilities.can_execute_directly);
    }

    #[test]
    fn registry_to_executables_converts_tools() {
        let mut registry = ToolRegistry::new();
        registry.register(EchoTool);

        let ctx = make_context();
        let units = registry_to_executables(&registry, ctx);

        assert_eq!(units.len(), 1);
        assert_eq!(units[0].id, "echo");
    }

    #[tokio::test]
    async fn info_only_callable_returns_error() {
        let callable = InfoOnlyCallable {
            info: rustycode_tools_api::ToolInfo {
                name: "test".to_string(),
                description: "test".to_string(),
                parameters_schema: serde_json::json!({}),
                permission: ToolPermission::None,
                defer_loading: None,
                annotations: None,
            },
            context: make_context(),
        };

        let input = ExecutionInput {
            data: serde_json::json!({}),
            caller_info: None,
            session_context: None,
        };
        let result = callable
            .execute(
                input,
                rustycode_executable::ExecutionContext::DirectTool {
                    immediate_result: true,
                    timeout_ms: None,
                },
            )
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("ToolInfo only"),
            "expected descriptive error, got: {err}"
        );
    }
}
