//! Core type definitions for `ExecutableUnits`

pub mod callable;
pub mod context;
pub mod errors;
pub mod executable;
pub mod metadata;

pub use callable::{Callable, ExecutionInput, ExecutionMetadata, ExecutionOutput};
pub use context::{ExecutionCapability, ExecutionContext};
pub use errors::ExecutableError;
pub use executable::{ExecutableUnit, UnitSource};
pub use metadata::{
    AdvancedToolMetadata, ExecutionExample, ExecutionMode, ResultProcessor, ToolSchema,
    UnitCapabilities,
};
