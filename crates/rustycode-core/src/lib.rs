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
    clippy::derivable_impls,
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
pub mod team;
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
pub use rustycode_protocol::{Plan, PlanId, PlanStatus, PlanStep, StepStatus};
pub use session::{AiMode, MessageType, SessionState, ToolExecution, ToolStatus};
mod sleep;
pub use rustycode_shared_runtime as shared_runtime;

pub mod code_utils;

pub use code_utils::{
    estimate_tokens, is_supported_source, select_code_excerpts, should_skip_path, task_terms,
};

// Plan generation re-exports (moved to plan::generation)
pub use plan::generation::{
    generate_plan_with_llm, generate_plan_with_llm_async, generate_smart_plan,
    generate_smart_plan_async, render_plan_markdown,
};

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
                "Read".to_string(),
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
              "tools": ["Read"],
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

        let plan = generate_plan_with_llm(&provider, "task", &["Read"]).expect("parsed plan");

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
