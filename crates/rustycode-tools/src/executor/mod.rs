//! Executor module — tool dispatch, caching, batching, and inspection.
//!
// ~6400 LOC across 12 files.

pub mod auto_tool;
pub mod batch;
pub mod batch_state;
pub mod cache;
pub mod convoy;
pub mod decompose;
pub mod executor;
pub mod gate;
pub mod inspector;
pub mod manager;
pub mod middleware;
pub mod permission;
pub mod rate_limit;
pub mod repetition;
pub mod task;
pub mod task_state;
pub mod tool_shim;

// Re-export key types for backward compatibility.
pub use auto_tool::{AutoTool, AutoToolConfig, AutoToolContext};
pub use batch::BatchTool;
pub use cache::{CacheConfig, CacheKey, CacheMetrics, CacheStats, CachedToolResult, ToolCache};
pub use convoy::ConvoyDispatcher;
pub use decompose::{DecomposeProblemTool, DecompositionResult, Module};
pub use executor::ToolDispatcher;
pub use inspector::{
    BudgetInspector, EgressInspector, InspectionAction, InspectionResult, OsvInspector,
    PermissionInspector, RateLimitInspector, RepetitionInspector, SecurityInspector, ToolCallInfo,
    ToolInspector,
};
pub use manager::ToolInspectionManager;
pub use middleware::{ExecutionMiddleware, MiddlewareConfig, MiddlewareState, PlanModeState};
pub use permission::{check_permission, check_sandbox_path, check_tool_permission};
pub use task::{SubAgentRunner, TaskTool};
pub use tool_shim::{
    extract_tool_calls, extract_tool_calls_with_config, format_tools_for_prompt,
    is_valid_function_name, tool_calls_to_text, ExtractedToolCall, ExtractionSource,
    ExtractorConfig, ToolCallExtractor,
};
