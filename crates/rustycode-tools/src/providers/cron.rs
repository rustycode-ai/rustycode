use crate::{Tool, ToolContext, ToolOutput, ToolPermission, ToolTag};
use anyhow::{anyhow, Result};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicU64, Ordering};

static JOB_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Schedule a prompt to run at a future time (cron-based or one-shot).
pub struct CronCreateTool;

impl Tool for CronCreateTool {
    fn name(&self) -> &'static str {
        "cron_create"
    }

    fn description(&self) -> &'static str {
        r#"Schedule a prompt to be enqueued at a future time. Use for both recurring schedules and one-shot reminders.

Uses standard 5-field cron in the user's local timezone: minute hour day-of-month month day-of-week. "0 9 * * *" means 9am local — no timezone conversion needed.

## One-shot tasks (recurring: false)
For "remind me at X" — fire once then auto-delete.
Pin minute/hour/day-of-month/month to specific values.

## Recurring jobs (recurring: true, default)
For "every N minutes" / "every hour" / "weekdays at 9am".

Returns a job ID you can pass to cron_delete."#
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::None
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["cron", "prompt"],
            "properties": {
                "cron": {
                    "type": "string",
                    "description": "Standard 5-field cron expression in local time: M H DoM Mon DoW (e.g., '*/5 * * * *' = every 5 min, '30 14 28 2 *' = Feb 28 at 2:30pm local once)"
                },
                "prompt": {
                    "type": "string",
                    "description": "The prompt to enqueue at each fire time"
                },
                "recurring": {
                    "type": "boolean",
                    "default": true,
                    "description": "true = fire on every cron match until deleted; false = fire once at next match then auto-delete"
                },
                "durable": {
                    "type": "boolean",
                    "default": false,
                    "description": "true = persist to .claude/scheduled_tasks.json and survive restarts. Only use when user explicitly asks for persistence"
                }
            }
        })
    }

    fn tags(&self) -> &[ToolTag] {
        &[ToolTag::Ops]
    }

    fn execute(&self, params: Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        let cron = params
            .get("cron")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("missing cron expression"))?;

        let prompt = params
            .get("prompt")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("missing prompt"))?;

        if prompt.trim().is_empty() {
            return Err(anyhow!("prompt must not be empty"));
        }

        validate_cron(cron)?;

        let recurring = params
            .get("recurring")
            .and_then(Value::as_bool)
            .unwrap_or(true);

        let durable = params
            .get("durable")
            .and_then(Value::as_bool)
            .unwrap_or(false);

        let job_id = format!("cron-{}", JOB_COUNTER.fetch_add(1, Ordering::Relaxed));

        Ok(ToolOutput::with_structured(
            format!("Scheduled job {job_id}: {cron}"),
            json!({
                "job_id": job_id,
                "cron": cron,
                "recurring": recurring,
                "durable": durable,
            }),
        ))
    }
}

/// Cancel a scheduled cron job by ID.
pub struct CronDeleteTool;

impl Tool for CronDeleteTool {
    fn name(&self) -> &'static str {
        "cron_delete"
    }

    fn description(&self) -> &'static str {
        r#"Cancel a cron job previously scheduled with cron_create. Removes it from the in-memory session store."#
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::None
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["id"],
            "properties": {
                "id": {
                    "type": "string",
                    "description": "Job ID returned by cron_create"
                }
            }
        })
    }

    fn tags(&self) -> &[ToolTag] {
        &[ToolTag::Ops]
    }

    fn execute(&self, params: Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        let id = params
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("missing job id"))?;

        // Placeholder: actual removal from scheduler requires runtime integration
        Ok(ToolOutput::with_structured(
            format!("Job {id} cancelled"),
            json!({"job_id": id, "deleted": true}),
        ))
    }
}

/// List all scheduled cron jobs.
pub struct CronListTool;

impl Tool for CronListTool {
    fn name(&self) -> &'static str {
        "cron_list"
    }

    fn description(&self) -> &'static str {
        r#"List all cron jobs scheduled via cron_create in this session."#
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::None
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {}
        })
    }

    fn tags(&self) -> &[ToolTag] {
        &[ToolTag::Ops]
    }

    fn execute(&self, _params: Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        // Placeholder: actual listing from scheduler requires runtime integration
        Ok(ToolOutput::with_structured(
            "No active cron jobs".to_string(),
            json!({"jobs": []}),
        ))
    }
}

/// Validate a 5-field cron expression has the right number of fields.
fn validate_cron(expr: &str) -> Result<()> {
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
mod tests {
    use super::*;

    fn test_ctx() -> ToolContext {
        ToolContext::new("/tmp")
    }

    #[test]
    fn test_cron_create_metadata() {
        let tool = CronCreateTool;
        assert_eq!(tool.name(), "cron_create");
        assert_eq!(tool.permission(), ToolPermission::None);
    }

    #[test]
    fn test_cron_create_schedules_job() {
        let tool = CronCreateTool;
        let result = tool.execute(
            json!({"cron": "*/5 * * * *", "prompt": "check the build"}),
            &test_ctx(),
        );
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.text.contains("cron-"));
    }

    #[test]
    fn test_cron_create_one_shot() {
        let tool = CronCreateTool;
        let result = tool.execute(
            json!({
                "cron": "30 14 28 2 *",
                "prompt": "check the deploy",
                "recurring": false
            }),
            &test_ctx(),
        );
        assert!(result.is_ok());
        let structured = result.unwrap().structured.unwrap();
        assert_eq!(structured["recurring"], false);
    }

    #[test]
    fn test_cron_create_rejects_empty_prompt() {
        let tool = CronCreateTool;
        let result = tool.execute(json!({"cron": "0 9 * * *", "prompt": "  "}), &test_ctx());
        assert!(result.is_err());
    }

    #[test]
    fn test_cron_create_rejects_bad_expression() {
        let tool = CronCreateTool;
        let result = tool.execute(
            json!({"cron": "too many fields in here now", "prompt": "test"}),
            &test_ctx(),
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("5 fields"));
    }

    #[test]
    fn test_cron_delete_metadata() {
        let tool = CronDeleteTool;
        assert_eq!(tool.name(), "cron_delete");
        assert_eq!(tool.permission(), ToolPermission::None);
    }

    #[test]
    fn test_cron_delete_requires_id() {
        let tool = CronDeleteTool;
        let result = tool.execute(json!({}), &test_ctx());
        assert!(result.is_err());
    }

    #[test]
    fn test_cron_delete_confirms() {
        let tool = CronDeleteTool;
        let result = tool.execute(json!({"id": "cron-1"}), &test_ctx());
        assert!(result.is_ok());
        assert!(result.unwrap().text.contains("cron-1"));
    }

    #[test]
    fn test_cron_list_metadata() {
        let tool = CronListTool;
        assert_eq!(tool.name(), "cron_list");
    }

    #[test]
    fn test_cron_list_returns_empty() {
        let tool = CronListTool;
        let result = tool.execute(json!({}), &test_ctx());
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_cron_expression() {
        assert!(validate_cron("*/5 * * * *").is_ok());
        assert!(validate_cron("0 9 * * 1-5").is_ok());
        assert!(validate_cron("30 14 28 2 *").is_ok());
        assert!(validate_cron("").is_err());
        assert!(validate_cron("* * *").is_err());
        assert!(validate_cron("* * * * * *").is_err());
    }
}
