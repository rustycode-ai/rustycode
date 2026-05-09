use super::*;
use crate::{ToolOutput, ToolPermission, ToolTag};
use serde_json::json;

rustycode_tools_api::define_tool! {
    pub struct CronListTool;

    name: "cron_list",
    description: r#"List all cron jobs scheduled via cron_create in this session."#,
    permission: ToolPermission::None,
    tags: [ToolTag::Ops],

    execute(_params: CronListParams, ctx) {
        // Placeholder: actual listing from scheduler requires runtime integration
        Ok(ToolOutput::text("No active cron jobs".to_string()).with_metadata(ctx, || json!({"jobs": []})))
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests_common::*;
    use super::*;
    use crate::Tool;

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
}
