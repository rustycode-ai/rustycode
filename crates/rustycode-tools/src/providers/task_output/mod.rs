use schemars::JsonSchema;

// Re-export all tools
pub use output::*;
pub use stop::*;

pub mod output;
pub mod stop;

#[derive(serde::Deserialize, JsonSchema)]
pub struct TaskOutputParams {
    /// The task ID to get output from
    pub task_id: String,
    /// Whether to wait for completion (default: true)
    #[serde(default = "default_true")]
    pub block: bool,
    /// Max wait time in ms (default: 30000)
    #[serde(default = "default_timeout")]
    pub timeout: u64,
}

pub(crate) fn default_true() -> bool {
    true
}

pub(crate) fn default_timeout() -> u64 {
    30000
}

#[derive(serde::Deserialize, JsonSchema)]
pub struct TaskStopParams {
    /// The task ID to stop
    pub task_id: String,
}

#[cfg(test)]
pub(crate) mod tests_common {
    use crate::ToolContext;

    pub fn test_ctx() -> ToolContext {
        ToolContext::new("/tmp")
    }
}
