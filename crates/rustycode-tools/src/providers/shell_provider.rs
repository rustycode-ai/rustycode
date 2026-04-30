//! Platform-aware shell tool provider.
//!
//! Registers available shell tools based on runtime detection:
//! - On Unix: BashTool (if bash/zsh/sh available)
//! - On Windows: PowerShellTool and/or CmdTool (conditionally), or BashTool (if WSL/Cygwin)
//!
//! This ensures the LLM only sees available shell tools with clear, explicit names
//! so it knows exactly which syntax to use.

use super::bash::{BashTool, CmdTool, PowerShellTool};
use crate::registry_builder::ToolProvider;
use crate::ToolRegistry;
use anyhow::Result;
use std::process::{Command, Stdio};

/// Check if a shell binary exists and is executable.
fn which_sh(name: &str) -> bool {
    let arg = if name == "powershell" {
        "-Command"
    } else {
        "-c"
    };
    
    Command::new(name)
        .arg(arg)
        .arg("true")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Provider for shell tools.
///
/// Registers available shell tools based on platform and detected shells.
/// This allows the LLM to see exactly which shells are available with clear names.
pub struct ShellProvider;

impl ToolProvider for ShellProvider {
    fn register_tools(&self, registry: &mut ToolRegistry) -> Result<()> {
        #[cfg(windows)]
        {
            // On Windows, check for bash first (WSL or Cygwin)
            if which_sh("bash") {
                registry.register(BashTool);
            }
            
            // Always register PowerShell if available
            if which_sh("powershell") {
                registry.register(PowerShellTool);
            }
            
            // Always register cmd.exe on Windows (guaranteed to exist)
            registry.register(CmdTool);
        }

        #[cfg(not(windows))]
        {
            // On Unix, register bash tool if any shell is available
            // (it auto-detects bash/zsh/sh)
            if which_sh("bash") || which_sh("zsh") || which_sh("sh") {
                registry.register(BashTool);
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_which_sh_bash() {
        // bash should exist on most Unix systems
        #[cfg(unix)]
        {
            assert!(which_sh("bash") || which_sh("zsh") || which_sh("sh"));
        }
    }

    #[test]
    fn test_which_sh_nonexistent() {
        assert!(!which_sh("nonexistent_shell_xyz123"));
    }

    #[test]
    fn test_shell_provider_registers_some_tool() {
        let mut registry = crate::ToolRegistry::new();
        let provider = ShellProvider;
        
        provider.register_tools(&mut registry).unwrap();
        
        // Should have registered at least one tool
        assert!(!registry.list().is_empty(), "ShellProvider should register at least one tool");
    }
}
