use super::*;
use crate::{ToolOutput, ToolPermission, ToolTag};
use anyhow::anyhow;
use serde_json::json;
use std::sync::atomic::Ordering;

rustycode_tools_api::define_tool! {
    pub struct CronCreateTool;


    name: "CronCreate",
    namespace: "cron",
    description: r#"Schedule a prompt to be enqueued at a future time. Use for both recurring schedules and one-shot reminders.

Uses standard 5-field cron in the user's local timezone: minute hour day-of-month month day-of-week. "0 9 * * *" means 9am local — no timezone conversion needed.

## One-shot tasks (recurring: false)
For "remind me at X" — fire once then auto-delete.
Pin minute/hour/day-of-month/month to specific values.

## Recurring jobs (recurring: true, default)
For "every N minutes" / "every hour" / "weekdays at 9am".

Returns a job ID you can pass to cron_delete."#,
    permission: ToolPermission::None,
    tags: [ToolTag::Ops],

    execute(params: CronCreateParams, ctx) {
        let cron = &params.cron;
        let prompt = &params.prompt;

        if prompt.trim().is_empty() {
            return Err(anyhow!("prompt must not be empty"));
        }

        validate_cron(cron)?;

        let recurring = params.recurring;
        let durable = params.durable;

        let job_id = format!("cron-{}", JOB_COUNTER.fetch_add(1, Ordering::Relaxed));

        Ok(ToolOutput::text(format!("Scheduled job {job_id}: {cron}")).with_metadata(ctx, || json!({
                "job_id": job_id,
                "cron": cron,
                "recurring": recurring,
                "durable": durable,
            })))
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests_common::*;
    use super::*;
    use crate::Tool;

    #[test]
    fn test_cron_create_metadata() {
        let tool = CronCreateTool;
        assert_eq!(tool.name(), "CronCreate");
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
    fn test_validate_cron_expression() {
        assert!(validate_cron("*/5 * * * *").is_ok());
        assert!(validate_cron("0 9 * * 1-5").is_ok());
        assert!(validate_cron("30 14 28 2 *").is_ok());
        assert!(validate_cron("").is_err());
        assert!(validate_cron("* * *").is_err());
        assert!(validate_cron("* * * * * *").is_err());
    }
}
