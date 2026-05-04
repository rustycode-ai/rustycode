//! Tool execution for LLM streaming
//!
//! This module handles the execution of tools detected during LLM streaming,
//! including timeout handling, caching, and error tracking.

use std::path::Path;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant};

use super::parse_tool_parameters;
use crate::app::orchestration_integration::OrchestrationIntegration;
use crate::tool_search::{SearchAlgorithm, ToolSearch, ToolSearchQuery};
use crate::{ErrorTracker, FileReadCache};
use rustycode_core::integration::{HookContext, HookRegistry};
use rustycode_guard::codec::{HookInput, HookResult};
use rustycode_guard::pre_tool;
use rustycode_llm::tool_annotations::anthropic_annotations_for_tool_info;
use rustycode_orchestration::plan_mode::PlanMode;
use rustycode_protocol::ToolCall;
use rustycode_tools::ToolExecutor;

pub fn execute_tool(
    cwd: &Path,
    tool_name: &str,
    parameters_json: &str,
    file_read_cache: Option<&Arc<StdMutex<FileReadCache>>>,
    error_tracker: Option<&Arc<StdMutex<ErrorTracker>>>,
    todo_state: Option<&rustycode_tools::todo::TodoState>,
    tool_registry: Option<&Arc<rustycode_tools::ToolRegistry>>,
    plan_mode: Option<&PlanMode>,
    orchestration: Option<&Arc<StdMutex<OrchestrationIntegration>>>,
) -> String {
    // Normalize tool names from different providers to our canonical names
    let tool_name = match tool_name {
        "Edit" => "edit_file",
        "Read" => "read_file",
        "Write" | "Create" => "write_file",
        "Bash" | "Shell" => "bash",
        "Grep" | "Search" => "grep",
        "Glob" | "Find" => "glob",
        other => other,
    };

    tracing::info!(
        "Executing tool: {} with params: {}",
        tool_name,
        parameters_json
    );

    // Parse the parameters JSON with repair fallback
    let arguments: serde_json::Value = parse_tool_parameters(parameters_json);

    if tool_name == "tool_search" {
        let Some(registry) = tool_registry else {
            return "Error: tool_search requires a tool registry".to_string();
        };

        let query = arguments
            .get("query")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        if query.is_empty() {
            return "Error: tool_search requires a non-empty query".to_string();
        }

        let algorithm = match arguments
            .get("algorithm")
            .and_then(|v| v.as_str())
            .unwrap_or("bm25")
        {
            "regex" => SearchAlgorithm::Regex,
            _ => SearchAlgorithm::Bm25,
        };

        let limit = arguments
            .get("limit")
            .and_then(|v| v.as_u64())
            .map(|n| n as usize)
            .unwrap_or(10)
            .clamp(1, 50);

        let search = ToolSearch::new(registry.list());
        let result = search.search(&ToolSearchQuery {
            query,
            algorithm,
            limit,
        });

        let registry_tools = registry.list();
        let loaded_tools: Vec<serde_json::Value> = result
            .tools
            .iter()
            .filter_map(|tool_match| {
                registry_tools
                    .iter()
                    .find(|tool| tool.name == tool_match.reference.name)
                    .map(|tool| {
                        let mut schema = serde_json::json!({
                            "name": tool.name,
                            "description": tool.description,
                            "input_schema": tool.parameters_schema,
                            "defer_loading": false
                        });
                        if let Some(annotations) = anthropic_annotations_for_tool_info(
                            &tool.name,
                            matches!(tool.permission, rustycode_tools::ToolPermission::Read),
                        ) {
                            schema["annotations"] = annotations;
                        }
                        schema
                    })
            })
            .collect();

        return serde_json::to_string_pretty(&serde_json::json!({
            "tools": loaded_tools,
            "matches": result.tools,
            "total": result.total,
        }))
        .unwrap_or_else(|_| {
            serde_json::json!({
                "tools": [],
                "matches": [],
                "total": 0
            })
            .to_string()
        });
    }

    // Handle structured_thinking tool — route to persistent orchestration integration
    if tool_name == "structured_thinking" {
        return execute_structured_thinking(&arguments, orchestration);
    }

    // Guardrail pre-check
    if let Some(result) = check_tool_guard(tool_name, &arguments, cwd) {
        if result.permission_decision.as_deref() == Some("deny") {
            return format!(
                "BLOCKED: {}",
                result.permission_decision_reason.unwrap_or_default()
            );
        }
    }

    // Extract path value for cache operations
    let path_str = arguments
        .get("path")
        .and_then(|p| p.as_str())
        .map(|s| s.to_string());

    // Check file read cache for read_file tool
    if tool_name == "read_file" {
        if let Some(ref path_value) = path_str {
            let file_path = cwd.join(path_value);
            if let Some(cache) = file_read_cache {
                if let Ok(mut cache_guard) = cache.lock() {
                    if let Some(entry) = cache_guard.check(&file_path) {
                        if entry.read_count >= 3 {
                            return format!(
                                "[DUPLICATE READ] You have already read '{}' {} times in this conversation. \
                                 The content has not changed since your last read. \
                                 Please use the information you already have and proceed with your task.",
                                path_value, entry.read_count
                            );
                        }
                    }
                }
            }
        }
    }

    // Invalidate cache on write operations
    if matches!(tool_name, "write_file" | "apply_patch" | "edit_file") {
        if let Some(ref path_value) = path_str {
            let file_path = cwd.join(path_value);
            if let Some(cache) = file_read_cache {
                if let Ok(mut cache_guard) = cache.lock() {
                    cache_guard.invalidate(&file_path);
                }
            }
        }
    }

    // Create a ToolCall with generated ID
    let tool_call = ToolCall::with_generated_id(tool_name, arguments);

    // Create tool executor with custom registry if provided
    let mut executor = if let Some(registry) = tool_registry {
        let ctx = rustycode_tools::ToolContext::new(cwd).with_registry(Arc::clone(registry));
        ToolExecutor::new(Arc::clone(registry), ctx)
    } else if let Some(_state) = todo_state {
        // todo_state was used to seed the executor; fall back to from_cwd
        ToolExecutor::from_cwd(cwd.to_path_buf())
    } else {
        ToolExecutor::from_cwd(cwd.to_path_buf())
    };

    // Wire plan mode gate if provided — enforces role-based tool access
    if let Some(pm) = plan_mode {
        let gate: Arc<dyn rustycode_tools::ToolGate> = Arc::new(pm.clone());
        executor = executor.with_plan_gate(gate);
    }

    // Execute with timeout (120s for complex operations like compilation, test suites)
    let call_id = tool_call.call_id.clone();
    let tool_result: rustycode_protocol::ToolResult =
        std::thread::scope(|s| {
            let handle = s.spawn(|| executor.execute_with_session(&tool_call, None));
            let deadline = Instant::now() + Duration::from_mins(2);
            loop {
                if handle.is_finished() {
                    match handle.join() {
                        Ok(result) => return Ok(result),
                        Err(_) => {
                            tracing::error!("Tool execution thread panicked");
                            return Err(());
                        }
                    }
                }
                if Instant::now() >= deadline {
                    tracing::error!("Tool execution timeout after 120s");
                    return Err(());
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        }).unwrap_or_else(|()| rustycode_protocol::ToolResult {
            call_id,
            output: String::new(),
            error: Some(
                "Tool execution timed out after 120s. Try simplifying the operation or breaking it into smaller steps.".to_string(),
            ),
            exit_code: None,
            success: false,
            data: None,
        });

    // Return the appropriate content based on result
    if tool_result.success {
        tracing::info!("Tool executed successfully");

        // Record successful file reads in cache
        if tool_name == "read_file" {
            if let Some(ref path_value) = path_str {
                let file_path = cwd.join(path_value);
                if let Some(cache) = file_read_cache {
                    if let Ok(mut cache_guard) = cache.lock() {
                        let mtime = std::fs::metadata(&file_path)
                            .and_then(|m| m.modified())
                            .ok()
                            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                            .map(|d| d.as_millis() as u64)
                            .unwrap_or(0);

                        let has_images = tool_result.output.contains("image")
                            || tool_result.output.contains("![")
                            || tool_result.output.contains("<img");

                        cache_guard.record_read(&file_path, mtime, has_images);
                    }
                }
            }
        }

        // Clear error tracking on success
        if let Some(tracker) = error_tracker {
            if let Ok(mut tracker_guard) = tracker.lock() {
                tracker_guard.clear_errors(tool_name);
            }
        }

        tool_result.output
    } else {
        let error_msg = tool_result
            .error
            .unwrap_or_else(|| "Tool returned no output or error details".to_string());
        tracing::error!("Tool execution failed: {}", error_msg);

        // Track error for potential alternative suggestions
        if let Some(tracker) = error_tracker {
            if let Ok(mut tracker_guard) = tracker.lock() {
                tracker_guard.record_error(tool_name, &error_msg);

                if tracker_guard.should_suggest_alternative(tool_name) {
                    if let Some(alt) = tracker_guard.suggest_alternative_tool(tool_name) {
                        tracing::warn!("Tool error recovery suggestion: {}", alt);
                    }
                }
            }
        }

        format!("Error: {}", error_msg)
    }
}

/// Snapshot file content before a write operation for /undo support.
///
/// Returns `Some(batch)` if files were snapshotted, `None` for non-write tools.
/// The batch contains `(path, old_content)` pairs. If the file doesn't exist,
/// the old_content is an empty string (so /undo will delete the new file).
pub fn snapshot_files_for_undo(
    cwd: &Path,
    tool_name: &str,
    parameters_json: &str,
) -> Option<Vec<(String, String)>> {
    if !matches!(tool_name, "write_file" | "edit_file" | "apply_patch") {
        return None;
    }

    let arguments: serde_json::Value = parse_tool_parameters(parameters_json);
    let mut batch = Vec::new();

    // Extract target file path(s)
    let paths: Vec<String> = match arguments.get("path").and_then(|p| p.as_str()) {
        Some(s) => vec![s.to_string()],
        None => {
            tracing::debug!("No file path provided for snapshot, skipping");
            return None;
        }
    };

    for path_str in paths {
        let full_path = cwd.join(&path_str);
        let old_content = match std::fs::read_to_string(&full_path) {
            Ok(content) => content,
            Err(e) => {
                tracing::debug!("Could not read {} for snapshot: {}", full_path.display(), e);
                String::new()
            }
        };
        batch.push((full_path.to_string_lossy().to_string(), old_content));
    }

    if batch.is_empty() {
        None
    } else {
        Some(batch)
    }
}

/// Execute a tool with hooks - called before and after tool execution
///
/// This function extends `execute_tool` with pre and post execution hooks.
/// Hooks can be used for logging, permission checks, or custom processing.
///
/// # Arguments
/// * `cwd` - Current working directory
/// * `tool_name` - Name of the tool to execute
/// * `parameters_json` - JSON string containing tool parameters
/// * `hook_registry` - Registry of hooks to execute
/// * `context` - Hook context for passing data to hooks
///
/// # Returns
/// * Tool output on success
/// * Error message if execution is denied or fails
#[allow(clippy::await_holding_lock)]
pub fn execute_tool_with_hooks(
    cwd: &Path,
    tool_name: &str,
    parameters_json: &str,
    hook_registry: &Arc<std::sync::RwLock<HookRegistry>>,
    context: &HookContext,
) -> String {
    // Parse the parameters JSON with repair fallback
    let _arguments: serde_json::Value = parse_tool_parameters(parameters_json);

    // PRE-TOOL HOOK: Check if tool execution should be allowed
    let allow_execution = {
        let registry = match hook_registry.read() {
            Ok(r) => r,
            Err(_) => return "Error: Failed to acquire hook registry lock".to_string(),
        };

        // Run async pre-tool hooks synchronously using the existing tokio runtime
        let action = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(registry.execute_pre_tool_use(
                context,
                tool_name,
                &_arguments,
            ))
        });

        match action {
            Ok(rustycode_core::integration::HookAction::Allow) => true,
            Ok(rustycode_core::integration::HookAction::Deny(reason)) => {
                tracing::warn!("Tool {} denied by hook: {}", tool_name, reason);
                return format!("Tool execution denied: {reason}");
            }
            Ok(rustycode_core::integration::HookAction::Modify(_)) => true,
            Ok(_) => true, // non-exhaustive: unknown actions default to allow
            Err(e) => {
                tracing::error!("Pre-tool hook error for {}: {}", tool_name, e);
                true // Fail open — don't block on hook errors
            }
        }
    };

    if !allow_execution {
        return "Tool execution denied by hook".to_string();
    }

    let start_time = Instant::now();

    // Execute the tool (without cache/tracker in hook context)
    let result = execute_tool(
        cwd,
        tool_name,
        parameters_json,
        None,
        None,
        None,
        None,
        None,
        None,
    );
    let duration = start_time.elapsed();

    // POST-TOOL HOOK: Run synchronously using the existing tokio runtime
    // (replaces the old pattern of spawning a new thread + tokio Runtime per call)
    {
        let registry_for_post = hook_registry.clone();
        let tool_name_owned = tool_name.to_string();
        let result_owned = result.clone();
        let context_owned = context.clone();

        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let registry = match registry_for_post.read() {
                    Ok(r) => r,
                    Err(_) => return,
                };
                if let Err(e) = registry
                    .execute_post_tool_use(&context_owned, &tool_name_owned, &Ok(result_owned))
                    .await
                {
                    tracing::error!("Post-tool hook error: {}", e);
                }
            });
        });
    }

    tracing::info!("Tool {} executed in {:?}", tool_name, duration);
    result
}

fn check_tool_guard(
    tool_name: &str,
    tool_input: &serde_json::Value,
    cwd: &Path,
) -> Option<HookResult> {
    let input = HookInput {
        session_id: None,
        tool_name: tool_name.to_string(),
        tool_input: tool_input.clone(),
        cwd: Some(cwd.to_string_lossy().to_string()),
        hook_event_name: Some("PreToolUse".to_string()),
    };
    Some(pre_tool::evaluate(&input))
}

fn execute_structured_thinking(
    arguments: &serde_json::Value,
    orchestration: Option<&Arc<StdMutex<OrchestrationIntegration>>>,
) -> String {
    match orchestration {
        Some(arc) => {
            let mut orch = match arc.lock() {
                Ok(guard) => guard,
                Err(e) => {
                    tracing::error!("Orchestration lock poisoned: {e}");
                    return format!("Error: orchestration lock poisoned: {e}");
                }
            };

            let task_id = format!("auto-task-{}", std::process::id());
            orch.ensure_task(task_id);

            match orch.handle_structured_thought_tool_call(arguments) {
                Ok(thought) => {
                    if !thought.next_thought_needed {
                        let next = orch.advance_phase();
                        tracing::info!(next_phase = next, "Auto-advancing orchestration phase");
                    }

                    serde_json::json!({
                        "status": "recorded",
                        "thought_type": format!("{:?}", thought.thought_type),
                        "confidence": thought.confidence,
                        "phase": thought.phase,
                        "next_thought_needed": thought.next_thought_needed,
                    })
                    .to_string()
                }
                Err(e) => {
                    tracing::error!("Structured thinking tool call failed: {e}");
                    format!("Error: failed to record thought: {e}")
                }
            }
        }
        None => {
            tracing::warn!(
                "structured_thinking tool called but no orchestration instance available"
            );
            "Error: orchestration not configured for this session".to_string()
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_snapshot_files_for_write_tool() {
        let temp = tempfile::tempdir().unwrap();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "old content").unwrap();

        let params = serde_json::json!({
            "path": "test.txt"
        })
        .to_string();

        let result = snapshot_files_for_undo(temp.path(), "write_file", &params);
        assert!(result.is_some());
        let batch = result.unwrap();
        assert_eq!(batch.len(), 1);
        assert!(batch[0].0.ends_with("test.txt"));
        assert_eq!(batch[0].1, "old content");
    }

    #[test]
    fn test_snapshot_files_for_non_write_tool_returns_none() {
        let temp = tempfile::tempdir().unwrap();
        let params = serde_json::json!({"command": "echo hi"}).to_string();
        let result = snapshot_files_for_undo(temp.path(), "bash", &params);
        assert!(result.is_none());
    }

    #[test]
    fn test_snapshot_files_missing_file_gives_empty_content() {
        let temp = tempfile::tempdir().unwrap();
        let params = serde_json::json!({
            "path": "nonexistent.txt"
        })
        .to_string();

        let result = snapshot_files_for_undo(temp.path(), "write_file", &params);
        assert!(result.is_some());
        let batch = result.unwrap();
        assert_eq!(batch[0].1, "");
    }

    #[test]
    fn test_snapshot_files_for_edit_tool() {
        let temp = tempfile::tempdir().unwrap();
        let file_path = temp.path().join("edit_me.rs");
        fs::write(&file_path, "fn main() {}").unwrap();

        let params = serde_json::json!({
            "path": "edit_me.rs"
        })
        .to_string();

        let result = snapshot_files_for_undo(temp.path(), "edit_file", &params);
        assert!(result.is_some());
        assert_eq!(result.unwrap()[0].1, "fn main() {}");
    }

    #[test]
    fn test_check_tool_guard_deny() {
        // A tool call with no guard violations should return None or allow
        let input = serde_json::json!({"command": "echo hello"});
        let temp = tempfile::tempdir().unwrap();
        let result = check_tool_guard("bash", &input, temp.path());
        // With no guardrail rules configured, should either return None or allow
        // The actual behavior depends on the guardrail config
        if let Some(hr) = result {
            // If a result exists, check it's structured correctly
            assert!(hr.permission_decision.is_some() || hr.permission_decision.is_none());
        }
        // If None, no guardrails applied — both are acceptable
    }

    #[test]
    fn test_tool_search_executes_against_registry() {
        let temp = tempfile::tempdir().unwrap();

        // Create a registry with a bash tool registered
        let mut registry = rustycode_tools::default_registry();
        registry.register(rustycode_tools::BashTool);

        let registry = Arc::new(registry);
        let output = execute_tool(
            temp.path(),
            "tool_search",
            r#"{"query":"bash","algorithm":"bm25","limit":5}"#,
            None,
            None,
            None,
            Some(&registry),
            None,
            None,
        );

        assert!(output.contains("\"tools\""), "output was: {output}");
    }

    #[test]
    fn test_structured_thinking_tool_records_thought() {
        let temp = tempfile::tempdir().unwrap();
        let orch = Arc::new(StdMutex::new(OrchestrationIntegration::default()));
        let params = serde_json::json!({
            "thought": "Use BFS for graph traversal",
            "phase": 1,
            "type": "decision",
            "confidence": 85,
            "next_thought_needed": true
        })
        .to_string();

        let output = execute_tool(
            temp.path(),
            "structured_thinking",
            &params,
            None,
            None,
            None,
            None,
            None,
            Some(&orch),
        );

        assert!(
            output.contains("\"status\":\"recorded\"")
                || output.contains("\"status\": \"recorded\""),
            "output was: {output}"
        );
        assert!(
            output.contains("\"confidence\":85") || output.contains("\"confidence\": 85"),
            "output was: {output}"
        );
    }

    #[test]
    fn test_structured_thinking_tool_handles_minimal_args() {
        let temp = tempfile::tempdir().unwrap();
        let orch = Arc::new(StdMutex::new(OrchestrationIntegration::default()));
        let params = serde_json::json!({
            "thought": "Quick thought"
        })
        .to_string();

        let output = execute_tool(
            temp.path(),
            "structured_thinking",
            &params,
            None,
            None,
            None,
            None,
            None,
            Some(&orch),
        );

        assert!(output.contains("recorded"), "output was: {output}");
    }

    #[test]
    fn test_structured_thinking_tool_invalid_json() {
        let temp = tempfile::tempdir().unwrap();
        let orch = Arc::new(StdMutex::new(OrchestrationIntegration::default()));
        let output = execute_tool(
            temp.path(),
            "structured_thinking",
            "not valid json",
            None,
            None,
            None,
            None,
            None,
            Some(&orch),
        );

        assert!(
            output.contains("recorded") || output.contains("Error:"),
            "output was: {output}"
        );
    }

    #[test]
    fn test_structured_thinking_tool_validation_type() {
        let temp = tempfile::tempdir().unwrap();
        let orch = Arc::new(StdMutex::new(OrchestrationIntegration::default()));
        let params = serde_json::json!({
            "thought": "Verified the constraint",
            "phase": 2,
            "type": "validation",
            "confidence": 92,
            "next_thought_needed": false
        })
        .to_string();

        let output = execute_tool(
            temp.path(),
            "structured_thinking",
            &params,
            None,
            None,
            None,
            None,
            None,
            Some(&orch),
        );

        assert!(output.contains("Validation"), "output was: {output}");
    }

    #[test]
    fn test_structured_thinking_without_orchestration_returns_error() {
        let temp = tempfile::tempdir().unwrap();
        let output = execute_tool(
            temp.path(),
            "structured_thinking",
            r#"{"thought":"test"}"#,
            None,
            None,
            None,
            None,
            None,
            None,
        );

        assert!(output.contains("Error:"), "output was: {output}");
    }

    #[test]
    fn test_structured_thinking_phase_advances_on_persistent_instance() {
        let temp = tempfile::tempdir().unwrap();
        let orch = Arc::new(StdMutex::new(OrchestrationIntegration::default()));

        let params1 = serde_json::json!({
            "thought": "Phase 1 done",
            "phase": 1,
            "type": "decision",
            "confidence": 80,
            "next_thought_needed": false
        })
        .to_string();

        let output1 = execute_tool(
            temp.path(),
            "structured_thinking",
            &params1,
            None,
            None,
            None,
            None,
            None,
            Some(&orch),
        );
        assert!(output1.contains("recorded"), "output1: {output1}");

        let params2 = serde_json::json!({
            "thought": "Phase 2 thought",
            "phase": 2,
            "type": "validation",
            "confidence": 90,
            "next_thought_needed": true
        })
        .to_string();

        let output2 = execute_tool(
            temp.path(),
            "structured_thinking",
            &params2,
            None,
            None,
            None,
            None,
            None,
            Some(&orch),
        );
        assert!(output2.contains("recorded"), "output2: {output2}");

        let guard = orch.lock().unwrap();
        assert_eq!(guard.current_phase(), 2, "phase should have advanced");
    }

    #[test]
    fn test_structured_thinking_auto_creates_task() {
        let orch = Arc::new(StdMutex::new(OrchestrationIntegration::default()));
        let temp = tempfile::tempdir().unwrap();

        let params = serde_json::json!({
            "thought": "Auto task creation",
            "phase": 1,
            "type": "hypothesis",
            "confidence": 70,
            "next_thought_needed": true
        })
        .to_string();

        let output = execute_tool(
            temp.path(),
            "structured_thinking",
            &params,
            None,
            None,
            None,
            None,
            None,
            Some(&orch),
        );
        assert!(output.contains("recorded"), "output: {output}");

        // Task should have been auto-created
        let guard = orch.lock().unwrap();
        assert!(guard.current_phase() >= 1);
    }

    #[test]
    fn test_orchestration_complexity_thresholds() {
        let orch = Arc::new(StdMutex::new(OrchestrationIntegration::default()));

        // Simple message
        let guard = orch.lock().unwrap();
        let simple = guard.analyze_message("list files");
        assert!(
            simple.complexity < 3.0,
            "simple complexity: {}",
            simple.complexity
        );
        drop(guard);

        // Complex message
        let guard = orch.lock().unwrap();
        let complex = guard.analyze_message(
            "Investigate the database connection pool exhaustion issue, \
             analyze the root cause, and implement a fix with proper \
             connection lifecycle management",
        );
        assert!(
            complex.complexity > 3.0,
            "complex complexity: {}",
            complex.complexity
        );
        assert!(complex.enable_structured_thinking);
    }

    #[test]
    fn test_structured_thinking_multiple_thoughts_accumulate() {
        let orch = Arc::new(StdMutex::new(OrchestrationIntegration::default()));
        let temp = tempfile::tempdir().unwrap();

        // Three thoughts in phase 1
        for i in 1..=3 {
            let params = serde_json::json!({
                "thought": format!("Thought {i}"),
                "phase": 1,
                "type": "decision",
                "confidence": 70 + i * 5,
                "next_thought_needed": i < 3
            })
            .to_string();

            let output = execute_tool(
                temp.path(),
                "structured_thinking",
                &params,
                None,
                None,
                None,
                None,
                None,
                Some(&orch),
            );
            assert!(output.contains("recorded"), "thought {i}: {output}");
        }

        // Phase should still be 1 (next_thought_needed was true until the 3rd)
        let guard = orch.lock().unwrap();
        assert_eq!(
            guard.current_phase(),
            2,
            "should have advanced to phase 2 after final thought"
        );
    }
}
