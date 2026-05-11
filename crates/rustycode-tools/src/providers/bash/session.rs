//! Persistent bash shell session management.
//!
//! [`BashSession`] wraps a long-lived child shell process and communicates
//! with it via stdin/stdout pipes plus a background stderr drain thread.

use crate::streaming::{StreamChunk, StreamSender};
use anyhow::{anyhow, Result};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::subprocess::SHELL_INFO;

/// Check if a line is a shell "command not found" error.
/// Handles bash, zsh, sh (dash/ash), and fish shell error formats.
pub(super) fn is_command_not_found_line(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with("command not found:")
        || trimmed.starts_with("zsh: command not found")
        || trimmed.starts_with("bash: command not found")
        || trimmed.starts_with("sh: ") && trimmed.contains(": not found")
        || trimmed.starts_with("fish: Unknown command")
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

pub(super) fn filter_shell_boilerplate(text: &str) -> String {
    text.lines()
        .filter(|line| !is_shell_boilerplate(line.trim()))
        .collect::<Vec<_>>()
        .join("\n")
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
    pub(super) child: Arc<Mutex<Option<Child>>>,
    pub(super) cwd: PathBuf,
    pub(super) _session_id: String,
    pub(super) stderr_buffer: Arc<Mutex<String>>,
    pub(super) stdout_rx: Arc<Mutex<std::sync::mpsc::Receiver<String>>>,
}

impl BashSession {
    pub fn new(cwd: PathBuf) -> Result<Self> {
        let session_id = uuid::Uuid::new_v4().to_string();
        let shell = SHELL_INFO.binary;
        let interactive_flag = SHELL_INFO.interactive_flag;

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
                tracing::debug!(session_id = %id, "stdout reader thread exited");
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

    pub fn restart(&mut self) -> Result<()> {
        if let Some(mut child) = self
            .child
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
        {
            let _ = child.kill();
            let _ = child.wait();
        }
        if let Ok(mut b) = self.stderr_buffer.lock() {
            b.clear();
        }

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
            });
        }

        self.stdout_rx = Arc::new(Mutex::new(stdout_rx));
        *self
            .child
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(new_child);
        Ok(())
    }

    pub fn execute(&self, command: &str, timeout_secs: u64) -> Result<(String, String, i32)> {
        {
            let mut child_guard = self
                .child
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let child = child_guard
                .as_mut()
                .ok_or_else(|| anyhow!("bash session not available"))?;

            let wrapped_command = if timeout_secs > 0 {
                format!("timeout {timeout_secs} {command}")
            } else {
                command.to_string()
            };

            if let Some(stdin) = child.stdin.as_mut() {
                writeln!(stdin, "{wrapped_command}")
                    .map_err(|e| anyhow!("failed to write command: {e}"))?;
                writeln!(stdin, "echo $?")
                    .map_err(|e| anyhow!("failed to write exit code query: {e}"))?;
                writeln!(stdin, "echo '---END---'")
                    .map_err(|e| anyhow!("failed to write delimiter: {e}"))?;
                stdin
                    .flush()
                    .map_err(|e| anyhow!("failed to flush stdin: {e}"))?;
            } else {
                return Err(anyhow!("shell stdin not available"));
            }
        }

        let stdout_rx = self
            .stdout_rx
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let mut output_lines = Vec::new();
        let mut exit_code_line = String::new();
        let mut read_timed_out = false;

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
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }

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
                    }
                    thread::sleep(Duration::from_millis(100));
                    if let Ok(status) = child.try_wait() {
                        if status.is_none() {
                            tracing::warn!("bash child still alive after SIGINT, sending kill");
                            let _ = child.kill();
                            let _ = child.wait();
                        }
                    }
                }
            }
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
        let exit_code: i32 = exit_code_line.trim().parse().unwrap_or(-1);

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

    pub fn execute_stream(
        &self,
        command: &str,
        timeout_secs: u64,
        sender: StreamSender,
    ) -> Result<(i32, Option<String>)> {
        {
            let mut child_guard = self
                .child
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let child = child_guard
                .as_mut()
                .ok_or_else(|| anyhow!("shell session not available"))?;

            if let Some(stdin) = child.stdin.as_mut() {
                let wrapped_command = if timeout_secs > 0 {
                    format!("timeout {timeout_secs} {command}")
                } else {
                    command.to_string()
                };
                writeln!(stdin, "{wrapped_command}")
                    .map_err(|e| anyhow!("failed to write command: {e}"))?;
                writeln!(stdin, "echo $?")
                    .map_err(|e| anyhow!("failed to write exit code query: {e}"))?;
                writeln!(stdin, "echo '---END---'")
                    .map_err(|e| anyhow!("failed to write delimiter: {e}"))?;
                stdin
                    .flush()
                    .map_err(|e| anyhow!("failed to flush stdin: {e}"))?;
            } else {
                return Err(anyhow!("shell stdin not available"));
            }
        }

        let stdout_rx = self
            .stdout_rx
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let mut exit_code_line = String::new();
        let mut read_timed_out = false;

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
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }

        if read_timed_out {
            if let Ok(mut child_guard) = self.child.lock() {
                if let Some(child) = child_guard.as_mut() {
                    if let Some(stdin) = child.stdin.as_mut() {
                        let _ = stdin.write_all(b"\x03\n");
                        let _ = stdin.flush();
                    }
                    thread::sleep(Duration::from_millis(100));
                    if let Ok(status) = child.try_wait() {
                        if status.is_none() {
                            tracing::warn!(
                                "bash child still alive after SIGINT in streaming, sending kill"
                            );
                            let _ = child.kill();
                            let _ = child.wait();
                        }
                    }
                }
            }
            while stdout_rx.try_recv().is_ok() {}
            let _ = sender.send(StreamChunk::done());
            return Ok((
                124,
                Some("command timed out - output may be incomplete".to_string()),
            ));
        }

        thread::sleep(Duration::from_millis(200));
        let raw_stderr = if let Ok(mut buf) = self.stderr_buffer.lock() {
            let s = buf.clone();
            buf.clear();
            s
        } else {
            String::new()
        };
        let stderr = filter_shell_boilerplate(&raw_stderr);

        if !stderr.is_empty() {
            let chunk = StreamChunk::new(format!("[stderr] {stderr}\n"));
            sender
                .send(chunk)
                .map_err(|e| anyhow!("failed to send stderr chunk: {e}"))?;
        }

        let exit_code: i32 = exit_code_line.trim().parse().unwrap_or(-1);

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

        let _ = sender.send(StreamChunk::done());
        Ok((exit_code, error))
    }
}

impl Drop for BashSession {
    fn drop(&mut self) {
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
