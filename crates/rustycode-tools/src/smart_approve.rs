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
    #[must_use]
    pub fn new() -> Self {
        Self {
            read_only_tools: HashSet::from([
                "read",
                "ListDir",
                "grep",
                "glob",
                "Find",
                "Inspect",
                "GitStatus",
                "GitDiff",
                "GitLog",
                "LspDiagnostics",
                "LspHover",
                "LspDefinition",
                "LspCompletion",
                "LspDocumentSymbols",
                "LspReferences",
                "LspFullDiagnostics",
                "LspCodeActions",
                "LspFormatting",
                "webfetch",
                "WebSearch",
                "SemanticSearch",
                "Codesearch",
                "Codesearch",
                "coverage",
                "ListPlans",
                "LoadPlan",
                "DockerImages",
                "DockerPs",
                "DockerInspect",
                "DockerLogs",
                "database_schema",
                "DatabaseQuery",
                "TodoRead",
                "brief",
                "TaskOutput",
                "ToolSearch",
            ]),
            write_tools: HashSet::from([
                "write",
                "edit",
                "text_editor_20250124",
                "ApplyPatch",
                "multi_edit",
                "MultiEdit",
                "GitCommit",
                "LspRename",
                "SavePlan",
                "create_plan",
                "ApprovePlan",
                "TodoWrite",
                "TodoUpdate",
                "SendMessage",
                "TaskStop",
                "notebookedit",
                "structuredoutput",
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
                // Download commands that write to disk
                "curl -o ",
                "curl --output ",
                "wget ",
                "wget -O ",
                // GitHub CLI — mutation operations
                "gh pr create",
                "gh pr merge",
                "gh pr close",
                "gh pr reopen",
                "gh pr review",
                "gh issue create",
                "gh issue close",
                "gh issue edit",
                "gh pr edit",
                "gh repo delete",
                "gh release create",
                "gh release delete",
                "gh repo fork",
                "npm publish",
                "cargo publish",
                "pip install --force",
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
                "cargo metadata",
                "npm list",
                "npm run",
                "node --version",
                "node --help",
                "rustc --version",
                "rustup show",
                // GitHub CLI — read-only operations
                "gh pr view",
                "gh pr list",
                "gh pr checks",
                "gh pr diff",
                "gh issue view",
                "gh issue list",
                "gh repo view",
                "gh repo list",
                "gh api ",
                "gh run view",
                "gh run list",
                "gh release view",
                "gh release list",
                "gh search ",
                // Other common read-only tools
                "curl ",
                "jq ",
                "yq ",
                // Environment/package inspection
                "env",
                "printenv",
                "npm view ",
                "npm info ",
                "npm pack --dry-run",
                "pip show ",
                "pip list",
                // GitHub CLI — additional read-only
                "gh pr checkout",
                "gh bcs ",
                "gh status",
            ],
        }
    }

    /// Classify a tool operation by tool name and optional arguments.
    ///
    #[must_use]
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
        if name == "DockerRun" || name == "DockerBuild" {
            return OperationClass::Write;
        }

        // Database mutations
        if name == "DatabaseTransaction" {
            return OperationClass::Destructive;
        }

        // Task tool — spawns sub-agents, treat as write
        if name == "task" {
            return OperationClass::Write;
        }

        // HTTP methods
        if name == "HttpPost" || name == "HttpPut" || name == "HttpDelete" {
            return OperationClass::Write;
        }
        if name == "HttpGet" {
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

        // Redirects to files are always destructive
        if cmd.contains('>') || cmd.contains(">>") {
            return OperationClass::Destructive;
        }

        let segments: Vec<&str> = cmd.split('|').map(str::trim).collect();
        let mut any_read_only = false;

        for segment in &segments {
            if segment.is_empty() {
                continue;
            }

            for pattern in self.destructive_bash_commands {
                if segment.starts_with(pattern) {
                    return OperationClass::Destructive;
                }
            }

            for pattern in self.read_only_bash_commands {
                if segment.starts_with(pattern) {
                    any_read_only = true;
                    break;
                }
            }
        }

        if cmd.contains("&&") || cmd.contains(";") {
            for part in cmd.split(&['&', ';'][..]) {
                let part = part.trim();
                if part.is_empty() {
                    continue;
                }
                for pattern in self.destructive_bash_commands {
                    if part.starts_with(pattern) {
                        return OperationClass::Destructive;
                    }
                }
            }
        }

        if any_read_only && !segments.is_empty() {
            let all_known = segments.iter().all(|segment| {
                if segment.is_empty() {
                    return true;
                }
                self.read_only_bash_commands
                    .iter()
                    .any(|pattern| segment.starts_with(pattern))
            });
            if all_known {
                return OperationClass::ReadOnly;
            }
        }

        // Default: unknown (require confirmation)
        OperationClass::Unknown
    }

    /// Check if a tool operation can be auto-approved.
    #[must_use]
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
            classifier().classify("Read", None),
            OperationClass::ReadOnly
        );
    }

    #[test]
    fn test_grep_is_readonly() {
        assert_eq!(
            classifier().classify("Grep", None),
            OperationClass::ReadOnly
        );
    }

    #[test]
    fn test_glob_is_readonly() {
        assert_eq!(
            classifier().classify("Glob", None),
            OperationClass::ReadOnly
        );
    }

    #[test]
    fn test_list_dir_is_readonly() {
        assert_eq!(
            classifier().classify("ListDir", None),
            OperationClass::ReadOnly
        );
    }

    #[test]
    fn test_git_status_is_readonly() {
        assert_eq!(
            classifier().classify("GitStatus", None),
            OperationClass::ReadOnly
        );
    }

    #[test]
    fn test_git_diff_is_readonly() {
        assert_eq!(
            classifier().classify("GitDiff", None),
            OperationClass::ReadOnly
        );
    }

    #[test]
    fn test_git_log_is_readonly() {
        assert_eq!(
            classifier().classify("GitLog", None),
            OperationClass::ReadOnly
        );
    }

    #[test]
    fn test_web_fetch_is_readonly() {
        assert_eq!(
            classifier().classify("WebFetch", None),
            OperationClass::ReadOnly
        );
    }

    #[test]
    fn test_lsp_tools_are_readonly() {
        let sa = classifier();
        for tool in &[
            "LspDiagnostics",
            "LspHover",
            "LspDefinition",
            "LspCompletion",
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
            classifier().classify("DockerInspect", None),
            OperationClass::ReadOnly
        );
    }

    #[test]
    fn test_database_query_is_readonly() {
        assert_eq!(
            classifier().classify("DatabaseQuery", None),
            OperationClass::ReadOnly
        );
    }

    // ── Write tool tests ─────────────────────────────────────────────────

    #[test]
    fn test_write_file_is_write() {
        assert_eq!(classifier().classify("Write", None), OperationClass::Write);
    }

    #[test]
    fn test_edit_file_is_write() {
        assert_eq!(classifier().classify("Edit", None), OperationClass::Write);
    }

    #[test]
    fn test_git_commit_is_write() {
        assert_eq!(
            classifier().classify("GitCommit", None),
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
            classifier().classify("LspRename", None),
            OperationClass::Write
        );
    }

    // ── Bash classification tests ────────────────────────────────────────

    #[test]
    fn test_bash_ls_is_readonly() {
        assert_eq!(
            classifier().classify("Bash", Some("ls -la")),
            OperationClass::ReadOnly
        );
    }

    #[test]
    fn test_bash_cat_is_readonly() {
        assert_eq!(
            classifier().classify("Bash", Some("cat /etc/hosts")),
            OperationClass::ReadOnly
        );
    }

    #[test]
    fn test_bash_grep_is_readonly() {
        assert_eq!(
            classifier().classify("Bash", Some("grep -r pattern src/")),
            OperationClass::ReadOnly
        );
    }

    #[test]
    fn test_bash_git_status_is_readonly() {
        assert_eq!(
            classifier().classify("Bash", Some("git status")),
            OperationClass::ReadOnly
        );
    }

    #[test]
    fn test_bash_cargo_check_is_readonly() {
        assert_eq!(
            classifier().classify("Bash", Some("cargo check")),
            OperationClass::ReadOnly
        );
    }

    #[test]
    fn test_bash_cargo_test_is_unknown() {
        assert_eq!(
            classifier().classify("Bash", Some("cargo test")),
            OperationClass::Unknown
        );
    }

    #[test]
    fn test_bash_pwd_is_readonly() {
        assert_eq!(
            classifier().classify("Bash", Some("pwd")),
            OperationClass::ReadOnly
        );
    }

    #[test]
    fn test_bash_echo_is_readonly() {
        assert_eq!(
            classifier().classify("Bash", Some("echo hello")),
            OperationClass::ReadOnly
        );
    }

    #[test]
    fn test_bash_rm_is_destructive() {
        assert_eq!(
            classifier().classify("Bash", Some("rm -rf /tmp/test")),
            OperationClass::Destructive
        );
    }

    #[test]
    fn test_bash_git_push_is_destructive() {
        assert_eq!(
            classifier().classify("Bash", Some("git push origin main")),
            OperationClass::Destructive
        );
    }

    #[test]
    fn test_bash_git_reset_is_destructive() {
        assert_eq!(
            classifier().classify("Bash", Some("git reset --hard HEAD~1")),
            OperationClass::Destructive
        );
    }

    #[test]
    fn test_bash_docker_rm_is_destructive() {
        assert_eq!(
            classifier().classify("Bash", Some("docker rm container_id")),
            OperationClass::Destructive
        );
    }

    #[test]
    fn test_bash_redirect_is_destructive() {
        assert_eq!(
            classifier().classify("Bash", Some("echo data > /tmp/file")),
            OperationClass::Destructive
        );
    }

    #[test]
    fn test_bash_chained_destructive() {
        assert_eq!(
            classifier().classify("Bash", Some("cd src && rm test.txt")),
            OperationClass::Destructive
        );
    }

    #[test]
    fn test_bash_echo_rm_is_not_destructive() {
        assert_eq!(
            classifier().classify("Bash", Some("echo 'rm file'")),
            OperationClass::ReadOnly
        );
    }

    #[test]
    fn test_bash_grep_sudo_is_not_destructive() {
        assert_eq!(
            classifier().classify("Bash", Some("grep 'sudo apt' log.txt")),
            OperationClass::ReadOnly
        );
    }

    #[test]
    fn test_bash_pipe_no_space_is_destructive() {
        assert_eq!(
            classifier().classify("Bash", Some("cat file |rm -rf /")),
            OperationClass::Destructive
        );
    }

    #[test]
    fn test_bash_cargo_build_is_unknown() {
        assert_eq!(
            classifier().classify("Bash", Some("cargo build")),
            OperationClass::Unknown
        );
    }

    #[test]
    fn test_bash_unknown_command() {
        assert_eq!(
            classifier().classify("Bash", Some("some-custom-tool arg1")),
            OperationClass::Unknown
        );
    }

    #[test]
    fn test_bash_empty_command() {
        assert_eq!(
            classifier().classify("Bash", Some("")),
            OperationClass::Unknown
        );
    }

    #[test]
    fn test_bash_sudo_is_destructive() {
        assert_eq!(
            classifier().classify("Bash", Some("sudo apt install foo")),
            OperationClass::Destructive
        );
    }

    #[test]
    fn test_bash_chmod_is_destructive() {
        assert_eq!(
            classifier().classify("Bash", Some("chmod 777 /etc/passwd")),
            OperationClass::Destructive
        );
    }

    #[test]
    fn test_bash_chown_is_destructive() {
        assert_eq!(
            classifier().classify("Bash", Some("chown root:root /etc/shadow")),
            OperationClass::Destructive
        );
    }

    // ── Case insensitivity ───────────────────────────────────────────────

    #[test]
    fn test_case_insensitive_tool_name() {
        assert_eq!(
            classifier().classify("READ", None),
            OperationClass::ReadOnly
        );
        assert_eq!(classifier().classify("WRITE", None), OperationClass::Write);
        assert_eq!(
            classifier().classify("Bash", Some("ls")),
            OperationClass::ReadOnly
        );
    }

    #[test]
    fn test_whitespace_trimmed() {
        assert_eq!(
            classifier().classify("  Read  ", None),
            OperationClass::ReadOnly
        );
    }

    // ── Auto-approve ─────────────────────────────────────────────────────

    #[test]
    fn test_can_auto_approve_readonly() {
        assert!(classifier().can_auto_approve("Read", None));
        assert!(classifier().can_auto_approve("Bash", Some("ls -la")));
    }

    #[test]
    fn test_cannot_auto_approve_write() {
        assert!(!classifier().can_auto_approve("Write", None));
        assert!(!classifier().can_auto_approve("Bash", Some("rm file.txt")));
    }

    #[test]
    fn test_cannot_auto_approve_unknown() {
        assert!(!classifier().can_auto_approve("unknown_tool", None));
    }

    // ── Special tools ────────────────────────────────────────────────────

    #[test]
    fn test_database_transaction_is_destructive() {
        assert_eq!(
            classifier().classify("DatabaseTransaction", None),
            OperationClass::Destructive
        );
    }

    #[test]
    fn test_task_tool_is_write() {
        assert_eq!(classifier().classify("task", None), OperationClass::Write);
    }

    // ── GitHub CLI tests ─────────────────────────────────────────────────

    #[test]
    fn test_gh_pr_view_is_readonly() {
        assert_eq!(
            classifier().classify("Bash", Some("gh pr view 123")),
            OperationClass::ReadOnly
        );
    }

    #[test]
    fn test_gh_pr_list_is_readonly() {
        assert_eq!(
            classifier().classify("Bash", Some("gh pr list")),
            OperationClass::ReadOnly
        );
    }

    #[test]
    fn test_gh_issue_view_is_readonly() {
        assert_eq!(
            classifier().classify("Bash", Some("gh issue view 456")),
            OperationClass::ReadOnly
        );
    }

    #[test]
    fn test_gh_api_is_readonly() {
        assert_eq!(
            classifier().classify("Bash", Some("gh api repos/owner/repo")),
            OperationClass::ReadOnly
        );
    }

    #[test]
    fn test_gh_pr_create_is_destructive() {
        assert_eq!(
            classifier().classify("Bash", Some("gh pr create --title test")),
            OperationClass::Destructive
        );
    }

    #[test]
    fn test_gh_pr_merge_is_destructive() {
        assert_eq!(
            classifier().classify("Bash", Some("gh pr merge 123")),
            OperationClass::Destructive
        );
    }

    #[test]
    fn test_gh_issue_create_is_destructive() {
        assert_eq!(
            classifier().classify("Bash", Some("gh issue create --title bug")),
            OperationClass::Destructive
        );
    }

    #[test]
    fn test_gh_pr_edit_is_destructive() {
        assert_eq!(
            classifier().classify("Bash", Some("gh pr edit 123 --title new")),
            OperationClass::Destructive
        );
    }

    // ── Other read-only tools ────────────────────────────────────────────

    #[test]
    fn test_jq_is_readonly() {
        assert_eq!(
            classifier().classify("Bash", Some("jq '.name' package.json")),
            OperationClass::ReadOnly
        );
    }

    #[test]
    fn test_curl_is_readonly() {
        assert_eq!(
            classifier().classify("Bash", Some("curl https://api.example.com/data")),
            OperationClass::ReadOnly
        );
    }

    #[test]
    fn test_curl_output_is_destructive() {
        assert_eq!(
            classifier().classify(
                "Bash",
                Some("curl -o file.zip https://example.com/file.zip")
            ),
            OperationClass::Destructive
        );
    }

    #[test]
    fn test_wget_is_destructive() {
        assert_eq!(
            classifier().classify("Bash", Some("wget https://example.com/file.tar.gz")),
            OperationClass::Destructive
        );
    }

    #[test]
    fn test_http_get_is_readonly() {
        assert_eq!(
            classifier().classify("HttpGet", None),
            OperationClass::ReadOnly
        );
    }

    #[test]
    fn test_http_post_is_write() {
        assert_eq!(
            classifier().classify("HttpPost", None),
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
            ("Read", None),
            ("Write", None),
            ("Bash", Some("rm -rf /")),
            ("Grep", None),
            ("Bash", Some("ls")),
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

    // ── Expanded classification tests ─────────────────────────────────────

    #[test]
    fn test_env_is_readonly() {
        assert_eq!(
            classifier().classify("Bash", Some("env")),
            OperationClass::ReadOnly
        );
    }

    #[test]
    fn test_npm_view_is_readonly() {
        assert_eq!(
            classifier().classify("Bash", Some("npm view express")),
            OperationClass::ReadOnly
        );
    }

    #[test]
    fn test_pip_show_is_readonly() {
        assert_eq!(
            classifier().classify("Bash", Some("pip show requests")),
            OperationClass::ReadOnly
        );
    }

    #[test]
    fn test_gh_pr_checkout_is_readonly() {
        assert_eq!(
            classifier().classify("Bash", Some("gh pr checkout 42")),
            OperationClass::ReadOnly
        );
    }

    #[test]
    fn test_npm_publish_is_destructive() {
        assert_eq!(
            classifier().classify("Bash", Some("npm publish")),
            OperationClass::Destructive
        );
    }

    #[test]
    fn test_cargo_publish_is_destructive() {
        assert_eq!(
            classifier().classify("Bash", Some("cargo publish")),
            OperationClass::Destructive
        );
    }
}
