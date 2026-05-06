#![allow(
    clippy::assigning_clones,
    clippy::bool_to_int_with_if,
    clippy::borrow_as_ptr,
    clippy::case_sensitive_file_extension_comparisons,
    clippy::cast_lossless,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::default_trait_access,
    clippy::derive_partial_eq_without_eq,
    clippy::doc_markdown,
    clippy::elidable_lifetime_names,
    clippy::expect_used,
    clippy::format_push_string,
    clippy::future_not_send,
    clippy::if_not_else,
    clippy::ignored_unit_patterns,
    clippy::implicit_clone,
    clippy::implicit_hasher,
    clippy::items_after_statements,
    clippy::literal_string_with_formatting_args,
    clippy::manual_assert,
    clippy::manual_let_else,
    clippy::manual_midpoint,
    clippy::manual_string_new,
    clippy::map_unwrap_or,
    clippy::match_bool,
    clippy::match_same_arms,
    clippy::match_wildcard_for_single_variants,
    clippy::missing_const_for_fn,
    clippy::missing_fields_in_debug,
    clippy::needless_collect,
    clippy::needless_continue,
    clippy::needless_pass_by_ref_mut,
    clippy::needless_pass_by_value,
    clippy::needless_raw_string_hashes,
    clippy::no_effect_underscore_binding,
    clippy::non_std_lazy_statics,
    clippy::option_if_let_else,
    clippy::or_fun_call,
    clippy::range_plus_one,
    clippy::redundant_clone,
    clippy::redundant_closure_for_method_calls,
    clippy::redundant_else,
    clippy::self_only_used_in_recursion,
    clippy::semicolon_if_nothing_returned,
    clippy::set_contains_or_insert,
    clippy::significant_drop_tightening,
    clippy::similar_names,
    clippy::single_char_pattern,
    clippy::single_match_else,
    clippy::stable_sort_primitive,
    clippy::struct_excessive_bools,
    clippy::struct_field_names,
    clippy::suboptimal_flops,
    clippy::suspicious_operation_groupings,
    clippy::too_long_first_doc_paragraph,
    clippy::too_many_lines,
    clippy::unchecked_time_subtraction,
    clippy::uninlined_format_args,
    clippy::unnecessary_debug_formatting,
    clippy::unnecessary_literal_bound,
    clippy::unnecessary_wraps,
    clippy::unnested_or_patterns,
    clippy::unreadable_literal,
    clippy::unused_async,
    clippy::unused_self,
    clippy::unwrap_used,
    clippy::use_self,
    clippy::used_underscore_binding
)]
#![cfg_attr(test, allow(clippy::float_cmp,))]
//! RustyCode Core - Core runtime and execution logic.
//!
//! This crate provides the core functionality for the RustyCode system, including:
//!
//! - **Plan Validation**: Pre-execution validation of plans to prevent failures

//! - **Context Management**: Budget-aware context assembly and prioritization
//! - **Error Recovery**: Intelligent error classification and recovery strategies
//! - **Step Execution**: Orchestration of plan step execution with error handling
//! - **Event Publishing**: Integration with the event bus for observability
//!
//! ## Plan Validation
//!
//! The `validation` module provides comprehensive plan validation before execution:
//!
//! ```ignore
//! use rustycode_core::validation::validate_plan;
//! use rustycode_protocol::Plan;
//! use rustycode_tools::ToolRegistry;
//! use std::path::Path;
//!
//! // Validate a plan before execution
//! validate_plan(&plan, &tool_registry, workspace_root)?;
//! ```
//!
//! Validation checks include:
//! - No circular dependencies between steps
//! - All required tools are registered
//! - File paths are valid and within workspace
//! - Steps are properly ordered
//! - All required fields are present
//!
//! ## Error Recovery
//!
//! The `recovery` module provides intelligent error recovery with automatic retry,
//! fallback, and skip strategies:
//!
//! ```ignore
//! use rustycode_core::recovery::{RecoveryEngine, RecoveryConfig};
//! use anyhow::Result;
//!
//! # #[tokio::main]
//! # async fn main() -> Result<()> {
//! let config = RecoveryConfig::default().with_max_attempts(3);
//! let engine = RecoveryEngine::new(config);
//!
//! // Recover from errors automatically
//! let result = engine.recover(
//!     anyhow::anyhow!("Temporary failure"),
//!     "my_operation",
//!     &|| async { Err(anyhow::anyhow!("Failed")) },
//! ).await?;
//! # Ok(())
//! # }
//! ```
//!
//! Recovery strategies include:
//! - **Retry**: Automatically retry with exponential backoff
//! - **Fallback**: Use alternative implementations or cached results
//! - **Skip**: Skip non-critical failures
//! - **Abort**: Stop execution for critical errors

pub mod agent;
pub mod build_detection;
pub mod checkpoint;
pub mod checkpoint_detector;
pub mod checkpoint_recovery;
pub mod checkpoint_store;
pub mod checkpoint_validator;
pub mod context;
pub mod context_management;
pub mod context_prio;
pub mod deadlock;
pub mod edit_history;
pub mod error;
pub mod execution;
pub mod headless;
pub mod integration;
pub mod iteration_checkpoint;
pub mod plan;
pub mod plan_executor;
pub mod recovery;
pub mod rollout;
pub mod runtime;
pub mod session;
pub mod session_context;
pub mod snapshot;
pub mod streaming;
pub use rustycode_team::*;
pub mod tenacity;
pub mod todo_enforcer;
pub mod tool_result_storage;
pub mod ultrawork;
pub mod validation;
pub mod verification_gates;
pub mod workspace_memory;

pub use checkpoint::{CheckpointSnapshot, ExecutionPhase};
pub use checkpoint_recovery::{Recovery, RecoveryState};
pub use checkpoint_validator::{CheckpointValidator, ValidationReport};
pub use execution::{
    CheckpointingStepExecutor, ExecutionCheckpointStore, ExecutionConfig, ExecutionContext,
    StepExecutor, StepExecutorRegistry, ToolInvocationWrapper,
};
pub use plan_executor::{ExecutionOptions, ExecutionReport, PlanExecutor};
pub use runtime::{CodeExcerpt, DoctorReport, PlanReport, RunReport, Runtime, ToolCallReport};
pub use session::{AiMode, MessageType, SessionState, ToolExecution, ToolStatus};

mod sleep;
pub use rustycode_shared_runtime as shared_runtime;

#[allow(unused_imports)]
use anyhow::{anyhow, bail, Context, Result};
use chrono::Utc;
use rustycode_git::GitStatus;
use rustycode_lsp::LspServerStatus;
use rustycode_memory::MemoryEntry;
use rustycode_protocol::{
    ContextPlan, ContextSection, ContextSectionKind, Plan, PlanId, PlanStatus, PlanStep, SessionId,
    StepStatus,
};
use rustycode_skill::Skill;
use std::fs;
use std::path::Path;
use walkdir::WalkDir;

pub fn estimate_tokens(value: &str) -> usize {
    context::TokenCounter::estimate_tokens(value)
}

// Code excerpt selector — used by context assembly
pub fn select_code_excerpts(cwd: &Path, task: &str, limit: usize) -> Result<Vec<CodeExcerpt>> {
    let terms = task_terms(task);
    let mut matches = Vec::new();
    let mut fallback = Vec::new();
    for entry in WalkDir::new(cwd)
        .max_depth(4)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_file())
    {
        let path = entry.path();
        if should_skip_path(path) || !is_supported_source(path) {
            continue;
        }
        let content = match fs::read_to_string(path) {
            Ok(content) => content,
            Err(_) => continue,
        };
        let preview = content
            .lines()
            .find(|line| !line.trim().is_empty())
            .unwrap_or("")
            .trim()
            .chars()
            .take(120)
            .collect::<String>();
        let path_text = path.display().to_string().to_lowercase();
        let content_text = content.to_lowercase();
        let mut score = 0;
        for term in &terms {
            if path_text.contains(term) {
                score += 5;
            }
            if content_text.contains(term) {
                score += 2;
            }
        }
        if score == 0 && terms.is_empty() {
            score = 1;
        }
        if score > 0 {
            matches.push(CodeExcerpt {
                path: path.display().to_string(),
                preview,
                score,
            });
        } else {
            fallback.push(CodeExcerpt {
                path: path.display().to_string(),
                preview,
                score: 1,
            });
        }
    }
    matches.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.path.cmp(&b.path)));
    fallback.sort_by(|a, b| a.path.cmp(&b.path));
    for excerpt in fallback {
        if matches.len() >= limit {
            break;
        }
        matches.push(excerpt);
    }
    matches.truncate(limit);
    Ok(matches)
}

// Task term extractor — used by code excerpt selection
pub fn task_terms(task: &str) -> Vec<String> {
    let mut terms = Vec::new();
    for raw in task.split(|char: char| !char.is_alphanumeric()) {
        let term = raw.trim().to_lowercase();
        if term.len() >= 3 && !terms.contains(&term) {
            terms.push(term);
        }
    }
    terms
}

// Source file filter — used by code excerpt selection
pub fn is_supported_source(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|ext| ext.to_str()),
        Some("rs" | "md" | "toml" | "json" | "yaml" | "yml" | "ts" | "js" | "py")
    )
}

// Path filter — used by code excerpt selection
pub fn should_skip_path(path: &Path) -> bool {
    path.components().any(|component| {
        let value = component.as_os_str().to_string_lossy();
        value == "target" || value == ".git" || value == "node_modules"
    })
}

/// Render a plan as markdown for human review.
pub fn render_plan_markdown(plan: &Plan) -> String {
    let mut out = String::new();
    out.push_str(&format!("# Plan: {}\n\n", plan.task));
    out.push_str(&format!("**Session:** `{}`  \n", plan.session_id));
    out.push_str(&format!("**Plan ID:** `{}`  \n", plan.id));
    out.push_str(&format!("**Status:** `{:?}`\n\n", plan.status));
    out.push_str("## Summary\n\n");
    out.push_str(&format!("{}\n\n", plan.summary));
    out.push_str("## Approach\n\n");
    if plan.approach.trim().is_empty() {
        out.push_str("<!-- Describe your approach here -->\n\n");
    } else {
        out.push_str(&format!("{}\n\n", plan.approach));
    }
    out.push_str("## Steps\n\n");
    if plan.steps.is_empty() {
        out.push_str("No steps defined.\n\n");
    } else {
        for step in &plan.steps {
            out.push_str(&format!("### {}. {}\n\n", step.order, step.title));
            out.push_str(&format!("{}\n\n", step.description));
            out.push_str(&format!("**Tools:** {}\n\n", step.tools.join(", ")));
            out.push_str(&format!(
                "**Expected outcome:** {}\n\n",
                step.expected_outcome
            ));
            out.push_str(&format!("**Rollback:** {}\n\n", step.rollback_hint));
        }
    }
    out.push_str("## Files to Modify\n\n");
    if plan.files_to_modify.is_empty() {
        out.push_str("<!-- List files that will change, one per line -->\n\n");
    } else {
        for path in &plan.files_to_modify {
            out.push_str(&format!("- {}\n", path));
        }
        out.push('\n');
    }
    out.push_str("## Risks\n\n");
    if plan.risks.is_empty() {
        out.push_str("<!-- List potential issues or caveats -->\n\n");
    } else {
        for risk in &plan.risks {
            out.push_str(&format!("- {}\n", risk));
        }
        out.push('\n');
    }
    out.push_str("---\n");
    out.push_str("*Edit this file, then run `rustycode plan approve <session-id>` to execute.*\n");
    out
}

/// Async implementation: generate a plan using an LLM provider
pub async fn generate_plan_with_llm_async(
    provider: &dyn rustycode_llm::provider::LLMProvider,
    task: &str,
    available_tools: &[&str],
) -> Result<Plan> {
    use rustycode_llm::provider::{ChatMessage, CompletionRequest};

    let tools_str = available_tools.join(", ");
    let prompt = format!(
        r#"You are a coding assistant. Generate a plan to accomplish the following task:

Task: {}

Available tools: {}

Respond in JSON format with the following structure:
{{
    "summary": "Brief summary of the plan",
    "approach": "High-level approach description",
    "steps": [
        {{
            "title": "Step title",
            "description": "What this step does",
            "tools": ["tool1", "tool2"],
            "expected_outcome": "What this step achieves",
            "rollback_hint": "How to undo (or N/A)"
        }}
    ],
    "files_to_modify": ["file1.rs", "file2.rs"],
    "risks": ["risk1", "risk2"]
}}

Generate a practical, actionable plan with 2-5 steps. Each step should use appropriate tools from the available list."#,
        task, tools_str
    );

    // Convert the prompt to a CompletionRequest
    let request =
        CompletionRequest::new("default-model".to_string(), vec![ChatMessage::user(prompt)]);

    // Async retry loop using tokio sleep
    #[allow(unused_assignments)]
    let mut last_err: Option<anyhow::Error> = None;
    let mut attempt = 0usize;
    let max_attempts = 3usize;
    let mut backoff = std::time::Duration::from_millis(200);

    let response = loop {
        attempt += 1;
        match provider.complete(request.clone()).await {
            Ok(resp) => break resp,
            Err(e) => {
                last_err = Some(anyhow::anyhow!("{}", e));
                if attempt >= max_attempts {
                    let err = last_err.unwrap_or_else(|| {
                        anyhow::anyhow!("LLM provider failed after retries (no error captured)")
                    });
                    return Err(err.context("LLM provider failed after retries"));
                }
                // If we're running inside a Tokio runtime prefer async sleep,
                // otherwise fall back to blocking sleep so sync tests (which use
                // futures::executor::block_on) don't require a Tokio reactor.
                // Use hybrid_sleep helper so tests and runtimes behave the
                // same without duplicating runtime detection logic.
                sleep::hybrid_sleep(backoff).await;
                backoff = std::cmp::min(backoff * 2, std::time::Duration::from_secs(5));
                continue;
            }
        }
    };

    let content = response.content;

    // Try to parse JSON from response
    let json: serde_json::Value = serde_json::from_str(&content)
        .or_else(|_| {
            // Try extracting JSON from markdown code block
            if let Some(start) = content.find("```json") {
                if let Some(end) = content[start + 7..].find("```") {
                    let json_str = &content[start + 7..start + 7 + end];
                    serde_json::from_str(json_str)
                } else {
                    serde_json::from_str(&content)
                }
            } else {
                serde_json::from_str(&content)
            }
        })
        .context("Failed to parse LLM response as JSON")?;

    // Extract plan from JSON
    let summary = json
        .get("summary")
        .and_then(|v| v.as_str())
        .unwrap_or(task)
        .to_string();

    let approach = json
        .get("approach")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let files_to_modify: Vec<String> = json
        .get("files_to_modify")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    let risks: Vec<String> = json
        .get("risks")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    let steps: Vec<PlanStep> = json
        .get("steps")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .enumerate()
                .map(|(i, step)| PlanStep {
                    order: i,
                    title: step
                        .get("title")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    description: step
                        .get("description")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    tools: step
                        .get("tools")
                        .and_then(|v| v.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|v| v.as_str().map(String::from))
                                .collect()
                        })
                        .unwrap_or_default(),
                    expected_outcome: step
                        .get("expected_outcome")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    rollback_hint: step
                        .get("rollback_hint")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    execution_status: StepStatus::default(),
                    tool_calls: vec![],
                    tool_executions: vec![],
                    results: vec![],
                    errors: vec![],
                    started_at: None,
                    completed_at: None,
                })
                .collect()
        })
        .unwrap_or_else(|| {
            vec![PlanStep {
                order: 0,
                title: "Explore codebase".to_string(),
                description: "Use available tools to understand the codebase.".to_string(),
                tools: vec![
                    "read_file".to_string(),
                    "grep".to_string(),
                    "list_dir".to_string(),
                ],
                expected_outcome: "Understand the codebase structure.".to_string(),
                rollback_hint: "N/A — read-only step.".to_string(),
                execution_status: StepStatus::default(),
                tool_calls: vec![],
                tool_executions: vec![],
                results: vec![],
                errors: vec![],
                started_at: None,
                completed_at: None,
            }]
        });

    Ok(Plan {
        id: PlanId::new(),
        session_id: SessionId::new(),
        task: task.to_string(),
        created_at: Utc::now(),
        status: PlanStatus::Draft,
        summary,
        approach,
        steps,
        files_to_modify,
        risks,
        current_step_index: None,
        execution_started_at: None,
        execution_completed_at: None,
        execution_error: None,
        task_profile: None,

        milestone_id: None,
    })
}

/// Synchronous wrapper kept for compatibility and tests. This executes the
/// async implementation on a lightweight executor (`futures::executor::block_on`).
pub fn generate_plan_with_llm(
    provider: &dyn rustycode_llm::provider::LLMProvider,
    task: &str,
    available_tools: &[&str],
) -> Result<Plan> {
    shared_runtime::block_on_shared(generate_plan_with_llm_async(
        provider,
        task,
        available_tools,
    ))
}

/// Generate a plan from user task, optionally using an LLM provider
/// Falls back to template if LLM is unavailable or fails
pub fn generate_smart_plan(
    task: &str,
    available_tools: &[&str],
    provider: Option<&dyn rustycode_llm::provider::LLMProvider>,
) -> Plan {
    // Synchronous path: call the async generator via the shared runtime to
    // preserve existing behaviour for sync callers/tests.
    shared_runtime::block_on_shared(generate_smart_plan_async(task, available_tools, provider))
}

async fn generate_smart_plan_async(
    task: &str,
    available_tools: &[&str],
    provider: Option<&dyn rustycode_llm::provider::LLMProvider>,
) -> Plan {
    // Try to generate plan with LLM if available
    if let Some(p) = provider {
        match generate_plan_with_llm_async(p, task, available_tools).await {
            Ok(plan) => return plan,
            Err(e) => tracing::warn!("LLM plan generation failed: {}", e),
        }
    }

    // Fall back to template-based plan
    let steps = vec![PlanStep {
        order: 0, // Start from 0 for proper ordering
        title: "Explore codebase".to_string(),
        description: "Use read_file, grep, and list_dir to understand the relevant code."
            .to_string(),
        tools: vec![
            "read_file".to_string(),
            "grep".to_string(),
            "list_dir".to_string(),
        ],
        expected_outcome: "Understand the files that need to change.".to_string(),
        rollback_hint: "N/A — read-only step.".to_string(),
        execution_status: StepStatus::default(),
        tool_calls: vec![],
        tool_executions: vec![],
        results: vec![],
        errors: vec![],
        started_at: None,
        completed_at: None,
    }];

    Plan {
        id: PlanId::new(),
        session_id: SessionId::new(), // Will be overwritten
        task: task.to_string(),
        created_at: Utc::now(),
        status: PlanStatus::Draft,
        summary: format!("Plan for: {}", task),
        approach: String::new(),
        steps,
        files_to_modify: vec![],
        risks: vec![],
        current_step_index: None,
        execution_started_at: None,
        execution_completed_at: None,
        execution_error: None,
        task_profile: None,

        milestone_id: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::Stream;
    use rustycode_protocol::{ContextSectionKind, SessionId};
    use std::fs;
    use std::path::PathBuf;
    use std::pin::Pin;

    fn temp_dir() -> PathBuf {
        let path = std::env::temp_dir().join(format!("rustycode-core-{}", SessionId::new()));
        fs::create_dir_all(&path).unwrap();
        path
    }

    // ── Step Executor Tests ────────────────────────────────────────────────

    #[test]
    fn step_executor_registry_can_register_and_retrieve() {
        let mut registry = StepExecutorRegistry::new();
        let executor = registry.default_executor(PathBuf::from("."));
        registry.register("generic".to_string(), executor.clone());

        assert!(registry.get("generic").is_some());
        assert!(registry.get("nonexistent").is_none());
    }

    // ──────────────────────────────────────────────────────────────────────

    #[test]
    #[ignore = "Complex integration test - requires specific file setup"]
    fn run_assembles_context_from_local_config() {
        let cwd = temp_dir();
        let data_dir = cwd.join("data");
        let skills_dir = cwd.join("skills");
        let memory_dir = cwd.join("memory");
        fs::create_dir_all(&skills_dir).unwrap();
        fs::create_dir_all(&memory_dir).unwrap();
        fs::create_dir_all(cwd.join("src")).unwrap();
        fs::create_dir_all(skills_dir.join("reviewer")).unwrap();
        fs::write(
            skills_dir.join("reviewer").join("SKILL.md"),
            "# Reviewer\n\nFinds regressions.\n",
        )
        .unwrap();
        fs::write(memory_dir.join("notes.md"), "prefer concise summaries\n").unwrap();
        fs::write(
            cwd.join("src").join("parser.rs"),
            "pub fn parse_feature_gate() {\n    let feature_gate = true;\n}\n",
        )
        .unwrap();
        // Config loader searches for .rustycode/config.json, not .rustycode.json
        let config_dir = cwd.join(".rustycode");
        fs::create_dir_all(&config_dir).unwrap();
        fs::write(
            config_dir.join("config.json"),
            format!(
                "{{\n  \"data_dir\": \"{}\",\n  \"skills_dir\": \"{}\",\n  \"memory_dir\": \"{}\",\n  \"lsp_servers\": []\n}}\n",
                data_dir.display(),
                skills_dir.display(),
                memory_dir.display()
            ),
        )
        .unwrap();

        let runtime = Runtime::load(&cwd).unwrap();
        let _ = runtime.run(&cwd, "previous task for history").unwrap();
        let report = runtime.run(&cwd, "Inspect parser feature gate").unwrap();

        assert_eq!(report.memory.len(), 1);
        assert_eq!(report.skills.len(), 1);
        assert_eq!(report.recent_tasks, vec!["previous task for history"]);
        assert!(!report.code_excerpts.is_empty());
        assert!(report.code_excerpts[0].path.ends_with("parser.rs"));
        assert_eq!(report.context_plan.total_budget, 8_000);
        assert_eq!(report.context_plan.reserved_budget, 8_000);
        assert!(report.context_plan.sections.iter().any(|section| {
            section.kind == ContextSectionKind::RecentTurns && !section.items.is_empty()
        }));
        assert!(
            report
                .context_plan
                .sections
                .iter()
                .any(|section| section.kind == ContextSectionKind::Memory
                    && !section.items.is_empty())
        );
        assert!(
            report
                .context_plan
                .sections
                .iter()
                .any(|section| section.kind == ContextSectionKind::Skills
                    && !section.items.is_empty())
        );
        let tool_report = runtime
            .run_tool(
                &cwd,
                "read_file".to_string(),
                serde_json::json!({ "path": ".rustycode/config.json" }),
            )
            .unwrap();
        let events = runtime.session_events(&tool_report.session.id).unwrap();
        assert!(tool_report.result.error.is_none()); // success = no error
        assert_eq!(events.len(), 2);
        assert!(events
            .iter()
            .any(|event| matches!(event.kind, rustycode_protocol::EventKind::ToolExecuted)));
    }

    #[test]
    fn code_excerpt_selection_prefers_task_matches() {
        let cwd = temp_dir();
        fs::create_dir_all(cwd.join("src")).unwrap();
        fs::write(
            cwd.join("src").join("planner.rs"),
            "pub fn planner_budget() {\n    let budget = 10;\n}\n",
        )
        .unwrap();
        fs::write(
            cwd.join("README.md"),
            "# RustyCode\n\nGeneral project notes.\n",
        )
        .unwrap();

        let excerpts = select_code_excerpts(&cwd, "planner budget", 2).unwrap();

        assert_eq!(excerpts.len(), 2);
        assert!(excerpts[0].path.ends_with("planner.rs"));
        assert!(excerpts[0].score >= excerpts[1].score);
    }

    // LLM plan generation tests
    #[tokio::test]
    async fn generate_plan_with_llm_parses_pure_json() {
        use rustycode_llm::provider::{
            CompletionRequest, CompletionResponse as CompletionResponseV2, LLMProvider,
            ProviderConfig, Usage,
        };

        struct MockProvider {
            content: String,
            config: ProviderConfig,
        }

        #[async_trait::async_trait]
        impl LLMProvider for MockProvider {
            fn name(&self) -> &'static str {
                "mock"
            }

            async fn is_available(&self) -> bool {
                true
            }

            async fn list_models(
                &self,
            ) -> Result<Vec<String>, rustycode_llm::provider::ProviderError> {
                Ok(vec!["mock-model".to_string()])
            }

            async fn complete(
                &self,
                request: CompletionRequest,
            ) -> Result<CompletionResponseV2, rustycode_llm::provider::ProviderError> {
                Ok(CompletionResponseV2 {
                    content: self.content.clone(),
                    model: request.model,
                    usage: Some(Usage::new(100, 50)),
                    stop_reason: None,
                    citations: Some(Vec::new()),
                    thinking_blocks: None,
                    structured_output: None,
                })
            }

            async fn complete_stream(
                &self,
                _request: CompletionRequest,
            ) -> Result<
                Pin<Box<dyn Stream<Item = rustycode_llm::provider::StreamChunk> + Send>>,
                rustycode_llm::provider::ProviderError,
            > {
                Err(rustycode_llm::provider::ProviderError::Configuration(
                    "stream not implemented".to_string(),
                ))
            }

            fn config(&self) -> Option<&ProviderConfig> {
                Some(&self.config)
            }
        }

        let json = r#"
        {
          "summary": "Do the thing",
          "approach": "Simple approach",
          "steps": [
            {
              "title": "Step One",
              "description": "Do step one",
              "tools": ["read_file"],
              "expected_outcome": "Done",
              "rollback_hint": "N/A"
            }
          ],
          "files_to_modify": ["src/lib.rs"],
          "risks": ["low risk"]
        }
        "#;

        let provider = MockProvider {
            content: json.to_string(),
            config: ProviderConfig::default(),
        };

        let plan = generate_plan_with_llm(&provider, "task", &["read_file"]).expect("parsed plan");

        assert_eq!(plan.summary, "Do the thing");
        assert_eq!(plan.approach, "Simple approach");
        assert_eq!(plan.steps.len(), 1);
        assert_eq!(plan.steps[0].title, "Step One");
        assert_eq!(plan.files_to_modify, vec!["src/lib.rs".to_string()]);
        assert_eq!(plan.risks, vec!["low risk".to_string()]);
    }

    #[tokio::test]
    async fn generate_plan_with_llm_parses_markdown_wrapped_json() {
        use futures::Stream;
        use rustycode_llm::provider::{
            CompletionRequest, CompletionResponse as CompletionResponseV2, LLMProvider,
            ProviderConfig, Usage,
        };

        struct MockProvider {
            content: String,
            config: ProviderConfig,
        }

        #[async_trait::async_trait]
        impl LLMProvider for MockProvider {
            fn name(&self) -> &'static str {
                "mock"
            }

            async fn is_available(&self) -> bool {
                true
            }

            async fn list_models(
                &self,
            ) -> Result<Vec<String>, rustycode_llm::provider::ProviderError> {
                Ok(vec!["mock-model".to_string()])
            }

            async fn complete(
                &self,
                request: CompletionRequest,
            ) -> Result<CompletionResponseV2, rustycode_llm::provider::ProviderError> {
                Ok(CompletionResponseV2 {
                    content: self.content.clone(),
                    model: request.model,
                    usage: Some(Usage::new(100, 50)),
                    stop_reason: None,
                    citations: Some(Vec::new()),
                    thinking_blocks: None,
                    structured_output: None,
                })
            }

            async fn complete_stream(
                &self,
                _request: CompletionRequest,
            ) -> Result<
                Pin<Box<dyn Stream<Item = rustycode_llm::provider::StreamChunk> + Send>>,
                rustycode_llm::provider::ProviderError,
            > {
                Err(rustycode_llm::provider::ProviderError::Configuration(
                    "stream not implemented".to_string(),
                ))
            }

            fn config(&self) -> Option<&ProviderConfig> {
                Some(&self.config)
            }
        }

        let body = r#"
        {
          "summary": "Wrapped",
          "approach": "Wrap approach",
          "steps": [
            { "title": "Wrapped Step", "description": "x", "tools": [], "expected_outcome": "ok", "rollback_hint": "N/A" }
          ]
        }
        "#;

        let wrapped = format!("Here is the plan:\n```json\n{}\n```", body);

        let provider = MockProvider {
            content: wrapped,
            config: ProviderConfig::default(),
        };

        let plan = generate_plan_with_llm(&provider, "task", &[]).expect("parsed wrapped plan");
        assert_eq!(plan.summary, "Wrapped");
        assert_eq!(plan.steps.len(), 1);
        assert_eq!(plan.steps[0].title, "Wrapped Step");
    }

    #[test]
    fn generate_smart_plan_falls_back_when_llm_fails() {
        use futures::Stream;
        use rustycode_llm::provider::{
            CompletionRequest, CompletionResponse as CompletionResponseV2, LLMProvider,
            ProviderConfig,
        };

        struct BadProvider {
            config: ProviderConfig,
        }

        #[async_trait::async_trait]
        impl LLMProvider for BadProvider {
            fn name(&self) -> &'static str {
                "bad_provider"
            }

            async fn is_available(&self) -> bool {
                true
            }

            async fn list_models(
                &self,
            ) -> Result<Vec<String>, rustycode_llm::provider::ProviderError> {
                Ok(vec!["bad-model".to_string()])
            }

            async fn complete(
                &self,
                _request: CompletionRequest,
            ) -> Result<CompletionResponseV2, rustycode_llm::provider::ProviderError> {
                Err(rustycode_llm::provider::ProviderError::Api(
                    "simulated failure".to_string(),
                ))
            }

            async fn complete_stream(
                &self,
                _request: CompletionRequest,
            ) -> Result<
                Pin<Box<dyn Stream<Item = rustycode_llm::provider::StreamChunk> + Send>>,
                rustycode_llm::provider::ProviderError,
            > {
                Err(rustycode_llm::provider::ProviderError::Configuration(
                    "stream not implemented".to_string(),
                ))
            }

            fn config(&self) -> Option<&ProviderConfig> {
                Some(&self.config)
            }
        }

        let provider = BadProvider {
            config: ProviderConfig::default(),
        };
        let plan = generate_smart_plan("do stuff", &[], Some(&provider));
        assert!(plan.summary.starts_with("Plan for:"));
        assert!(!plan.steps.is_empty());
        assert_eq!(plan.steps[0].title, "Explore codebase");
    }
}
