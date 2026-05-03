//! Loader for native `RustyCode` tools

use crate::registry::loaders::UnitLoader;
use crate::{
    AdvancedToolMetadata, ExecutableError, ExecutableUnit, ExecutionContext, ExecutionExample,
    ExecutionMode, UnitCapabilities, UnitSource,
};
use async_trait::async_trait;

/// Wraps native tools from `rustycode-tools` as `ExecutableUnit`s
pub struct NativeToolLoader {
    tool_names: Vec<String>,
}

impl NativeToolLoader {
    #[allow(clippy::missing_const_for_fn)]
    pub fn new(tool_names: Vec<String>) -> Self {
        Self { tool_names }
    }

    /// Create stub units for all known native tools
    fn create_stub_unit(name: &str) -> ExecutableUnit {
        let examples = Self::get_examples(name);
        ExecutableUnit {
            id: name.to_string(),
            name: name.to_string(),
            description: format!("Native tool: {name}"),
            capabilities: UnitCapabilities {
                can_execute_directly: true,
                can_bundle_knowledge: false,
                can_reason_autonomously: false,
            },
            advanced_metadata: AdvancedToolMetadata {
                examples,
                defer_loading: false,
                search_hints: vec![name.to_string()],
                execution_strategy: ExecutionMode::Direct,
                result_processor: None,
            },
            handler: std::sync::Arc::new(crate::types::callable::NoOpCallable),
            source: UnitSource::NativeTool {
                path: "native".to_string(),
            },
            schema: None,
            tags: vec!["native".to_string()],
            version: None,
        }
    }

    /// Provide usage examples for well-known native tools.
    ///
    /// These examples help the LLM understand how to invoke each tool
    /// and are included in tool definitions sent to the provider.
    fn get_examples(name: &str) -> Vec<ExecutionExample> {
        match name {
            "bash" => vec![
                ExecutionExample {
                    scenario: "List files in a directory".to_string(),
                    input: serde_json::json!({
                        "command": "ls -la /path/to/directory"
                    }),
                    output: serde_json::json!({
                        "stdout": "total 8\ndrwxr-xr-x  2 user user 64 Apr 1 10:00 .\ndrwxr-xr-x  3 user user 96 Apr 1 09:00 ..",
                        "exit_code": 0
                    }),
                    context: ExecutionContext::DirectTool {
                        immediate_result: true,
                        timeout_ms: Some(30_000),
                    },
                    explanation: Some(
                        "Run a shell command and return stdout, stderr, and exit code.".to_string(),
                    ),
                },
                ExecutionExample {
                    scenario: "Find files by name pattern".to_string(),
                    input: serde_json::json!({
                        "command": "find /src -name '*.rs' -type f"
                    }),
                    output: serde_json::json!({
                        "stdout": "/src/main.rs\n/src/lib.rs\n/src/utils.rs",
                        "exit_code": 0
                    }),
                    context: ExecutionContext::DirectTool {
                        immediate_result: true,
                        timeout_ms: Some(30_000),
                    },
                    explanation: Some(
                        "Use find to locate files matching a pattern recursively.".to_string(),
                    ),
                },
            ],
            "read" | "read_file" => vec![ExecutionExample {
                scenario: "Read a source file".to_string(),
                input: serde_json::json!({
                    "file_path": "/src/main.rs"
                }),
                output: serde_json::json!({
                    "content": "fn main() {\n    println!(\"Hello, world!\");\n}\n"
                }),
                context: ExecutionContext::DirectTool {
                    immediate_result: true,
                    timeout_ms: None,
                },
                explanation: Some("Read the full contents of a file.".to_string()),
            }],
            "edit" | "edit_file" => vec![ExecutionExample {
                scenario: "Replace a function body".to_string(),
                input: serde_json::json!({
                    "file_path": "/src/main.rs",
                    "old_string": "println!(\"Hello, world!\");",
                    "new_string": "println!(\"Hello, RustyCode!\");"
                }),
                output: serde_json::json!({
                    "status": "ok",
                    "diff": "-     println!(\"Hello, world!\");\n+     println!(\"Hello, RustyCode!\");"
                }),
                context: ExecutionContext::DirectTool {
                    immediate_result: true,
                    timeout_ms: None,
                },
                explanation: Some(
                    "Replace an exact string match in a file with a new string.".to_string(),
                ),
            }],
            "write" | "write_file" => vec![ExecutionExample {
                scenario: "Create a new file".to_string(),
                input: serde_json::json!({
                    "file_path": "/src/config.toml",
                    "content": "[settings]\nverbose = true\n"
                }),
                output: serde_json::json!({
                    "status": "ok"
                }),
                context: ExecutionContext::DirectTool {
                    immediate_result: true,
                    timeout_ms: None,
                },
                explanation: Some(
                    "Write content to a file, creating or overwriting it.".to_string(),
                ),
            }],
            "glob" => vec![ExecutionExample {
                scenario: "Find Rust source files".to_string(),
                input: serde_json::json!({
                    "pattern": "**/*.rs",
                    "path": "/src"
                }),
                output: serde_json::json!({
                    "matches": ["/src/main.rs", "/src/lib.rs", "/src/utils.rs"]
                }),
                context: ExecutionContext::DirectTool {
                    immediate_result: true,
                    timeout_ms: None,
                },
                explanation: Some(
                    "Find files matching a glob pattern under a directory.".to_string(),
                ),
            }],
            "grep" => vec![ExecutionExample {
                scenario: "Search for a string in source files".to_string(),
                input: serde_json::json!({
                    "pattern": "fn main",
                    "path": "/src",
                    "include": "*.rs"
                }),
                output: serde_json::json!({
                    "matches": [
                        {"file": "/src/main.rs", "line": 1, "content": "fn main() {"}
                    ]
                }),
                context: ExecutionContext::DirectTool {
                    immediate_result: true,
                    timeout_ms: None,
                },
                explanation: Some(
                    "Search file contents for a pattern, optionally filtered by file type."
                        .to_string(),
                ),
            }],
            _ => vec![],
        }
    }
}

#[async_trait]
impl UnitLoader for NativeToolLoader {
    fn name(&self) -> &'static str {
        "native_tools"
    }

    async fn load_units(&self) -> Result<Vec<ExecutableUnit>, ExecutableError> {
        Ok(self
            .tool_names
            .iter()
            .map(|n| Self::create_stub_unit(n))
            .collect())
    }
}
