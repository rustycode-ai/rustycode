#![allow(dead_code)]

use anyhow::Result;
use rustycode_protocol::ToolResult;

pub const MAX_TOOL_ITERATIONS: u32 = 3;

/// Check if a shell command should be auto-executed.
pub fn should_auto_execute(command: &str) -> bool {
    let safe_prefixes = [
        "git add",
        "git commit",
        "git status",
        "git log",
        "git diff",
        "ls", // Fixed: removed space to match "ls", "ls -la", etc.
        "pwd",
        "echo ",
        "cat ",
        "head ",
        "tail ",
    ];

    safe_prefixes
        .iter()
        .any(|prefix| command.starts_with(prefix))
}

/// Parse a command pipeline into individual commands.
/// Handles simple pipes (ignoring quoted pipes for simplicity).
fn parse_pipeline(command: &str) -> Vec<&str> {
    command
        .split('|')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Check if a single command (not a pipeline) is whitelisted and safe.
fn is_single_command_allowed(base_command: &str) -> bool {
    const ALLOWED_COMMANDS: &[&str] = &[
        "git", "ls", "pwd", "echo", "cat", "head", "tail", "grep", "find", "wc", "sort", "uniq",
        "cut", "date", "whoami", "basename", "dirname", "realpath", "readlink", "xargs", "awk",
        "sed", "tr", "tee", // Added for pipeline support
    ];
    ALLOWED_COMMANDS.contains(&base_command)
}

/// Check for truly dangerous patterns (command injection vectors).
fn has_dangerous_patterns(command: &str) -> bool {
    // These are patterns that enable command injection
    let dangerous = [
        ";",    // Command separator
        "&",    // Background operator
        "`",    // Command substitution (old style)
        "$(",   // Command substitution (new style)
        "\\(",  // Subshell
        "\\${", // Variable expansion with braces (potential injection)
    ];

    dangerous.iter().any(|pattern| command.contains(pattern))
}

/// Check if file redirection is safe (only with whitelisted commands).
fn has_unsafe_redirection(command: &str) -> bool {
    // Allow output/input redirection with whitelisted commands
    // Block redirection to/from dangerous locations
    let dangerous_paths = [
        "/etc/passwd",
        "/etc/shadow",
        "~/.ssh/",
        "id_rsa",
        ".env",
        "/dev/",
    ];

    // Check for redirection to dangerous paths
    if (command.contains(">") || command.contains("<"))
        && dangerous_paths.iter().any(|path| command.contains(path))
    {
        return true;
    }

    false
}

/// Sanitize a shell command to prevent command injection while allowing safe pipelines.
///
/// This function:
/// - Allows pipelines (|) between whitelisted commands
/// - Allows file redirection (> / <) to safe locations
/// - Blocks command injection patterns (;, &, backticks, $(), etc.)
/// - Blocks dangerous git commands
pub fn sanitize_command(command: &str) -> Result<String> {
    if command.trim().is_empty() {
        anyhow::bail!("Empty command");
    }

    // Check for truly dangerous patterns first
    if has_dangerous_patterns(command) {
        anyhow::bail!(
            "Command contains potentially dangerous patterns (command separators, substitutions, etc)"
        );
    }

    // Check for unsafe file redirection
    if has_unsafe_redirection(command) {
        anyhow::bail!("Command redirects to/from a sensitive system location");
    }

    // Parse pipeline and validate each command
    let pipeline_commands = parse_pipeline(command);

    for cmd in &pipeline_commands {
        let base_command = cmd
            .split_whitespace()
            .next()
            .ok_or_else(|| anyhow::anyhow!("Empty command in pipeline"))?;

        // Check if command is whitelisted
        if !is_single_command_allowed(base_command) {
            anyhow::bail!(
                "Command '{}' is not allowed. See documentation for allowed commands.",
                base_command
            );
        }

        // Special checks for git commands
        if base_command == "git" {
            let dangerous_git_flags = ["clean", "reset --", "rm", "checkout --", "branch -D"];
            for flag in dangerous_git_flags {
                if cmd.contains(flag) {
                    anyhow::bail!("Git command '{}' is not allowed for safety reasons.", flag);
                }
            }
        }
    }

    Ok(command.to_string())
}

/// Create a compact, bundled multiline summary for command output.
///
/// This function filters empty lines, limits output to a maximum number of lines,
/// and appends a count of any omitted lines.
pub fn bundle_command_output(output: &str, output_max_lines: usize) -> String {
    let visible_lines: Vec<&str> = output
        .lines()
        .filter(|line| !line.trim().is_empty())
        .take(output_max_lines)
        .collect();
    let total_non_empty_lines = output
        .lines()
        .filter(|line| !line.trim().is_empty())
        .count();

    let mut bundled = if visible_lines.is_empty() {
        "(no output)".to_string()
    } else {
        visible_lines.join("\n")
    };

    if total_non_empty_lines > output_max_lines {
        bundled.push_str(&format!(
            "\n... ({} more lines)",
            total_non_empty_lines - output_max_lines
        ));
    }

    bundled
}

pub fn is_dangerous_shell_command(command: &str) -> bool {
    let cmd = command.to_lowercase();
    let blocked = [
        "rm -rf /", "rm -fr /", "rm -rf ~", "sudo rm", "mkfs", "dd if=", "shutdown", "reboot",
        "poweroff", "halt", ":(){",
    ];
    blocked.iter().any(|p| cmd.contains(p))
}

/// Build a canonical conversation summary for a tool result.
pub fn format_tool_result_summary(tool_result: &ToolResult, tool_name: &str) -> String {
    if tool_result.error.is_none() {
        let output_lines = tool_result.output.lines().count();
        let output_chars = tool_result.output.chars().count();

        // For small outputs, include the full output. For large outputs, include preview.
        let include_full_output = output_chars <= 2000 && output_lines <= 50;

        let summary = if include_full_output {
            format!(
                "Tool result: call_id={} name={} success=true\noutput:\n{}",
                tool_result.call_id, tool_name, tool_result.output
            )
        } else {
            let first_line_preview = tool_result
                .output
                .lines()
                .find(|l| !l.trim().is_empty())
                .map(|l| {
                    if l.chars().count() > 160 {
                        let mut s: String = l.chars().take(160).collect();
                        s.push('…');
                        s
                    } else {
                        l.to_string()
                    }
                })
                .unwrap_or_else(|| "(no output)".to_string());

            format!(
                "Tool result: call_id={} name={} success=true output_lines={} output_chars={} preview={}",
                tool_result.call_id,
                tool_name,
                output_lines,
                output_chars,
                first_line_preview
            )
        };

        summary
    } else {
        format!(
            "Tool result: call_id={} name={} success=false\nerror: {}",
            tool_result.call_id,
            tool_name,
            tool_result.error.as_deref().unwrap_or("unknown error")
        )
    }
}

/// Return a user-facing hint for common tool execution errors.
pub fn tool_error_hint(error: &str) -> Option<&'static str> {
    let error_lower = error.to_lowercase();
    if error_lower.contains("permission denied") || error_lower.contains("access denied") {
        Some("💡 Tip: Check file permissions or try with elevated privileges")
    } else if error_lower.contains("not found") {
        Some("💡 Tip: Check if the file/path exists and is correct")
    } else if error_lower.contains("timeout") {
        Some("💡 Tip: Operation timed out. Try again or break into smaller steps")
    } else {
        None
    }
}

/// Return a user-facing hint for common shell/command errors.
pub fn command_error_hint(command: &str, stderr: &str) -> Option<String> {
    let error_lower = stderr.to_lowercase();
    if error_lower.contains("permission denied") || error_lower.contains("access denied") {
        Some("💡 Tip: Try running with sudo or check file permissions".to_string())
    } else if error_lower.contains("command not found") || error_lower.contains("not recognized") {
        let cmd = command.split_whitespace().next().unwrap_or("command");
        Some(format!(
            "💡 Tip: Install {} or check if it's in your PATH",
            cmd
        ))
    } else if error_lower.contains("no such file") || error_lower.contains("cannot find") {
        Some("💡 Tip: Check if the file/path exists and is correct".to_string())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_should_auto_execute_known_safe_commands() {
        assert!(should_auto_execute("git status"));
        assert!(should_auto_execute("pwd"));
        assert!(!should_auto_execute("rm -rf ."));
    }

    #[test]
    fn test_sanitize_command_blocks_dangerous_patterns() {
        // Command separators should be blocked
        assert!(sanitize_command("git status; rm -rf /").is_err());
        assert!(sanitize_command("ls & echo done").is_err());
        assert!(sanitize_command("ls $(echo test)").is_err());
        assert!(sanitize_command("ls `echo test`").is_err());
    }

    #[test]
    fn test_sanitize_command_allows_safe_pipelines() {
        // Pipes with whitelisted commands should be allowed
        assert!(sanitize_command("ls | head -10").is_ok());
        assert!(sanitize_command("find . -name '*.rs' | wc -l").is_ok());
        assert!(sanitize_command("cat file.txt | grep test | sort | uniq").is_ok());
    }

    #[test]
    fn test_sanitize_command_blocks_unsafe_pipeline_commands() {
        // Pipeline with non-whitelisted command should be blocked
        assert!(sanitize_command("ls | rm -rf /").is_err());
    }

    #[test]
    fn test_sanitize_command_allows_safe_commands() {
        assert!(sanitize_command("git status").is_ok());
        assert!(sanitize_command("ls src").is_ok());
    }

    #[test]
    fn test_is_dangerous_shell_command() {
        assert!(is_dangerous_shell_command("rm -rf /tmp && rm -rf /"));
        assert!(!is_dangerous_shell_command("cargo clippy"));
    }

    #[test]
    fn test_format_tool_result_summary_success_with_structured() {
        let result = ToolResult {
            call_id: "c1".to_string(),
            output: "ok".to_string(),
            error: None,
            success: true,
            data: Some(serde_json::json!({"k":"v"})),
            exit_code: Some(0),
        };
        let summary = format_tool_result_summary(&result, "read_file");
        assert!(summary.contains("success=true"));
    }

    #[test]
    fn test_tool_error_hint_permission() {
        assert_eq!(
            tool_error_hint("Permission denied while opening file"),
            Some("💡 Tip: Check file permissions or try with elevated privileges")
        );
    }

    #[test]
    fn test_command_error_hint_not_found() {
        let hint = command_error_hint("foo --bar", "command not found: foo");
        assert_eq!(
            hint,
            Some("💡 Tip: Install foo or check if it's in your PATH".to_string())
        );
    }
}
