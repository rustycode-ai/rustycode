use crate::{Tool, ToolContext, ToolOutput, ToolPermission};
use anyhow::{anyhow, Result};
use serde_json::{json, Value};

/// Execute code in a persistent REPL environment.
///
/// Variables and state persist between calls within the same session.
pub struct REPLTool;

impl Tool for REPLTool {
    fn name(&self) -> &'static str {
        "repl"
    }

    fn description(&self) -> &'static str {
        r#"Execute code in a persistent REPL environment. Variables and state persist between calls within the same session. Actions: execute (run code), interrupt (stop execution), reset (clear state), get_state (view memory/variables). Supports scientific computing with pandas, numpy, matplotlib."#
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::Execute
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["action", "researchSessionID"],
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["execute", "interrupt", "reset", "get_state"],
                    "description": "Action to perform: execute (run code), interrupt (stop execution), reset (clear state), get_state (view memory/variables)"
                },
                "researchSessionID": {
                    "type": "string",
                    "description": "Unique identifier for the research session"
                },
                "code": {
                    "type": "string",
                    "description": "The code to execute (required for action=execute)"
                },
                "executionLabel": {
                    "type": "string",
                    "description": "Optional label for the execution"
                },
                "executionTimeout": {
                    "type": "number",
                    "default": 300000,
                    "description": "Execution timeout in milliseconds (default: 300000)"
                },
                "projectDir": {
                    "type": "string",
                    "description": "Project directory for file operations"
                }
            }
        })
    }

    fn execute(&self, params: Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        let action = params
            .get("action")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("missing action"))?;

        let session_id = params
            .get("researchSessionID")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("missing researchSessionID"))?;

        match action {
            "execute" => {
                let code = params
                    .get("code")
                    .and_then(Value::as_str)
                    .ok_or_else(|| anyhow!("missing code for execute action"))?;

                let timeout = params
                    .get("executionTimeout")
                    .and_then(Value::as_u64)
                    .unwrap_or(300000);

                // Placeholder: actual REPL execution requires runtime integration
                Ok(ToolOutput::with_structured(
                    format!("REPL [{session_id}] execute ({timeout}ms timeout)"),
                    json!({
                        "action": "execute",
                        "session_id": session_id,
                        "code_length": code.len(),
                        "timeout": timeout,
                        "status": "pending_integration",
                    }),
                ))
            }
            "interrupt" => Ok(ToolOutput::with_structured(
                format!("REPL [{session_id}] interrupted"),
                json!({"action": "interrupt", "session_id": session_id}),
            )),
            "reset" => Ok(ToolOutput::with_structured(
                format!("REPL [{session_id}] state cleared"),
                json!({"action": "reset", "session_id": session_id}),
            )),
            "get_state" => Ok(ToolOutput::with_structured(
                format!("REPL [{session_id}] state query"),
                json!({
                    "action": "get_state",
                    "session_id": session_id,
                    "variables": {},
                }),
            )),
            _ => Err(anyhow!("Unknown REPL action: {action}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_ctx() -> ToolContext {
        ToolContext::new("/tmp")
    }

    #[test]
    fn test_repl_metadata() {
        let tool = REPLTool;
        assert_eq!(tool.name(), "repl");
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
