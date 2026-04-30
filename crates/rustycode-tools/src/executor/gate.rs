// Re-export ToolGate from rustycode_tools_api.
// The trait definition lives in rustycode_tools_api so that ToolContext can
// reference it without creating a circular dependency.
pub use rustycode_tools_api::ToolGate;
