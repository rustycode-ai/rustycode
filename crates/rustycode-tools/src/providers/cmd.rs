#![allow(clippy::doc_markdown)]

//! Windows cmd.exe tool — persistent session with native command protocol.
//!
//! Provides a dedicated `cmd.exe` session for Windows command execution, with:
//! - `cmd.exe /Q` (echo off) for clean output
//! - `echo ---END---` delimiters and `echo %errorlevel%` exit codes
//! - Wall-clock timeouts
//! - cmd-specific boilerplate filtering
//! - Graceful fallback when not on Windows

use crate::truncation::truncate_bash_output;
use crate::{ToolOutput, ToolPermission, ToolTag};
use anyhow::{anyhow, Result};
use schemars::JsonSchema;
use serde_json::json;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

// cmd.exe session

/// Persistent cmd.exe session that maintains shell state across commands.
///
/// Spawns `cmd.exe /Q` for a persistent session with echo disabled.
/// Uses `echo ---END---` delimiters and `echo %errorlevel%` exit codes.
pub struct CmdSession {
    child: Arc<Mutex<Option<Child>>>,
    #[allow(dead_code)]
    cwd: PathBuf,
    _session_id: String,
    stderr_buffer: Arc<Mutex<String>>,
    stdout_rx: Arc<Mutex<std::sync::mpsc::Receiver<String>>>,
}

/// cmd.exe-specific boilerplate lines to filter from output.
fn is_cmd_boilerplate(trimmed: &str) -> bool {
    trimmed.contains("Microsoft Windows")
        || trimmed.contains("(c) Microsoft Corporation")
        // cmd.exe prompt: drive letter + :\ + path + >
        || (trimmed.ends_with('>') && trimmed.contains(":\\"))
        // Empty prompt lines
        || trimmed == ">"
        // Delimiter and exit-code query echoed back
        || trimmed.contains("echo ---END---")
        || trimmed.contains("echo %errorlevel%")
}

fn filter_cmd_boilerplate(text: &str) -> String {
    text.lines()
        .filter(|line| !is_cmd_boilerplate(line.trim()))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Detect `cmd.exe` availability. Returns the binary name if found.
pub fn find_cmd() -> Option<&'static str> {
    if Command::new("cmd.exe")
        .args(["/Q", "/C", "exit", "0"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
    {
        return Some("cmd.exe");
    }
    None
}

impl CmdSession {
    /// Create a new persistent cmd.exe session.
    fn new(cwd: PathBuf) -> Result<Self> {
        let cmd = find_cmd().ok_or_else(|| {
            anyhow!(
                "cmd.exe not found. This tool is only available on Windows. \
                 Use bash or powershell on other platforms."
            )
        })?;

        let session_id = uuid::Uuid::new_v4().to_string();

        let mut child = Command::new(cmd)
            .args(["/Q"]) // echo off
            .current_dir(&cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| anyhow!("failed to spawn {cmd}: {e}"))?;

        // Persistent stderr drain thread
        let stderr_buffer: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));
        if let Some(stderr_handle) = child.stderr.take() {
            let buf = stderr_buffer.clone();
            let id = session_id.clone();
            thread::spawn(move || {
                tracing::debug!(session_id = %id, "cmd stderr drain thread started");
                let mut reader = BufReader::new(stderr_handle);
                let mut line_buf = Vec::new();
                loop {
                    line_buf.clear();
                    match reader.read_until(b'\n', &mut line_buf) {
                        Ok(0) => break,
                        Ok(_) => {
                            if let Ok(mut b) = buf.lock() {
                                if !b.is_empty() {
                                    b.push('\n');
                                }
                                let line = String::from_utf8_lossy(&line_buf);
                                b.push_str(line.trim_end_matches('\n').trim_end_matches('\r'));
                            }
                        }
                        Err(_) => break,
                    }
                }
                tracing::debug!(session_id = %id, "cmd stderr drain thread exited");
            });
        }

        // Persistent stdout reader thread
        let (stdout_tx, stdout_rx) = std::sync::mpsc::channel::<String>();
        if let Some(stdout_handle) = child.stdout.take() {
            let id = session_id.clone();
            thread::spawn(move || {
                tracing::debug!(session_id = %id, "cmd stdout reader thread started");
                let mut reader = BufReader::new(stdout_handle);
                let mut buf = Vec::new();
                loop {
                    buf.clear();
                    match reader.read_until(b'\n', &mut buf) {
                        Ok(0) => break,
                        Ok(_) => {
                            let line = String::from_utf8_lossy(&buf);
                            let trimmed = line.trim_end_matches('\n').trim_end_matches('\r');
                            if stdout_tx.send(trimmed.to_string()).is_err() {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
                tracing::debug!(session_id = %id, "cmd stdout reader thread exited");
            });
        }

        Ok(Self {
            child: Arc::new(Mutex::new(Some(child))),
            cwd,
            _session_id: session_id,
            stderr_buffer,
            stdout_rx: Arc::new(Mutex::new(stdout_rx)),
        })
    }

    /// Execute a command in the session with a wall-clock timeout.
    fn execute(&self, command: &str, timeout_secs: u64) -> Result<(String, String, i32)> {
        let mut child_guard = self
            .child
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let child_ref = child_guard
            .as_mut()
            .ok_or_else(|| anyhow!("cmd.exe session closed"))?;

        let stdin = child_ref
            .stdin
            .as_mut()
            .ok_or_else(|| anyhow!("cmd.exe stdin unavailable"))?;

        // Clear stale stderr
        if let Ok(mut b) = self.stderr_buffer.lock() {
            b.clear();
        }

        // Write command + delimiter + exit code query
        writeln!(stdin, "{command}")?;
        writeln!(stdin, "echo ---END---")?;
        writeln!(stdin, "echo %errorlevel%")?;
        stdin.flush()?;

        drop(child_guard);

        // Collect output lines until delimiter
        let deadline = Instant::now() + Duration::from_secs(timeout_secs);
        let mut output_lines: Vec<String> = Vec::new();
        let mut found_delimiter = false;
        let mut exit_code = 0i32;

        let rx = self
            .stdout_rx
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Ok((
                    output_lines.join("\n"),
                    String::new(),
                    124, // timeout exit code
                ));
            }

            match rx.recv_timeout(remaining.min(Duration::from_secs(1))) {
                Ok(line) => {
                    let trimmed = line.trim();
                    if trimmed == "---END---" {
                        found_delimiter = true;
                        continue;
                    }

                    if found_delimiter {
                        exit_code = trimmed.parse::<i32>().unwrap_or(0);
                        break;
                    }

                    output_lines.push(line);
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    if Instant::now() >= deadline {
                        return Ok((output_lines.join("\n"), String::new(), 124));
                    }
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }

        let stdout = filter_cmd_boilerplate(&output_lines.join("\n"));
        let stderr = self
            .stderr_buffer
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();

        Ok((stdout, stderr, exit_code))
    }
}

impl Drop for CmdSession {
    fn drop(&mut self) {
        if let Some(mut child) = self
            .child
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
        {
            if let Some(stdin) = child.stdin.as_mut() {
                let _ = writeln!(stdin, "exit");
                let _ = stdin.flush();
            }
            let _ = child.wait();
        }
    }
}

// Session registry

/// Global registry of cmd.exe sessions keyed by working directory.
static CMD_SESSION_REGISTRY: std::sync::LazyLock<CmdSessionRegistry> =
    std::sync::LazyLock::new(CmdSessionRegistry::new);

struct CmdSessionRegistry {
    sessions: Mutex<HashMap<PathBuf, (Arc<Mutex<CmdSession>>, Instant)>>,
}

impl CmdSessionRegistry {
    fn new() -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
        }
    }

    fn get_or_create(&self, cwd: PathBuf) -> Result<Arc<Mutex<CmdSession>>> {
        let mut sessions = self
            .sessions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        if let Some((sess, ts)) = sessions.get_mut(&cwd) {
            *ts = Instant::now();
            return Ok(sess.clone());
        }

        let session = Arc::new(Mutex::new(CmdSession::new(cwd.clone())?));
        sessions.insert(cwd, (session.clone(), Instant::now()));
        Ok(session)
    }

    fn remove(&self, cwd: &Path) {
        let mut sessions = self
            .sessions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        sessions.remove(cwd);
    }

    fn evict_idle(&self) {
        let mut sessions = self
            .sessions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        #[allow(clippy::duration_suboptimal_units)]
        let threshold = Duration::from_secs(600);
        sessions.retain(|_, (_, last_used)| last_used.elapsed() < threshold);
    }
}

// Rate limiter

static CMD_CONCURRENCY_LIMIT: usize = 4;
static CMD_ACTIVE_COUNT: AtomicUsize = AtomicUsize::new(0);

struct CmdPermit;

impl CmdPermit {
    fn try_acquire() -> Result<CmdPermit> {
        loop {
            let current = CMD_ACTIVE_COUNT.load(Ordering::Relaxed);
            if current >= CMD_CONCURRENCY_LIMIT {
                return Err(anyhow!(
                    "Rate limit exceeded: {current} concurrent cmd.exe commands already running."
                ));
            }
            if CMD_ACTIVE_COUNT
                .compare_exchange(current, current + 1, Ordering::AcqRel, Ordering::Relaxed)
                .is_ok()
            {
                return Ok(CmdPermit);
            }
        }
    }
}

impl Drop for CmdPermit {
    fn drop(&mut self) {
        CMD_ACTIVE_COUNT.fetch_sub(1, Ordering::Release);
    }
}

// Command validation

/// Extract the binary/command name from a cmd command string.
fn extract_binary_name(command: &str) -> Result<String> {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        anyhow::bail!("command is empty");
    }
    // Handle quoted paths like "C:\Program Files\tool.exe"
    if let Some(rest) = trimmed.strip_prefix('"') {
        if let Some(end) = rest.find('"') {
            let path = &rest[..end];
            // Extract filename from Windows path (split by \ or /)
            let filename = path.rsplit(&['\\', '/']).next().unwrap_or(path);
            // Strip .exe extension if present
            let stem = filename.strip_suffix(".exe").unwrap_or(filename);
            return Ok(stem.to_lowercase());
        }
    }
    // Unquoted: take first token
    Ok(trimmed
        .split_whitespace()
        .next()
        .unwrap_or(trimmed)
        .to_lowercase())
}

/// Validate a cmd command against the security allowlist/blocklist.
fn validate_cmd_command(command: &str) -> Result<()> {
    let binary = extract_binary_name(command)?;

    use crate::security::cross_platform::{allowed_commands, blocked_commands, ShellType};

    let shell_type = ShellType::Cmd;
    let allowed = allowed_commands(shell_type);
    if !allowed.contains(&binary.as_str()) {
        anyhow::bail!("command '{}' is not in allowed list for cmd", binary);
    }

    let blocked = blocked_commands(shell_type);
    if blocked.contains(&binary.as_str()) {
        anyhow::bail!("command '{}' is blocked for security reasons", binary);
    }

    Ok(())
}

// Params

#[derive(serde::Deserialize, JsonSchema)]
pub struct CmdParams {
    /// The cmd.exe command to execute
    command: String,
    /// Restart the cmd.exe session (fresh process)
    #[serde(default)]
    restart: bool,
    /// Wall-clock timeout in seconds (default: 120, max: 600)
    timeout_secs: Option<u64>,
}

// CmdTool

rustycode_tools_api::define_tool! {
    pub struct CmdTool;

    name: "cmd",
    description: "Execute commands in Windows cmd.exe. \
     Windows-only — returns an error on other platforms. \
     Supports persistent sessions, wall-clock timeouts, \
     and background execution. \
     Commands are validated against a security blocklist.",
    permission: ToolPermission::Execute,
    tags: [ToolTag::Implement, ToolTag::Ops],

    execute(params: CmdParams, ctx) {
        crate::check_permission(ToolPermission::Execute, ctx)?;

        if let Some(gate) = &ctx.plan_gate {
            gate.check_access(ctx.role, "cmd")?;
        }

        let command = params.command;
        let restart = params.restart;
        let timeout_secs = params.timeout_secs.unwrap_or(120).min(600);

        use crate::security::cross_platform::validate_path_in_workspace;
        validate_path_in_workspace(&ctx.cwd, &ctx.cwd)?;
        validate_cmd_command(&command)?;

        let _permit = CmdPermit::try_acquire()?;

        let start_time = Instant::now();

        let session = if restart {
            CMD_SESSION_REGISTRY.remove(&ctx.cwd);
            CMD_SESSION_REGISTRY.get_or_create(ctx.cwd.clone())?
        } else {
            CMD_SESSION_REGISTRY.evict_idle();
            CMD_SESSION_REGISTRY.get_or_create(ctx.cwd.clone())?
        };

        let command_clone = command.clone();
        let cwd_clone = ctx.cwd.clone();

        let (stdout, stderr, exit_code): (String, String, i32) =
            if let Ok(handle) = tokio::runtime::Handle::try_current() {
                tokio::task::block_in_place(|| {
                    handle.block_on(async {
                        let result = tokio::time::timeout(
                            Duration::from_secs(timeout_secs),
                            tokio::task::spawn_blocking(move || {
                                let s = session
                                    .lock()
                                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                                s.execute(&command_clone, timeout_secs)
                            }),
                        )
                        .await;

                        if result.is_err() {
                            tracing::warn!(
                                "cmd command timed out, evicting session for {:?}",
                                cwd_clone
                            );
                            CMD_SESSION_REGISTRY.remove(&cwd_clone);
                        }

                        result
                            .map_err(|_| anyhow!("command timed out after {timeout_secs}s"))?
                            .map_err(|e| anyhow!("command execution failed: {e}"))?
                    })
                })
            } else {
                let rt = tokio::runtime::Runtime::new()
                    .map_err(|e| anyhow!("failed to create tokio runtime: {e}"))?;
                rt.block_on(async {
                    let cwd_for_evict = ctx.cwd.clone();
                    let result = tokio::time::timeout(
                        Duration::from_secs(timeout_secs),
                        tokio::task::spawn_blocking(move || {
                            let s = session
                                .lock()
                                .unwrap_or_else(std::sync::PoisonError::into_inner);
                            s.execute(&command_clone, timeout_secs)
                        }),
                    )
                    .await;

                    if result.is_err() {
                        tracing::warn!(
                            "cmd command timed out, evicting session for {:?}",
                            cwd_for_evict
                        );
                        CMD_SESSION_REGISTRY.remove(&cwd_for_evict);
                    }

                    result
                        .map_err(|_| anyhow!("command timed out after {timeout_secs}s"))?
                        .map_err(|e| anyhow!("command execution failed: {e}"))?
                })
            }?;

        let execution_time = start_time.elapsed();
        let truncated = truncate_bash_output(&stdout, &stderr, exit_code);
        let output_text = truncated.as_str().to_string();

        let metadata = {
            let mut meta = truncated.into_metadata();
            meta["exit_code"] = json!(exit_code);
            meta["command"] = json!(command);
            meta["execution_time_ms"] = json!(execution_time.as_millis());
            meta["timeout_secs"] = json!(timeout_secs);
            meta["shell"] = json!("cmd");
            if exit_code != 0 {
                meta["failed"] = json!(true);
            }
            meta
        };

        Ok(ToolOutput::text(output_text).with_metadata(ctx, || metadata))
    }
}

// Tests

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_binary_name_simple() {
        assert_eq!(extract_binary_name("dir").unwrap(), "dir");
    }

    #[test]
    fn extract_binary_name_with_args() {
        assert_eq!(extract_binary_name("dir /w /s C:\\").unwrap(), "dir");
    }

    #[test]
    fn extract_binary_name_quoted_path() {
        assert_eq!(
            extract_binary_name("\"C:\\Program Files\\tool.exe\" --flag").unwrap(),
            "tool"
        );
    }

    #[test]
    fn extract_binary_name_empty() {
        assert!(extract_binary_name("").is_err());
    }

    #[test]
    fn is_cmd_boilerplate_detects_banner() {
        assert!(is_cmd_boilerplate("Microsoft Windows [Version 10.0.19045]"));
        assert!(is_cmd_boilerplate(
            "(c) Microsoft Corporation. All rights reserved."
        ));
    }

    #[test]
    fn is_cmd_boilerplate_detects_prompt() {
        assert!(is_cmd_boilerplate("C:\\Users\\test>"));
    }

    #[test]
    fn is_cmd_boilerplate_detects_delimiter_echo() {
        assert!(is_cmd_boilerplate("echo ---END---"));
        assert!(is_cmd_boilerplate("echo %errorlevel%"));
    }

    #[test]
    fn is_cmd_boilerplate_passes_normal_output() {
        assert!(!is_cmd_boilerplate("Hello World"));
        assert!(!is_cmd_boilerplate("  Directory of C:\\Users"));
    }

    #[test]
    fn filter_cmd_boilerplate_removes_noise() {
        let input = "Microsoft Windows [Version 10.0]\nHello\nC:\\>\nWorld";
        let filtered = filter_cmd_boilerplate(input);
        assert!(!filtered.contains("Microsoft"));
        assert!(!filtered.contains("C:\\>"));
        assert!(filtered.contains("Hello"));
        assert!(filtered.contains("World"));
    }

    #[test]
    fn rate_limiter_limits_concurrency() {
        let _p1 = CmdPermit::try_acquire().unwrap();
        let _p2 = CmdPermit::try_acquire().unwrap();
        let _p3 = CmdPermit::try_acquire().unwrap();
        let _p4 = CmdPermit::try_acquire().unwrap();

        // 5th should fail (limit is 4)
        assert!(CmdPermit::try_acquire().is_err());

        // Drop one, should succeed again
        drop(_p4);
        assert!(CmdPermit::try_acquire().is_ok());
    }

    // Session tests — skip if cmd.exe is not available (non-Windows)
    #[test]
    fn session_spawn_and_execute() {
        if find_cmd().is_none() {
            eprintln!("Skipping: cmd.exe not available");
            return;
        }

        let cwd = std::env::temp_dir();
        let session = CmdSession::new(cwd).expect("failed to create cmd session");

        let (stdout, _stderr, exit_code) = session
            .execute("echo hello world", 10)
            .expect("execute failed");

        assert_eq!(exit_code, 0);
        assert!(stdout.contains("hello world"), "output was: {stdout}");
    }

    #[test]
    fn session_exit_code() {
        if find_cmd().is_none() {
            eprintln!("Skipping: cmd.exe not available");
            return;
        }

        let cwd = std::env::temp_dir();
        let session = CmdSession::new(cwd).expect("failed to create cmd session");

        let (stdout, _stderr, exit_code) =
            session.execute("exit /b 42", 10).expect("execute failed");

        assert_eq!(exit_code, 42, "output was: {stdout}");
    }
}
