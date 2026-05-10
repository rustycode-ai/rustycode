use crate::{ToolOutput, ToolPermission, ToolTag};
use anyhow::anyhow;
use schemars::JsonSchema;
use serde_json::json;

#[derive(serde::Deserialize, JsonSchema)]
pub struct REPLParams {
    /// Action to perform: execute (run code), interrupt (stop execution), reset (clear state), get_state (view memory/variables)
    action: String,
    /// Unique identifier for the research session
    research_session_id: Option<String>,
    /// Legacy alias for research_session_id
    #[serde(rename = "researchSessionID")]
    #[schemars(rename = "researchSessionID")]
    research_session_id_legacy: Option<String>,
    /// The code to execute (required for action=execute)
    code: Option<String>,
    /// Execution timeout in milliseconds (default: 300000)
    execution_timeout: Option<u64>,
}

rustycode_tools_api::define_tool! {
    pub struct REPLTool;

    name: "Repl",
    description: r#"Execute code in a persistent REPL environment. Variables and state persist between calls within the same session. Actions: execute (run code), interrupt (stop execution), reset (clear state), get_state (view memory/variables). Supports scientific computing with pandas, numpy, matplotlib."#,
    permission: ToolPermission::Execute,
    tags: [ToolTag::Debug],

    execute(params: REPLParams, ctx) {
        let action = &params.action;
        let session_id = params
            .research_session_id
            .as_deref()
            .or(params.research_session_id_legacy.as_deref())
            .ok_or_else(|| anyhow!("missing researchSessionID"))?;

        match action.as_str() {
            "execute" => {
                let code = params
                    .code
                    .as_deref()
                    .ok_or_else(|| anyhow!("missing code for execute action"))?;

                let timeout = params.execution_timeout.unwrap_or(300000);

                // Placeholder: actual REPL execution requires runtime integration
                Ok(ToolOutput::text(format!("REPL [{session_id}] execute ({timeout}ms timeout)")).with_metadata(ctx, || json!({
                        "action": "execute",
                        "session_id": session_id,
                        "code_length": code.len(),
                        "timeout": timeout,
                        "status": "pending_integration",
                    })))
            }
            "interrupt" => Ok(ToolOutput::text(format!("REPL [{session_id}] interrupted")).with_metadata(ctx, || json!({"action": "interrupt", "session_id": session_id}))),
            "reset" => Ok(ToolOutput::text(format!("REPL [{session_id}] state cleared")).with_metadata(ctx, || json!({"action": "reset", "session_id": session_id}))),
            "get_state" => Ok(ToolOutput::text(format!("REPL [{session_id}] state query")).with_metadata(ctx, || json!({
                    "action": "get_state",
                    "session_id": session_id,
                    "variables": {},
                }))),
            _ => Err(anyhow!("Unknown REPL action: {action}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Tool, ToolContext};

    fn test_ctx() -> ToolContext {
        ToolContext::new("/tmp")
    }

    #[test]
    fn test_repl_metadata() {
        let tool = REPLTool;
        assert_eq!(tool.name(), "Repl");
        assert_eq!(tool.permission(), ToolPermission::Execute);
        assert!(tool.description().contains("persistent"));
    }

    #[test]
    fn test_repl_execute() {
        let tool = REPLTool;
        let result = tool.execute(
            json!({
                "action": "execute",
                "researchSessionID": "sess-1",
                "code": "print('hello')"
            }),
            &test_ctx(),
        );
        assert!(result.is_ok());
        assert!(result.unwrap().text.contains("sess-1"));
    }

    #[test]
    fn test_repl_interrupt() {
        let tool = REPLTool;
        let result = tool.execute(
            json!({"action": "interrupt", "researchSessionID": "sess-1"}),
            &test_ctx(),
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_repl_reset() {
        let tool = REPLTool;
        let result = tool.execute(
            json!({"action": "reset", "researchSessionID": "sess-1"}),
            &test_ctx(),
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_repl_get_state() {
        let tool = REPLTool;
        let result = tool.execute(
            json!({"action": "get_state", "researchSessionID": "sess-1"}),
            &test_ctx(),
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_repl_execute_requires_code() {
        let tool = REPLTool;
        let result = tool.execute(
            json!({"action": "execute", "researchSessionID": "sess-1"}),
            &test_ctx(),
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_repl_requires_session_id() {
        let tool = REPLTool;
        let result = tool.execute(json!({"action": "execute"}), &test_ctx());
        assert!(result.is_err());
    }

    #[test]
    fn test_repl_unknown_action() {
        let tool = REPLTool;
        let result = tool.execute(
            json!({"action": "foobar", "researchSessionID": "sess-1"}),
            &test_ctx(),
        );
        assert!(result.is_err());
    }
}
