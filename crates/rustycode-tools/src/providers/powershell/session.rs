//! PowerShell session management and PTY lifecycle.
//!
//! Provides persistent PowerShell process with stdin/stdout/stderr handling,
//! delimiter-based command framing, and exit code detection.

use crate::telemetry::streaming::{StreamChunk, StreamSender};
use anyhow::{anyhow, Result};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use super::filter::is_ps_boilerplate;

/// Persistent PowerShell session that maintains shell state across commands.
///
/// Spawns `pwsh -NoLogo -NoProfile -NoExit -Command -` for an interactive
/// stdin-driven session. Uses `Write-Output` delimiters and `$LASTEXITCODE`
/// for command boundary detection.
#[derive(Debug)]
pub struct PowerShellSession {
    pub child: Arc<Mutex<Option<Child>>>,
    #[allow(dead_code)]
    pub cwd: PathBuf,
    _session_id: String,
    stderr_buffer: Arc<Mutex<String>>,
    stdout_rx: Arc<Mutex<std::sync::mpsc::Receiver<String>>>,
}

/// PowerShell edition inferred from the binary name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PSEdition {
    /// PowerShell Core 7+ (`pwsh`). Supports `&&`, `||`, null-coalescing, ternary.
    Core,
    /// Windows PowerShell 5.1 (`powershell`). No `&&`/`||` operators.
    Desktop,
}

impl PSEdition {
    /// Whether this edition supports chain operators (`&&`, `||`).
    pub const fn supports_chain_operators(self) -> bool {
        matches!(self, Self::Core)
    }
}

/// Cached result of PowerShell binary detection.
static CACHED_PWSH: std::sync::OnceLock<Option<&'static str>> = std::sync::OnceLock::new();

/// Detect `pwsh` binary availability. Returns the binary name if found.
/// Result is cached after first call.
pub fn find_pwsh() -> Option<&'static str> {
    *CACHED_PWSH.get_or_init(detect_pwsh_uncached)
}

fn detect_pwsh_uncached() -> Option<&'static str> {
    // Prefer pwsh (PowerShell Core, cross-platform)
    if probe_pwsh("pwsh") {
        return Some("pwsh");
    }

    // Snap workaround on Linux: the `pwsh` snap wrapper may not be on PATH,
    // but the real binary exists at a known location.
    #[cfg(unix)]
    {
        for path in &[
            "/snap/pwsh/current/usr/bin/pwsh",
            "/opt/microsoft/powershell/7/pwsh",
        ] {
            if std::path::Path::new(path).exists() && probe_pwsh(path) {
                return Some(*path);
            }
        }
    }

    // Windows PowerShell fallback (Windows only)
    #[cfg(windows)]
    {
        if probe_pwsh("PowerShell") {
            return Some("PowerShell");
        }
    }
    None
}

/// Probe a PowerShell binary to see if it starts successfully.
pub fn probe_pwsh(binary: &str) -> bool {
    Command::new(binary)
        .args(["-NoLogo", "-NoProfile", "-Command", "exit 0"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Determine the PowerShell edition from the detected binary name.
/// No spawning needed — `pwsh` = Core (7+), `powershell` = Desktop (5.1).
pub fn ps_edition() -> Option<PSEdition> {
    match find_pwsh()? {
        "pwsh" => Some(PSEdition::Core),
        "PowerShell" => Some(PSEdition::Desktop),
        _ => None,
    }
}

/// Detect the PowerShell version string by running `$PSVersionTable.PSVersion`.
pub fn detect_ps_version() -> Option<String> {
    let binary = find_pwsh()?;
    let output = Command::new(binary)
        .args([
            "-NoLogo",
            "-NoProfile",
            "-Command",
            "$PSVersionTable.PSVersion.ToString()",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if version.is_empty() {
        None
    } else {
        Some(version)
    }
}

impl PowerShellSession {
    /// Create a new persistent PowerShell session.
    pub fn new(cwd: PathBuf) -> Result<Self> {
        let pwsh = find_pwsh().ok_or_else(|| {
            anyhow!(
                "PowerShell not found. Install PowerShell Core: \
                 https://learn.microsoft.com/powershell/scripting/install/installing-powershell"
            )
        })?;

        let session_id = uuid::Uuid::new_v4().to_string();

        let mut child = Command::new(pwsh)
            .args(["-NoLogo", "-NoProfile", "-NoExit", "-Command", "-"])
            .current_dir(&cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| anyhow!("failed to spawn {pwsh}: {e}"))?;

        // Persistent stderr drain thread
        let stderr_buffer: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));
        if let Some(stderr_handle) = child.stderr.take() {
            let buf = stderr_buffer.clone();
            let id = session_id.clone();
            thread::spawn(move || {
                tracing::debug!(session_id = %id, "pwsh stderr drain thread started");
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
                tracing::debug!(session_id = %id, "pwsh stderr drain thread exited");
            });
        }

        // Persistent stdout reader thread
        let (stdout_tx, stdout_rx) = std::sync::mpsc::channel::<String>();
        if let Some(stdout_handle) = child.stdout.take() {
            let id = session_id.clone();
            thread::spawn(move || {
                tracing::debug!(session_id = %id, "pwsh stdout reader thread started");
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
                tracing::debug!(session_id = %id, "pwsh stdout reader thread exited");
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

    /// Restart the session with a fresh `pwsh` process.
    #[allow(dead_code)]
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

        let pwsh = find_pwsh().ok_or_else(|| anyhow!("PowerShell not found"))?;
        let mut new_child = Command::new(pwsh)
            .args(["-NoLogo", "-NoProfile", "-NoExit", "-Command", "-"])
            .current_dir(&self.cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| anyhow!("failed to restart {pwsh}: {e}"))?;

        // Re-spawn stderr drain
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

        // Re-spawn stdout reader
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

    /// Execute a command and return (stdout, stderr, exit_code).
    pub fn execute(&self, command: &str, timeout_secs: u64) -> Result<(String, String, i32)> {
        {
            let mut child_guard = self
                .child
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let child = child_guard
                .as_mut()
                .ok_or_else(|| anyhow!("pwsh session not available"))?;

            if let Some(stdin) = child.stdin.as_mut() {
                // Write command
                writeln!(stdin, "{command}")
                    .map_err(|e| anyhow!("failed to write command: {e}"))?;
                // Write exit code query — smart fallback: prefer $LASTEXITCODE for
                // native exes (git, node), fall back to $? for cmdlet-only pipelines.
                // PS 5.1 bug: native commands writing to stderr set $? = $false even
                // on exit 0, so $LASTEXITCODE is more reliable.
                writeln!(stdin, "$_ec = if ($null -ne $LASTEXITCODE) {{ $LASTEXITCODE }} elseif ($?) {{ 0 }} else {{ 1 }}; Write-Output $_ec")
                    .map_err(|e| anyhow!("failed to write exit code query: {e}"))?;
                // Write delimiter
                writeln!(stdin, "Write-Output '---END---'")
                    .map_err(|e| anyhow!("failed to write delimiter: {e}"))?;
                stdin
                    .flush()
                    .map_err(|e| anyhow!("failed to flush stdin: {e}"))?;
            } else {
                return Err(anyhow!("pwsh stdin not available"));
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

        // Drain stderr
        thread::sleep(Duration::from_millis(200));
        let raw_stderr = if let Ok(mut buf) = self.stderr_buffer.lock() {
            let s = buf.clone();
            buf.clear();
            s
        } else {
            String::new()
        };
        let stderr = super::filter::filter_ps_boilerplate(&raw_stderr);

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
                            tracing::warn!("pwsh child still alive after Ctrl+C, sending kill");
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

        output_lines.retain(|line| !is_ps_boilerplate(line.trim()));

        let stdout = output_lines.join("\n");
        let exit_code: i32 = exit_code_line.trim().parse().unwrap_or(-1);

        Ok((stdout, stderr, exit_code))
    }

    /// Execute a command with streaming output.
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
                .ok_or_else(|| anyhow!("pwsh session not available"))?;

            if let Some(stdin) = child.stdin.as_mut() {
                writeln!(stdin, "{command}")
                    .map_err(|e| anyhow!("failed to write command: {e}"))?;
                writeln!(stdin, "Write-Output $LASTEXITCODE")
                    .map_err(|e| anyhow!("failed to write exit code query: {e}"))?;
                writeln!(stdin, "Write-Output '---END---'")
                    .map_err(|e| anyhow!("failed to write delimiter: {e}"))?;
                stdin
                    .flush()
                    .map_err(|e| anyhow!("failed to flush stdin: {e}"))?;
            } else {
                return Err(anyhow!("pwsh stdin not available"));
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
                        if is_ps_boilerplate(line.trim()) {
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
                                "pwsh child still alive after Ctrl+C in streaming, killing"
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
        let stderr = super::filter::filter_ps_boilerplate(&raw_stderr);

        if !stderr.is_empty() {
            let chunk = StreamChunk::new(format!("[stderr] {stderr}\n"));
            sender
                .send(chunk)
                .map_err(|e| anyhow!("failed to send stderr chunk: {e}"))?;
        }

        let exit_code: i32 = exit_code_line.trim().parse().unwrap_or(-1);

        let error = if stderr.contains("command not found") || stderr.contains("is not recognized")
        {
            Some(format!("command not found: {command}"))
        } else if stderr.contains("Permission denied") || stderr.contains("Access is denied") {
            Some(format!("permission denied: {command}"))
        } else {
            None
        };

        let _ = sender.send(StreamChunk::done());
        Ok((exit_code, error))
    }
}

impl Drop for PowerShellSession {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn pwsh_available() -> bool {
        find_pwsh().is_some()
    }

    #[test]
    #[ignore = "requires functional PowerShell session; unreliable on CI Linux"]
    fn test_session_spawn_and_execute() {
        if !pwsh_available() {
            eprintln!("skipping: pwsh not installed");
            return;
        }

        let dir = std::env::current_dir().unwrap();
        let session = PowerShellSession::new(dir).expect("failed to create pwsh session");

        let (stdout, stderr, exit_code) = session
            .execute("Write-Output 'hello from pwsh'", 30)
            .expect("execute failed");

        assert_eq!(exit_code, 0, "stderr: {stderr}");
        assert!(stdout.contains("hello from pwsh"), "stdout was: {stdout}");
    }

    #[test]
    #[ignore = "requires functional PowerShell session; unreliable on CI Linux"]
    fn test_session_exit_code() {
        if !pwsh_available() {
            eprintln!("skipping: pwsh not installed");
            return;
        }

        let dir = std::env::current_dir().unwrap();
        let session = PowerShellSession::new(dir).unwrap();

        let (stdout, stderr, exit_code) = session.execute("exit 42", 30).expect("execute failed");

        assert_eq!(exit_code, 42, "stdout: {stdout}, stderr: {stderr}");
    }

    #[test]
    #[ignore = "requires functional PowerShell session; unreliable on CI Linux"]
    fn test_session_environment_persistence() {
        if !pwsh_available() {
            eprintln!("skipping: pwsh not installed");
            return;
        }

        let dir = std::env::current_dir().unwrap();
        let session = PowerShellSession::new(dir).unwrap();

        // Set a variable
        let (stdout, _, exit_code) = session
            .execute("$env:_RUSTYCODE_TEST = 'hello'", 30)
            .unwrap();
        assert_eq!(exit_code, 0, "set var failed, stdout: {stdout}");

        // Read it back
        let (stdout, _, exit_code) = session
            .execute("Write-Output $env:_RUSTYCODE_TEST", 30)
            .unwrap();
        assert_eq!(exit_code, 0);
        assert!(stdout.contains("hello"), "stdout was: {stdout}");

        // Clean up
        let _ = session.execute("Remove-Item Env:_RUSTYCODE_TEST", 10);
    }

    #[test]
    fn test_session_restart() {
        if !pwsh_available() {
            eprintln!("skipping: pwsh not installed");
            return;
        }

        let dir = std::env::current_dir().unwrap();
        let mut session = PowerShellSession::new(dir).unwrap();

        // Set a variable
        let _ = session.execute("$env:_RUSTYCODE_RESTART_TEST = 'before'", 10);

        // Restart
        session.restart().expect("restart failed");

        // Variable should be gone
        let (stdout, _, _) = session
            .execute("Write-Output $env:_RUSTYCODE_RESTART_TEST", 10)
            .unwrap();
        // Should be empty (env var doesn't exist in new session)
        assert!(
            !stdout.contains("before"),
            "variable survived restart, stdout: {stdout}"
        );
    }

    #[test]
    fn test_session_not_found() {
        if !pwsh_available() {
            eprintln!("skipping: pwsh not installed");
            return;
        }

        let dir = std::env::current_dir().unwrap();
        let session = PowerShellSession::new(dir).unwrap();

        let result = session.execute("Get-NonExistentCmdlet12345", 10);
        // Should complete (pwsh writes error to stderr) but may have non-zero exit
        // or error in stderr — just verify it doesn't panic
        assert!(result.is_ok(), "execute should not panic on bad commands");
    }

    #[test]
    fn test_graceful_no_pwsh() {
        // PowerShellSession::new should return a clear error if pwsh not found.
        // We can't easily test this if pwsh IS installed, but we verify the
        // error message is helpful.
        if pwsh_available() {
            // If pwsh is available, just verify the session works
            let dir = std::env::current_dir().unwrap();
            let session = PowerShellSession::new(dir);
            assert!(
                session.is_ok(),
                "session should work when pwsh is available"
            );
        } else {
            let dir = std::env::current_dir().unwrap();
            let err = PowerShellSession::new(dir).unwrap_err();
            assert!(
                err.to_string().contains("PowerShell not found"),
                "error should mention PowerShell not found: {err}"
            );
        }
    }
}
