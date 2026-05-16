use anyhow::{anyhow, Result};
use schemars::JsonSchema;
use std::sync::atomic::AtomicU64;

// Re-export all tools
pub use create::*;
pub use delete::*;
pub use list::*;

pub mod create;
pub mod delete;
pub mod list;

pub(crate) static JOB_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(serde::Deserialize, JsonSchema)]
pub struct CronCreateParams {
    /// Standard 5-field cron expression in local time: M H DoM Mon DoW (e.g., '*/5 * * * *' = every 5 min, '30 14 28 2 *' = Feb 28 at 2:30pm local once)
    pub cron: String,
    /// The prompt to enqueue at each fire time
    pub prompt: String,
    /// true = fire on every cron match until deleted; false = fire once at next match then auto-delete
    #[serde(default = "default_true")]
    pub recurring: bool,
    /// true = persist to .claude/scheduled_tasks.json and survive restarts. Only use when user explicitly asks for persistence
    #[serde(default)]
    pub durable: bool,
}

use rustycode_protocol::default_true;

#[derive(serde::Deserialize, JsonSchema)]
pub struct CronDeleteParams {
    /// Job ID returned by cron_create
    pub id: String,
}

#[derive(serde::Deserialize, JsonSchema)]
pub struct CronListParams {}

/// Validate a 5-field cron expression has the right number of fields.
pub(crate) fn validate_cron(expr: &str) -> Result<()> {
    let fields: Vec<&str> = expr.split_whitespace().collect();
    if fields.len() != 5 {
        return Err(anyhow!(
            "cron expression must have exactly 5 fields (M H DoM Mon DoW), got {}: '{expr}'",
            fields.len()
        ));
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests_common {
    pub use crate::test_helpers::test_ctx_with_structured_output as test_ctx;
}
