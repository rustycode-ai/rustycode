//! Benchmark runners — orchestrate task execution in different environments.

pub mod docker;
pub mod native;

pub use docker::{DockerRunner, DockerRunnerConfig};
pub use native::{AgentFactory, NativeRunner, NativeRunnerConfig};
