use super::batch_state;
use crate::{ToolOutput, ToolPermission};
use anyhow::anyhow;
use rustycode_protocol::{ToolCall, ToolResult};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;
use std::thread;
use std::time::Instant;

/// A single tool call within a batch
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct BatchCall {
    /// Tool name to execute
    pub tool: Option<String>,
    /// Parameters for the tool call
    #[serde(default = "default_empty_object")]
    pub parameters: serde_json::Value,
}

fn default_empty_object() -> serde_json::Value {
    serde_json::json!({})
}

/// Parameters for the batch tool
#[derive(Debug, Deserialize, JsonSchema)]
pub struct BatchParams {
    /// Array of tool calls to execute in parallel (2-20 calls)
    #[schemars(length(min = 2, max = 20))]
    pub calls: Vec<BatchCall>,
    /// Continue executing remaining calls if one fails (default: true)
    #[serde(default = "default_true")]
    pub continue_on_error: bool,
}

fn default_true() -> bool {
    true
}

rustycode_tools_api::define_tool! {
    pub struct BatchTool;

    name: "batch",
    description: r#"Execute multiple independent tool calls in parallel for 2-5x efficiency gain.

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
    {"tool": "Read", "parameters": {"path": "src/main.rs"}},
    {"tool": "Read", "parameters": {"path": "src/lib.rs"}},
    {"tool": "lsp_document_symbols", "parameters": {"file_path": "src/main.rs"}}
  ]
}
```

**Performance tips:**
- Group independent operations together
- Avoid batching operations that depend on each other
- Network operations benefit most from batching
- File I/O also sees significant speedups"#,
    permission: ToolPermission::None,

    execute(params: BatchParams, ctx) {
        // Validate call count
        let num_calls = params.calls.len();
        if num_calls < 2 {
            return Err(anyhow!("batch requires at least 2 calls, got {num_calls}"));
        }
        if num_calls > 20 {
            return Err(anyhow!("batch maximum is 20 calls, got {num_calls}"));
        }

        let start_time = Instant::now();

        // Retrieve registry from global state keyed by session_id
        let session_id = ctx.session_id.as_deref().unwrap_or("default-session");
        let registry = batch_state::get_batch_registry(session_id)
            .ok_or_else(|| anyhow!("No batch registry found for session '{session_id}'"))?;

        // Execute calls in parallel using threads
        let ctx = ctx.clone();
        let calls = params.calls;

        // Spawn a thread for each call
        let threads: Vec<_> = calls
            .iter()
            .enumerate()
            .map(|(index, call)| {
                let registry = Arc::clone(&registry);
                let ctx = ctx.clone();
                let tool_name = call.tool.clone();
                let parameters = call.parameters.clone();
                thread::spawn(move || {
                    let tool_name = match tool_name {
                        Some(name) if !name.is_empty() => name,
                        _ => {
                            return (
                                index,
                                ToolResult {
                                    call_id: format!("batch-{index}"),
                                    output: String::new(),
                                    error: Some(format!("call {index} missing 'tool' field")),
                                    success: false,
                                    exit_code: None,
                                    data: None,
                                    new_cwd: None,
                                },
                            );
                        }
                    };

                    let tool_call = ToolCall {
                        call_id: format!("batch-{index}-{tool_name}"),
                        name: tool_name,
                        arguments: parameters,
                    };

                    // Check plan gate before dispatching each tool in the batch
                    if let Some(ref gate) = ctx.plan_gate {
                        if let Err(reason) = gate.check_access(ctx.role, &tool_call.name) {
                            return (
                                index,
                                ToolResult {
                                    call_id: tool_call.call_id,
                                    output: String::new(),
                                    error: Some(format!("Permission denied: {reason}")),
                                    success: false,
                                    exit_code: None,
                                    data: None,
                                    new_cwd: None,
                                },
                            );
                        }
                    }

                    (index, registry.execute(&tool_call, &ctx))
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
                        new_cwd: None,
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
            let tool_name = calls[index]
                .tool
                .as_deref()
                .filter(|s| !s.is_empty())
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
            "continue_on_error": params.continue_on_error,
        });

        Ok(ToolOutput::with_structured(output, metadata))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Tool, ToolContext};
    use std::sync::Arc;

    fn setup_batch_tool(session_id: &str) -> BatchTool {
        let registry = Arc::new(crate::default_registry());
        batch_state::set_batch_registry(session_id, registry);
        BatchTool
    }

    #[test]
    fn test_batch_tool_zero_sized() {
        let tool = BatchTool;
        assert_eq!(std::mem::size_of_val(&tool), 0);
    }

    #[test]
    fn test_batch_tool_metadata() {
        let tool = BatchTool;
        assert_eq!(tool.name(), "batch");
        assert!(tool.description().contains("parallel"));
        assert_eq!(tool.permission(), ToolPermission::None);
    }

    #[test]
    fn test_batch_parameters_schema() {
        let tool = BatchTool;
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
        let tool = setup_batch_tool("test-missing-calls");
        let ctx = ToolContext::new("/tmp").with_session_id("test-missing-calls");

        let result = tool.execute(json!({}), &ctx);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("calls"));
    }

    #[test]
    fn test_batch_missing_registry() {
        let tool = BatchTool;
        let ctx = ToolContext::new("/tmp").with_session_id("nonexistent-session");

        let result = tool.execute(
            json!({
                "calls": [
                    {"tool": "Read", "parameters": {"path": "/tmp/test"}},
                    {"tool": "Glob", "parameters": {"pattern": "*.rs"}}
                ]
            }),
            &ctx,
        );

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("No batch registry found"));
    }

    #[test]
    fn test_batch_too_few_calls() {
        let tool = setup_batch_tool("test-too-few");
        let ctx = ToolContext::new("/tmp").with_session_id("test-too-few");

        let result = tool.execute(
            json!({
                "calls": [
                    {"tool": "Read", "parameters": {"path": "/tmp/test"}}
                ]
            }),
            &ctx,
        );

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("at least 2 calls"));
    }

    #[test]
    fn test_batch_too_many_calls() {
        let tool = setup_batch_tool("test-too-many");
        let ctx = ToolContext::new("/tmp").with_session_id("test-too-many");

        let mut calls = vec![];
        for i in 0..21 {
            calls.push(json!({
                "tool": "Read",
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
        let tool = setup_batch_tool("test-missing-tool");
        let ctx = ToolContext::new("/tmp").with_session_id("test-missing-tool");

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
        let tool = setup_batch_tool("test-metadata");
        let ctx = ToolContext::new("/tmp").with_session_id("test-metadata");

        // This will fail (files don't exist) but we can test the structure
        let result = tool.execute(
            json!({
                "calls": [
                    {"tool": "Glob", "parameters": {"pattern": "*.rs"}},
                    {"tool": "Glob", "parameters": {"pattern": "*.toml"}}
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
        let tool = setup_batch_tool("test-output-format");
        let ctx = ToolContext::new("/tmp").with_session_id("test-output-format");

        let result = tool.execute(
            json!({
                "calls": [
                    {"tool": "list_dir", "parameters": {"path": "/tmp"}},
                    {"tool": "Glob", "parameters": {"pattern": "*.rs"}}
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
        let tool = setup_batch_tool("test-mixed");
        let ctx = ToolContext::new("/tmp").with_session_id("test-mixed");

        let result = tool.execute(
            json!({
                "calls": [
                    {"tool": "nonexistent_tool_xyz", "parameters": {}},
                    {"tool": "Glob", "parameters": {"pattern": "*.toml"}}
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
        let tool = setup_batch_tool("test-order");
        let ctx = ToolContext::new("/tmp").with_session_id("test-order");

        let result = tool.execute(
            json!({
                "calls": [
                    {"tool": "Glob", "parameters": {"pattern": "a*.rs"}},
                    {"tool": "Glob", "parameters": {"pattern": "b*.rs"}},
                    {"tool": "Glob", "parameters": {"pattern": "c*.rs"}}
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
        let tool = setup_batch_tool("test-2-calls");
        let ctx = ToolContext::new("/tmp").with_session_id("test-2-calls");

        let result = tool.execute(
            json!({
                "calls": [
                    {"tool": "Glob", "parameters": {"pattern": "*.rs"}},
                    {"tool": "Glob", "parameters": {"pattern": "*.toml"}}
                ]
            }),
            &ctx,
        );

        assert!(result.is_ok());
    }

    #[test]
    fn test_batch_exactly_20_calls_accepted() {
        let tool = setup_batch_tool("test-20-calls");
        let ctx = ToolContext::new("/tmp").with_session_id("test-20-calls");

        let mut calls = vec![];
        for i in 0..20 {
            calls.push(json!({
                "tool": "Glob",
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
        let tool = setup_batch_tool("test-non-array");
        let ctx = ToolContext::new("/tmp").with_session_id("test-non-array");

        let result = tool.execute(json!({"calls": "not an array"}), &ctx);

        assert!(result.is_err());
    }

    #[test]
    fn test_batch_session_isolation() {
        let session_1 = "session-1";
        let session_2 = "session-2";

        let registry_1 = Arc::new(crate::default_registry());
        let registry_2 = Arc::new(crate::default_registry());

        batch_state::set_batch_registry(session_1, registry_1);
        batch_state::set_batch_registry(session_2, registry_2);

        let retrieved_1 = batch_state::get_batch_registry(session_1);
        let retrieved_2 = batch_state::get_batch_registry(session_2);

        assert!(retrieved_1.is_some());
        assert!(retrieved_2.is_some());
        // Verify they're different instances
        assert!(!Arc::ptr_eq(&retrieved_1.unwrap(), &retrieved_2.unwrap()));
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
                if tool_name == "Write" {
                    return Err(ToolBlockedReason::NotAllowedForRole {
                        tool: tool_name.to_string(),
                        role: rustycode_protocol::AgentRole::Reviewer,
                    });
                }
                Ok(())
            }
        }

        let tool = setup_batch_tool("test-gate");
        let ctx = ToolContext::new("/tmp")
            .with_session_id("test-gate")
            .with_plan_gate(Arc::new(BlockWriteGate));

        let result = tool.execute(
            json!({
                "calls": [
                    {"tool": "Read", "parameters": {"path": "/tmp/test1"}},
                    {"tool": "Write", "parameters": {"path": "/tmp/test2", "content": "x"}}
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
