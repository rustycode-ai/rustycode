//! Unified callable abstraction for `RustyCode`
//!
//! Treats tools, skills, and agents as context-dependent `ExecutableUnits`.

pub mod types;
pub mod router;
pub mod registry;
pub mod discovery;
pub mod constants;
pub mod programmatic;

// Re-export commonly used types
pub use types::{
    ExecutableUnit, ExecutionContext, ExecutionCapability, Callable,
    ExecutionInput, ExecutionOutput, ExecutionMetadata, ExecutableError, UnitCapabilities,
    AdvancedToolMetadata, ExecutionMode, ExecutionExample, UnitSource,
    ToolSchema, ResultProcessor,
};
pub use router::ExecutionRouter;
pub use registry::ExecutableRegistry;
pub use discovery::ToolSearchService;
pub use programmatic::{CallChain, ChainStep, ChainResult, InputTransform, OutputTransform};
