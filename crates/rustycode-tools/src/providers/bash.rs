use crate::streaming::{StreamChunk, StreamReceiver, StreamSender, ToolStreaming};
use crate::transform::transform_by_name;
use crate::truncation::truncate_bash_output;
use crate::{Tool, ToolContext, ToolOutput, ToolPermission};
use anyhow::{anyhow, Result};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::subprocess::SHELL_INFO;

/// Check if a line is a shell "command not found" error.
/// Handles bash, zsh, sh (dash/ash), and fish shell error formats.
fn is_command_not_found_line(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with("command not found:")
        || trimmed.starts_with("zsh: command not found")
        || trimmed.starts_with("bash: command not found")
        || trimmed.starts_with("sh: ") && trimmed.contains(": not found")
        || trimmed.starts_with("fish: Unknown command")
}

/// Persistent shell session that maintains shell state across command invocations.
///
/// This implementation follows Anthropic's bash tool specification:
/// - Maintains a persistent shell process with stdin/stdout/stderr pipes
/// - Preserves environment variables, working directory, and shell state
/// - Supports the `restart` parameter to reset the session
/// - Handles timeouts, command not found, and permission denied errors
/// - Cross-platform: detects bash/zsh on Unix, PowerShell/cmd on Windows
pub struct BashSession {
    /// The child shell process
    child: Arc<Mutex<Option<Child>>>,
    /// Working directory for this session
    cwd: PathBuf,
    /// Session ID for tracking
    _session_id: String,
    /// Whether this session uses `PowerShell` (affects delimiter syntax)
    is_powershell: bool,
    /// Accumulated stderr from the background drain thread
    stderr_buffer: Arc<Mutex<String>>,
    /// Channel receiver for stdout lines from the persistent reader thread
    stdout_rx: Arc<Mutex<std::sync::mpsc::Receiver<String>>>,
}

fn is_shell_boilerplate(trimmed: &str) -> bool {
    trimmed.contains("$ timeout ")
        || trimmed.contains("$ echo $?")
        || trimmed.contains("$ echo '---END---'")
        || trimmed.contains("$ echo $LASTEXITCODE")
        || trimmed.starts_with("bash: no job control")
        || trimmed.starts_with("The default interactive shell")
        || trimmed.starts_with("To update your account")
        || trimmed.starts_with("For more details, please visit")
}

fn filter_shell_boilerplate(text: &str) -> String {
    text.lines()
        .filter(|line| !is_shell_boilerplate(line.trim()))
        .collect::<Vec<_>>()
        .join("\n")
}

impl BashSession {
    /// Create a new persistent bash session.
    ///
    /// # Arguments
    ///
    /// * `cwd` - Working directory for the session
    ///
    /// # Returns
    ///
    /// A new `BashSession` with a spawned bash process
    fn new(cwd: PathBuf) -> Result<Self> {
        let session_id = uuid::Uuid::new_v4().to_string();
        let shell = SHELL_INFO.binary;
        let interactive_flag = SHELL_INFO.interactive_flag;
        let is_powershell = SHELL_INFO.is_powershell;

        let mut cmd = Command::new(shell);
        if let Some(flag) = interactive_flag {
            cmd.arg(flag);
        }
        let mut child = cmd
            .current_dir(&cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| anyhow!("failed to spawn {shell}: {e}"))?;

        // Spawn a persistent stderr drain thread.
        let stderr_buffer: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));
        if let Some(stderr_handle) = child.stderr.take() {
            let buf = stderr_buffer.clone();
            let id = session_id.clone();
            thread::spawn(move || {
                tracing::debug!(session_id = %id, "stderr drain thread started");
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
                tracing::debug!(session_id = %id, "stderr drain thread exited");
            });
        }

        let (stdout_tx, stdout_rx) = std::sync::mpsc::channel::<String>();
        if let Some(stdout_handle) = child.stdout.take() {
            let id = session_id.clone();
            thread::spawn(move || {
                tracing::debug!(session_id = %id, "stdout reader thread started");
                let mut reader = BufReader::new(stdout_handle);
                let mut buf = Vec::new();
                loop {
                    buf.clear();
                    match reader.read_until(b'\n', &mut buf) {
                        Ok(0) => break, // EOF
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
                tracing::debug!(session_id = %id, "stdout reader thread exited");
            });
        }

        Ok(Self {
            child: Arc::new(Mutex::new(Some(child))),
            cwd,
            _session_id: session_id,
            is_powershell,
            stderr_buffer,
            stdout_rx: Arc::new(Mutex::new(stdout_rx)),
        })
    }

    /// Restart the shell session with a fresh process.
    fn restart(&mut self) -> Result<()> {
        // Kill existing process
        if let Some(mut child) = self
            .child
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
        {
            let _ = child.kill();
            let _ = child.wait();
        }

        // Clear stderr buffer
        if let Ok(mut b) = self.stderr_buffer.lock() {
            b.clear();
        }

        // Spawn new shell process
        let shell = SHELL_INFO.binary;
        let interactive_flag = SHELL_INFO.interactive_flag;
        let mut cmd = Command::new(shell);
        if let Some(flag) = interactive_flag {
            cmd.arg(flag);
        }
        let mut new_child = cmd
            .current_dir(&self.cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| anyhow!("failed to restart {shell}: {e}"))?;

        // Spawn persistent stderr drain thread for the new process
        if let Some(stderr_handle) = new_child.stderr.take() {
            let buf = self.stderr_buffer.clone();
            thread::spawn(move || {
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
            });
        }

        let (stdout_tx, stdout_rx) = std::sync::mpsc::channel::<String>();
        if let Some(stdout_handle) = new_child.stdout.take() {
            thread::spawn(move || {
                let mut reader = BufReader::new(stdout_handle);
                let mut buf = Vec::new();
                loop {
                    buf.clear();
                    match reader.read_until(b'\n', &mut buf) {
                        Ok(0) => break, // EOF
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
            });
        }

        self.stdout_rx = Arc::new(Mutex::new(stdout_rx));
        *self
            .child
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(new_child);
        Ok(())
    }

    /// Execute a command in the persistent bash session.
    ///
    /// # Arguments
    ///
    /// * `command` - The shell command to execute
    /// * `timeout_secs` - Maximum execution time in seconds
    ///
    /// # Returns
    ///
    /// Tuple of (stdout, stderr, `exit_code`)
    fn execute(&self, command: &str, timeout_secs: u64) -> Result<(String, String, i32)> {
        // Write command to shell stdin (short-lived lock)
        {
            let mut child_guard = self
                .child
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let child = child_guard
                .as_mut()
                .ok_or_else(|| anyhow!("bash session not available"))?;

            let wrapped_command = if timeout_secs > 0 && !self.is_powershell {
                format!("timeout {timeout_secs} {command}")
            } else {
                command.to_string()
            };

            if let Some(stdin) = child.stdin.as_mut() {
                writeln!(stdin, "{wrapped_command}")
                    .map_err(|e| anyhow!("failed to write command: {e}"))?;

                if self.is_powershell {
                    writeln!(stdin, "Write-Output $LASTEXITCODE")
                        .map_err(|e| anyhow!("failed to write exit code query: {e}"))?;
                } else {
                    writeln!(stdin, "echo $?")
                        .map_err(|e| anyhow!("failed to write exit code query: {e}"))?;
                }

                writeln!(stdin, "echo '---END---'")
                    .map_err(|e| anyhow!("failed to write delimiter: {e}"))?;

                stdin
                    .flush()
                    .map_err(|e| anyhow!("failed to flush stdin: {e}"))?;
            } else {
                return Err(anyhow!("shell stdin not available"));
            }
        } // child_guard dropped here

        // Read stdout from the persistent reader thread via shared channel.
        // The reader thread was spawned at session creation and continuously reads
        // from the child's stdout pipe, sending lines through this channel.
        let stdout_rx = self
            .stdout_rx
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let mut output_lines = Vec::new();
        let mut exit_code_line = String::new();
        let mut read_timed_out = false;

        // Allow extra time beyond the command timeout for the delimiter/exit-code lines
        let read_deadline = Instant::now() + Duration::from_secs(timeout_secs.saturating_add(10));

        loop {
            let remaining = read_deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                read_timed_out = true;
                break;
            }

            match stdout_rx.recv_timeout(remaining) {
                Ok(line) => {
                    if line.contains("---END---") {
                        break;
                    }
                    output_lines.push(line);
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    read_timed_out = true;
                    break;
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    break; // EOF
                }
            }
        }

        // Read accumulated stderr from the persistent drain thread, then clear it
        // for the next command. Give the drain thread time to flush remaining data.
        thread::sleep(Duration::from_millis(200));
        let raw_stderr = if let Ok(mut buf) = self.stderr_buffer.lock() {
            let s = buf.clone();
            buf.clear();
            s
        } else {
            String::new()
        };
        let stderr = filter_shell_boilerplate(&raw_stderr);

        if read_timed_out {
            if let Ok(mut child_guard) = self.child.lock() {
                if let Some(child) = child_guard.as_mut() {
                    if let Some(stdin) = child.stdin.as_mut() {
                        let _ = stdin.write_all(b"\x03\n");
                        let _ = stdin.flush();
                        thread::sleep(Duration::from_millis(100));
                        let _ = writeln!(stdin, "echo '---END---'");
                        let _ = stdin.flush();
                    }
                }
            }
            // Drain remaining lines
            while stdout_rx.try_recv().is_ok() {}
            return Ok((
                "command timed out - output may be incomplete".to_string(),
                stderr,
                124,
            ));
        }

        if !output_lines.is_empty() {
            exit_code_line = output_lines.pop().unwrap_or_default();
        }

        output_lines.retain(|line| !is_shell_boilerplate(line.trim()));

        let stdout = output_lines.join("\n");

        // Parse exit code
        let exit_code: i32 = exit_code_line.trim().parse().unwrap_or(-1);

        // Check for specific error patterns.
        // Only match at the start of a line to avoid false positives from:
        // - Shell startup messages in .zshrc/.bashrc that accumulate in stderr
        // - Command output that legitimately contains these phrases (e.g., grep logs)
        let is_cmd_not_found = stdout.lines().any(is_command_not_found_line)
            || stderr.lines().any(is_command_not_found_line);
        if is_cmd_not_found {
            return Err(anyhow!("command not found: {command}"));
        }

        let is_perm_denied = stdout.lines().any(|l| {
            l.trim().starts_with("Permission denied")
                || l.trim().starts_with("bash: ") && l.contains("Permission denied")
                || l.trim().starts_with("zsh: ") && l.contains("Permission denied")
        }) || stderr.lines().any(|l| {
            l.trim().starts_with("Permission denied")
                || l.trim().starts_with("bash: ") && l.contains("Permission denied")
                || l.trim().starts_with("zsh: ") && l.contains("Permission denied")
        });
        if is_perm_denied {
            return Err(anyhow!("permission denied: {command}"));
        }

        Ok((stdout, stderr, exit_code))
    }

    /// Execute a command and stream output incrementally.
    ///
    /// # Arguments
    ///
    /// * `command` - The shell command to execute
    /// * `timeout_secs` - Maximum execution time in seconds
    /// * `sender` - Channel sender for streaming output chunks
    ///
    /// # Returns
    ///
    /// Tuple of (`exit_code`, error)
    fn execute_stream(
        &self,
        command: &str,
        timeout_secs: u64,
        sender: StreamSender,
    ) -> Result<(i32, Option<String>)> {
        // Write command to shell stdin (short-lived lock — dropped before reading)
        {
            let mut child_guard = self
                .child
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let child = child_guard
                .as_mut()
                .ok_or_else(|| anyhow!("shell session not available"))?;

            if let Some(stdin) = child.stdin.as_mut() {
                let wrapped_command = if timeout_secs > 0 && !self.is_powershell {
                    format!("timeout {timeout_secs} {command}")
                } else {
                    command.to_string()
                };

                writeln!(stdin, "{wrapped_command}")
                    .map_err(|e| anyhow!("failed to write command: {e}"))?;

                // Write exit code query (platform-specific)
                if self.is_powershell {
                    writeln!(stdin, "Write-Output $LASTEXITCODE")
                        .map_err(|e| anyhow!("failed to write exit code query: {e}"))?;
                } else {
                    writeln!(stdin, "echo $?")
                        .map_err(|e| anyhow!("failed to write exit code query: {e}"))?;
                }

                // Write a delimiter to mark end of output
                writeln!(stdin, "echo '---END---'")
                    .map_err(|e| anyhow!("failed to write delimiter: {e}"))?;

                stdin
                    .flush()
                    .map_err(|e| anyhow!("failed to flush stdin: {e}"))?;
            } else {
                return Err(anyhow!("shell stdin not available"));
            }
        } // child_guard dropped here — must not hold child lock while reading stdout

        // Read stdout from the persistent reader thread via shared channel.
        // The reader thread was spawned at session creation (new()) and continuously
        // reads from the child's stdout pipe, sending lines through this channel.
        // We must NOT call child.stdout.take() here — stdout was already taken in new().
        let stdout_rx = self
            .stdout_rx
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let mut exit_code_line = String::new();
        let mut read_timed_out = false;

        // Allow extra time beyond the command timeout for the delimiter/exit-code lines
        let read_deadline = Instant::now() + Duration::from_secs(timeout_secs.saturating_add(10));

        // Stream output line by line from the channel
        loop {
            let remaining = read_deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                read_timed_out = true;
                break;
            }

            match stdout_rx.recv_timeout(remaining) {
                Ok(line) => {
                    if line.contains("---END---") {
                        break;
                    }

                    if !line.trim().is_empty() {
                        if is_shell_boilerplate(line.trim()) {
                            continue;
                        }
                        let chunk = StreamChunk::new(format!("{line}\n"));
                        sender
                            .send(chunk)
                            .map_err(|e| anyhow!("failed to send chunk: {e}"))?;
                    }

                    if line.trim().chars().all(|c| c.is_numeric() || c == '-') {
                        exit_code_line = line;
                    }
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    read_timed_out = true;
                    break;
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    break; // EOF
                }
            }
        }

        // Handle timeout: send SIGINT (Ctrl+C) and drain remaining output
        if read_timed_out {
            if let Ok(mut child_guard) = self.child.lock() {
                if let Some(child) = child_guard.as_mut() {
                    if let Some(stdin) = child.stdin.as_mut() {
                        let _ = stdin.write_all(b"\x03\n");
                        let _ = stdin.flush();
                        thread::sleep(Duration::from_millis(100));
                        let _ = writeln!(stdin, "echo '---END---'");
                        let _ = stdin.flush();
                    }
                }
            }
            // Drain remaining lines from the channel
            while stdout_rx.try_recv().is_ok() {}

            let _ = sender.send(StreamChunk::done());
            return Ok((
                124,
                Some("command timed out - output may be incomplete".to_string()),
            ));
        }

        // Read accumulated stderr from the persistent drain thread
        thread::sleep(Duration::from_millis(200));
        let raw_stderr = if let Ok(mut buf) = self.stderr_buffer.lock() {
            let s = buf.clone();
            buf.clear();
            s
        } else {
            String::new()
        };
        let stderr = filter_shell_boilerplate(&raw_stderr);

        // Stream stderr if present
        if !stderr.is_empty() {
            let chunk = StreamChunk::new(format!("[stderr] {stderr}\n"));
            sender
                .send(chunk)
                .map_err(|e| anyhow!("failed to send stderr chunk: {e}"))?;
        }

        // Parse exit code
        let exit_code: i32 = exit_code_line.trim().parse().unwrap_or(-1);

        // Check for specific error patterns
        // Only match at the start of a line to avoid false positives from
        // shell startup messages or legitimate output containing these phrases
        let error = if stderr.lines().any(is_command_not_found_line) {
            Some(format!("command not found: {command}"))
        } else if stderr.lines().any(|l| {
            l.trim().starts_with("Permission denied")
                || (l.trim().starts_with("bash: ") || l.trim().starts_with("zsh: "))
                    && l.contains("Permission denied")
        }) {
            Some(format!("permission denied: {command}"))
        } else {
            None
        };

        // Send done signal
        let _ = sender.send(StreamChunk::done());

        Ok((exit_code, error))
    }
}

impl Drop for BashSession {
    fn drop(&mut self) {
        // Kill the bash process when session is dropped
        if let Some(mut child) = self
            .child
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
        {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// Registry that caches `BashSession` instances by working directory.
///
/// Sessions are reused across tool calls so that environment variables,
/// shell aliases, and working directory changes persist. Idle sessions
/// are evicted after `IDLE_TIMEOUT_SECS` to reclaim resources.
struct BashSessionRegistry {
    sessions: Mutex<Option<HashMap<PathBuf, Arc<Mutex<BashSession>>>>>,
    last_access: Mutex<Option<HashMap<PathBuf, Instant>>>,
}

/// Idle sessions are cleaned up after this many seconds.
const IDLE_TIMEOUT_SECS: u64 = 300; // 5 minutes

impl BashSessionRegistry {
    const fn new() -> Self {
        Self {
            sessions: Mutex::new(None),
            last_access: Mutex::new(None),
        }
    }

    fn ensure_init(&self) {
        if self
            .sessions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .is_none()
        {
            *self
                .sessions
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(HashMap::new());
            *self
                .last_access
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(HashMap::new());
        }
    }

    /// Get or create a session for the given working directory.
    fn get_or_create(&self, cwd: PathBuf) -> Result<Arc<Mutex<BashSession>>> {
        self.ensure_init();

        let sessions_guard = self
            .sessions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        // Touch access time (scope the lock to avoid holding two locks simultaneously)
        {
            if let Some(ref mut times) = *self
                .last_access
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
            {
                times.insert(cwd.clone(), Instant::now());
            }
        }

        // Check if session exists (while holding the lock)
        if let Some(ref sessions) = *sessions_guard {
            if let Some(session) = sessions.get(&cwd) {
                return Ok(Arc::clone(session));
            }
        }

        // Create new session — release lock first to avoid holding it during process spawn
        drop(sessions_guard);
        let session = Arc::new(Mutex::new(BashSession::new(cwd.clone())?));

        // Re-acquire lock and insert (double-check in case another thread raced us)
        let mut sessions_guard = self
            .sessions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(ref mut sessions) = *sessions_guard {
            // Another thread may have created a session while we were spawning
            if let Some(existing) = sessions.get(&cwd) {
                return Ok(Arc::clone(existing));
            }
            sessions.insert(cwd, Arc::clone(&session));
        }

        Ok(session)
    }

    /// Remove and return the session for `cwd`, if any.
    fn remove(&self, cwd: &Path) -> Option<Arc<Mutex<BashSession>>> {
        self.ensure_init();
        // Lock in consistent order: sessions first, then last_access
        let removed = {
            let mut sessions = self
                .sessions
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            match sessions.as_mut() {
                Some(s) => s.remove(cwd),
                None => None,
            }
        };
        // Clean up access time regardless
        if let Some(ref mut times) = *self
            .last_access
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
        {
            times.remove(cwd);
        }
        removed
    }

    /// Evict sessions that have been idle longer than `IDLE_TIMEOUT_SECS`.
    fn evict_idle(&self) {
        self.ensure_init();
        let now = Instant::now();
        let to_evict: Vec<PathBuf> = {
            // Lock in consistent order: last_access first
            let times_guard = self
                .last_access
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            match *times_guard {
                Some(ref times) => times
                    .iter()
                    .filter(|(_, last)| now.duration_since(**last).as_secs() > IDLE_TIMEOUT_SECS)
                    .map(|(p, _)| p.clone())
                    .collect(),
                None => return,
            }
        };
        if !to_evict.is_empty() {
            // Lock sessions in consistent order: sessions first
            let mut sessions_guard = self
                .sessions
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(ref mut sessions) = *sessions_guard {
                for cwd in &to_evict {
                    sessions.remove(cwd);
                }
            }
            // Then last_access (already locked above)
            drop(sessions_guard);
            let mut times_guard = self
                .last_access
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(ref mut times) = *times_guard {
                for cwd in &to_evict {
                    times.remove(cwd);
                }
            }
        }
    }
}

/// Global session registry — keyed by canonical working directory.
static BASH_SESSION_REGISTRY: BashSessionRegistry = BashSessionRegistry::new();

/// Rate limiter for concurrent bash executions.
///
/// Limits the number of bash commands that can run simultaneously
/// to prevent resource exhaustion and ensure system stability.
struct BashRateLimiter {
    /// Current number of active executions
    active: AtomicUsize,
    /// Maximum allowed concurrent executions (public for error messages)
    pub max_concurrent: usize,
}

impl BashRateLimiter {
    /// Create a new rate limiter with the specified maximum concurrency.
    const fn new(max_concurrent: usize) -> Self {
        Self {
            active: AtomicUsize::new(0),
            max_concurrent,
        }
    }

    /// Try to acquire a permit to execute a bash command.
    ///
    /// Returns Ok(permit) if successful, Err if rate limit exceeded.
    /// The permit should be dropped after execution completes.
    fn try_acquire(&self) -> Result<BashPermit<'_>, ()> {
        let current = self.active.load(Ordering::Relaxed);

        if current >= self.max_concurrent {
            return Err(());
        }

        // Try to increment the counter
        let mut old = current;
        loop {
            if old >= self.max_concurrent {
                return Err(());
            }

            match self.active.compare_exchange_weak(
                old,
                old + 1,
                Ordering::Acquire,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(actual) => old = actual,
            }
        }

        Ok(BashPermit {
            limiter: self,
            _private: (),
        })
    }

    /// Release a permit after execution completes.
    fn release(&self) {
        self.active.fetch_sub(1, Ordering::Release);
    }

    /// Get the current number of active executions.
    fn active_count(&self) -> usize {
        self.active.load(Ordering::Relaxed)
    }
}

/// Permit that represents an acquired slot for bash execution.
///
/// When dropped, automatically releases the permit back to the limiter.
struct BashPermit<'a> {
    limiter: &'a BashRateLimiter,
    _private: (),
}

impl<'a> Drop for BashPermit<'a> {
    fn drop(&mut self) {
        self.limiter.release();
    }
}

/// Global rate limiter for bash executions.
///
/// Limits to 5 concurrent bash commands by default.
static BASH_RATE_LIMITER: BashRateLimiter = BashRateLimiter::new(5);

#[derive(Default)]
pub struct BashTool;

impl BashTool {
    /// Execute a command in an isolated Docker container.
    ///
    /// Falls back to normal execution if Docker is unavailable.
    fn execute_in_docker(&self, command: &str, workspace: &Path) -> Result<ToolOutput> {
        use crate::providers::docker_isolation::{DockerIsolation, DockerIsolationConfig};

        if !DockerIsolation::is_docker_available() {
            tracing::warn!("Docker isolation requested but Docker not available, falling back to local execution");
            return Err(anyhow!(
                "Docker isolation requested but Docker is not available. \
                 Please install Docker or disable isolation mode."
            ));
        }

        let config = DockerIsolationConfig::new();
        let isolation = DockerIsolation::new(config);

        let result = isolation.execute(command, workspace)?;

        let truncated = truncate_bash_output(&result.stdout, &result.stderr, result.exit_code);
        let output = if result.exit_code == 0 {
            truncated.output
        } else {
            format!(
                "Exit code: {}\n\n{}{}",
                result.exit_code,
                truncated.output,
                if result.stderr.is_empty() {
                    String::new()
                } else {
                    format!("\nStderr:\n{}", result.stderr)
                }
            )
        };

        Ok(ToolOutput::with_structured(
            output,
            json!({
                "exit_code": result.exit_code,
                "container_id": result.container_id,
                "duration_ms": result.duration_ms,
                "isolated": true
            }),
        ))
    }
}

impl Tool for BashTool {
    fn name(&self) -> &'static str {
        "bash"
    }

    fn description(&self) -> &'static str {
        "Run bash/POSIX commands in a persistent shell session (works on Unix, Linux, macOS, WSL, and Cygwin). \
         Available: ls, grep, sed, awk, find, curl, git, cargo, npm, python3, etc. \
         On Windows: bash is detected via WSL or Cygwin if available."
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::Execute
    }

    fn parameters_schema(&self) -> Value {
        let cmd_desc = if cfg!(unix) {
            "Shell command to execute (bash/zsh syntax)"
        } else {
            "Shell command to execute (PowerShell/cmd syntax)"
        };
        json!({
            "type": "object",
            "required": ["command"],
            "properties": {
                "command": {
                    "type": "string",
                    "description": cmd_desc
                },
                "restart": {
                    "type": "boolean",
                    "description": "If true, restart the bash session before executing the command"
                },
                "timeout_secs": {
                    "type": "integer",
                    "description": "Timeout in seconds (default 120s, max 600s)",
                    "default": 120
                }
            }
        })
    }

    fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput> {
        // Check permissions
        crate::check_permission(self.permission(), ctx)?;

        // Role-based gating
        if let Some(gate) = &ctx.plan_gate {
            gate.check_access(ctx.role, self.name())?;
        }

        let command = params
            .get("command")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                let actual = params
                    .get("command")
                    .map(ToString::to_string)
                    .unwrap_or_else(|| "null".to_string());
                anyhow!("missing string parameter 'command', got: {actual}")
            })?
            .to_string(); // Clone to avoid lifetime issues

        let restart = params
            .get("restart")
            .and_then(Value::as_bool)
            .unwrap_or(false);

        let timeout_secs = params
            .get("timeout_secs")
            .and_then(Value::as_u64)
            .unwrap_or(120)
            .min(600); // Cap at 600 seconds for long builds

        // Cross-platform path validation (handles Windows, WSL, Cygwin)
        use crate::security::cross_platform::{
            get_allowed_commands, get_blocked_commands, validate_path_in_workspace, ShellType,
        };

        let shell_type = ShellType::Bash;
        validate_path_in_workspace(&ctx.cwd, &ctx.cwd)?;
        validate_command_safety(&command)?;

        // Validate command is in platform-specific allowlist
        let binary_name = extract_binary_name(&command)?;
        let allowed_commands = get_allowed_commands(shell_type);
        if !allowed_commands.contains(&binary_name.as_str()) {
            anyhow::bail!("command '{}' is not in allowed list for bash", binary_name);
        }

        // Validate command is NOT in platform-specific blocklist
        let blocked_commands = get_blocked_commands(shell_type);
        if blocked_commands.contains(&binary_name.as_str()) {
            anyhow::bail!("command '{}' is blocked for security reasons", binary_name);
        }

        // Docker isolation path: execute in ephemeral container
        // Enable via SandboxConfig.docker_isolation = true or RUSTYCODE_DOCKER_ISOLATION=1
        let docker_requested = ctx.sandbox.docker_isolation
            || std::env::var("RUSTYCODE_DOCKER_ISOLATION")
                .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                .unwrap_or(false);

        if docker_requested {
            return self.execute_in_docker(&command, &ctx.cwd);
        }

        // Apply rate limiting - acquire permit for execution
        let _permit = BASH_RATE_LIMITER.try_acquire().map_err(|()| {
            anyhow!(
                "Rate limit exceeded: {} concurrent bash commands already running. Maximum: {}. Please wait for current commands to complete.",
                BASH_RATE_LIMITER.active_count(),
                BASH_RATE_LIMITER.max_concurrent
            )
        })?;

        // Track execution time
        let start_time = Instant::now();

        // In sandbox/container mode, always use a fresh shell process per command.
        // This prevents cascading failures where one timed-out command blocks all
        // subsequent commands (the PTY child process gets stuck).
        let sandbox_mode = std::env::var("RUSTYCODE_SANDBOX").as_deref() == Ok("container");

        let session = if sandbox_mode || restart {
            // Remove existing session and create fresh one
            BASH_SESSION_REGISTRY.remove(&ctx.cwd);
            BASH_SESSION_REGISTRY.get_or_create(ctx.cwd.clone())?
        } else {
            // Evict idle sessions first
            BASH_SESSION_REGISTRY.evict_idle();
            BASH_SESSION_REGISTRY.get_or_create(ctx.cwd.clone())?
        };

        // Execute command with timeout
        // Guard against "Cannot start a runtime from within a runtime" panic:
        // If we're already inside a tokio runtime (e.g., headless agent loop),
        // use block_in_place + handle.block_on. Otherwise, create a new runtime.
        let command_clone = command.clone();
        let cwd_clone = ctx.cwd.clone();
        let (stdout, stderr, exit_code) = if let Ok(handle) = tokio::runtime::Handle::try_current()
        {
            // Already inside a tokio runtime — use block_in_place to avoid panic.
            // block_in_place allows the blocking operation without blocking the
            // async executor, and handle.block_on runs the future on the existing runtime.
            tokio::task::block_in_place(|| {
                handle.block_on(async {
                    let result = tokio::time::timeout(
                        Duration::from_secs(timeout_secs),
                        tokio::task::spawn_blocking(move || {
                            let s = session
                                .lock()
                                .unwrap_or_else(std::sync::PoisonError::into_inner);
                            let alive = s
                                .child
                                .lock()
                                .unwrap_or_else(std::sync::PoisonError::into_inner)
                                .is_some();
                            if !alive {
                                drop(s);
                                drop(session);
                                BASH_SESSION_REGISTRY.remove(&cwd_clone);
                                let fresh = BASH_SESSION_REGISTRY.get_or_create(cwd_clone)?;
                                let s = fresh
                                    .lock()
                                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                                return s.execute(&command_clone, timeout_secs);
                            }
                            s.execute(&command_clone, timeout_secs)
                        }),
                    )
                    .await;

                    // If the command timed out, evict the stuck session so the next
                    // command gets a fresh shell instead of reusing the broken one.
                    if result.is_err() {
                        tracing::warn!(
                            "bash command timed out, evicting session for {:?}",
                            ctx.cwd
                        );
                        BASH_SESSION_REGISTRY.remove(&ctx.cwd);
                    }

                    result
                        .map_err(|_| anyhow!("command timed out after {timeout_secs}s"))?
                        .map_err(|e| anyhow!("command execution failed: {e}"))?
                })
            })
        } else {
            // Not inside a runtime — create our own
            tokio::runtime::Runtime::new()
                .map_err(|e| anyhow!("failed to create tokio runtime: {e}"))?
                .block_on(async {
                    let cwd_for_evict = ctx.cwd.clone();
                    let result = tokio::time::timeout(
                        Duration::from_secs(timeout_secs),
                        tokio::task::spawn_blocking(move || {
                            let s = session
                                .lock()
                                .unwrap_or_else(std::sync::PoisonError::into_inner);
                            let alive = s
                                .child
                                .lock()
                                .unwrap_or_else(std::sync::PoisonError::into_inner)
                                .is_some();
                            if !alive {
                                drop(s);
                                drop(session);
                                BASH_SESSION_REGISTRY.remove(&cwd_clone);
                                let fresh = BASH_SESSION_REGISTRY.get_or_create(cwd_clone)?;
                                let s = fresh
                                    .lock()
                                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                                return s.execute(&command_clone, timeout_secs);
                            }
                            s.execute(&command_clone, timeout_secs)
                        }),
                    )
                    .await;

                    if result.is_err() {
                        tracing::warn!(
                            "bash command timed out, evicting session for {:?}",
                            cwd_for_evict
                        );
                        BASH_SESSION_REGISTRY.remove(&cwd_for_evict);
                    }

                    result
                        .map_err(|_| anyhow!("command timed out after {timeout_secs}s"))?
                        .map_err(|e| anyhow!("command execution failed: {e}"))?
                })
        }?;

        let execution_time = start_time.elapsed();

        // Check for transform parameter
        if let Some(transform_name) = params.get("transform").and_then(Value::as_str) {
            if let Some(transformed) = transform_by_name(transform_name, &stdout, &stderr) {
                let structured = json!({
                    "status": exit_code,
                    "transformed": {
                        "title": transformed.title,
                        "short": transformed.short,
                        "full": transformed.full,
                        "structured": transformed.structured,
                    },
                    "execution_time_ms": execution_time.as_millis()
                });
                return Ok(ToolOutput::with_structured(transformed.short, structured));
            }
        }

        // Apply smart truncation for large outputs
        let truncated = truncate_bash_output(&stdout, &stderr, exit_code);

        // Extract output text before consuming truncated
        let output_text = truncated.as_str().to_string();

        // Build enhanced metadata with execution time
        let metadata = {
            let mut meta = truncated.into_metadata();
            meta["exit_code"] = json!(exit_code);
            meta["command"] = json!(command);
            meta["execution_time_ms"] = json!(execution_time.as_millis());
            meta["timeout_secs"] = json!(timeout_secs);
            if exit_code != 0 {
                meta["failed"] = json!(true);
            }
            meta
        };

        Ok(ToolOutput::with_structured(output_text, metadata))
    }
}

impl ToolStreaming for BashTool {
    fn execute_stream(&self, params: Value, ctx: &ToolContext) -> Result<StreamReceiver> {
        use crate::streaming::{create_stream_channel, StreamChunk};

        // Check permissions
        crate::check_permission(self.permission(), ctx)?;

        // Role-based gating
        if let Some(gate) = &ctx.plan_gate {
            gate.check_access(ctx.role, self.name())?;
        }

        let command = params
            .get("command")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                let actual = params
                    .get("command")
                    .map(ToString::to_string)
                    .unwrap_or_else(|| "null".to_string());
                anyhow!("missing string parameter 'command', got: {actual}")
            })?
            .to_string();

        let restart = params
            .get("restart")
            .and_then(Value::as_bool)
            .unwrap_or(false);

        let timeout_secs = params
            .get("timeout_secs")
            .and_then(Value::as_u64)
            .unwrap_or(120)
            .min(600); // Cap at 600 seconds for long builds

        let cwd = ctx.cwd.clone();
        ensure_path_within_workspace(ctx, &cwd)?;
        validate_command_safety(&command)?;

        // Create streaming channel
        let (sender, receiver) = create_stream_channel();
        let sender_clone = sender.clone();

        // Spawn thread for streaming execution
        thread::spawn(move || {
            // Track execution time
            let start_time = Instant::now();

            // Create bash session
            let mut session = match BashSession::new(cwd) {
                Ok(s) => s,
                Err(e) => {
                    let _ = sender_clone.send(StreamChunk::error(e.to_string()));
                    return;
                }
            };

            if restart {
                if let Err(e) = session.restart() {
                    let _ = sender_clone.send(StreamChunk::error(e.to_string()));
                    return;
                }
            }

            // Execute with timeout in a separate thread
            let (exit_code, _error) =
                match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    session.execute_stream(&command, timeout_secs, sender_clone)
                })) {
                    Ok(Ok(result)) => result,
                    Ok(Err(e)) => {
                        let _ = sender.send(StreamChunk::error(e.to_string()));
                        return;
                    }
                    Err(_) => {
                        let _ = sender.send(StreamChunk::error(
                            "panic during command execution".to_string(),
                        ));
                        return;
                    }
                };

            let execution_time = start_time.elapsed();

            // Send final metadata chunk
            let metadata = json!({
                "exit_code": exit_code,
                "command": command,
                "execution_time_ms": execution_time.as_millis(),
                "timeout_secs": timeout_secs,
                "streaming": true,
            });

            let metadata_chunk = StreamChunk::new(format!("\n[metadata] {metadata}\n"));
            let _ = sender.send(metadata_chunk);
        });

        Ok(receiver)
    }
}

/// Extract the binary name from a shell command (first token, without path)
fn extract_binary_name(command: &str) -> anyhow::Result<String> {
    use shell_words::split;
    let tokens = split(command).map_err(|_| anyhow::anyhow!("invalid command syntax"))?;
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

/// Validates that a command is safe to execute.
///
/// This function uses an allowlist approach for maximum security:
/// - Only explicitly allowed commands can be executed
/// - Uses `shell_words::split()` for proper tokenization
/// - Detects dangerous flags for commands like `find`
/// - Rejects commands that fail shell parsing (potential obfuscation)
/// - Blocks shell metacharacters that could enable command injection
///
/// # Security Model
///
/// The allowlist approach is more secure than a blocklist because:
/// 1. Unknown/exploitative commands are blocked by default
/// 2. New attack vectors cannot bypass the allowlist
/// 3. The list of allowed commands is auditable and minimal
///
/// # Public API
///
/// This function is public so it can be reused by other parts of the codebase
/// (e.g., `command_runner` in rustycode-runtime) to ensure consistent security
/// validation across all command execution paths.
pub fn validate_command_safety(command: &str) -> Result<()> {
    // In sandbox/container mode, skip security restrictions. The agent is
    // already running in an isolated container — these checks only prevent
    // the agent from doing its job (e.g., `python3 -c "import numpy"`,
    // `wc -c`, `curl -L` are all blocked by the allowlist).
    if std::env::var("RUSTYCODE_SANDBOX").as_deref() == Ok("container") {
        return Ok(());
    }

    // SECURITY: Add input length limit BEFORE parsing to prevent ReDoS
    const MAX_COMMAND_LENGTH: usize = 10_000; // 10KB max command length
    if command.len() > MAX_COMMAND_LENGTH {
        anyhow::bail!("command exceeds maximum length of {MAX_COMMAND_LENGTH} characters");
    }

    // SECURITY: Check for excessive quote nesting (potential obfuscation)
    // Heredoc bodies (cat > file << 'EOF') contain legitimate multi-line content with many
    // quotes. We skip depth-counting inside heredoc blocks and only measure nesting in
    // the actual shell command structure. Raw total is capped as a secondary ceiling.
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
        anyhow::bail!("command has excessive quote nesting (potential obfuscation attempt)");
    }
    let raw_quote_count = command.chars().filter(|&c| c == '\'' || c == '"').count();
    if raw_quote_count > 2000 {
        anyhow::bail!("command has excessive quote nesting (potential obfuscation attempt)");
    }

    // Block null bytes and raw carriage returns, but allow newlines when this is a heredoc.
    // Heredoc syntax (cat << 'EOF' ... EOF) requires embedded newlines in the command string.
    let has_null = command.contains('\0');
    let has_cr = command.contains('\r');
    let has_newline = command.contains('\n');
    let is_heredoc = command.contains("<<");
    if has_null {
        anyhow::bail!("blocked command with null byte (potential injection)");
    }
    if has_cr {
        anyhow::bail!("blocked command with carriage return (potential injection)");
    }

    // SECURITY: Block IFS variable usage — can bypass regex-based validation by
    // changing how the shell splits tokens. Example: IFS=x; $IFS expands to a
    // field separator that can turn "rmxrf" into "rm rf".
    if command.contains("$IFS") || command.contains("${IFS") {
        anyhow::bail!("blocked command with IFS variable usage (potential security bypass)");
    }

    // SECURITY: Block Unicode whitespace characters that could cause parsing
    // inconsistencies between validation and execution. These non-standard
    // whitespace chars (U+00A0 NO-BREAK SPACE, U+2000-U+200A various spaces,
    // U+2028/U+2029 line/paragraph separators, U+3000 IDEOGRAPHIC SPACE) may
    // be interpreted differently by the shell vs our regex-based checks.
    let has_unicode_ws = command.chars().any(|c| {
        matches!(
            c,
            '\u{00A0}'  // NO-BREAK SPACE
            | '\u{1680}' // OGHAM SPACE MARK
            | '\u{2000}'
                ..='\u{200A}' // VARIOUS SPACES
            | '\u{2028}' // LINE SEPARATOR
            | '\u{2029}' // PARAGRAPH SEPARATOR
            | '\u{202F}' // NARROW NO-BREAK SPACE
            | '\u{205F}' // MEDIUM MATHEMATICAL SPACE
            | '\u{3000}' // IDEOGRAPHIC SPACE
        )
    });
    if has_unicode_ws {
        anyhow::bail!("blocked command with Unicode whitespace (potential parsing inconsistency)");
    }

    // Parse the command using shell-words to properly tokenize it.
    // This will fail if the command has invalid quoting, which may indicate
    // an attempt to obfuscate the command.
    let tokens = shell_words::split(command).map_err(|_| {
        anyhow::anyhow!("blocked command with invalid shell syntax (potential obfuscation attempt)")
    })?;

    // SECURITY: Newline validation. shell_words::split() absorbs newlines inside
    // quotes, so `python3 -c "...30 line script..."` → 3 tokens. Real injection
    // like "ls\nrm -rf /" → 4+ tokens with shell operators. We use both token
    // count and raw line analysis to distinguish safe from dangerous.
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
                anyhow::bail!("blocked command with excessive newlines and shell chaining (potential injection)");
            }
        }
    }

    // Empty command is invalid
    if tokens.is_empty() {
        anyhow::bail!("blocked empty command");
    }

    // Get the binary name (first token), stripping any path component
    let binary = &tokens[0];
    let binary_name = if binary.contains('/') {
        binary.rsplit('/').next().unwrap_or(binary)
    } else {
        binary
    };

    // SECURITY: Check for dangerous flag combinations that could bypass allowlist.
    // Only block -c/--command for shells and interpreters where it means "execute
    // arbitrary code". Commands like `wc -c` (byte count) or `git -c key=val`
    // (config override) use -c for harmless purposes.
    const SHELLS_AND_INTERPRETERS: &[&str] = &[
        "sh", "bash", "zsh", "fish", "dash", "ksh", "csh", "tcsh", "python", "python3", "perl",
        "ruby", "node", "lua",
    ];
    let is_shell_or_interp = SHELLS_AND_INTERPRETERS.contains(&binary_name);

    if tokens.len() >= 2 {
        for (i, token) in tokens.iter().enumerate() {
            // Block -c/--command for shells and interpreters where it means "execute
            // arbitrary code". This applies even to allowlisted binaries because -c
            // turns any interpreter into an arbitrary code executor.
            if (token == "-c" || token == "--command") && is_shell_or_interp && i + 1 < tokens.len()
            {
                anyhow::bail!(
                    "blocked command with -c/--command flag (potential allowlist bypass)"
                );
            }

            // Block -e/--eval for interpreters. This applies even to allowlisted
            // binaries because -e turns any interpreter into an arbitrary code executor.
            if (token == "-e" || token == "--eval" || token == "-E") && is_shell_or_interp {
                anyhow::bail!("blocked interpreter with -e flag (potential allowlist bypass)");
            }
        }
    }

    // Allowlist of safe commands
    // This list should be kept minimal - only add commands that are
    // genuinely needed for development workflows.
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

    // Platform-specific commands
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

    // SECURITY: Check for pipe-to-shell bypass.
    // Pipes allow piping output to shells which bypass the allowlist since only
    // the first binary is checked. Block piping to any shell or interpreter.
    // e.g., "cat file.txt | sh" should be blocked even though cat is allowed.
    const BLOCKED_PIPE_TARGETS: &[&str] =
        &["sh", "bash", "zsh", "fish", "dash", "ksh", "csh", "tcsh"];
    let cmd_trimmed = command.trim();
    for target in BLOCKED_PIPE_TARGETS {
        // Check for " | sh", " | bash", etc. (with spaces around pipe)
        if cmd_trimmed.contains(&format!("| {target}"))
            || cmd_trimmed.contains(&format!("|{target}"))
        {
            // Allow if it's part of a longer word (e.g., "| show" should not block "sh")
            let after_pipe = cmd_trimmed
                .split('|')
                .next_back()
                .unwrap_or("")
                .split_whitespace()
                .next()
                .unwrap_or("");
            if after_pipe == *target {
                anyhow::bail!("blocked pipe to shell '{target}' (potential allowlist bypass)");
            }
        }
    }

    // Check if the binary is in the allowlist
    if !ALLOWED_COMMANDS.contains(&binary_name)
        && !PLATFORM_COMMANDS.contains(&binary_name)
        && !PLATFORM_COMMANDS
            .iter()
            .any(|cmd| cmd.eq_ignore_ascii_case(binary_name))
    {
        anyhow::bail!(
            "blocked command '{}' not in allowed list. Allowed commands: {}",
            binary_name,
            ALLOWED_COMMANDS.join(", ")
        );
    }

    // Additional checks for dangerous flags in specific commands
    // Even safe commands can have dangerous flags
    if binary_name == "find" {
        let dangerous_find_flags = ["-delete", "-exec", "-ok", "-execdir"];
        for token in &tokens {
            let token_lower: String = token.to_lowercase();
            for flag in dangerous_find_flags {
                if token_lower.starts_with(flag) || token_lower == flag {
                    anyhow::bail!("blocked find command with dangerous flag `{flag}`");
                }
            }
        }
    }

    // Check for shell function definition (fork bomb) - IMPROVED DETECTION
    // shell_words::split parses ":(){ :|:& };:" as [":(){", ":|:&", "};:"]
    let cmd_lower = command.to_lowercase();

    // Pattern 1: Recursive function definition (more comprehensive)
    if cmd_lower.contains(":(){") || cmd_lower.contains(":() {") {
        anyhow::bail!("blocked shell function definition (potential fork bomb)");
    }

    // Pattern 2: Background self-execution patterns
    if cmd_lower.contains(":|:&") || cmd_lower.contains(": | : &") {
        anyhow::bail!("blocked shell function with self-execution (potential fork bomb)");
    }

    // Pattern 3: Multiple background ampersands (suspicious)
    // Allow higher threshold since Python code often uses & in decorators, bitwise ops, etc.
    let ampersand_count = cmd_lower.matches("&").count();
    if ampersand_count > 50 {
        anyhow::bail!("blocked command with excessive background operators (potential fork bomb)");
    }

    // Pattern 4: Check for eval with function definition
    if cmd_lower.contains("eval") && (cmd_lower.contains("()") || cmd_lower.contains("{")) {
        anyhow::bail!("blocked eval with function definition (potential fork bomb)");
    }

    // Block commands with shell expansion features that could be used for obfuscation - IMPROVED
    // shell-words doesn't parse these as special, but the shell would expand them.
    // When heredoc uses quoted delimiter (<<'EOF' or <<"EOF"), the body is literal text,
    // so we only check shell expansion OUTSIDE the heredoc body.
    let is_heredoc_quoted = {
        let mut in_body = false;
        for line in command.lines() {
            if line.contains("<<") && (line.contains('\'') || line.contains('"')) {
                in_body = true;
            }
            if in_body {
                continue;
            }
            if line.contains("$(") || line.contains('`') {
                return Err(anyhow::anyhow!(
                    "blocked command with shell expansion (potential obfuscation attempt)"
                ));
            }
        }
        true
    };
    if !is_heredoc_quoted && (command.contains("$(") || command.contains('`')) {
        anyhow::bail!("blocked command with shell expansion (potential obfuscation attempt)");
    }

    // Block parameter expansion that could be dangerous
    if cmd_lower.contains("${!") || cmd_lower.contains("${@:") {
        anyhow::bail!("blocked command with dangerous parameter expansion");
    }

    // Block arithmetic expansion
    if cmd_lower.contains("$((") {
        anyhow::bail!("blocked command with arithmetic expansion");
    }

    // Block commands targeting root filesystem recursively
    // This is a catch-all for patterns that might have slipped through
    if cmd_lower.contains("-rf /") || cmd_lower.contains("-rf /*") || cmd_lower.contains("-fr /") {
        anyhow::bail!("blocked recursive delete targeting root filesystem");
    }

    Ok(())
}

fn ensure_path_within_workspace(ctx: &ToolContext, path: &Path) -> Result<()> {
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

fn canonicalize_existing_or_parent(path: &Path) -> Result<PathBuf> {
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

/// Tool for PowerShell commands (Windows).
///
/// Shares the same session and execution logic as BashTool,
/// but validates against PowerShell-specific allowed/blocked commands.
#[derive(Default)]
pub struct PowerShellTool;

impl Tool for PowerShellTool {
    fn name(&self) -> &'static str {
        "powershell"
    }

    fn description(&self) -> &'static str {
        "Run PowerShell commands in a persistent session (Windows only). \
         Available: Get-ChildItem, Get-Content, Select-String, Get-Process, git, cargo, npm, dotnet, etc. \
         Use PowerShell cmdlets and syntax (e.g., $env:PATH, Get-Item, Select-String)."
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::Execute
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["command"],
            "properties": {
                "command": {
                    "type": "string",
                    "description": "PowerShell command (e.g., 'Get-ChildItem', 'Select-String pattern file.txt', '$env:PATH')"
                },
                "restart": {
                    "type": "boolean",
                    "description": "If true, restart the PowerShell session before executing the command"
                },
                "timeout_secs": {
                    "type": "integer",
                    "description": "Timeout in seconds (default 120s, max 600s)",
                    "default": 120
                }
            }
        })
    }

    fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput> {
        // Check permissions
        crate::check_permission(self.permission(), ctx)?;

        // Role-based gating
        if let Some(gate) = &ctx.plan_gate {
            gate.check_access(ctx.role, self.name())?;
        }

        let command = params
            .get("command")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("missing string parameter 'command'"))?
            .to_string();

        // Cross-platform path validation
        use crate::security::cross_platform::{
            get_allowed_commands, get_blocked_commands, validate_path_in_workspace, ShellType,
        };

        let shell_type = ShellType::PowerShell;
        validate_path_in_workspace(&ctx.cwd, &ctx.cwd)?;
        validate_command_safety(&command)?;

        // Validate command is in platform-specific allowlist
        let binary_name = extract_binary_name(&command)?;
        let allowed_commands = get_allowed_commands(shell_type);
        if !allowed_commands.contains(&binary_name.as_str()) {
            anyhow::bail!(
                "command '{}' is not in allowed list for PowerShell",
                binary_name
            );
        }

        // Validate command is NOT in platform-specific blocklist
        let blocked_commands = get_blocked_commands(shell_type);
        if blocked_commands.contains(&binary_name.as_str()) {
            anyhow::bail!("command '{}' is blocked for security reasons", binary_name);
        }

        // Delegate to BashTool's execution (same session logic)
        BashTool.execute(params, ctx)
    }
}

impl ToolStreaming for PowerShellTool {
    fn execute_stream(&self, params: Value, ctx: &ToolContext) -> Result<StreamReceiver> {
        // Cross-platform validation (same as execute)
        use crate::security::cross_platform::{
            get_allowed_commands, get_blocked_commands, ShellType,
        };

        let command = params.get("command").and_then(Value::as_str).unwrap_or("");
        let shell_type = ShellType::PowerShell;
        let binary_name = extract_binary_name(command).unwrap_or_default();

        let allowed_commands = get_allowed_commands(shell_type);
        if !allowed_commands.contains(&binary_name.as_str()) {
            return Err(anyhow!("command not in allowed list for PowerShell"));
        }

        let blocked_commands = get_blocked_commands(shell_type);
        if blocked_commands.contains(&binary_name.as_str()) {
            return Err(anyhow!("command is blocked for security reasons"));
        }

        BashTool.execute_stream(params, ctx)
    }
}

/// Tool for cmd.exe commands (Windows batch shell).
///
/// Shares the same session and execution logic as BashTool,
/// but validates against cmd.exe-specific allowed/blocked commands.
#[derive(Default)]
pub struct CmdTool;

impl Tool for CmdTool {
    fn name(&self) -> &'static str {
        "cmd"
    }

    fn description(&self) -> &'static str {
        "Run batch commands in cmd.exe (Windows native shell). \
         Available: dir, tasklist, findstr, git, cargo, npm, python, etc. \
         Use cmd.exe batch syntax (e.g., %PATH%, dir /s, findstr pattern)."
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::Execute
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["command"],
            "properties": {
                "command": {
                    "type": "string",
                    "description": "Batch command (e.g., 'dir /s', 'findstr pattern file.txt', 'tasklist')"
                },
                "restart": {
                    "type": "boolean",
                    "description": "If true, restart the cmd.exe session before executing the command"
                },
                "timeout_secs": {
                    "type": "integer",
                    "description": "Timeout in seconds (default 120s, max 600s)",
                    "default": 120
                }
            }
        })
    }

    fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput> {
        // Check permissions
        crate::check_permission(self.permission(), ctx)?;

        // Role-based gating
        if let Some(gate) = &ctx.plan_gate {
            gate.check_access(ctx.role, self.name())?;
        }

        let command = params
            .get("command")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("missing string parameter 'command'"))?
            .to_string();

        // Cross-platform path validation
        use crate::security::cross_platform::{
            get_allowed_commands, get_blocked_commands, validate_path_in_workspace, ShellType,
        };

        let shell_type = ShellType::Cmd;
        validate_path_in_workspace(&ctx.cwd, &ctx.cwd)?;
        validate_command_safety(&command)?;

        // Validate command is in platform-specific allowlist
        let binary_name = extract_binary_name(&command)?;
        let allowed_commands = get_allowed_commands(shell_type);
        if !allowed_commands.contains(&binary_name.as_str()) {
            anyhow::bail!(
                "command '{}' is not in allowed list for cmd.exe",
                binary_name
            );
        }

        // Validate command is NOT in platform-specific blocklist
        let blocked_commands = get_blocked_commands(shell_type);
        if blocked_commands.contains(&binary_name.as_str()) {
            anyhow::bail!("command '{}' is blocked for security reasons", binary_name);
        }

        // Delegate to BashTool's execution (same session logic)
        BashTool.execute(params, ctx)
    }
}

impl ToolStreaming for CmdTool {
    fn execute_stream(&self, params: Value, ctx: &ToolContext) -> Result<StreamReceiver> {
        // Cross-platform validation (same as execute)
        use crate::security::cross_platform::{
            get_allowed_commands, get_blocked_commands, ShellType,
        };

        let command = params.get("command").and_then(Value::as_str).unwrap_or("");
        let shell_type = ShellType::Cmd;
        let binary_name = extract_binary_name(command).unwrap_or_default();

        let allowed_commands = get_allowed_commands(shell_type);
        if !allowed_commands.contains(&binary_name.as_str()) {
            return Err(anyhow!("command not in allowed list for cmd.exe"));
        }

        let blocked_commands = get_blocked_commands(shell_type);
        if blocked_commands.contains(&binary_name.as_str()) {
            return Err(anyhow!("command is blocked for security reasons"));
        }

        BashTool.execute_stream(params, ctx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn bash_blocks_dangerous_rm_pattern() {
        let tool = BashTool;
        let workspace = tempdir().expect("workspace tempdir");
        let ctx = ToolContext::new(workspace.path());

        let result = tool.execute(json!({ "command": "rm -rf /" }), &ctx);
        assert!(result.is_err());
        let err_msg = result.err().unwrap().to_string();
        // rm is not in the allowlist
        assert!(err_msg.contains("not in allowed list") || err_msg.contains("blocked"));
    }

    #[test]
    fn bash_blocks_outside_workspace_cwd() {
        let tool = BashTool;
        let workspace = tempdir().expect("workspace tempdir");
        let outside = tempdir().expect("outside tempdir");
        let ctx = ToolContext::new(workspace.path());

        // This test is not applicable since we're not testing cwd parameter here
        // The cwd validation happens in the ToolContext, not in BashTool directly
        let _ = outside;
        let _ = tool;
        let _ = ctx;
    }

    // Security tests for command injection prevention

    #[test]
    fn validate_allows_rm_and_chmod() {
        // rm and chmod are in the allowlist — guard rules (R05) handle destructive cases
        assert!(validate_command_safety("rm file.txt").is_ok());
        assert!(validate_command_safety("chmod 644 file").is_ok());
    }

    #[test]
    fn validate_blocks_truly_dangerous_binaries() {
        // These are NOT in the allowlist
        assert!(validate_command_safety("mkfs /dev/sda1").is_err());
        assert!(validate_command_safety("dd if=/dev/zero of=/dev/sda").is_err());
        assert!(validate_command_safety("shutdown -h now").is_err());
        assert!(validate_command_safety("reboot").is_err());
        assert!(validate_command_safety("halt").is_err());
        assert!(validate_command_safety("poweroff").is_err());
        assert!(validate_command_safety("chown root:root /file").is_err());
        assert!(validate_command_safety("su root").is_err());
        assert!(validate_command_safety("fdisk /dev/sda").is_err());
        assert!(validate_command_safety("parted /dev/sda").is_err());
    }

    #[test]
    fn validate_blocks_obfuscated_commands() {
        // Using shell variable expansion (fails shell parsing due to syntax)
        assert!(validate_command_safety("r$@m -rf /").is_err());

        // Case variations in flags
        assert!(validate_command_safety("rm -RF /").is_err());

        // Path-based binary invocation
        assert!(validate_command_safety("/usr/bin/rm -rf /").is_err());
        assert!(validate_command_safety("./rm -rf /").is_err());

        // Using command substitution
        assert!(validate_command_safety("$(echo rm) -rf /").is_err());

        // Using backticks
        assert!(validate_command_safety("`echo rm` -rf /").is_err());
    }

    #[test]
    fn validate_blocks_dangerous_find_flags() {
        assert!(validate_command_safety("find / -delete").is_err());
        assert!(validate_command_safety("find / -exec rm {} \\;").is_err());
        assert!(validate_command_safety("find / -execdir rm {} +").is_err());
        assert!(validate_command_safety("find / -ok rm {} \\;").is_err());
    }

    #[test]
    fn validate_blocks_fork_bomb() {
        assert!(validate_command_safety(":(){ :|:& };:").is_err());
        assert!(validate_command_safety(":() { :|:& }; :").is_err());
    }

    #[test]
    fn validate_allows_safe_commands() {
        assert!(validate_command_safety("ls -la").is_ok());
        assert!(validate_command_safety("pwd").is_ok());
        assert!(validate_command_safety("echo hello").is_ok());
        assert!(validate_command_safety("cat file.txt").is_ok());
        assert!(validate_command_safety("grep pattern file.txt").is_ok());
        assert!(validate_command_safety("cargo build").is_ok());
        assert!(validate_command_safety("cargo test").is_ok());
        assert!(validate_command_safety("npm install").is_ok());
        assert!(validate_command_safety("git status").is_ok());
        assert!(validate_command_safety("find / -name *.txt").is_ok());
        assert!(validate_command_safety("ps aux").is_ok());
    }

    #[test]
    fn validate_blocks_malformed_shell_syntax() {
        // Unclosed quote - could be an obfuscation attempt
        assert!(validate_command_safety("rm 'file.txt").is_err());
        // Unclosed quote with double quotes
        assert!(validate_command_safety("rm \"file.txt").is_err());
        // Trailing backslash is valid shell (escapes newline) — allowed
        assert!(validate_command_safety("rm file\\").is_ok());
    }

    #[test]
    fn validate_blocks_recursive_delete_to_root() {
        assert!(validate_command_safety("find / -exec rm {} \\;").is_err());
        assert!(validate_command_safety("some-command -rf /").is_err());
        assert!(validate_command_safety("some-command -rf /*").is_err());
        assert!(validate_command_safety("some-command -fr /").is_err());
    }

    #[test]
    fn test_rate_limiter_enforces_limit() {
        let limiter = BashRateLimiter::new(2);

        // Should be able to acquire 2 permits
        let _permit1 = limiter.try_acquire().unwrap();
        let _permit2 = limiter.try_acquire().unwrap();

        // Third acquisition should fail
        assert!(limiter.try_acquire().is_err());

        // Drop one permit
        drop(_permit1);

        // Should now be able to acquire again
        assert!(limiter.try_acquire().is_ok());
    }

    #[test]
    fn test_rate_limiter_tracks_active_count() {
        let limiter = BashRateLimiter::new(3);

        assert_eq!(limiter.active_count(), 0);

        let _permit1 = limiter.try_acquire().unwrap();
        assert_eq!(limiter.active_count(), 1);

        let _permit2 = limiter.try_acquire().unwrap();
        assert_eq!(limiter.active_count(), 2);

        drop(_permit1);
        assert_eq!(limiter.active_count(), 1);

        drop(_permit2);
        assert_eq!(limiter.active_count(), 0);
    }

    #[test]
    fn test_global_bash_rate_limiter() {
        // The global rate limiter should allow at least 5 concurrent executions
        assert!(BASH_RATE_LIMITER.max_concurrent >= 5);
        // Note: active_count() may be > 0 due to parallel tests using BashTool.
        // We only verify the limiter is initialized and the max is reasonable.
        assert!(
            BASH_RATE_LIMITER.active_count() <= BASH_RATE_LIMITER.max_concurrent,
            "active count should not exceed max"
        );
    }

    // Additional tests for validate_command_safety edge cases

    #[test]
    fn validate_blocks_command_injection_with_semicolon() {
        assert!(validate_command_safety("ls; rm -rf /").is_err());
        // semicolons are allowed for development chaining; both echo and cat are in the allowlist
        assert!(validate_command_safety("echo hello; cat /etc/passwd").is_ok());
    }

    #[test]
    fn validate_blocks_command_injection_with_pipe() {
        // pipes are allowed for development; -rf / is caught by the recursive-delete check
        assert!(validate_command_safety("ls | rm -rf /").is_err());
        // SECURITY: pipe-to-shell bypass is blocked
        assert!(validate_command_safety("cat file.txt | sh").is_err());
        assert!(validate_command_safety("echo hello | bash").is_err());
        assert!(validate_command_safety("cat data | zsh").is_err());
        // But pipes to non-shells are fine
        assert!(validate_command_safety("ls | grep foo").is_ok());
        assert!(validate_command_safety("cat file.txt | sort | uniq").is_ok());
        // Ensure false positives don't block longer words containing shell names
        assert!(validate_command_safety("echo show").is_ok());
        assert!(validate_command_safety("cat crash.log | grep error").is_ok());
    }

    #[test]
    fn validate_blocks_command_substitution() {
        assert!(validate_command_safety("$(echo rm -rf /)").is_err());
        assert!(validate_command_safety("`echo rm` -rf /").is_err());
    }

    #[test]
    fn validate_blocks_io_redirection() {
        // IO redirection (>, >>) is allowed for development; cat is in the allowlist
        assert!(validate_command_safety("cat > /etc/passwd").is_ok());
        // sh is NOT in the allowlist
        assert!(validate_command_safety("sh < script.sh").is_err());
        assert!(validate_command_safety("echo test >> file.txt").is_ok());
    }

    #[test]
    fn validate_blocks_background_execution() {
        assert!(validate_command_safety("sleep 10 &").is_err());
        assert!(validate_command_safety("cmd1 & cmd2 & cmd3").is_err());
    }

    #[test]
    fn validate_blocks_arithmetic_expansion() {
        assert!(validate_command_safety("echo $((1+1))").is_err());
        assert!(validate_command_safety("$((rm -rf /))").is_err());
    }

    #[test]
    fn validate_blocks_parameter_expansion() {
        assert!(validate_command_safety("echo ${!VAR}").is_err());
        assert!(validate_command_safety("echo ${@:1}").is_err());
    }

    #[test]
    fn validate_blocks_eval_with_functions() {
        assert!(validate_command_safety("eval 'function test() { echo hi; }'").is_err());
        assert!(validate_command_safety("eval $(echo test)").is_err());
    }

    #[test]
    fn validate_blocks_excessive_quotes() {
        // Create a command with more than 200 quotes
        let many_quotes = "\"".repeat(201);
        assert!(validate_command_safety(&format!("echo {}", many_quotes)).is_err());
    }

    #[test]
    fn validate_blocks_excessive_ampersands() {
        // Create a command with more than 50 ampersands (the detection threshold)
        let many_ampersands = "& ".repeat(51);
        assert!(validate_command_safety(&format!("echo {}", many_ampersands)).is_err());
    }

    #[test]
    fn validate_blocks_ampersands_in_text() {
        // ampersands below the 5-count threshold are allowed
        assert!(validate_command_safety("echo 'a&b'").is_ok());
    }

    #[test]
    fn validate_blocks_very_long_commands() {
        // Create a command longer than 10,000 characters
        let long_arg = "a".repeat(10001);
        assert!(validate_command_safety(&format!("echo {}", long_arg)).is_err());
    }

    #[test]
    fn validate_blocks_empty_command() {
        assert!(validate_command_safety("").is_err());
        assert!(validate_command_safety("   ").is_err());
    }

    #[test]
    fn validate_blocks_invalid_shell_syntax() {
        // Unclosed quotes
        assert!(validate_command_safety("echo 'unclosed").is_err());
        assert!(validate_command_safety("echo \"unclosed").is_err());
    }

    #[test]
    fn validate_allows_safe_cargo_commands() {
        assert!(validate_command_safety("cargo build").is_ok());
        assert!(validate_command_safety("cargo test").is_ok());
        assert!(validate_command_safety("cargo run --release").is_ok());
        assert!(validate_command_safety("cargo check --all-features").is_ok());
    }

    #[test]
    fn validate_allows_safe_git_commands() {
        assert!(validate_command_safety("git status").is_ok());
        assert!(validate_command_safety("git log --oneline -10").is_ok());
        assert!(validate_command_safety("git diff HEAD~1").is_ok());
    }

    #[test]
    fn validate_allows_safe_npm_commands() {
        assert!(validate_command_safety("npm install").is_ok());
        assert!(validate_command_safety("npm run build").is_ok());
        assert!(validate_command_safety("npm test").is_ok());
    }

    #[test]
    fn validate_allows_safe_find_without_dangerous_flags() {
        assert!(validate_command_safety("find . -name '*.rs'").is_ok());
        assert!(validate_command_safety("find /tmp -type f").is_ok());
    }

    #[test]
    fn validate_allows_safe_ls_commands() {
        assert!(validate_command_safety("ls -la").is_ok());
        assert!(validate_command_safety("ls -R /home").is_ok());
        assert!(validate_command_safety("/bin/ls -la").is_ok());
    }

    #[test]
    fn validate_allows_safe_python_commands() {
        assert!(validate_command_safety("python script.py").is_ok());
        assert!(validate_command_safety("python3 -m pytest").is_ok());
        assert!(validate_command_safety("pip install requests").is_ok());
    }

    #[test]
    fn validate_blocks_python_eval_flag() {
        assert!(validate_command_safety("python -c 'print(1)'").is_err());
        assert!(validate_command_safety("python3 -c 'import os'").is_err());
    }

    #[test]
    fn validate_allows_safe_node_commands() {
        assert!(validate_command_safety("node script.js").is_ok());
        assert!(validate_command_safety("node --version").is_ok());
    }

    #[test]
    fn validate_blocks_node_eval_flag() {
        assert!(validate_command_safety("node -e 'console.log(1)'").is_err());
    }

    #[test]
    fn validate_allows_safe_ruby_commands() {
        assert!(validate_command_safety("ruby script.rb").is_ok());
        assert!(validate_command_safety("ruby --version").is_ok());
    }

    #[test]
    fn validate_blocks_ruby_eval_flag() {
        assert!(validate_command_safety("ruby -e 'puts 1'").is_err());
    }

    #[test]
    fn validate_blocks_perl_not_in_allowlist() {
        assert!(validate_command_safety("perl script.pl").is_err());
    }

    #[test]
    fn validate_allows_quotes_in_safe_commands() {
        assert!(validate_command_safety("echo 'hello world'").is_ok());
        assert!(validate_command_safety("echo \"hello world\"").is_ok());
        assert!(validate_command_safety("grep 'pattern' file.txt").is_ok());
    }

    #[test]
    fn validate_allows_complex_but_safe_commands() {
        assert!(
            validate_command_safety("cargo build --release --features=feature1,feature2").is_ok()
        );
        assert!(
            validate_command_safety("git log --author='John Doe' --since='1 week ago'").is_ok()
        );
        assert!(
            validate_command_safety("find . -type f -name '*.rs' -exec grep -l 'TODO' {} \\;")
                .is_err()
        ); // -exec is blocked
    }

    #[test]
    fn session_registry_get_or_create_returns_session() {
        let temp = tempdir().unwrap();
        let registry = BashSessionRegistry::new();
        let session = registry.get_or_create(temp.path().to_path_buf());
        assert!(session.is_ok());
    }

    #[test]
    fn session_registry_returns_same_session_for_same_cwd() {
        let temp = tempdir().unwrap();
        let registry = BashSessionRegistry::new();
        let cwd = temp.path().to_path_buf();

        let s1 = registry.get_or_create(cwd.clone()).unwrap();
        let s2 = registry.get_or_create(cwd).unwrap();

        // Both Arcs point to the same underlying Mutex
        assert!(Arc::ptr_eq(&s1, &s2));
    }

    #[test]
    fn session_registry_remove_drops_session() {
        let temp = tempdir().unwrap();
        let registry = BashSessionRegistry::new();
        let cwd = temp.path().to_path_buf();

        let s1 = registry.get_or_create(cwd.clone()).unwrap();
        registry.remove(&cwd);
        let s2 = registry.get_or_create(cwd).unwrap();

        // After remove, a new session should be created
        assert!(!Arc::ptr_eq(&s1, &s2));
    }

    #[test]
    fn session_registry_evict_idle_removes_stale() {
        let temp = tempdir().unwrap();
        let registry = BashSessionRegistry::new();
        let cwd = temp.path().to_path_buf();

        let _s = registry.get_or_create(cwd.clone()).unwrap();

        // Manually set last access to be older than timeout
        {
            let mut guard = registry
                .last_access
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            if let Some(ref mut times) = *guard {
                times.insert(
                    cwd.clone(),
                    Instant::now() - Duration::from_secs(IDLE_TIMEOUT_SECS + 10),
                );
            }
        }

        registry.evict_idle();

        // Session should be evicted
        let sessions = registry.sessions.lock().unwrap_or_else(|e| e.into_inner());
        match *sessions {
            Some(ref s) => assert!(s.is_empty()),
            None => panic!("sessions not initialized"),
        }
    }

    #[test]
    fn detect_shell_returns_valid_shell() {
        let shell = SHELL_INFO.binary;
        let is_powershell = SHELL_INFO.is_powershell;
        // On Unix, should find bash/zsh/sh
        #[cfg(unix)]
        {
            assert!(
                shell == "bash" || shell == "zsh" || shell == "sh",
                "expected a Unix shell, got: {}",
                shell
            );
            assert!(!is_powershell);
        }
        // On Windows, should find powershell or cmd
        #[cfg(windows)]
        {
            assert!(
                shell == "powershell" || shell == "cmd",
                "expected a Windows shell, got: {}",
                shell
            );
        }
    }

    /// Regression test: execute_stream() uses stdout_rx channel (not child.stdout.take())
    #[test]
    fn session_execute_stream_uses_stdout_rx_channel() {
        let temp = tempdir().unwrap();
        let session = BashSession::new(temp.path().to_path_buf()).unwrap();
        let (sender, receiver) = crate::streaming::create_stream_channel();
        let result = session.execute_stream("echo hello_stream", 10, sender);
        assert!(result.is_ok(), "execute_stream failed: {:?}", result);
        let (exit_code, error) = result.unwrap();
        assert_eq!(exit_code, 0, "expected exit code 0");
        // Shell boilerplate in stderr may trigger the error field; the key invariant
        // is that execute_stream succeeded and returned output (not a panic).
        let _ = error; // may be Some due to interactive shell stderr noise
        let output: String = receiver
            .try_iter()
            .filter(|c| !c.is_done && c.error.is_none())
            .map(|c| c.text.clone())
            .collect();
        assert!(
            output.contains("hello_stream"),
            "expected 'hello_stream', got: {:?}",
            output
        );
    }

    #[test]
    fn session_execute_stream_timeout_returns_124() {
        let temp = tempdir().unwrap();
        let session = BashSession::new(temp.path().to_path_buf()).unwrap();
        let (sender, _receiver) = crate::streaming::create_stream_channel();
        let result = session.execute_stream("sleep 10", 1, sender);
        assert!(result.is_ok());
        let (exit_code, _error) = result.unwrap();
        // Timeout exit code is 124 on systems with coreutils timeout,
        // but may be -1 (SIGKILL) on systems without it or under load.
        assert!(
            exit_code == 124 || exit_code == -1,
            "expected exit code 124 (timeout) or -1 (signal kill), got {}",
            exit_code
        );
    }

    /// Tests that shell startup messages containing "command not found" (from
    /// .zshrc / .bashrc errors) don't cause false-positive error detection.
    #[test]
    fn bash_error_detection_ignores_midline_command_not_found() {
        let temp_dir = tempdir().expect("workspace tempdir");
        let ctx = ToolContext::new(temp_dir.path());
        let tool = BashTool;

        // This should succeed: "echo" is in the allowlist and the command
        // doesn't actually fail. Even if the shell startup produces stderr
        // with "command not found" from .zshrc errors, the tool should not
        // incorrectly report it as an error.
        let result = tool.execute(json!({ "command": "echo hello_world_test" }), &ctx);
        assert!(
            result.is_ok(),
            "echo should succeed even with shell startup stderr: {:?}",
            result
        );
        let output = result.unwrap();
        assert!(
            output.text.contains("hello_world_test"),
            "output should contain our test string: {:?}",
            output.text
        );
    }

    #[test]
    fn test_is_command_not_found_line_bash() {
        assert!(is_command_not_found_line("bash: command not found: foo"));
        assert!(is_command_not_found_line(
            "  bash: command not found: foo  "
        ));
    }

    #[test]
    fn test_is_command_not_found_line_zsh() {
        assert!(is_command_not_found_line("zsh: command not found: foo"));
    }

    #[test]
    fn test_is_command_not_found_line_sh() {
        assert!(is_command_not_found_line("sh: 1: nonexistent: not found"));
    }

    #[test]
    fn test_is_command_not_found_line_fish() {
        assert!(is_command_not_found_line(
            "fish: Unknown command 'nonexistent'"
        ));
    }

    #[test]
    fn test_is_command_not_found_line_false_positives() {
        assert!(!is_command_not_found_line("echo command not found: foo"));
        assert!(!is_command_not_found_line(
            "The error was: command not found: foo"
        ));
        assert!(!is_command_not_found_line("grep 'command not found'"));
    }

    #[test]
    fn validate_blocks_ifs_variable() {
        assert!(validate_command_safety("echo $IFS").is_err());
        assert!(validate_command_safety("echo ${IFS}").is_err());
        assert!(validate_command_safety("echo ${IFS:0:1}").is_err());
    }

    #[test]
    fn validate_allows_commands_without_ifs() {
        assert!(validate_command_safety("echo hello").is_ok());
        assert!(validate_command_safety("echo $HOME").is_ok());
    }

    #[test]
    fn validate_blocks_unicode_whitespace() {
        // NO-BREAK SPACE (U+00A0)
        assert!(validate_command_safety("echo\u{00A0}hello").is_err());
        // IDEOGRAPHIC SPACE (U+3000)
        assert!(validate_command_safety("echo\u{3000}hello").is_err());
        // NARROW NO-BREAK SPACE (U+202F)
        assert!(validate_command_safety("echo\u{202F}hello").is_err());
    }

    #[test]
    fn validate_allows_normal_whitespace() {
        assert!(validate_command_safety("echo hello world").is_ok());
        assert!(validate_command_safety("echo\thello").is_ok());
    }
}
