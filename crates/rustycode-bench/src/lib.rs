#![allow(unsafe_code)]
#![allow(
    clippy::branches_sharing_code,
    clippy::case_sensitive_file_extension_comparisons,
    clippy::cast_lossless,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::default_trait_access,
    clippy::derive_partial_eq_without_eq,
    clippy::doc_markdown,
    clippy::expect_used,
    clippy::format_push_string,
    clippy::if_not_else,
    clippy::ignored_unit_patterns,
    clippy::implicit_hasher,
    clippy::items_after_statements,
    clippy::literal_string_with_formatting_args,
    clippy::manual_assert,
    clippy::manual_let_else,
    clippy::manual_midpoint,
    clippy::map_unwrap_or,
    clippy::match_same_arms,
    clippy::match_wildcard_for_single_variants,
    clippy::missing_const_for_fn,
    clippy::missing_fields_in_debug,
    clippy::needless_continue,
    clippy::needless_pass_by_value,
    clippy::needless_raw_string_hashes,
    clippy::non_std_lazy_statics,
    clippy::option_if_let_else,
    clippy::or_fun_call,
    clippy::redundant_clone,
    clippy::redundant_closure_for_method_calls,
    clippy::redundant_else,
    clippy::significant_drop_tightening,
    clippy::single_match_else,
    clippy::stable_sort_primitive,
    clippy::struct_excessive_bools,
    clippy::struct_field_names,
    clippy::suboptimal_flops,
    clippy::too_many_lines,
    clippy::unchecked_time_subtraction,
    clippy::uninlined_format_args,
    clippy::unnecessary_debug_formatting,
    clippy::unnecessary_literal_bound,
    clippy::unnested_or_patterns,
    clippy::unreadable_literal,
    clippy::unused_self,
    clippy::unwrap_used
)]
#![cfg_attr(test, allow(clippy::float_cmp,))]
//! Benchmark runner for agent evaluation.
//!
//! Provides a Harbor-compatible pipeline for running benchmark tasks:
//! environment (container) → agent → verifier → result.
//!
//! Supports two execution modes:
//! - **Docker** — Uses Docker/Podman containers ( Harbor-compatible )
//! - **Native** — Runs tasks directly on the host ( no QEMU overhead on macOS )
//!
//! # Quick Start
//!
//! ```bash
//! # Run Terminal Bench 2.0 oracle (validates infrastructure)
//! rtk-bench run --dataset terminal-bench@2.0 --agent oracle --n-concurrent 4
//!
//! # Run with an LLM agent
//! rtk-bench run --dataset ./my-tasks --agent code --model claude-sonnet-4-6
//!
//! # List available datasets
//! rtk-bench list
//!
//! # Native execution (no Docker)
//! rtk-bench run --dataset ./my-tasks --agent oracle --env native
//! ```

pub mod agent;
pub mod config;
pub mod dataset;
pub mod environment;
pub mod history;
pub mod hooks;
pub mod job;
pub mod mcp_bridge;
pub mod registry;
pub mod report;
pub mod runner;
pub mod swebench;
pub mod task;
pub mod trial;
pub mod verifier;

pub use agent::{BenchAgent, CodeAgent, CodeAgentConfig, NopAgent, OracleAgent};
pub use config::{
    create_agent, resolve_provider_model, AgentConfig, BenchConfig, ResourceOverrides,
};
pub use dataset::{DatasetInfo, DatasetRegistry};
pub use environment::bollard_env::BollardEnvironment;
pub use environment::native::NativeEnvironment;
pub use environment::{BenchEnvironment, ExecResult};
pub use hooks::{Hooks, TrialEvent};
pub use job::{BenchmarkResults, Job, JobConfig};
pub use mcp_bridge::{BenchMcpBridge, ToolResult as BenchToolResult};
pub use registry::RegistryDownloader;
pub use runner::AgentFactory;
pub use runner::{DockerRunner, DockerRunnerConfig, NativeRunner, NativeRunnerConfig};
pub use task::steps::{MultiStepConfig, TaskStep};
pub use task::{ResolvedTask, TaskConfig};
pub use trial::artifacts::{collect_trial_artifacts, Artifact, ArtifactFilter};
pub use trial::{RetryConfig, Trial, TrialResult};
pub use verifier::native::NativeVerifier;
pub use verifier::pass_at_k::compute_pass_at_k;
pub use verifier::reward::{CtrfReport, RewardResult};
pub use verifier::{ScriptVerifier, Verifier};
