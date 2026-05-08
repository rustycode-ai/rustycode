//! Executor module — tool dispatch, caching, batching, and inspection.
//!
// ~6400 LOC across 11 files.

pub mod auto_tool;
pub mod batch;
pub mod batch_state;
pub mod cache;
pub mod convoy;
pub mod decompose;
pub mod gate;
pub mod inspector;
pub mod middleware;
pub mod task;
pub mod task_state;
pub mod tool_shim;

// Re-export key types for backward compatibility.
pub use auto_tool::{AutoTool, AutoToolConfig, AutoToolContext};
pub use batch::BatchTool;
pub use cache::{CacheConfig, CacheKey, CacheMetrics, CacheStats, CachedToolResult, ToolCache};
pub use convoy::ConvoyDispatcher;
pub use decompose::{DecomposeProblemTool, DecompositionResult, Module};
pub use inspector::{
    BudgetInspector, InspectionAction, InspectionResult, PermissionInspector, RateLimitInspector,
    RepetitionInspector, SecurityInspector, ToolCallInfo, ToolInspectionManager,
};
pub use middleware::{ExecutionMiddleware, MiddlewareConfig, MiddlewareState, PlanModeState};
pub use task::{SubAgentRunner, TaskTool};
pub use tool_shim::{
    extract_tool_calls, extract_tool_calls_with_config, format_tools_for_prompt,
    is_valid_function_name, sanitize_function_name, tool_calls_to_text, ExtractedToolCall,
    ExtractionSource, ExtractorConfig, ToolCallExtractor,
};
