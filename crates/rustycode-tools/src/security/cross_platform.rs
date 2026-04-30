//! Cross-platform security validation for shell execution.
//!
//! Handles:
//! - Path normalization (Windows, WSL, Cygwin paths)
//! - Platform-specific command allowlists (bash, PowerShell, cmd.exe)
//! - Platform-specific dangerous command blocklists

use anyhow::{anyhow, Result};
use std::path::{Path, PathBuf};

/// Represents the target shell/platform for validation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellType {
    Bash,       // bash/zsh/sh on Unix, or bash via WSL/Cygwin on Windows
    PowerShell, // PowerShell on Windows
    Cmd,        // cmd.exe on Windows
}

impl ShellType {
    /// Detect shell type from shell name/path
    pub fn detect(shell: &str) -> Self {
        let lower = shell.to_lowercase();
        if lower.contains("powershell") {
            ShellType::PowerShell
        } else if lower.contains("cmd") || lower.ends_with(".exe") && lower.contains("cmd") {
            ShellType::Cmd
        } else {
            ShellType::Bash
        }
    }
}

/// Validate a path is within workspace, handling cross-platform path formats
pub fn validate_path_in_workspace(path: &Path, workspace: &Path) -> Result<()> {
    // Normalize paths to canonical form
    let workspace_canonical = normalize_path_for_comparison(workspace)?;
    let path_canonical = normalize_path_for_comparison(path)?;

    // Check if path is within workspace
    if path_canonical.starts_with(&workspace_canonical) {
        Ok(())
    } else {
        Err(anyhow!(
            "path '{}' is outside workspace '{}'",
            path.display(),
            workspace.display()
        ))
    }
}

/// Normalize a path for comparison, handling WSL, Cygwin, and Windows paths
fn normalize_path_for_comparison(path: &Path) -> Result<PathBuf> {
    // First try canonical form if path exists
    if path.exists() {
        return std::fs::canonicalize(path)
            .map_err(|e| anyhow!("failed to canonicalize path: {}", e));
    }

    // Path doesn't exist - convert to canonical form by walking up to existing parent
    let mut current = path.to_path_buf();
    while !current.exists() {
        if !current.pop() {
            // Reached root without finding an existing ancestor
            break;
        }
    }

    if current.exists() {
        return std::fs::canonicalize(&current)
            .map_err(|e| anyhow!("failed to canonicalize path: {}", e));
    }

    Err(anyhow!("unable to resolve path: {}", path.display()))
}

/// Normalize Windows path formats for consistent comparison
#[cfg(windows)]
pub fn normalize_path_syntax(path: &str) -> String {
    // /mnt/c/path → C:/path
    if let Some(rest) = path.strip_prefix("/mnt/") {
        if let Some(post_drive) = rest
            .chars()
            .nth(1)
            .and_then(|_| rest[2..].strip_prefix('/'))
        {
            return format!(
                "{}:/{}",
                rest.chars().next().unwrap().to_uppercase(),
                post_drive
            );
        }
    }

    // /cygdrive/c/path → C:/path
    if let Some(rest) = path.strip_prefix("/cygdrive/") {
        if let Some(post_drive) = rest
            .chars()
            .nth(1)
            .and_then(|_| rest[2..].strip_prefix('/'))
        {
            return format!(
                "{}:/{}",
                rest.chars().next().unwrap().to_uppercase(),
                post_drive
            );
        }
    }

    // /C:/path → C:/path
    if let Some(rest) = path.strip_prefix('/') {
        if rest.len() > 2 && rest.chars().nth(1) == Some(':') {
            return rest.to_string();
        }
    }

    path.to_string()
}

#[cfg(not(windows))]
pub fn normalize_path_syntax(path: &str) -> String {
    path.to_string()
}

/// Get platform-specific allowed commands
pub fn get_allowed_commands(shell: ShellType) -> &'static [&'static str] {
    match shell {
        ShellType::Bash => BASH_ALLOWED_COMMANDS,
        ShellType::PowerShell => POWERSHELL_ALLOWED_COMMANDS,
        ShellType::Cmd => CMD_ALLOWED_COMMANDS,
    }
}

/// Get platform-specific dangerous commands to block
pub fn get_blocked_commands(shell: ShellType) -> &'static [&'static str] {
    match shell {
        ShellType::Bash => BASH_BLOCKED_COMMANDS,
        ShellType::PowerShell => POWERSHELL_BLOCKED_COMMANDS,
        ShellType::Cmd => CMD_BLOCKED_COMMANDS,
    }
}

// ── Bash/Unix Allowed Commands ─────────────────────────────────────────
const BASH_ALLOWED_COMMANDS: &[&str] = &[
    // File operations (read-only)
    "ls",
    "cat",
    "head",
    "tail",
    "less",
    "more",
    "wc",
    "sort",
    "uniq",
    "file",
    "stat",
    "tree",
    "du",
    "df",
    "readlink",
    "realpath",
    "basename",
    "dirname",
    // Search tools
    "grep",
    "rg",
    "ag",
    "ack",
    "find",
    "locate",
    // Build tools
    "cargo",
    "rustc",
    "rustup",
    "npm",
    "yarn",
    "pnpm",
    "node",
    "python",
    "python3",
    "pip",
    "pip3",
    "ruby",
    "go",
    "cc",
    "gcc",
    "clang",
    "make",
    "cmake",
    "gradle",
    "maven",
    "java",
    "javac",
    // Version control
    "git",
    "svn",
    "hg",
    // System info
    "echo",
    "pwd",
    "whoami",
    "id",
    "uname",
    "arch",
    "lsb_release",
    "env",
    "printenv",
    "date",
    "uptime",
    "free",
    "top",
    "htop",
    "ps",
    "pgrep",
    "pstree",
    "lsof",
    // Network
    "curl",
    "wget",
    "nc",
    "netcat",
    "ping",
    "traceroute",
    "dig",
    "nslookup",
    "host",
    "whois",
    "ifconfig",
    "ip",
    "netstat",
    "ss",
    // Text processing
    "sed",
    "awk",
    "cut",
    "paste",
    "tr",
    "xargs",
    "tee",
    "wc",
    "nl",
    "comm",
    "join",
    "expand",
    "unexpand",
    // Archive
    "tar",
    "gzip",
    "gunzip",
    "bzip2",
    "xz",
    "zip",
    "unzip",
    // Other useful
    "which",
    "whereis",
    "type",
    "command",
    "alias",
    "unalias",
    "history",
    "time",
    "timeout",
    "watch",
    "screen",
    "tmux",
];

const BASH_BLOCKED_COMMANDS: &[&str] = &[
    // Filesystem operations
    "mkfs",
    "fsck",
    "fsck.ext4",
    "e2fsck",
    "fsck.ntfs",
    // Block device operations
    "dd",
    "fdisk",
    "parted",
    "gdisk",
    "cfdisk",
    "sfdisk",
    "blockdev",
    // System management
    "shutdown",
    "reboot",
    "halt",
    "poweroff",
    "init",
    "systemctl",
    "service",
    // User management
    "su",
    "sudo",
    "useradd",
    "userdel",
    "usermod",
    "groupadd",
    "groupdel",
    "groupmod",
    "passwd",
    "visudo",
    "chown",
    "chgrp",
    // Security
    "setfacl",
    "getfacl",
    "semanage",
    "getenforce",
    "setenforce",
    "iptables",
    "ufw",
];

// ── PowerShell Allowed Commands ────────────────────────────────────────
const POWERSHELL_ALLOWED_COMMANDS: &[&str] = &[
    // File operations
    "Get-ChildItem",
    "gci",
    "ls",
    "dir",
    "Get-Content",
    "gc",
    "cat",
    "type",
    "Get-Item",
    "Test-Path",
    "Get-ItemProperty",
    // Directory navigation
    "Set-Location",
    "sl",
    "cd",
    "Get-Location",
    "pwd",
    // File search
    "Get-ChildItem",
    "Where-Object",
    "Select-String",
    "sls",
    "findstr",
    // Text output
    "Write-Host",
    "Write-Output",
    "echo",
    "Get-Content",
    "cat",
    // Build tools
    "cargo",
    "rustc",
    "npm",
    "yarn",
    "node",
    "python",
    "python3",
    "pip",
    "pip3",
    "java",
    "javac",
    "dotnet",
    "msbuild",
    // Version control
    "git",
    "svn",
    // Process management
    "Get-Process",
    "gps",
    "ps",
    "tasklist",
    "Stop-Process",
    "kill",
    // System info
    "Get-ComputerInfo",
    "systeminfo",
    "Get-Date",
    "Get-Host",
    "Get-TimeZone",
    "Get-Hotfix",
    "Get-NetAdapter",
    "ipconfig",
    "Get-NetIPAddress",
    // Network
    "Test-NetConnection",
    "tnc",
    "ping",
    "Invoke-WebRequest",
    "iwr",
    "curl",
    "wget",
    // Archive
    "Compress-Archive",
    "Expand-Archive",
    "tar",
    "zip",
    "7z",
];

const POWERSHELL_BLOCKED_COMMANDS: &[&str] = &[
    // Filesystem/Disk operations
    "Format-Volume",
    "Clear-Disk",
    "Initialize-Disk",
    // System management
    "Restart-Computer",
    "Stop-Computer",
    "Shutdown",
    "Set-ExecutionPolicy",
    // User/Security management
    "New-LocalUser",
    "Remove-LocalUser",
    "Set-LocalUser",
    "Set-ItemProperty",
    // Critical Windows operations
    "Remove-Item",
    "Clear-Item",
    "Remove-PSDrive",
];

// ── cmd.exe Allowed Commands ───────────────────────────────────────────
const CMD_ALLOWED_COMMANDS: &[&str] = &[
    // File operations
    "dir",
    "type",
    "copy",
    "xcopy",
    "move",
    "attrib",
    "findstr",
    "find",
    "where",
    "cls",
    "more",
    "tree",
    // Directory
    "cd",
    "chdir",
    "md",
    "mkdir",
    "rmdir",
    "pushd",
    "popd",
    // System info
    "echo",
    "set",
    "systeminfo",
    "tasklist",
    "taskkill",
    "wmic",
    "powershell",
    "cmd",
    // Network
    "ping",
    "ipconfig",
    "netstat",
    "tracert",
    "nslookup",
    "route",
    // Build tools
    "cargo",
    "rustc",
    "npm",
    "yarn",
    "node",
    "python",
    "python3",
    "java",
    "javac",
    "dotnet",
    "msbuild",
    // Version control
    "git",
    "svn",
    // Archive
    "tar",
    "7z",
    "zip",
];

const CMD_BLOCKED_COMMANDS: &[&str] = &[
    // Disk operations
    "format", "diskpart", "chkdsk", "defrag", // System management
    "shutdown", "restart", "del", "deltree", "erase", "cipher", // User/security
    "net", "user", "group", "icacls", "takeown",
];

// ────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_shell_type_detection() {
        assert_eq!(ShellType::detect("bash"), ShellType::Bash);
        assert_eq!(ShellType::detect("/bin/bash"), ShellType::Bash);
        assert_eq!(ShellType::detect("zsh"), ShellType::Bash);
        assert_eq!(ShellType::detect("powershell"), ShellType::PowerShell);
        assert_eq!(
            ShellType::detect("C:\\Windows\\System32\\powershell.exe"),
            ShellType::PowerShell
        );
        assert_eq!(ShellType::detect("cmd"), ShellType::Cmd);
        assert_eq!(ShellType::detect("cmd.exe"), ShellType::Cmd);
    }

    #[test]
    fn test_path_in_workspace() {
        let workspace = tempdir().unwrap();
        let valid_path = workspace.path().join("file.txt");
        assert!(validate_path_in_workspace(&valid_path, workspace.path()).is_ok());
    }

    #[test]
    fn test_path_outside_workspace() {
        let workspace = tempdir().unwrap();
        let other = tempdir().unwrap();
        let invalid_path = other.path().join("file.txt");
        assert!(validate_path_in_workspace(&invalid_path, workspace.path()).is_err());
    }

    #[test]
    fn test_bash_allowed_commands() {
        let commands = get_allowed_commands(ShellType::Bash);
        assert!(commands.contains(&"ls"));
        assert!(commands.contains(&"grep"));
        assert!(commands.contains(&"cargo"));
        assert!(commands.contains(&"git"));
    }

    #[test]
    fn test_bash_blocked_commands() {
        let commands = get_blocked_commands(ShellType::Bash);
        assert!(commands.contains(&"mkfs"));
        assert!(commands.contains(&"dd"));
        assert!(commands.contains(&"shutdown"));
        assert!(!commands.contains(&"ls"));
    }

    #[test]
    fn test_powershell_allowed_commands() {
        let commands = get_allowed_commands(ShellType::PowerShell);
        assert!(commands.contains(&"Get-ChildItem"));
        assert!(commands.contains(&"Get-Content"));
        assert!(commands.contains(&"cargo"));
    }

    #[test]
    fn test_powershell_blocked_commands() {
        let commands = get_blocked_commands(ShellType::PowerShell);
        assert!(commands.contains(&"Format-Volume"));
        assert!(commands.contains(&"Restart-Computer"));
        assert!(!commands.contains(&"Get-ChildItem"));
    }

    #[test]
    fn test_cmd_allowed_commands() {
        let commands = get_allowed_commands(ShellType::Cmd);
        assert!(commands.contains(&"dir"));
        assert!(commands.contains(&"tasklist"));
        assert!(commands.contains(&"npm"));
    }

    #[test]
    fn test_cmd_blocked_commands() {
        let commands = get_blocked_commands(ShellType::Cmd);
        assert!(commands.contains(&"format"));
        assert!(commands.contains(&"diskpart"));
        assert!(!commands.contains(&"dir"));
    }

    #[cfg(windows)]
    #[test]
    fn test_normalize_wsl_path() {
        assert_eq!(normalize_path_syntax("/mnt/c/Users"), "C:/Users");
    }

    #[cfg(windows)]
    #[test]
    fn test_normalize_cygwin_path() {
        assert_eq!(normalize_path_syntax("/cygdrive/c/Users"), "C:/Users");
    }

    #[cfg(unix)]
    #[test]
    fn test_normalize_unix_path() {
        assert_eq!(normalize_path_syntax("/home/user"), "/home/user");
    }
}
