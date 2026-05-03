//! Unified callable abstraction for `RustyCode`
//!
//! Treats tools, skills, and agents as context-dependent `ExecutableUnits`.

pub mod constants;
pub mod discovery;
pub mod programmatic;
pub mod registry;
pub mod router;
pub mod types;

// Re-export commonly used types
pub use discovery::ToolSearchService;
pub use programmatic::{CallChain, ChainResult, ChainStep, InputTransform, OutputTransform};
pub use registry::ExecutableRegistry;
pub use router::ExecutionRouter;
pub use types::{
    AdvancedToolMetadata, Callable, ExecutableError, ExecutableUnit, ExecutionCapability,
    ExecutionContext, ExecutionExample, ExecutionInput, ExecutionMetadata, ExecutionMode,
    ExecutionOutput, ResultProcessor, ToolSchema, UnitCapabilities, UnitSource,
};
