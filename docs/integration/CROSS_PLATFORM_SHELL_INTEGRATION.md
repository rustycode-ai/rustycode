# Cross-Platform Shell Tool Integration Guide

This document explains how to integrate cross-platform security validation into BashTool, PowerShellTool, and CmdTool.

## Architecture Overview

```
Shell Tool (bash/powershell/cmd)
    ↓
ShellContext Setup (environment variables, paths)
    ↓
Cross-Platform Validation
    ├── Path Validation (workspace boundaries)
    ├── Command Validation (allowlist/blocklist)
    └── Environment Variable Validation
    ↓
Shell Session Execution
```

## Validation Components

### 1. ShellType Detection

Automatically detects which shell is being used:

```rust
use crate::security::cross_platform::ShellType;

let shell = ShellType::detect("powershell");  // → ShellType::PowerShell
let shell = ShellType::detect("/bin/bash");   // → ShellType::Bash
let shell = ShellType::detect("cmd");         // → ShellType::Cmd
```

### 2. Path Validation (Cross-Platform)

Validates that working directory is within workspace, handling:
- Windows absolute paths: `C:\Users\...`
- WSL paths: `/mnt/c/Users/...`
- Cygwin paths: `/cygdrive/c/Users/...`
- Unix paths: `/home/...`

```rust
use crate::security::cross_platform::validate_path_in_workspace;

// Handles all path formats automatically
validate_path_in_workspace(&ctx.cwd, &workspace_root)?;
```

### 3. Command Validation (Platform-Specific Allowlists)

Each shell has its own allowlist:

**Bash Commands:**
- File ops: `ls`, `cat`, `grep`, `find`
- Tools: `cargo`, `git`, `npm`, `python`

**PowerShell Commands:**
- File ops: `Get-ChildItem`, `Get-Content`, `Select-String`
- Tools: `cargo`, `git`, `npm`, `dotnet`

**cmd.exe Commands:**
- File ops: `dir`, `type`, `findstr`
- Tools: `cargo`, `git`, `npm`

```rust
use crate::security::cross_platform::{ShellType, get_allowed_commands};

let allowed = get_allowed_commands(shell_type);
if allowed.contains(&command_name) {
    // Safe to execute
}
```

### 4. Blocked Commands (Platform-Specific)

Each shell also has a blocklist of dangerous commands:

**Bash Blocked:**
- `mkfs`, `dd`, `shutdown`, `reboot`, `su`, `sudo`

**PowerShell Blocked:**
- `Format-Volume`, `Restart-Computer`, `Stop-Computer`, `Remove-Item`

**cmd.exe Blocked:**
- `format`, `diskpart`, `shutdown`, `del`, `cipher`

```rust
use crate::security::cross_platform::{ShellType, get_blocked_commands};

let blocked = get_blocked_commands(shell_type);
if blocked.contains(&command_name) {
    return Err(anyhow!("command '{}' is not allowed", command_name));
}
```

## Integration in Shell Tools

### In BashTool::execute()

```rust
impl Tool for BashTool {
    fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput> {
        // 1. Permission checks
        crate::check_permission(self.permission(), ctx)?;
        if let Some(gate) = &ctx.plan_gate {
            gate.check_access(ctx.role, self.name())?;
        }

        // 2. Get command
        let command = params.get("command").and_then(Value::as_str)?;
        
        // 3. Cross-platform validation
        let shell_type = ShellType::detect("bash");  // or from context
        
        // Validate path is in workspace (handles WSL, Cygwin, etc.)
        validate_path_in_workspace(&ctx.cwd, &workspace_root)?;
        
        // Validate command syntax (existing)
        validate_command_safety(&command)?;
        
        // Validate against platform-specific allowlist
        let allowed_commands = get_allowed_commands(shell_type);
        let binary = extract_binary_name(&command)?;
        if !allowed_commands.contains(&binary) {
            bail!("command '{}' not in allowed list", binary);
        }
        
        // Validate against platform-specific blocklist
        let blocked_commands = get_blocked_commands(shell_type);
        if blocked_commands.contains(&binary) {
            bail!("command '{}' is blocked", binary);
        }

        // 4. Build shell context with environment
        let shell_ctx = build_shell_context(&ctx.cwd, "bash");
        
        // 5. Execute
        let session = BashSession::new(ctx.cwd.clone(), Some(shell_ctx))?;
        let (stdout, stderr, exit_code) = session.execute(&command, timeout_secs)?;
        
        // Return formatted output
        Ok(ToolOutput::text(format_output(stdout, stderr, exit_code)))
    }
}
```

### In PowerShellTool::execute()

```rust
impl Tool for PowerShellTool {
    fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput> {
        // Same structure as BashTool, but:
        let shell_type = ShellType::detect("powershell");
        
        // Path normalization for PowerShell (handles /mnt/c/ → C:/)
        let normalized_cwd = if cfg!(windows) {
            paths::normalize(&ctx.cwd.to_string_lossy())
        } else {
            ctx.cwd.to_string_lossy().to_string()
        };
        
        validate_path_in_workspace(&PathBuf::from(&normalized_cwd), &workspace)?;
        
        // PowerShell-specific command validation
        validate_powershell_syntax(&command)?;
        
        // Rest is same...
    }
}
```

### In CmdTool::execute()

```rust
impl Tool for CmdTool {
    fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let shell_type = ShellType::detect("cmd");
        
        // Path handling for cmd.exe (backslashes)
        validate_path_in_workspace(&ctx.cwd, &workspace)?;
        
        // cmd.exe specific validation
        validate_cmd_syntax(&command)?;
        
        // Rest is same...
    }
}
```

## Adding New Commands

To add a command to the allowlist:

1. Find the appropriate `BASH_ALLOWED_COMMANDS`, `POWERSHELL_ALLOWED_COMMANDS`, or `CMD_ALLOWED_COMMANDS` in `security/cross_platform.rs`
2. Add the command name to the appropriate array
3. Document why it's safe in a comment
4. Add a test case

Example:

```rust
const BASH_ALLOWED_COMMANDS: &[&str] = &[
    // ... existing ...
    "jq",           // JSON query tool - read-only by default
    // ... more ...
];
```

## Testing Cross-Platform Validation

```rust
#[test]
fn test_wsl_path_validation() {
    let wsl_path = Path::new("/mnt/c/workspace/file.txt");
    let workspace = Path::new("C:\\workspace");
    
    // Should pass - path is within workspace
    assert!(validate_path_in_workspace(wsl_path, workspace).is_ok());
}

#[test]
fn test_platform_specific_allowlist() {
    let bash_allowed = get_allowed_commands(ShellType::Bash);
    let ps_allowed = get_allowed_commands(ShellType::PowerShell);
    
    // bash has grep, PowerShell has Select-String
    assert!(bash_allowed.contains(&"grep"));
    assert!(ps_allowed.contains(&"Select-String"));
    assert!(!bash_allowed.contains(&"Select-String"));
}
```

## Environment Variable Setup

Each shell gets the right environment variables via `ShellContext`:

```rust
pub struct ShellContext {
    pub shell: String,                          // "bash", "powershell", etc.
    pub env: HashMap<String, String>,           // All env vars
    pub is_powershell: bool,
    pub is_cmd: bool,
    pub home_dir: Option<PathBuf>,              // $HOME or $USERPROFILE
    pub path: Option<String>,                   // PATH variable
}
```

### Auto-Setup for Each Shell

**Bash:**
```rust
env["HOME"] = "/home/user"
env["SHELL"] = "bash"
env["PATH"] = "/usr/bin:/bin:..." (Unix style)
```

**PowerShell:**
```rust
env["USERPROFILE"] = "C:\Users\user"
env["HOME"] = "C:\Users\user"  // Also set for ~ expansion
env["SHELL"] = "powershell"
env["PATH"] = "C:\...\bin;..." (Windows style)
```

**cmd.exe:**
```rust
env["USERPROFILE"] = "C:\Users\user"
env["SHELL"] = "cmd.exe"
env["PATH"] = "C:\...\bin;..." (Windows style)
```

## Migration Checklist

- [ ] Add `use crate::security::cross_platform::*;` to bash.rs
- [ ] Update BashTool::execute() to use cross-platform validation
- [ ] Update PowerShellTool::execute() (in powershell.rs)
- [ ] Update CmdTool::execute() (in cmd.rs)
- [ ] Add platform-specific commands to allowlists
- [ ] Update tests to validate cross-platform paths
- [ ] Document in tool descriptions which commands are available per platform
- [ ] Test WSL/Cygwin path handling on Windows

## References

- `crates/rustycode-tools/src/security/cross_platform.rs` - Implementation
- `crates/rustycode-tools/src/providers/shell_provider.rs` - Shell registration
- `crates/rustycode-tools/src/providers/bash.rs` - Integration example
