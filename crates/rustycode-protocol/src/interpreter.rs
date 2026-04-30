use crate::tool::ToolResult;

pub trait ToolResultInterpreter {
    fn is_error(&self, result: &ToolResult) -> bool;
}

pub struct DefaultInterpreter;

impl ToolResultInterpreter for DefaultInterpreter {
    fn is_error(&self, result: &ToolResult) -> bool {
        !result.success
            || result.error.is_some()
            || result.exit_code.map(|code| code != 0).unwrap_or(false)
    }
}

pub struct LenientInterpreter;

impl ToolResultInterpreter for LenientInterpreter {
    fn is_error(&self, result: &ToolResult) -> bool {
        !result.success
    }
}

pub struct StrictInterpreter;

impl ToolResultInterpreter for StrictInterpreter {
    fn is_error(&self, result: &ToolResult) -> bool {
        !result.success
            || result.error.is_some()
            || result.exit_code.map(|c| c != 0).unwrap_or(false)
            || result.output.is_empty()
    }
}
