//! Tool Risk Classification

/// Known tool name constants — single source of truth for classification.
pub mod tool_names {
    pub const READ_FILE: &str = "read_file";
    pub const WRITE_FILE: &str = "write_file";
    pub const EDIT_FILE: &str = "edit_file";
    pub const APPLY_PATCH: &str = "apply_patch";
    pub const BASH: &str = "bash";
    pub const GREP: &str = "grep";
    pub const GLOB: &str = "glob";
    pub const LIST_FILES: &str = "list_files";
    pub const LIST_DIR: &str = "list_dir";
    pub const GIT_STATUS: &str = "git_status";
    pub const GIT_DIFF: &str = "git_diff";
    pub const GIT_LOG: &str = "git_log";
    pub const GIT_COMMIT: &str = "git_commit";
}

/// Tool risk level
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[non_exhaustive]
pub enum RiskLevel {
    /// Safe tools - read-only, no side effects
    Safe = 0,
    /// Medium risk - file writes, modifications
    Medium = 1,
    /// High risk - system commands, execution
    High = 2,
    /// Dangerous - destructive operations
    Dangerous = 3,
}

/// Tool type categories
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ToolType {
    /// Read file contents
    ReadFile,
    /// Write/create files
    WriteFile,
    /// Execute bash command
    Bash,
    /// Search/grep operations
    Grep,
    /// Find files
    Find,
    /// List directory
    ListDirectory,
    /// Delete files
    DeleteFile,
    /// Git operations
    Git,
    /// Custom tool
    Custom(String),
}

/// Classify tool risk based on type and command
pub fn classify_tool_risk(tool_type: &ToolType, command: &str) -> RiskLevel {
    match tool_type {
        // Read operations are always safe
        ToolType::ReadFile => RiskLevel::Safe,

        // Search operations are safe
        ToolType::Grep | ToolType::Find => RiskLevel::Safe,

        // List directory is safe
        ToolType::ListDirectory => RiskLevel::Safe,

        // Write operations are medium risk
        ToolType::WriteFile => RiskLevel::Medium,

        // Git operations are medium risk
        ToolType::Git => RiskLevel::Medium,

        // Bash commands - use SmartApprove for fine-grained analysis
        ToolType::Bash => {
            use rustycode_tools_security::approve::OperationClass;
            let sa = rustycode_tools_security::approve::SmartApprove::new();
            match sa.classify("bash", Some(command)) {
                OperationClass::ReadOnly => RiskLevel::Safe,
                OperationClass::Write => RiskLevel::Medium,
                OperationClass::Destructive => RiskLevel::Dangerous,
                OperationClass::Unknown => classify_bash_command_risk(command),
                #[allow(unreachable_patterns)]
                _ => classify_bash_command_risk(command),
            }
        }

        // Delete operations are dangerous
        ToolType::DeleteFile => RiskLevel::Dangerous,

        // Custom tools - assume high risk
        ToolType::Custom(_) => RiskLevel::High,
    }
}

/// Classify bash command risk level
fn classify_bash_command_risk(command: &str) -> RiskLevel {
    let command_lower = command.to_lowercase();

    // Check for destructive patterns
    if command_lower.contains("rm -rf") ||
       command_lower.contains("rm -fr") ||
       command_lower.contains(":() {") ||  // fork bomb
       command_lower.contains("dd if=") ||  // disk destroyer
       command_lower.contains("mkfs") ||   // format filesystem
       command_lower.contains("> format") ||  // format command
       command_lower.contains("fdisk")
    {
        return RiskLevel::Dangerous;
    }

    // Check for high-risk operations
    if command_lower.contains("rm ")
        || command_lower.contains("kill ")
        || command_lower.contains("pkill ")
        || command_lower.contains("killall ")
        || command_lower.contains("shutdown")
        || command_lower.contains("reboot")
        || command_lower.contains("systemctl")
    {
        return RiskLevel::High;
    }

    // Check for build/compile operations - medium risk
    if command_lower.contains("cargo build")
        || command_lower.contains("cargo run")
        || command_lower.contains("make")
        || command_lower.contains("npm install")
        || command_lower.contains("npm run")
    {
        return RiskLevel::Medium;
    }

    // Read operations are safe
    if command_lower.contains("cat ")
        || command_lower.contains("ls ")
        || command_lower.contains("echo ")
        || command_lower.contains("pwd")
    {
        return RiskLevel::Safe;
    }

    // Default bash commands to high risk
    RiskLevel::High
}

/// Get risk level color for display
pub fn risk_level_color(risk: RiskLevel) -> &'static str {
    match risk {
        RiskLevel::Safe => "green",
        RiskLevel::Medium => "yellow",
        RiskLevel::High => "orange",
        RiskLevel::Dangerous => "red",
    }
}

/// Get risk level description
pub fn risk_level_description(risk: RiskLevel) -> &'static str {
    match risk {
        RiskLevel::Safe => "Safe - Read-only operation",
        RiskLevel::Medium => "Medium - May modify files",
        RiskLevel::High => "High - System command execution",
        RiskLevel::Dangerous => "Dangerous - Destructive operation",
    }
}

pub fn should_auto_approve(tool_type: &ToolType, command: &str) -> bool {
    let risk = classify_tool_risk(tool_type, command);
    // Only automatically approve safe (read-only) operations.
    matches!(risk, RiskLevel::Safe)
}

/// Map a tool name string to a ToolType
pub fn classify_tool_type(tool_name: &str) -> ToolType {
    match tool_name {
        tool_names::READ_FILE => ToolType::ReadFile,
        tool_names::WRITE_FILE => ToolType::WriteFile,
        tool_names::BASH => ToolType::Bash,
        tool_names::GREP => ToolType::Grep,
        tool_names::GLOB | tool_names::LIST_FILES | tool_names::LIST_DIR => ToolType::ListDirectory,
        tool_names::EDIT_FILE | tool_names::APPLY_PATCH => ToolType::WriteFile,
        tool_names::GIT_STATUS
        | tool_names::GIT_DIFF
        | tool_names::GIT_LOG
        | tool_names::GIT_COMMIT => ToolType::Git,
        _ => ToolType::Custom(tool_name.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_tools_are_auto_approved() {
        assert!(should_auto_approve(
            &ToolType::ReadFile,
            r#"{"path":"src/main.rs"}"#
        ));
        assert!(should_auto_approve(
            &ToolType::Grep,
            r#"{"pattern":"TODO"}"#
        ));
        assert!(should_auto_approve(
            &ToolType::ListDirectory,
            r#"{"path":"."}"#
        ));
    }

    #[test]
    fn medium_risk_tools_require_confirmation() {
        assert!(!should_auto_approve(
            &ToolType::WriteFile,
            r#"{"path":"src/main.rs"}"#
        ));
        assert!(!should_auto_approve(
            &ToolType::Git,
            r#"{"command":"git diff"}"#
        ));
    }
}
