//! Parallel tool execution with concurrency limiting and streaming results.
//!
//! This module provides two execution strategies:
//!
//! - [`ParallelExecutor`] runs multiple tools concurrently with a semaphore-based
//!   concurrency cap and returns results in input order.
//! - [`StreamingToolExecutor`] sends results via an `mpsc` channel as each tool
//!   completes, enabling early consumption without waiting for the full batch.

pub mod parallel_executor;
pub mod streaming_results;

// Re-export the shared trait so consumers can import from the module root.
pub use parallel_executor::{ParallelExecutor, ToolExecution};
pub use streaming_results::{StreamingToolExecutor, ToolResult};
