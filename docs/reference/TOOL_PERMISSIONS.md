# Tool Permission Matrix

This document provides a comprehensive overview of the RustyCode tool permission system, including all available tools, their required permission levels, session modes, and security considerations.

## Table of Contents

- [Overview](#overview)
- [Permission Levels](#permission-levels)
- [Session Modes](#session-modes)
- [Tool Permission Matrix](#tool-permission-matrix)
- [Permission Enforcement](#permission-enforcement)
- [Security Considerations](#security-considerations)
- [Usage Examples](#usage-examples)
- [Testing Permissions](#testing-permissions)

## Overview

RustyCode implements a layered permission system to control tool access based on session modes. This ensures safe exploration of codebases during planning while allowing full access during execution.

### Key Principles

1. **Default Safe**: Planning mode only allows read-only operations
2. **Explicit Approval**: Write/Execute tools require explicit mode transition
3. **Hierarchical**: Permission levels form a hierarchy (Read < Write < Execute < Network)
4. **Sandboxing**: Additional path-based and resource restrictions available

## Permission Levels

Tools are categorized into four permission levels:

### 1. Read (None)

**Description**: Read-only operations that cannot modify the filesystem or system state.

**Characteristics**:
- Cannot modify files
- Cannot execute commands
- Cannot make network requests
- Safe for exploratory analysis

**Tools with Read Permission**:
- `read_file` - Read file contents
- `list_dir` - List directory contents
- `grep` - Search file contents with regex
- `glob` - Find files by pattern
- `git_status` - Show git repository status
- `git_diff` - Show git diffs
- `git_log` - Show git commit history
- `lsp_diagnostics` - Show LSP server status

### 2. Write

**Description**: Operations that can modify the filesystem but cannot execute arbitrary commands.

**Characteristics**:
- Can create/modify files
- Cannot execute arbitrary shell commands
- Cannot make network requests
- Requires explicit user approval

**Tools with Write Permission**:
- `write_file` - Write or overwrite files
- `git_commit` - Create git commits (modifies .git directory)

### 3. Execute

**Description**: Operations that can execute arbitrary commands.

**Characteristics**:
- Can run shell commands
- Can potentially modify system state
- Highest risk level
- Requires explicit user approval

**Tools with Execute Permission**:
- `bash` - Execute arbitrary shell commands

### 4. Dangerous

**Description**: Operations that can cause destructive or irreversible changes (currently unused in built-in tools but reserved for future tools with destructive capabilities).

**Characteristics**:
- Can cause data loss
- Can make irreversible system changes
- Highest risk level
- Reserved for future tools

**Tools with Dangerous Permission**:
- None currently (reserved for future use, e.g., destructive tools like `rm`, destructive git operations)

## Session Modes

### Planning Mode

**Description**: Read-only exploration mode for analyzing codebases.

**Characteristics**:
- Only Read permission tools allowed
- Write, Execute, and Network tools blocked
- Safe for automated exploration
- Default mode for new sessions

**Allowed Tools**:
- All Read permission tools

**Blocked Tools**:
- `write_file`
- `git_commit`
- `bash`
- Any future Dangerous tools

### Executing Mode

**Description**: Full access mode for implementing changes.

**Characteristics**:
- All permission levels allowed
- Requires explicit user approval
- Unrestricted tool access
- Used after plan approval

**Allowed Tools**:
- All tools (Read, Write, Execute, Dangerous)

## Tool Permission Matrix

| Tool Name | Permission | Planning Mode | Executing Mode | Category | Risk Level |
|-----------|------------|---------------|----------------|----------|------------|
| `read_file` | Read | ✅ Allowed | ✅ Allowed | Filesystem | None |
| `list_dir` | Read | ✅ Allowed | ✅ Allowed | Filesystem | None |
| `grep` | Read | ✅ Allowed | ✅ Allowed | Search | None |
| `glob` | Read | ✅ Allowed | ✅ Allowed | Search | None |
| `git_status` | Read | ✅ Allowed | ✅ Allowed | Git | None |
| `git_diff` | Read | ✅ Allowed | ✅ Allowed | Git | None |
| `git_log` | Read | ✅ Allowed | ✅ Allowed | Git | None |
| `lsp_diagnostics` | Read | ✅ Allowed | ✅ Allowed | LSP | None |
| `write_file` | Write | ❌ Blocked | ✅ Allowed | Filesystem | Medium |
| `git_commit` | Write | ❌ Blocked | ✅ Allowed | Git | Medium |
| `bash` | Execute | ❌ Blocked | ✅ Allowed | Shell | High |
| `dangerous_tool` | Dangerous | ❌ Blocked | ❌ Blocked | Destructive | Critical |

### Legend

- ✅ **Allowed**: Tool can be used in this mode
- ❌ **Blocked**: Tool cannot be used in this mode
- **None**: No risk - read-only operations
- **Medium**: Moderate risk - can modify files
- **High**: High risk - arbitrary code execution

## Permission Enforcement

### Runtime Permission Checking

Permissions are enforced at multiple levels:

#### 1. Tool Registration Level

Each tool declares its required permission:

```rust
impl Tool for WriteFileTool {
    fn permission(&self) -> ToolPermission {
        ToolPermission::Write  // Declares Write permission requirement
    }
}
```

#### 2. Session Mode Level

Before tool execution, the system checks:

```rust
pub fn check_tool_permission(tool_name: &str, mode: SessionMode) -> bool {
    let permission = match get_tool_permission(tool_name) {
        Some(p) => p,
        None => return false, // Unknown tools are not allowed
    };

    match (mode, permission) {
        // Planning mode: only read operations allowed
        (SessionMode::Planning, ToolPermission::Read) => true,
        (SessionMode::Planning, _) => false,
        // Executing mode: all tools allowed
        (SessionMode::Executing, _) => true,
    }
}
```

#### 3. Tool Context Level

The `ToolContext` carries maximum permission:

```rust
pub struct ToolContext {
    pub cwd: PathBuf,
    pub sandbox: SandboxConfig,
    pub max_permission: ToolPermission,  // Cap on allowed permissions
}
```

During execution:

```rust
// Check if tool's required permission is within the session's max permission
let tool_perm = tool.permission();
if tool_perm as u8 > ctx.max_permission as u8 {
    return ToolResult {
        error: Some(format!(
            "permission denied: tool '{}' requires {:?} permission, but session only allows {:?}",
            call.name, tool_perm, ctx.max_permission
        )),
        // ... other fields
    };
}
```

### Permission Hierarchy

The permission levels form a numeric hierarchy for comparison:

```rust
pub enum ToolPermission {
    Read,       // Read-only operations
    Write,      // Write operations
    Execute,    // Execute commands
    Dangerous,  // Dangerous/destructive operations
}
```

This allows ordering: Read < Write < Execute < Dangerous (for future extensibility).

## Security Considerations

### Threat Model

#### Planning Mode Threats

**Protected Against**:
- Accidental file modification
- Unintended command execution
- Data loss from automated exploration
- Unauthorized system changes

**Remaining Risks**:
- Information disclosure (reading sensitive files)
- Resource exhaustion (reading large files repeatedly)

#### Executing Mode Threats

**Protected Against**:
- None (full access intended)

**Risks**:
- All risks from Planning Mode, plus:
- Intentional or accidental file deletion
- Malicious command execution
- System compromise via bash
- Data corruption
- Destructive operations (future Dangerous tools)

### Mitigation Strategies

#### 1. Path Sandboxing

Restrict tool access to specific directories:

```rust
let sandbox = SandboxConfig::new()
    .allow_path("/workspace")
    .deny_path("/workspace/private")
    .deny_path("/workspace/.credentials");

let ctx = ToolContext::new("/workspace")
    .with_sandbox(sandbox);
```

#### 2. Resource Limits

Prevent resource exhaustion:

```rust
let sandbox = SandboxConfig::new()
    .timeout(30)              // 30 second timeout
    .max_output(10_485_760);  // 10MB max output

let ctx = ToolContext::new("/workspace")
    .with_sandbox(sandbox);
```

#### 3. Explicit Approval

Require user confirmation for mode transitions:

```
Planning Mode → [User Approves Plan] → Executing Mode
```

#### 4. Audit Logging

Log all tool executions via event bus:

```rust
bus.publish(ToolExecutedEvent::new(
    session_id,
    tool_name,
    arguments,
    success,
    output,
    error,
)).await?;
```

### Best Practices

#### For Users

1. **Review Plans Carefully**: Always review the plan before approving execution
2. **Use Planning Mode**: Start in planning mode for exploration
3. **Check Permissions**: Verify tool permissions before use
4. **Monitor Logs**: Review tool execution logs
5. **Sandbox When Possible**: Use path restrictions for untrusted code

#### For Tool Developers

1. **Declare Minimum Permission**: Always declare the minimum required permission
2. **Validate Inputs**: Validate all parameters before execution
3. **Handle Errors**: Return clear error messages for permission failures
4. **Log Operations**: Use the event bus for audit trails
5. **Test Permissions**: Write tests for permission enforcement

## Usage Examples

### Example 1: Planning Mode Exploration

```rust
use rustycode_runtime::AsyncRuntime;
use rustycode_protocol::{SessionMode, ToolCall};
use serde_json::json;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let runtime = AsyncRuntime::load(".").await?;

    // Create a planning mode session
    let session = Session::builder()
        .task("Explore codebase structure")
        .with_mode(SessionMode::Planning)
        .build()?;

    // Read operations succeed in planning mode
    let read_call = ToolCall {
        call_id: "1".to_string(),
        name: "read_file".to_string(),
        arguments: json!({"path": "README.md"}),
    };
    let result = runtime.execute_tool(&session.id, read_call, ".").await?;
    assert!(result.success);

    // Write operations fail in planning mode
    let write_call = ToolCall {
        call_id: "2".to_string(),
        name: "write_file".to_string(),
        arguments: json!({"path": "test.txt", "content": "test"}),
    };
    let result = runtime.execute_tool(&session.id, write_call, ".").await?;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("permission denied"));

    Ok(())
}
```

### Example 2: Executing Mode Implementation

```rust
use rustycode_runtime::AsyncRuntime;
use rustycode_protocol::{SessionMode, ToolCall};
use serde_json::json;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let runtime = AsyncRuntime::load(".").await?;

    // Create an executing mode session
    let session = Session::builder()
        .task("Implement feature")
        .with_mode(SessionMode::Executing)
        .build()?;

    // All tools work in executing mode
    let write_call = ToolCall {
        call_id: "1".to_string(),
        name: "write_file".to_string(),
        arguments: json!({"path": "src/main.rs", "content": "fn main() {}"}),
    };
    let result = runtime.execute_tool(&session.id, write_call, ".").await?;
    assert!(result.success);

    // Bash execution works in executing mode
    let bash_call = ToolCall {
        call_id: "2".to_string(),
        name: "bash".to_string(),
        arguments: json!({"command": "cargo check"}),
    };
    let result = runtime.execute_tool(&session.id, bash_call, ".").await?;
    assert!(result.success);

    Ok(())
}
```

### Example 3: Sandboxed Execution

```rust
use rustycode_tools::{SandboxConfig, ToolContext};
use rustycode_protocol::ToolCall;
use serde_json::json;

fn main() -> anyhow::Result<()> {
    // Create sandbox configuration
    let sandbox = SandboxConfig::new()
        .allow_path("/workspace/src")
        .deny_path("/workspace/src/internal")
        .timeout(10)
        .max_output(1_048_576);  // 1MB

    // Create tool context with sandbox
    let ctx = ToolContext::new("/workspace")
        .with_sandbox(sandbox)
        .with_max_permission(rustycode_tools::ToolPermission::Write);

    // Tool execution will respect sandbox limits
    // - Can only access /workspace/src (not /workspace/src/internal)
    // - Commands timeout after 10 seconds
    // - Output limited to 1MB

    Ok(())
}
```

### Example 4: Permission Checking

```rust
use rustycode_tools::{check_tool_permission, get_tool_permission};
use rustycode_protocol::{SessionMode, ToolPermission};

fn main() {
    // Check if a tool is allowed in planning mode
    assert!(check_tool_permission("read_file", SessionMode::Planning));
    assert!(!check_tool_permission("write_file", SessionMode::Planning));
    assert!(!check_tool_permission("bash", SessionMode::Planning));

    // Check if a tool is allowed in executing mode
    assert!(check_tool_permission("read_file", SessionMode::Executing));
    assert!(check_tool_permission("write_file", SessionMode::Executing));
    assert!(check_tool_permission("bash", SessionMode::Executing));

    // Get a tool's required permission
    assert_eq!(
        get_tool_permission("read_file"),
        Some(ToolPermission::Read)
    );
    assert_eq!(
        get_tool_permission("write_file"),
        Some(ToolPermission::Write)
    );
    assert_eq!(
        get_tool_permission("bash"),
        Some(ToolPermission::Execute)
    );

    // Unknown tools return None
    assert_eq!(get_tool_permission("unknown_tool"), None);
}
```

## Testing Permissions

### Unit Tests

The permission system includes comprehensive tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_tools_allowed_in_planning_mode() {
        assert!(check_tool_permission("read_file", SessionMode::Planning));
        assert!(check_tool_permission("grep", SessionMode::Planning));
        assert!(check_tool_permission("list_dir", SessionMode::Planning));
    }

    #[test]
    fn test_write_tools_blocked_in_planning_mode() {
        assert!(!check_tool_permission("write_file", SessionMode::Planning));
        assert!(!check_tool_permission("git_commit", SessionMode::Planning));
    }

    #[test]
    fn test_execute_tools_blocked_in_planning_mode() {
        assert!(!check_tool_permission("bash", SessionMode::Planning));
    }

    #[test]
    fn test_all_tools_allowed_in_executing_mode() {
        assert!(check_tool_permission("read_file", SessionMode::Executing));
        assert!(check_tool_permission("write_file", SessionMode::Executing));
        assert!(check_tool_permission("bash", SessionMode::Executing));
    }

    #[test]
    fn test_permission_hierarchy() {
        assert_eq!(get_tool_permission("read_file"), Some(ToolPermission::Read));
        assert_eq!(get_tool_permission("write_file"), Some(ToolPermission::Write));
        assert_eq!(get_tool_permission("bash"), Some(ToolPermission::Execute));
    }
}
```

### Integration Tests

Test permission enforcement with actual tool execution:

```rust
#[tokio::test]
async fn test_planning_mode_blocks_write_operations() {
    let runtime = AsyncRuntime::load(".").await?;
    let session = Session::builder()
        .task("Test planning mode")
        .with_mode(SessionMode::Planning)
        .build()?;

    let write_call = ToolCall {
        call_id: "1".to_string(),
        name: "write_file".to_string(),
        arguments: json!({"path": "test.txt", "content": "test"}),
    };

    let result = runtime.execute_tool(&session.id, write_call, ".").await?;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("permission denied"));
}

#[tokio::test]
async fn test_executing_mode_allows_write_operations() {
    let runtime = AsyncRuntime::load(".").await?;
    let session = Session::builder()
        .task("Test executing mode")
        .with_mode(SessionMode::Executing)
        .build()?;

    let write_call = ToolCall {
        call_id: "1".to_string(),
        name: "write_file".to_string(),
        arguments: json!({"path": "test.txt", "content": "test"}),
    };

    let result = runtime.execute_tool(&session.id, write_call, ".").await?;
    assert!(result.success);

    // Cleanup
    std::fs::remove_file("test.txt")?;
}
```

### Running Tests

```bash
# Run all permission-related tests
cargo test --lib permission

# Run tests with output
cargo test --lib permission -- --nocapture

# Run specific test
cargo test test_read_tools_allowed_in_planning_mode
```

## Appendix: Complete Tool Reference

### Filesystem Tools

#### read_file
- **Permission**: Read
- **Description**: Read a UTF-8 text file relative to current workspace
- **Parameters**:
  - `path` (string, required): File path
  - `start_line` (integer, optional): First line to return (1-indexed)
  - `end_line` (integer, optional): Last line to return (1-indexed)
- **Planning Mode**: ✅ Allowed
- **Executing Mode**: ✅ Allowed

#### write_file
- **Permission**: Write
- **Description**: Write UTF-8 text to a file relative to current workspace
- **Parameters**:
  - `path` (string, required): File path
  - `content` (string, required): File content
- **Planning Mode**: ❌ Blocked
- **Executing Mode**: ✅ Allowed

#### list_dir
- **Permission**: Read
- **Description**: List directory entries relative to current workspace
- **Parameters**:
  - `path` (string, optional): Directory path (default: ".")
- **Planning Mode**: ✅ Allowed
- **Executing Mode**: ✅ Allowed

### Search Tools

#### grep
- **Permission**: Read
- **Description**: Search text files for a regex pattern
- **Parameters**:
  - `pattern` (string, required): Regex pattern
  - `path` (string, optional): Search path (default: ".")
- **Planning Mode**: ✅ Allowed
- **Executing Mode**: ✅ Allowed

#### glob
- **Permission**: Read
- **Description**: Find files whose path contains a glob-like fragment
- **Parameters**:
  - `pattern` (string, required): Glob pattern
- **Planning Mode**: ✅ Allowed
- **Executing Mode**: ✅ Allowed

### Git Tools

#### git_status
- **Permission**: Read
- **Description**: Show git status for current workspace
- **Parameters**: None
- **Planning Mode**: ✅ Allowed
- **Executing Mode**: ✅ Allowed

#### git_diff
- **Permission**: Read
- **Description**: Show git diff, optionally staged and/or for a specific path
- **Parameters**:
  - `staged` (boolean, optional): Show staged diff (default: false)
  - `path` (string, optional): Specific path to diff
- **Planning Mode**: ✅ Allowed
- **Executing Mode**: ✅ Allowed

#### git_log
- **Permission**: Read
- **Description**: Show recent git commits
- **Parameters**:
  - `limit` (integer, optional): Number of commits (default: 10)
- **Planning Mode**: ✅ Allowed
- **Executing Mode**: ✅ Allowed

#### git_commit
- **Permission**: Write
- **Description**: Stage files and create a git commit with provided message
- **Parameters**:
  - `message` (string, required): Commit message
  - `files` (array of strings, optional): Files to stage before committing
- **Planning Mode**: ❌ Blocked
- **Executing Mode**: ✅ Allowed

### Shell Tools

#### bash
- **Permission**: Execute
- **Description**: Execute a shell command in the current workspace
- **Parameters**:
  - `command` (string, required): Shell command to execute
  - `cwd` (string, optional): Working directory override
  - `timeout_secs` (integer, optional): Timeout in seconds (default: 30)
- **Planning Mode**: ❌ Blocked
- **Executing Mode**: ✅ Allowed

### LSP Tools

#### lsp_diagnostics
- **Permission**: Read
- **Description**: Report which configured language servers are installed
- **Parameters**:
  - `servers` (array of strings, optional): LSP servers to check
- **Planning Mode**: ✅ Allowed
- **Executing Mode**: ✅ Allowed

## Version History

- **v1.0.0** (2025-03-12): Initial permission system documentation
  - Defined 4 permission levels (Read, Write, Execute, Network)
  - Implemented Planning/Executing session modes
  - Documented all 11 built-in tools
  - Added security considerations and best practices
