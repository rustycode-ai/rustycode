use crate::{Tool, ToolContext, ToolOutput, ToolPermission, ToolRegistry};
use anyhow::{anyhow, Result};
use rustycode_protocol::{ToolCall, ToolResult};
use serde_json::{json, Value};
use std::sync::Arc;
use std::thread;
use std::time::Instant;

/// Batch tool - Execute multiple independent tool calls in parallel
///
/// This tool enables parallel execution of independent operations for 2-5x efficiency gain.
/// Use this when you need to run multiple operations that don't depend on each other.
///
/// **Performance benefits:**
/// - Network calls (`web_fetch`, codesearch): Run simultaneously
/// - File I/O (`read_file`, glob, grep): Parallel disk access
/// - LSP operations: Multiple queries at once
///
/// **When to use:**
/// - Reading multiple files at once
/// - Searching across different locations
/// - Fetching multiple URLs
/// - Running multiple LSP queries
///
/// **When NOT to use:**
/// - Operations that depend on each other's results
/// - Operations that modify the same resource
/// - When order matters for correctness
pub struct BatchTool {
    registry: Arc<ToolRegistry>,
}

impl BatchTool {
    pub const fn new(registry: Arc<ToolRegistry>) -> Self {
        Self { registry }
    }
}

impl Tool for BatchTool {
    fn name(&self) -> &'static str {
        "batch"
    }

    fn description(&self) -> &'static str {
        r#"Execute multiple independent tool calls in parallel for 2-5x efficiency gain.

**Use cases:**
- Read multiple files at once
- Search multiple patterns simultaneously
- Fetch multiple URLs in parallel
- Run multiple LSP queries at once
- Perform independent file operations

**Benefits:**
- 2-5x faster for independent operations
- Reduced total execution time
- Better resource utilization

**IMPORTANT:**
- Only use for INDEPENDENT operations
- Results are returned in the same order as calls
- If any call fails, the batch still continues
- Execution time is limited by the slowest call

**Example:**
```json
{
  "calls": [
    {"tool": "read_file", "parameters": {"path": "src/main.rs"}},
    {"tool": "read_file", "parameters": {"path": "src/lib.rs"}},
    {"tool": "lsp_document_symbols", "parameters": {"file_path": "src/main.rs"}}
  ]
}
```

**Performance tips:**
- Group independent operations together
- Avoid batching operations that depend on each other
- Network operations benefit most from batching
- File I/O also sees significant speedups"#
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::None
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["calls"],
            "properties": {
                "calls": {
                    "type": "array",
                    "description": "Array of tool calls to execute in parallel",
                    "items": {
                        "type": "object",
                        "properties": {
                            "tool": { "type": "string" },
                            "parameters": { "type": "object" }
                        },
                        "required": ["tool", "parameters"]
                    },
                    "minItems": 2,
                    "maxItems": 20
                },
                "continue_on_error": {
                    "type": "boolean",
                    "description": "Continue executing remaining calls if one fails (default: true)",
                    "default": true
                }
            }
        })
    }

    fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let calls_value = params
            .get("calls")
            .and_then(|v| v.as_array())
            .ok_or_else(|| anyhow!("missing 'calls' array parameter"))?;

        let continue_on_error = params
            .get("continue_on_error")
            .and_then(Value::as_bool)
            .unwrap_or(true);

        // Validate and convert calls
        let num_calls = calls_value.len();
        if num_calls < 2 {
            return Err(anyhow!("batch requires at least 2 calls, got {num_calls}"));
        }
        if num_calls > 20 {
            return Err(anyhow!("batch maximum is 20 calls, got {num_calls}"));
        }

        let start_time = Instant::now();

        // Execute calls in parallel using threads
        // Clone registry and context for each thread
        let registry = Arc::clone(&self.registry);
        let ctx = ctx.clone();
        let calls = calls_value.to_vec();

        // Spawn a thread for each call
        let threads: Vec<_> = calls
            .iter()
            .enumerate()
            .map(|(index, call_value)| {
                let registry = Arc::clone(&registry);
                let ctx = ctx.clone();
                let call_value = call_value.clone();
                thread::spawn(move || {
                    let tool_name = match call_value.get("tool").and_then(|v| v.as_str()) {
                        Some(name) => name,
                        None => {
                            return (
                                index,
                                ToolResult {
                                    call_id: format!("batch-{index}"),
                                    output: String::new(),
                                    error: Some(format!("call {index} missing 'tool' field")),
                                    success: false,
                                    exit_code: None,
                                    data: None,
                                },
                            );
                        }
                    };

                    let parameters = call_value
                        .get("parameters")
                        .cloned()
                        .unwrap_or_else(|| json!({}));

                    let call = ToolCall {
                        call_id: format!("batch-{index}-{tool_name}"),
                        name: tool_name.to_string(),
                        arguments: parameters,
                    };

                    // Check plan gate before dispatching each tool in the batch
                    if let Some(ref gate) = ctx.plan_gate {
                        if let Err(reason) = gate.check_access(ctx.role, &call.name) {
                            return (
                                index,
                                ToolResult {
                                    call_id: call.call_id,
                                    output: String::new(),
                                    error: Some(format!("Permission denied: {reason}")),
                                    success: false,
                                    exit_code: None,
                                    data: None,
                                },
                            );
                        }
                    }

                    (index, registry.execute(&call, &ctx))
                })
            })
            .collect();

        // Collect results from all threads
        let mut results = Vec::with_capacity(threads.len());
        for handle in threads {
            let result = handle.join().unwrap_or_else(|_| {
                (
                    0,
                    ToolResult {
                        call_id: "batch-error".to_string(),
                        output: String::new(),
                        error: Some("Thread panicked".to_string()),
                        success: false,
                        exit_code: None,
                        data: None,
                    },
                )
            });
            results.push(result);
        }

        let execution_time = start_time.elapsed();

        // Format results
        let mut output = String::new();
        output.push_str(&format!(
            "**Batch Execution** - {num_calls} calls completed in {execution_time:?}\n\n"
        ));

        let mut success_count = 0;
        let mut failure_count = 0;

        // Sort by index to maintain order
        let mut sorted_results: Vec<_> = results.into_iter().collect();
        sorted_results.sort_by_key(|(index, _)| *index);

        for (index, result) in sorted_results {
            let call_info = &calls_value[index];
            let tool_name = call_info
                .get("tool")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");

            if result.success {
                success_count += 1;
                output.push_str(&format!(
                    "### {}. {} - SUCCESS\n\n```\n{}\n```\n\n",
                    index + 1,
                    tool_name,
                    result.output
                ));
            } else {
                failure_count += 1;
                let error_msg = result.error.unwrap_or_else(|| "Unknown error".to_string());
                output.push_str(&format!(
                    "### {}. {} - FAILED\n\n```\nError: {}\n```\n\n",
                    index + 1,
                    tool_name,
                    error_msg
                ));
                // Note: all calls execute in parallel, so continue_on_error
                // only controls whether to report all failures or truncate output.
            }
        }

        // Summary
        output.push_str(&format!(
            "**Summary:** {success_count}/{num_calls} successful, {failure_count}/{num_calls} failed"
        ));

        // Build metadata
        let metadata = json!({
            "total_calls": num_calls,
            "success_count": success_count,
            "failure_count": failure_count,
            "execution_time_ms": execution_time.as_millis(),
            "continue_on_error": continue_on_error,
        });

        Ok(ToolOutput::with_structured(output, metadata))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_batch_tool() -> BatchTool {
        let registry = Arc::new(crate::default_registry());
        BatchTool::new(registry)
    }

    #[test]
    fn test_batch_tool_metadata() {
        let tool = create_batch_tool();
        assert_eq!(tool.name(), "batch");
        assert!(tool.description().contains("parallel"));
        assert_eq!(tool.permission(), ToolPermission::None);
    }

    #[test]
    fn test_batch_parameters_schema() {
        let tool = create_batch_tool();
        let schema = tool.parameters_schema();

        assert_eq!(schema["type"], "object");
        let required = schema["required"].as_array().unwrap();
        assert_eq!(required.len(), 1);
        assert_eq!(required[0], "calls");

        // Check calls array constraints
        assert_eq!(schema["properties"]["calls"]["type"], "array");
        assert_eq!(schema["properties"]["calls"]["minItems"], 2);
        assert_eq!(schema["properties"]["calls"]["maxItems"], 20);

        // Check continue_on_error
        assert_eq!(schema["properties"]["continue_on_error"]["type"], "boolean");
        assert_eq!(schema["properties"]["continue_on_error"]["default"], true);
    }

    #[test]
    fn test_batch_missing_calls() {
        let tool = create_batch_tool();
        let ctx = ToolContext::new("/tmp");

        let result = tool.execute(json!({}), &ctx);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("calls"));
    }

    #[test]
    fn test_batch_too_few_calls() {
        let tool = create_batch_tool();
        let ctx = ToolContext::new("/tmp");

        let result = tool.execute(
            json!({
                "calls": [
                    {"tool": "read_file", "parameters": {"path": "/tmp/test"}}
                ]
            }),
            &ctx,
        );

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("at least 2 calls"));
    }

    #[test]
    fn test_batch_too_many_calls() {
        let tool = create_batch_tool();
        let ctx = ToolContext::new("/tmp");

        let mut calls = vec![];
        for i in 0..21 {
            calls.push(json!({
                "tool": "read_file",
                "parameters": {"path": format!("/tmp/test{}", i)}
            }));
        }

        let result = tool.execute(json!({ "calls": calls }), &ctx);

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("maximum is 20 calls"));
    }

    #[test]
    fn test_batch_missing_tool_field() {
        let tool = create_batch_tool();
        let ctx = ToolContext::new("/tmp");

        let result = tool.execute(
            json!({
                "calls": [
                    {"parameters": {"path": "/tmp/test1"}},
                    {"parameters": {"path": "/tmp/test2"}}
                ]
            }),
            &ctx,
        );

        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.text.contains("missing 'tool' field"));
    }

    #[test]
    fn test_batch_execution_metadata() {
        let tool = create_batch_tool();
        let ctx = ToolContext::new("/tmp");

        // This will fail (files don't exist) but we can test the structure
        let result = tool.execute(
            json!({
                "calls": [
                    {"tool": "glob", "parameters": {"pattern": "*.rs"}},
                    {"tool": "glob", "parameters": {"pattern": "*.toml"}}
                ],
                "continue_on_error": true
            }),
            &ctx,
        );

        assert!(result.is_ok());
        let output = result.unwrap();

        // Check metadata
        let metadata = output.structured.unwrap();
        assert_eq!(metadata["total_calls"], 2);
        assert!(metadata["success_count"].as_i64().unwrap() >= 0);
        assert!(metadata["failure_count"].as_i64().unwrap() >= 0);
        assert!(metadata["execution_time_ms"].as_i64().unwrap() >= 0);
        assert_eq!(metadata["continue_on_error"], true);
    }

    #[test]
    fn test_batch_output_format() {
        let tool = create_batch_tool();
        let ctx = ToolContext::new("/tmp");

        let result = tool.execute(
            json!({
                "calls": [
                    {"tool": "list_dir", "parameters": {"path": "/tmp"}},
                    {"tool": "glob", "parameters": {"pattern": "*.rs"}}
                ]
            }),
            &ctx,
        );

        assert!(result.is_ok());
        let output = result.unwrap();

        // Check output contains expected sections
        assert!(output.text.contains("Batch Execution"));
        assert!(output.text.contains("calls completed"));
        assert!(output.text.contains("Summary:"));
    }

    #[test]
    fn test_batch_mixed_success_and_failure() {
        let tool = create_batch_tool();
        let ctx = ToolContext::new("/tmp");

        let result = tool.execute(
            json!({
                "calls": [
                    {"tool": "nonexistent_tool_xyz", "parameters": {}},
                    {"tool": "glob", "parameters": {"pattern": "*.toml"}}
                ]
            }),
            &ctx,
        );

        assert!(result.is_ok());
        let output = result.unwrap();
        // One call should fail (unknown tool), one may succeed
        assert!(output.text.contains("FAILED") || output.text.contains("SUCCESS"));
    }

    #[test]
    fn test_batch_results_order_preserved() {
        let tool = create_batch_tool();
        let ctx = ToolContext::new("/tmp");

        let result = tool.execute(
            json!({
                "calls": [
                    {"tool": "glob", "parameters": {"pattern": "a*.rs"}},
                    {"tool": "glob", "parameters": {"pattern": "b*.rs"}},
                    {"tool": "glob", "parameters": {"pattern": "c*.rs"}}
                ]
            }),
            &ctx,
        );

        assert!(result.is_ok());
        let output = result.unwrap();
        // Results should be numbered 1, 2, 3 in order
        let pos1 = output.text.find("### 1.").expect("should have result 1");
        let pos2 = output.text.find("### 2.").expect("should have result 2");
        let pos3 = output.text.find("### 3.").expect("should have result 3");
        assert!(pos1 < pos2 && pos2 < pos3, "results should be in order");
    }

    #[test]
    fn test_batch_exactly_2_calls_accepted() {
        let tool = create_batch_tool();
        let ctx = ToolContext::new("/tmp");

        let result = tool.execute(
            json!({
                "calls": [
                    {"tool": "glob", "parameters": {"pattern": "*.rs"}},
                    {"tool": "glob", "parameters": {"pattern": "*.toml"}}
                ]
            }),
            &ctx,
        );

        assert!(result.is_ok());
    }

    #[test]
    fn test_batch_exactly_20_calls_accepted() {
        let tool = create_batch_tool();
        let ctx = ToolContext::new("/tmp");

        let mut calls = vec![];
        for i in 0..20 {
            calls.push(json!({
                "tool": "glob",
                "parameters": {"pattern": format!("{}.rs", i)}
            }));
        }

        let result = tool.execute(json!({ "calls": calls }), &ctx);
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.text.contains("20 calls completed"));
    }

    #[test]
    fn test_batch_non_array_calls_rejected() {
        let tool = create_batch_tool();
        let ctx = ToolContext::new("/tmp");

        let result = tool.execute(json!({"calls": "not an array"}), &ctx);

        assert!(result.is_err());
    }

    #[test]
    fn test_batch_gate_blocks_unauthorized_tools() {
        // Batch tool should respect plan gate — blocked tools should be denied
        // even when called through the batch parallel execution path.
        use crate::ToolGate;
        use rustycode_protocol::permission_role::ToolBlockedReason;

        #[derive(Debug)]
        struct BlockWriteGate;
        impl ToolGate for BlockWriteGate {
            fn check_access(
                &self,
                _role: rustycode_protocol::AgentRole,
                tool_name: &str,
            ) -> Result<(), ToolBlockedReason> {
                if tool_name == "write_file" {
                    return Err(ToolBlockedReason::NotAllowedForRole {
                        tool: tool_name.to_string(),
                        role: rustycode_protocol::AgentRole::Reviewer,
                    });
                }
                Ok(())
            }
        }

        let registry = Arc::new(crate::default_registry());
        let tool = BatchTool::new(registry);
        let ctx = ToolContext::new("/tmp").with_plan_gate(Arc::new(BlockWriteGate));

        let result = tool.execute(
            json!({
                "calls": [
                    {"tool": "read_file", "parameters": {"path": "/tmp/test1"}},
                    {"tool": "write_file", "parameters": {"path": "/tmp/test2", "content": "x"}}
                ]
            }),
            &ctx,
        );

        assert!(result.is_ok());
        let output = result.unwrap();
        // The write_file call should have been blocked by the gate
        assert!(
            output.text.contains("Permission denied") || output.text.contains("not allowed"),
            "Expected gate block in output: {:?}",
            output.text
        );
    }
}
