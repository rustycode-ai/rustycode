//! Cross-platform test helpers for finding common shell commands and utilities.
//!
//! This module provides functions to locate common commands in a cross-platform way,
//! avoiding hardcoded paths like `/bin/bash` or `/usr/bin/python`.

use std::path::PathBuf;
use std::process::Command;

/// Find a command in PATH, handling platform-specific executable extensions.
pub fn find_command(name: &str) -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        for ext in &["", ".exe", ".cmd", ".bat"] {
            let full_name = format!("{}{}", name, ext);
            if let Ok(output) = Command::new("where").arg(&full_name).output() {
                if output.status.success() {
                    let path_str = String::from_utf8(output.stdout)
                        .ok()?
                        .lines()
                        .next()?
                        .trim()
                        .to_string();
                    return Some(PathBuf::from(path_str));
                }
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        if let Ok(output) = Command::new("which").arg(name).output() {
            if output.status.success() {
                let path_str = String::from_utf8(output.stdout).ok()?.trim().to_string();
                return Some(PathBuf::from(path_str));
            }
        }
    }

    None
}

/// Get a shell command, preferring available shells in order: bash, zsh, sh, powershell, cmd.
pub fn find_shell() -> Option<PathBuf> {
    for shell in &["Bash", "zsh", "sh"] {
        if let Some(path) = find_command(shell) {
            return Some(path);
        }
    }

    #[cfg(target_os = "windows")]
    {
        for shell in &["powershell", "cmd"] {
            if let Some(path) = find_command(shell) {
                return Some(path);
            }
        }
    }

    None
}

/// Get a no-op command that does nothing and exits successfully.
pub fn noop_command() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        find_command("cmd").unwrap_or_else(|| PathBuf::from("cmd.exe"))
    }

    #[cfg(target_os = "macos")]
    {
        find_command("true").unwrap_or_else(|| PathBuf::from("/usr/bin/true"))
    }

    #[cfg(target_os = "linux")]
    {
        find_command("true").unwrap_or_else(|| PathBuf::from("/bin/true"))
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        find_command("true").unwrap_or_else(|| PathBuf::from("/bin/true"))
    }
}

/// Get a command that removes a file: `rm` on Unix, `del` on Windows.
pub fn remove_command() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        "del"
    }

    #[cfg(not(target_os = "windows"))]
    {
        "rm"
    }
}

/// Get path to Python interpreter, if available.
pub fn find_python() -> Option<PathBuf> {
    for name in &["python3", "python"] {
        if let Some(path) = find_command(name) {
            return Some(path);
        }
    }
    None
}

/// Get path to ls (or dir on Windows).
pub fn find_list_command() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        find_command("dir")
    }

    #[cfg(not(target_os = "windows"))]
    {
        find_command("ls")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_shell() {
        let shell = find_shell();
        assert!(shell.is_some(), "Should find a shell on this system");
    }

    #[test]
    fn test_noop_command_exists() {
        let cmd = noop_command();
        assert!(
            !cmd.as_os_str().is_empty(),
            "noop_command should return a path"
        );
    }
}
