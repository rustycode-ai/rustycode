//! MCP bridge — exposes `BenchEnvironment` operations as MCP-compatible tools.
//!
//! This module provides tool definitions and execution logic that wraps
//! a `BenchEnvironment` (container), allowing any MCP-compatible agent
//! to exec commands, upload files, and download files in the container.
//!
//! # Usage
//!
//! ```ignore
//! let bridge = BenchMcpBridge::new(env);
//! let tools = bridge.tool_definitions();
//! // Pass `tools` to an MCP agent's tool list
//!
//! // When the agent calls a tool:
//! let result = bridge.execute_tool("bench_exec", args).await?;
//! ```

// Clippy doesn't see that `&mut self` is needed to reborrow `self.env`
// which is `&mut dyn BenchEnvironment`.
#![allow(clippy::needless_pass_by_ref_mut)]

use std::path::Path;

use serde_json::json;

use crate::environment::BenchEnvironment;

/// MCP bridge that wraps a `BenchEnvironment` for tool-based access.
///
/// Provides three tools:
/// - `bench_exec` — execute a command in the container
/// - `bench_upload` — upload a file to the container
/// - `bench_download` — download a file from the container
pub struct BenchMcpBridge<'a> {
    env: &'a mut dyn BenchEnvironment,
}

impl<'a> BenchMcpBridge<'a> {
    /// Create a new bridge wrapping the given environment.
    pub fn new(env: &'a mut dyn BenchEnvironment) -> Self {
        Self { env }
    }

    /// Return MCP-compatible tool definitions for the bridge.
    pub fn tool_definitions() -> Vec<serde_json::Value> {
        vec![
            json!({
                "name": "bench_exec",
                "description": "Execute a command in the benchmark container. Returns stdout, stderr, and exit code.",
                "input_schema": {
                    "type": "object",
                    "properties": {
                        "command": {
                            "type": "string",
                            "description": "The shell command to execute"
                        },
                        "timeout_secs": {
                            "type": "integer",
                            "description": "Optional timeout in seconds (default: 300)"
                        }
                    },
                    "required": ["command"]
                }
            }),
            json!({
                "name": "bench_upload",
                "description": "Upload a local file to the benchmark container.",
                "input_schema": {
                    "type": "object",
                    "properties": {
                        "local_path": {
                            "type": "string",
                            "description": "Path to the local file to upload"
                        },
                        "container_path": {
                            "type": "string",
                            "description": "Destination path inside the container"
                        }
                    },
                    "required": ["local_path", "container_path"]
                }
            }),
            json!({
                "name": "bench_download",
                "description": "Download a file from the benchmark container to the local filesystem.",
                "input_schema": {
                    "type": "object",
                    "properties": {
                        "container_path": {
                            "type": "string",
                            "description": "Path of the file inside the container"
                        },
                        "local_path": {
                            "type": "string",
                            "description": "Destination path on the local filesystem"
                        }
                    },
                    "required": ["container_path", "local_path"]
                }
            }),
        ]
    }

    /// Execute a tool call by name.
    ///
    /// # Errors
    ///
    /// Returns an error if the tool name is unknown or the execution fails.
    pub async fn execute_tool(
        &mut self,
        name: &str,
        args: &serde_json::Value,
    ) -> anyhow::Result<ToolResult> {
        match name {
            "bench_exec" => self.exec(args).await,
            "bench_upload" => self.upload(args).await,
            "bench_download" => self.download(args).await,
            other => anyhow::bail!("Unknown bench tool: {other}"),
        }
    }

    async fn exec(&mut self, args: &serde_json::Value) -> anyhow::Result<ToolResult> {
        let command = args["command"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'command' argument"))?;

        let timeout_secs = args["timeout_secs"].as_u64().unwrap_or(300);

        let result = self.env.exec_with_timeout(command, timeout_secs).await?;

        let output = format_output(&result.stdout, &result.stderr, result.exit_code);

        Ok(ToolResult {
            content: json!({
                "stdout": result.stdout,
                "stderr": result.stderr,
                "exit_code": result.exit_code,
                "output": output,
            }),
            is_error: !result.success(),
        })
    }

    async fn upload(&mut self, args: &serde_json::Value) -> anyhow::Result<ToolResult> {
        let local_path = args["local_path"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'local_path' argument"))?;
        let container_path = args["container_path"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'container_path' argument"))?;

        self.env
            .upload_file(Path::new(local_path), container_path)
            .await?;

        Ok(ToolResult {
            content: json!({
                "message": format!("Uploaded {local_path} to {container_path}"),
            }),
            is_error: false,
        })
    }

    async fn download(&mut self, args: &serde_json::Value) -> anyhow::Result<ToolResult> {
        let container_path = args["container_path"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'container_path' argument"))?;
        let local_path = args["local_path"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'local_path' argument"))?;

        self.env
            .download_file(container_path, Path::new(local_path))
            .await?;

        Ok(ToolResult {
            content: json!({
                "message": format!("Downloaded {container_path} to {local_path}"),
            }),
            is_error: false,
        })
    }
}

/// Result from executing a bench MCP tool.
#[derive(Debug)]
pub struct ToolResult {
    /// JSON content of the result.
    pub content: serde_json::Value,
    /// Whether the result represents an error.
    pub is_error: bool,
}

/// Format command output for display.
fn format_output(stdout: &str, stderr: &str, exit_code: i32) -> String {
    let mut out = String::new();
    if !stdout.is_empty() {
        out.push_str(stdout.trim());
    }
    if !stderr.is_empty() {
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str("STDERR: ");
        out.push_str(stderr.trim());
    }
    if !exit_code == 0 && out.is_empty() {
        out = "(no output)".to_string();
    }
    if exit_code != 0 {
        use std::fmt::Write;
        let _ = write!(out, "\n(exit code: {exit_code})");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_definitions_count() {
        let tools = BenchMcpBridge::tool_definitions();
        assert_eq!(tools.len(), 3);
    }

    #[test]
    fn test_tool_definitions_have_names() {
        let tools = BenchMcpBridge::tool_definitions();
        let names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
        assert!(names.contains(&"bench_exec"));
        assert!(names.contains(&"bench_upload"));
        assert!(names.contains(&"bench_download"));
    }

    #[test]
    fn test_tool_definitions_have_schemas() {
        let tools = BenchMcpBridge::tool_definitions();
        for tool in &tools {
            assert!(tool["input_schema"].is_object());
            assert!(tool["input_schema"]["properties"].is_object());
            assert!(tool["input_schema"]["required"].is_array());
        }
    }

    #[test]
    fn test_format_output_success() {
        let out = format_output("hello\n", "", 0);
        assert_eq!(out, "hello");
    }

    #[test]
    fn test_format_output_with_stderr() {
        let out = format_output("out", "err", 0);
        assert!(out.contains("STDERR: err"));
    }

    #[test]
    fn test_format_output_nonzero_exit() {
        let out = format_output("", "", 1);
        assert!(out.contains("exit code: 1"));
    }

    #[test]
    fn test_format_output_empty_success() {
        let out = format_output("", "", 0);
        assert_eq!(out, "");
    }

    #[test]
    fn test_format_output_with_both() {
        let out = format_output("stdout", "stderr", 42);
        assert!(out.contains("stdout"));
        assert!(out.contains("STDERR: stderr"));
        assert!(out.contains("exit code: 42"));
    }
}
