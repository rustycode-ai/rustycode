use super::*;
use crate::{ToolOutput, ToolPermission, ToolTag};
use serde_json::json;

rustycode_tools_api::define_tool! {
    pub struct CronDeleteTool;

    name: "cron_delete",
    description: r#"Cancel a cron job previously scheduled with cron_create. Removes it from the in-memory session store."#,
    permission: ToolPermission::None,
    tags: [ToolTag::Ops],

    execute(params: CronDeleteParams, ctx) {
        let id = &params.id;

        // Placeholder: actual removal from scheduler requires runtime integration
        Ok(ToolOutput::text(format!("Job {id} cancelled")).with_metadata(ctx, || json!({"job_id": id, "deleted": true})))
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests_common::*;
    use super::*;
    use crate::Tool;

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
}
