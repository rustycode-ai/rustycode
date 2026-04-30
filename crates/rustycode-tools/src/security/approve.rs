//! Smart Approval for Tool Operations
//!
//! Heuristic-based classification of tool calls into read-only, write, and
//! destructive categories. Read-only operations can be auto-approved;
//! destructive operations require explicit confirmation.
//!
//! Inspired by goose's `SmartApprove` pattern.

use std::collections::HashSet;

/// Classification of a tool operation.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum OperationClass {
    /// Safe, read-only operation — can auto-approve.
    ReadOnly,
    /// Write operation — requires confirmation.
    Write,
    /// Destructive operation — requires confirmation with warning.
    Destructive,
    /// Could not classify — treat as requiring confirmation.
    Unknown,
}

impl std::fmt::Display for OperationClass {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ReadOnly => write!(f, "read-only"),
            Self::Write => write!(f, "write"),
            Self::Destructive => write!(f, "destructive"),
            Self::Unknown => write!(f, "unknown"),
        }
    }
}

/// Heuristic-based tool operation classifier.
///
/// Uses tool name and argument inspection to classify operations
/// without requiring LLM calls.
pub struct SmartApprove {
    read_only_tools: HashSet<&'static str>,
    write_tools: HashSet<&'static str>,
    destructive_bash_commands: &'static [&'static str],
    read_only_bash_commands: &'static [&'static str],
}

impl SmartApprove {
    /// Create a new classifier with built-in heuristics.
    pub fn new() -> Self {
        Self {
            read_only_tools: HashSet::from([
                "read_file",
                "list_dir",
                "grep",
                "glob",
                "git_status",
                "git_diff",
                "git_log",
                "lsp_diagnostics",
                "lsp_hover",
                "lsp_definition",
                "lsp_completion",
                "lsp_document_symbols",
                "lsp_references",
                "lsp_full_diagnostics",
                "lsp_code_actions",
                "lsp_formatting",
                "web_fetch",
                "web_search",
                "semantic_search",
                "code_search",
                "coverage",
                "list_plans",
                "load_plan",
                "docker_images",
                "docker_ps",
                "docker_inspect",
                "docker_logs",
                "database_schema",
                "database_query",
                "todo_read",
            ]),
            write_tools: HashSet::from([
                "write_file",
                "edit_file",
                "text_editor_20250728",
                "text_editor_20250124",
                "search_replace",
                "apply_patch",
                "multi_edit",
                "git_commit",
                "lsp_rename",
                "save_plan",
                "create_plan",
                "approve_plan",
                "todo_write",
                "todo_update",
            ]),
            destructive_bash_commands: &[
                "rm ",
                "rm -",
                "rmdir",
                "sudo ",
                "chmod ",
                "chown ",
                "git push",
                "git push --force",
                "git reset",
                "git checkout --",
                "git clean",
                "git rebase",
                "git cherry-pick",
                "docker rm",
                "docker rmi",
                "docker stop",
                "docker kill",
                "drop table",
                "delete from",
                "truncate",
                "truncate table",
                "mkfs",
                "dd if=",
                "shred",
                "format",
                "> /dev/",
                "pip uninstall",
                "npm uninstall",
                "cargo clean",
            ],
            read_only_bash_commands: &[
                "cat ",
                "head ",
                "tail ",
                "less ",
                "more ",
                "ls",
                "find ",
                "grep ",
                "rg ",
                "ag ",
                "wc ",
                "sort ",
                "uniq ",
                "diff ",
                "file ",
                "stat ",
                "du ",
                "df ",
                "ps ",
                "top ",
                "echo ",
                "which ",
                "where ",
                "type ",
                "pwd",
                "whoami",
                "id",
                "uname",
                "hostname",
                "date",
                "uptime",
                "git status",
                "git diff",
                "git log",
                "git show",
                "git branch",
                "git remote",
                "git stash list",
                "git tag",
                "git config --get",
                "cargo check",
                "cargo test",
                "cargo build",
                "cargo clippy",
                "cargo doc",
                "cargo metadata",
                "npm test",
                "npm list",
                "npm run",
                "npx ",
                "python",
                "node -e",
                "rustc --version",
                "rustup show",
            ],
        }
    }

    /// Classify a tool operation by tool name and optional arguments.
    ///
    /// # Arguments
    ///
    /// * `tool_name` - Name of the tool being invoked
    /// * `args` - Optional JSON string of tool arguments (used for bash command inspection)
    pub fn classify(&self, tool_name: &str, args: Option<&str>) -> OperationClass {
        // Normalize tool name
        let name = tool_name.trim().to_lowercase();

        // Direct lookup in read-only tools
        if self.read_only_tools.contains(name.as_str()) {
            return OperationClass::ReadOnly;
        }

        // Direct lookup in write tools
        if self.write_tools.contains(name.as_str()) {
            return OperationClass::Write;
        }

        // Bash needs special handling — inspect the command
        if name == "bash" {
            return self.classify_bash_command(args.unwrap_or(""));
        }

        // Docker tools — run/build are write-tier
        if name == "docker_run" || name == "docker_build" {
            return OperationClass::Write;
        }

        // Database mutations
        if name == "database_transaction" {
            return OperationClass::Destructive;
        }

        // Task tool — spawns sub-agents, treat as write
        if name == "task" {
            return OperationClass::Write;
        }

        // HTTP methods
        if name == "http_post" || name == "http_put" || name == "http_delete" {
            return OperationClass::Write;
        }
        if name == "http_get" {
            return OperationClass::ReadOnly;
        }

        // Batch tool — depends on contents, treat as unknown
        if name == "batch" {
            return OperationClass::Unknown;
        }

        OperationClass::Unknown
    }

    /// Classify a bash command by inspecting its content.
    fn classify_bash_command(&self, command: &str) -> OperationClass {
        let cmd = command.trim().to_lowercase();

        if cmd.is_empty() {
            return OperationClass::Unknown;
        }

        // Check destructive patterns first (higher priority)
        for pattern in self.destructive_bash_commands {
            if cmd.starts_with(pattern) || cmd.contains(pattern) {
                return OperationClass::Destructive;
            }
        }

        // Pipes and redirects suggest mutation potential (before read-only check)
        if cmd.contains('>') || cmd.contains(">>") || cmd.contains("| rm") {
            return OperationClass::Destructive;
        }

        // Check read-only patterns
        for pattern in self.read_only_bash_commands {
            if cmd.starts_with(pattern) {
                return OperationClass::ReadOnly;
            }
        }

        // Chained commands with && or ; — check each part
        if cmd.contains("&&") || cmd.contains(";") {
            for part in cmd.split(&['&', ';'][..]) {
                let part = part.trim();
                if part.is_empty() {
                    continue;
                }
                // If any part looks destructive, whole command is destructive
                for pattern in self.destructive_bash_commands {
                    if part.starts_with(pattern) || part.contains(pattern) {
                        return OperationClass::Destructive;
                    }
                }
            }
        }

        // Default: unknown (require confirmation)
        OperationClass::Unknown
    }

    /// Check if a tool operation can be auto-approved.
    pub fn can_auto_approve(&self, tool_name: &str, args: Option<&str>) -> bool {
        matches!(self.classify(tool_name, args), OperationClass::ReadOnly)
    }
}

impl Default for SmartApprove {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn classifier() -> SmartApprove {
        SmartApprove::new()
    }

    // ── Read-only tool tests ─────────────────────────────────────────────

    #[test]
    fn test_read_file_is_readonly() {
        assert_eq!(
            classifier().classify("read_file", None),
            OperationClass::ReadOnly
        );
    }

    #[test]
    fn test_grep_is_readonly() {
        assert_eq!(
            classifier().classify("grep", None),
            OperationClass::ReadOnly
        );
    }

    #[test]
    fn test_glob_is_readonly() {
        assert_eq!(
            classifier().classify("glob", None),
            OperationClass::ReadOnly
        );
    }

    #[test]
    fn test_list_dir_is_readonly() {
        assert_eq!(
            classifier().classify("list_dir", None),
            OperationClass::ReadOnly
        );
    }

    #[test]
    fn test_git_status_is_readonly() {
        assert_eq!(
            classifier().classify("git_status", None),
            OperationClass::ReadOnly
        );
    }

    #[test]
    fn test_git_diff_is_readonly() {
        assert_eq!(
            classifier().classify("git_diff", None),
            OperationClass::ReadOnly
        );
    }

    #[test]
    fn test_git_log_is_readonly() {
        assert_eq!(
            classifier().classify("git_log", None),
            OperationClass::ReadOnly
        );
    }

    #[test]
    fn test_web_fetch_is_readonly() {
        assert_eq!(
            classifier().classify("web_fetch", None),
            OperationClass::ReadOnly
        );
    }

    #[test]
    fn test_lsp_tools_are_readonly() {
        let sa = classifier();
        for tool in &[
            "lsp_diagnostics",
            "lsp_hover",
            "lsp_definition",
            "lsp_completion",
        ] {
            assert_eq!(
                sa.classify(tool, None),
                OperationClass::ReadOnly,
                "{}",
                tool
            );
        }
    }

    #[test]
    fn test_docker_inspect_is_readonly() {
        assert_eq!(
            classifier().classify("docker_inspect", None),
            OperationClass::ReadOnly
        );
    }

    #[test]
    fn test_database_query_is_readonly() {
        assert_eq!(
            classifier().classify("database_query", None),
            OperationClass::ReadOnly
        );
    }

    // ── Write tool tests ─────────────────────────────────────────────────

    #[test]
    fn test_write_file_is_write() {
        assert_eq!(
            classifier().classify("write_file", None),
            OperationClass::Write
        );
    }

    #[test]
    fn test_edit_file_is_write() {
        assert_eq!(
            classifier().classify("edit_file", None),
            OperationClass::Write
        );
    }

    #[test]
    fn test_git_commit_is_write() {
        assert_eq!(
            classifier().classify("git_commit", None),
            OperationClass::Write
        );
    }

    #[test]
    fn test_search_replace_is_write() {
        assert_eq!(
            classifier().classify("search_replace", None),
            OperationClass::Write
        );
    }

    #[test]
    fn test_multi_edit_is_write() {
        assert_eq!(
            classifier().classify("multi_edit", None),
            OperationClass::Write
        );
    }

    #[test]
    fn test_lsp_rename_is_write() {
        assert_eq!(
            classifier().classify("lsp_rename", None),
            OperationClass::Write
        );
    }

    // ── Bash classification tests ────────────────────────────────────────

    #[test]
    fn test_bash_ls_is_readonly() {
        assert_eq!(
            classifier().classify("bash", Some("ls -la")),
            OperationClass::ReadOnly
        );
    }

    #[test]
    fn test_bash_cat_is_readonly() {
        assert_eq!(
            classifier().classify("bash", Some("cat /etc/hosts")),
            OperationClass::ReadOnly
        );
    }

    #[test]
    fn test_bash_grep_is_readonly() {
        assert_eq!(
            classifier().classify("bash", Some("grep -r pattern src/")),
            OperationClass::ReadOnly
        );
    }

    #[test]
    fn test_bash_git_status_is_readonly() {
        assert_eq!(
            classifier().classify("bash", Some("git status")),
            OperationClass::ReadOnly
        );
    }

    #[test]
    fn test_bash_cargo_check_is_readonly() {
        assert_eq!(
            classifier().classify("bash", Some("cargo check")),
            OperationClass::ReadOnly
        );
    }

    #[test]
    fn test_bash_cargo_test_is_readonly() {
        assert_eq!(
            classifier().classify("bash", Some("cargo test")),
            OperationClass::ReadOnly
        );
    }

    #[test]
    fn test_bash_pwd_is_readonly() {
        assert_eq!(
            classifier().classify("bash", Some("pwd")),
            OperationClass::ReadOnly
        );
    }

    #[test]
    fn test_bash_echo_is_readonly() {
        assert_eq!(
            classifier().classify("bash", Some("echo hello")),
            OperationClass::ReadOnly
        );
    }

    #[test]
    fn test_bash_rm_is_destructive() {
        assert_eq!(
            classifier().classify("bash", Some("rm -rf /tmp/test")),
            OperationClass::Destructive
        );
    }

    #[test]
    fn test_bash_git_push_is_destructive() {
        assert_eq!(
            classifier().classify("bash", Some("git push origin main")),
            OperationClass::Destructive
        );
    }

    #[test]
    fn test_bash_git_reset_is_destructive() {
        assert_eq!(
            classifier().classify("bash", Some("git reset --hard HEAD~1")),
            OperationClass::Destructive
        );
    }

    #[test]
    fn test_bash_docker_rm_is_destructive() {
        assert_eq!(
            classifier().classify("bash", Some("docker rm container_id")),
            OperationClass::Destructive
        );
    }

    #[test]
    fn test_bash_redirect_is_destructive() {
        assert_eq!(
            classifier().classify("bash", Some("echo data > /tmp/file")),
            OperationClass::Destructive
        );
    }

    #[test]
    fn test_bash_chained_destructive() {
        assert_eq!(
            classifier().classify("bash", Some("cd src && rm test.txt")),
            OperationClass::Destructive
        );
    }

    #[test]
    fn test_bash_unknown_command() {
        assert_eq!(
            classifier().classify("bash", Some("some-custom-tool arg1")),
            OperationClass::Unknown
        );
    }

    #[test]
    fn test_bash_empty_command() {
        assert_eq!(
            classifier().classify("bash", Some("")),
            OperationClass::Unknown
        );
    }

    #[test]
    fn test_bash_sudo_is_destructive() {
        assert_eq!(
            classifier().classify("bash", Some("sudo apt install foo")),
            OperationClass::Destructive
        );
    }

    #[test]
    fn test_bash_chmod_is_destructive() {
        assert_eq!(
            classifier().classify("bash", Some("chmod 777 /etc/passwd")),
            OperationClass::Destructive
        );
    }

    #[test]
    fn test_bash_chown_is_destructive() {
        assert_eq!(
            classifier().classify("bash", Some("chown root:root /etc/shadow")),
            OperationClass::Destructive
        );
    }

    // ── Case insensitivity ───────────────────────────────────────────────

    #[test]
    fn test_case_insensitive_tool_name() {
        assert_eq!(
            classifier().classify("Read_File", None),
            OperationClass::ReadOnly
        );
        assert_eq!(
            classifier().classify("WRITE_FILE", None),
            OperationClass::Write
        );
        assert_eq!(
            classifier().classify("Bash", Some("ls")),
            OperationClass::ReadOnly
        );
    }

    #[test]
    fn test_whitespace_trimmed() {
        assert_eq!(
            classifier().classify("  read_file  ", None),
            OperationClass::ReadOnly
        );
    }

    // ── Auto-approve ─────────────────────────────────────────────────────

    #[test]
    fn test_can_auto_approve_readonly() {
        assert!(classifier().can_auto_approve("read_file", None));
        assert!(classifier().can_auto_approve("bash", Some("ls -la")));
    }

    #[test]
    fn test_cannot_auto_approve_write() {
        assert!(!classifier().can_auto_approve("write_file", None));
        assert!(!classifier().can_auto_approve("bash", Some("rm file.txt")));
    }

    #[test]
    fn test_cannot_auto_approve_unknown() {
        assert!(!classifier().can_auto_approve("unknown_tool", None));
    }

    // ── Special tools ────────────────────────────────────────────────────

    #[test]
    fn test_database_transaction_is_destructive() {
        assert_eq!(
            classifier().classify("database_transaction", None),
            OperationClass::Destructive
        );
    }

    #[test]
    fn test_task_tool_is_write() {
        assert_eq!(classifier().classify("task", None), OperationClass::Write);
    }

    #[test]
    fn test_http_get_is_readonly() {
        assert_eq!(
            classifier().classify("http_get", None),
            OperationClass::ReadOnly
        );
    }

    #[test]
    fn test_http_post_is_write() {
        assert_eq!(
            classifier().classify("http_post", None),
            OperationClass::Write
        );
    }

    #[test]
    fn test_batch_is_unknown() {
        assert_eq!(
            classifier().classify("batch", None),
            OperationClass::Unknown
        );
    }

    // ── Display ──────────────────────────────────────────────────────────

    #[test]
    fn test_operation_class_display() {
        assert_eq!(OperationClass::ReadOnly.to_string(), "read-only");
        assert_eq!(OperationClass::Write.to_string(), "write");
        assert_eq!(OperationClass::Destructive.to_string(), "destructive");
        assert_eq!(OperationClass::Unknown.to_string(), "unknown");
    }

    // ── Batch classification ─────────────────────────────────────────────

    #[test]
    fn test_batch_classify_mixed() {
        let sa = classifier();
        let results: Vec<_> = [
            ("read_file", None),
            ("write_file", None),
            ("bash", Some("rm -rf /")),
            ("grep", None),
            ("bash", Some("ls")),
        ]
        .iter()
        .map(|(name, args)| sa.classify(name, *args))
        .collect();

        assert_eq!(results[0], OperationClass::ReadOnly);
        assert_eq!(results[1], OperationClass::Write);
        assert_eq!(results[2], OperationClass::Destructive);
        assert_eq!(results[3], OperationClass::ReadOnly);
        assert_eq!(results[4], OperationClass::ReadOnly);
    }
}
