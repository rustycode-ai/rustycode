use anyhow::{anyhow, bail, Result};
use std::fs;
use std::path::{Path, PathBuf};

/// Extract the binary name from a shell command (first token, without path)
pub fn extract_binary_name(command: &str) -> anyhow::Result<String> {
    use shell_words::split;
    let tokens = split(command).map_err(|e| anyhow::anyhow!("invalid command syntax: {e}"))?;
    if tokens.is_empty() {
        return Err(anyhow::anyhow!("empty command"));
    }

    let binary = &tokens[0];
    let binary_name = if binary.contains('/') {
        binary.rsplit('/').next().unwrap_or(binary)
    } else if binary.contains('\\') {
        binary.rsplit('\\').next().unwrap_or(binary)
    } else {
        binary
    };

    Ok(binary_name.to_lowercase())
}

// Module-level constants for command safety validation

/// Shells and interpreters where `-c`/`-e` flags mean "execute arbitrary code".
const SHELLS_AND_INTERPRETERS: &[&str] = &[
    "sh", "bash", "zsh", "fish", "dash", "ksh", "csh", "tcsh", "python", "python3", "perl", "ruby",
    "node", "lua",
];

/// Allowlist of safe commands. Only add commands genuinely needed for development.
const ALLOWED_COMMANDS: &[&str] = &[
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
    "rustfmt",
    "clippy",
    "npm",
    "pnpm",
    "yarn",
    "bun",
    "node",
    "deno",
    "python",
    "python3",
    "pip",
    "pip3",
    "poetry",
    "uv",
    "ruby",
    "gem",
    "bundle",
    "go",
    "gofmt",
    "golint",
    "javac",
    "java",
    "gradle",
    "maven",
    "mvn",
    "gcc",
    "clang",
    "cc",
    "c++",
    "g++",
    "make",
    "cmake",
    "meson",
    "ninja",
    "zig",
    "cargo-zigbuild",
    // Version control
    "git",
    "hg",
    "svn",
    // Text processing
    "sed",
    "awk",
    "tr",
    "cut",
    "paste",
    "join",
    "diff",
    "patch",
    "jq",
    "yq",
    "tomlq",
    "xonq",
    // Binary analysis and inspection
    "xxd",
    "hexdump",
    "od",
    "readelf",
    "objdump",
    "nm",
    "strings",
    "size",
    "file",
    "strip",
    // Network utilities (read-only)
    "curl",
    "wget",
    "httpie",
    "http",
    // Process utilities (read-only)
    "ps",
    "top",
    "htop",
    "btop",
    "pgrep",
    "pkill",
    // System utilities (read-only or safe)
    "pwd",
    "date",
    "uptime",
    "whoami",
    "id",
    "env",
    "printenv",
    "echo",
    "which",
    "type",
    "whereis",
    "what",
    "command",
    // Compression/decompression
    "tar",
    "gzip",
    "gunzip",
    "xz",
    "unxz",
    "zip",
    "unzip",
    "zstd",
    // Testing
    "pytest",
    "jest",
    "vitest",
    "mocha",
    "jasmine",
    "karma",
    "go-test",
    "cargo-test",
    // Documentation
    "man",
    "help",
    "tldr",
    "pydoc",
    // Docker/Podman (container operations)
    "docker",
    "podman",
    "docker-compose",
    // Database clients
    "psql",
    "mysql",
    "mongosh",
    "redis-cli",
    "sqlite3",
    // Cloud CLIs
    "aws",
    "az",
    "gcloud",
    // Package managers
    "apt",
    "apt-get",
    "yum",
    "dnf",
    "pacman",
    "brew",
    "choco",
    "scoop",
    // Misc development tools
    "ln",
    "mkdir",
    "touch",
    "cp",
    "mv",
    "rm",
    "chmod", // Basic file ops
    // REMOVED: "sh", "bash", "zsh", "fish", "dash" - SECURITY: Shells bypass allowlist
    "rsync",
    "scp", // Sync/copy
    "ssh", // Remote shell
    "cd",  // Change directory (shell builtin, but common in scripts)
];

/// Platform-specific commands allowed in addition to [`ALLOWED_COMMANDS`].
#[cfg(unix)]
const PLATFORM_COMMANDS: &[&str] = &[
    "sed", "awk", "grep", "find", "curl", "wget", "xargs", "tee", "nohup", "screen", "tmux",
    "strace", "lsof", "ss", "nc", "socat",
];

#[cfg(windows)]
const PLATFORM_COMMANDS: &[&str] = &[
    // PowerShell cmdlets (common aliases)
    "Get-Content",
    "Get-ChildItem",
    "Select-String",
    "Get-Process",
    "Set-Location",
    "Copy-Item",
    "Remove-Item",
    "Move-Item",
    "New-Item",
    "Test-Path",
    "Get-Location",
    "Write-Output",
    "Get-Date",
    "Get-Host",
    "Invoke-WebRequest",
    "Invoke-RestMethod",
    // Windows utilities
    "dir",
    "type",
    "findstr",
    "where",
    "cmdkey",
    "netstat",
    "tasklist",
    "systeminfo",
    "winget",
    "scoop",
    "choco",
];

#[cfg(not(any(unix, windows)))]
const PLATFORM_COMMANDS: &[&str] = &[];

/// Shell targets blocked from pipe destinations to prevent allowlist bypass.
const BLOCKED_PIPE_TARGETS: &[&str] = &["sh", "bash", "zsh", "fish", "dash", "ksh", "csh", "tcsh"];

// Helper functions for validate_command_safety

/// Checks for excessive quote nesting that may indicate obfuscation.
fn check_quote_nesting(command: &str) -> Result<()> {
    let mut max_depth = 0i32;
    let mut in_heredoc = false;
    for line in command.lines() {
        if line.contains("<<") && (line.contains('\'') || line.contains('"')) {
            in_heredoc = true;
            continue;
        }
        if in_heredoc {
            continue;
        }
        let mut depth = 0i32;
        let mut prev_quote = ' ';
        for ch in line.chars() {
            if ch == '\'' || ch == '"' {
                if ch == prev_quote {
                    depth -= 1;
                    prev_quote = ' ';
                } else {
                    depth += 1;
                    prev_quote = ch;
                }
                max_depth = max_depth.max(depth);
            }
        }
    }
    if max_depth > 30 {
        bail!("command has excessive quote nesting (potential obfuscation attempt)");
    }
    let raw_quote_count = command.chars().filter(|&c| c == '\'' || c == '"').count();
    if raw_quote_count > 2000 {
        bail!("command has excessive quote nesting (potential obfuscation attempt)");
    }
    Ok(())
}

/// Checks for dangerous input encoding: null bytes, carriage returns, IFS
/// variable usage, and Unicode whitespace.
fn check_input_encoding(command: &str) -> Result<(bool, bool)> {
    let has_null = command.contains('\0');
    let has_cr = command.contains('\r');
    let has_newline = command.contains('\n');
    let is_heredoc = command.contains("<<");
    if has_null {
        bail!("blocked command with null byte (potential injection)");
    }
    if has_cr {
        bail!("blocked command with carriage return (potential injection)");
    }

    if command.contains("$IFS") || command.contains("${IFS") {
        bail!("blocked command with IFS variable usage (potential security bypass)");
    }

    let has_unicode_ws = command.chars().any(|c| {
        matches!(
            c,
            '\u{00A0}' | '\u{1680}' | '\u{2000}'
                ..='\u{200A}' | '\u{2028}' | '\u{2029}' | '\u{202F}' | '\u{205F}' | '\u{3000}'
        )
    });
    if has_unicode_ws {
        bail!("blocked command with Unicode whitespace (potential parsing inconsistency)");
    }

    Ok((has_newline, is_heredoc))
}

/// Checks for newline-based command injection.
fn check_newline_injection(
    command: &str,
    tokens: &[String],
    has_newline: bool,
    is_heredoc: bool,
) -> Result<()> {
    if has_newline && !is_heredoc {
        let raw_lines: Vec<&str> = command.lines().filter(|l| !l.trim().is_empty()).collect();

        if raw_lines.len() > 5 && tokens.len() > 3 {
            let has_chain_op = raw_lines.iter().any(|line| {
                let trimmed = line.trim();
                trimmed.starts_with("&&")
                    || trimmed.starts_with("||")
                    || trimmed.starts_with(";")
                    || trimmed.ends_with("&&")
                    || trimmed.ends_with("||")
            });
            if has_chain_op {
                bail!(
                    "blocked command with excessive newlines and shell chaining (potential injection)"
                );
            }
        }
    }
    Ok(())
}

/// Checks for `-c`/`--command` and `-e`/`--eval` flags on shells and interpreters.
fn check_interpreter_flags(tokens: &[String], binary_name: &str) -> Result<()> {
    let is_shell_or_interp = SHELLS_AND_INTERPRETERS.contains(&binary_name);

    if tokens.len() >= 2 && is_shell_or_interp {
        for (i, token) in tokens.iter().enumerate() {
            if (token == "-c" || token == "--command") && i + 1 < tokens.len() {
                bail!("blocked command with -c/--command flag (potential allowlist bypass)");
            }

            if token == "-e" || token == "--eval" || token == "-E" {
                bail!("blocked interpreter with -e flag (potential allowlist bypass)");
            }
        }
    }
    Ok(())
}

/// Checks for pipe-to-shell bypasses (e.g., `cat file | sh`).
fn check_pipe_to_shell(command: &str) -> Result<()> {
    let cmd_trimmed = command.trim();
    for target in BLOCKED_PIPE_TARGETS {
        if cmd_trimmed.contains(&format!("| {target}"))
            || cmd_trimmed.contains(&format!("|{target}"))
        {
            let after_pipe = cmd_trimmed
                .split('|')
                .next_back()
                .unwrap_or("")
                .split_whitespace()
                .next()
                .unwrap_or("");
            if after_pipe == *target {
                bail!("blocked pipe to shell '{target}' (potential allowlist bypass)");
            }
        }
    }
    Ok(())
}

/// Checks for dangerous patterns: fork bombs, shell expansion, parameter
/// expansion, arithmetic expansion, root-filesystem delete, and dangerous
/// `find` flags.
fn check_dangerous_patterns(command: &str, binary_name: &str, tokens: &[String]) -> Result<()> {
    if binary_name == "find" {
        let dangerous_find_flags = ["-delete", "-exec", "-ok", "-execdir"];
        for token in tokens {
            let token_lower: String = token.to_lowercase();
            for flag in dangerous_find_flags {
                if token_lower.starts_with(flag) || token_lower == flag {
                    bail!("blocked find command with dangerous flag `{flag}`");
                }
            }
        }
    }

    let cmd_lower = command.to_lowercase();

    if cmd_lower.contains(":(){") || cmd_lower.contains(":() {") {
        bail!("blocked shell function definition (potential fork bomb)");
    }
    if cmd_lower.contains(":|:") || cmd_lower.contains(": | : ") {
        bail!("blocked shell function with self-execution (potential fork bomb)");
    }
    let ampersand_count = cmd_lower.matches('&').count();
    if ampersand_count > 50 {
        bail!("blocked command with excessive background operators (potential fork bomb)");
    }
    if cmd_lower.contains("eval") && (cmd_lower.contains("()") || cmd_lower.contains('{')) {
        bail!("blocked eval with function definition (potential fork bomb)");
    }

    let mut in_body = false;
    for line in command.lines() {
        if line.contains("<<") && (line.contains('\'') || line.contains('"')) {
            in_body = true;
        }
        if in_body {
            continue;
        }
        if line.contains("$(") || line.contains('`') {
            bail!("blocked command with shell expansion (potential obfuscation attempt)");
        }
    }

    if cmd_lower.contains("${!") || cmd_lower.contains("${@:") {
        bail!("blocked command with dangerous parameter expansion");
    }
    if cmd_lower.contains("$((") {
        bail!("blocked command with arithmetic expansion");
    }
    if cmd_lower.contains("-rf /") || cmd_lower.contains("-rf /*") || cmd_lower.contains("-fr /") {
        bail!("blocked recursive delete targeting root filesystem");
    }

    Ok(())
}

/// Validates that a command is safe to execute.
pub fn validate_command_safety(command: &str) -> Result<()> {
    if std::env::var("RUSTYCODE_SANDBOX").as_deref() == Ok("container") {
        tracing::warn!("sandbox mode: skipping command safety validation");
        return Ok(());
    }

    const MAX_COMMAND_LENGTH: usize = 10_000;
    if command.len() > MAX_COMMAND_LENGTH {
        bail!("command exceeds maximum length of {MAX_COMMAND_LENGTH} characters");
    }

    check_quote_nesting(command)?;

    let (has_newline, is_heredoc) = check_input_encoding(command)?;

    let tokens = shell_words::split(command).map_err(|e| {
        anyhow::anyhow!(
            "blocked command with invalid shell syntax: {e} (potential obfuscation attempt)"
        )
    })?;

    check_newline_injection(command, &tokens, has_newline, is_heredoc)?;

    if tokens.is_empty() {
        bail!("blocked empty command");
    }

    let binary = &tokens[0];
    let binary_name = if binary.contains('/') {
        binary.rsplit('/').next().unwrap_or(binary)
    } else {
        binary
    };

    check_interpreter_flags(&tokens, binary_name)?;
    check_pipe_to_shell(command)?;

    if !ALLOWED_COMMANDS.contains(&binary_name)
        && !PLATFORM_COMMANDS.contains(&binary_name)
        && !PLATFORM_COMMANDS
            .iter()
            .any(|cmd| cmd.eq_ignore_ascii_case(binary_name))
    {
        bail!(
            "blocked command '{}' not in allowed list. Allowed commands: {}",
            binary_name,
            ALLOWED_COMMANDS.join(", ")
        );
    }

    check_dangerous_patterns(command, binary_name, &tokens)?;

    Ok(())
}

/// Ensure a path is within the workspace directory.
pub fn ensure_path_within_workspace(ctx: &crate::ToolContext, path: &Path) -> Result<()> {
    let workspace_root = fs::canonicalize(&ctx.cwd).unwrap_or_else(|_| ctx.cwd.clone());

    let canonical_path = canonicalize_existing_or_parent(path)?;
    anyhow::ensure!(
        canonical_path.starts_with(&workspace_root),
        "working directory '{}' is outside workspace '{}' and is blocked",
        path.display(),
        workspace_root.display()
    );
    Ok(())
}

/// Canonicalize a path, falling back to parent directories.
pub fn canonicalize_existing_or_parent(path: &Path) -> Result<PathBuf> {
    let mut current = path.to_path_buf();
    loop {
        if current.exists() {
            return fs::canonicalize(&current)
                .map_err(|e| anyhow!("failed to canonicalize '{}': {}", current.display(), e));
        }
        if !current.pop() {
            return Err(anyhow!(
                "unable to resolve path anchor for '{}'",
                path.display()
            ));
        }
    }
}
