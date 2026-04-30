// Example: Compile-Time Tool System
// This demonstrates the design from docs/design/compile-time-tools.md

use std::path::{Path, PathBuf};
use std::fs;
use std::io;
use std::error::Error;

// ============================================================================
// Core Trait Definitions
// ============================================================================

/// Core tool trait with associated types
pub trait Tool {
    /// Input parameter type
    type Input;

    /// Output type
    type Output;

    /// Error type
    type Error: Error + Send + Sync + 'static;

    /// Tool metadata (const evaluable)
    const METADATA: ToolMetadata;

    /// Execute the tool
    fn execute(&self, input: Self::Input) -> Result<Self::Output, Self::Error>;

    /// Validate parameters (compile-time checked)
    fn validate(input: &Self::Input) -> Result<(), ToolValidationError> {
        let _ = input; // Use input to avoid unused warning
        Ok(())
    }
}

/// Tool metadata (const-friendly)
#[derive(Clone, Debug)]
pub struct ToolMetadata {
    pub name: &'static str,
    pub category: ToolCategory,
    pub permission: Permission,
    pub description: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ToolCategory {
    ReadOnly,
    Write,
    Execute,
    Network,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Permission {
    None,
    Read,
    Write,
    Execute,
    Network,
}

#[derive(Debug)]
pub enum ToolValidationError {
    MissingParameter(&'static str),
    InvalidType(&'static str, &'static str),
}

// ============================================================================
// Tool Implementations
// ============================================================================

/// Read file tool
#[derive(Debug, Clone)]
pub struct ReadFile;

impl Tool for ReadFile {
    type Input = ReadFileInput;
    type Output = String;
    type Error = io::Error;

    const METADATA: ToolMetadata = ToolMetadata {
        name: "read_file",
        category: ToolCategory::ReadOnly,
        permission: Permission::Read,
        description: "Read file contents from disk",
    };

    fn execute(&self, input: Self::Input) -> Result<Self::Output, Self::Error> {
        fs::read_to_string(&input.path)
    }
}

#[derive(Debug, Clone)]
pub struct ReadFileInput {
    pub path: PathBuf,
}

/// Write file tool
#[derive(Debug, Clone)]
pub struct WriteFile;

impl Tool for WriteFile {
    type Input = WriteFileInput;
    type Output = ();
    type Error = io::Error;

    const METADATA: ToolMetadata = ToolMetadata {
        name: "write_file",
        category: ToolCategory::Write,
        permission: Permission::Write,
        description: "Write content to a file",
    };

    fn execute(&self, input: Self::Input) -> Result<Self::Output, Self::Error> {
        if input.create_parents {
            if let Some(parent) = input.path.parent() {
                fs::create_dir_all(parent)?;
            }
        }
        fs::write(&input.path, &input.content)
    }

    fn validate(input: &Self::Input) -> Result<(), ToolValidationError> {
        if input.path.as_os_str().is_empty() {
            return Err(ToolValidationError::MissingParameter("path"));
        }
        if input.content.is_empty() && !input.allow_empty {
            return Err(ToolValidationError::InvalidType(
                "content",
                "cannot be empty"
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct WriteFileInput {
    pub path: PathBuf,
    pub content: String,
    pub create_parents: bool,
    pub allow_empty: bool,
}

/// Execute command tool
#[derive(Debug, Clone)]
pub struct ExecuteCommand;

impl Tool for ExecuteCommand {
    type Input = ExecuteCommandInput;
    type Output = CommandOutput;
    type Error = ExecuteError;

    const METADATA: ToolMetadata = ToolMetadata {
        name: "execute_command",
        category: ToolCategory::Execute,
        permission: Permission::Execute,
        description: "Execute a shell command",
    };

    fn execute(&self, input: Self::Input) -> Result<Self::Output, Self::Error> {
        use std::process::Command;

        let mut cmd = Command::new(&input.command);
        cmd.args(&input.args);
        if let Some(dir) = &input.working_dir {
            cmd.current_dir(dir);
        }

        let output = cmd.output()
            .map_err(|e| ExecuteError::ExecutionFailed(e.to_string()))?;

        Ok(CommandOutput {
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            exit_code: output.status.code().unwrap_or(-1),
        })
    }
}

#[derive(Debug, Clone)]
pub struct ExecuteCommandInput {
    pub command: String,
    pub args: Vec<String>,
    pub working_dir: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct CommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

#[derive(Debug, thiserror::Error)]
pub enum ExecuteError {
    #[error("Execution failed: {0}")]
    ExecutionFailed(String),

    #[error("Command not found: {0}")]
    CommandNotFound(String),
}

// ============================================================================
// Type-Safe Tool Dispatcher
// ============================================================================

/// Zero-cost tool dispatcher (static dispatch)
pub struct ToolDispatcher<T: Tool> {
    _marker: std::marker::PhantomData<T>,
}

impl<T: Tool> ToolDispatcher<T> {
    pub fn dispatch(input: T::Input) -> Result<T::Output, T::Error> {
        // Direct call - monomorphized, potentially inlined
        let tool = T;
        tool.validate(&input).map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidInput, "Validation failed")
        })?;
        tool.execute(input)
    }
}

// ============================================================================
// Usage Examples
// ============================================================================

fn main() -> Result<(), Box<dyn Error>> {
    println!("=== Compile-Time Tool System Examples ===\n");

    // Example 1: Read file (compile-time type checking)
    println!("1. Reading Cargo.toml:");
    let result = ToolDispatcher::<ReadFile>::dispatch(ReadFileInput {
        path: PathBuf::from("Cargo.toml"),
    })?;
    println!("First 100 chars: {}\n", &result[..result.len().min(100)]);

    // Example 2: Write file (with validation)
    println!("2. Writing test file:");
    let write_result = ToolDispatcher::<WriteFile>::dispatch(WriteFileInput {
        path: PathBuf::from("/tmp/rustycode_test.txt"),
        content: "Hello from compile-time tools!".into(),
        create_parents: true,
        allow_empty: true,
    })?;
    println!("Write result: {:?}\n", write_result);

    // Example 3: Execute command
    println!("3. Executing command:");
    let output = ToolDispatcher::<ExecuteCommand>::dispatch(ExecuteCommandInput {
        command: "echo".into(),
        args: vec!["Hello, Rust!".into()],
        working_dir: None,
    })?;
    println!("Command output: {}", output.stdout.trim());

    // Example 4: Type error (commented out - won't compile)
    // let result = ToolDispatcher::<ReadFile>::dispatch(ReadFileInput {
    //     path: 123,  // Compile error: expected PathBuf, found integer
    // });

    // Example 5: Missing parameter (commented out - won't compile)
    // let result = ToolDispatcher::<ReadFile>::dispatch(ReadFileInput {
    //     // path: PathBuf::from("test.txt"),  // Compile error: missing field
    // });

    println!("\n=== All examples completed successfully! ===");
    Ok(())
}

// ============================================================================
// Comparison with Runtime Tool System
// ============================================================================

#[cfg(test)]
mod comparisons {
    use super::*;

    /// This shows what a RUNTIME tool system looks like (for comparison)
    #[derive(Debug)]
    struct RuntimeTool {
        name: String,
        parameters: Vec<Parameter>,
        execute_fn: fn(&[Parameter]) -> Result<String, Box<dyn Error>>,
    }

    #[derive(Debug)]
    struct Parameter {
        name: String,
        value: serde_json::Value,
    }

    /// Runtime tool execution (type unsafe)
    fn execute_runtime_tool(
        tool: &RuntimeTool,
        params: &[Parameter],
    ) -> Result<String, Box<dyn Error>> {
        // All validation happens at runtime:
        // - Parameter names checked at runtime
        // - Parameter types checked at runtime
        // - Required parameters checked at runtime
        // - Tool existence checked at runtime
        (tool.execute_fn)(params)
    }

    #[test]
    fn compare_compile_time_vs_runtime() {
        // Compile-time: Type-safe
        let result = ToolDispatcher::<ReadFile>::dispatch(ReadFileInput {
            path: PathBuf::from("Cargo.toml"),
        });
        assert!(result.is_ok());

        // Runtime: Would need string-based lookup, JSON parsing, etc.
        // (Commented out to avoid complexity in this example)
        // let tool = find_tool("read_file").unwrap();
        // let params = vec![Parameter {
        //     name: "path".into(),
        //     value: serde_json::json!("Cargo.toml"),
        // }];
        // let result = execute_runtime_tool(&tool, &params);
    }
}