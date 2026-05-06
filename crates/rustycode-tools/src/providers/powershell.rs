#![allow(clippy::doc_markdown)]

//! PowerShell (pwsh) tool — persistent session with PS-native protocol.
//!
//! Provides a dedicated `pwsh` session separate from `BashTool`, with:
//! - PowerShell Core (`pwsh`) as the shell binary
//! - `Write-Output` delimiters and `$LASTEXITCODE` exit codes
//! - Wall-clock timeouts (no Unix `timeout` command)
//! - PS-specific boilerplate filtering
//! - Case-insensitive cmdlet validation

use crate::streaming::{StreamChunk, StreamReceiver, StreamSender, ToolStreaming};
use crate::truncation::truncate_bash_output;
use crate::{Tool, ToolContext, ToolOutput, ToolPermission, ToolTag};
use anyhow::{anyhow, Result};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

// PowerShell session

/// Persistent PowerShell session that maintains shell state across commands.
///
/// Spawns `pwsh -NoLogo -NoProfile -NoExit -Command -` for an interactive
/// stdin-driven session. Uses `Write-Output` delimiters and `$LASTEXITCODE`
/// for command boundary detection.
#[derive(Debug)]
pub struct PowerShellSession {
    child: Arc<Mutex<Option<Child>>>,
    #[allow(dead_code)]
    cwd: PathBuf,
    _session_id: String,
    stderr_buffer: Arc<Mutex<String>>,
    stdout_rx: Arc<Mutex<std::sync::mpsc::Receiver<String>>>,
}

/// PS-specific boilerplate lines to filter from output.
fn is_ps_boilerplate(trimmed: &str) -> bool {
    trimmed.starts_with("PowerShell")
        || trimmed.starts_with("Windows PowerShell")
        || trimmed.starts_with("PS ")
        || trimmed.contains("> Write-Host")
        || trimmed.contains(">> ")
        // Delimiter and exit-code query echoed back
        || trimmed.contains("Write-Output '---END---'")
        || trimmed.contains("Write-Output $LASTEXITCODE")
}

fn filter_ps_boilerplate(text: &str) -> String {
    text.lines()
        .filter(|line| !is_ps_boilerplate(line.trim()))
        .collect::<Vec<_>>()
        .join("\n")
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
        if probe_pwsh("powershell") {
            return Some("powershell");
        }
    }
    None
}

/// Probe a PowerShell binary to see if it starts successfully.
fn probe_pwsh(binary: &str) -> bool {
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
        "powershell" => Some(PSEdition::Desktop),
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
    fn new(cwd: PathBuf) -> Result<Self> {
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
    fn restart(&mut self) -> Result<()> {
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
    fn execute(&self, command: &str, timeout_secs: u64) -> Result<(String, String, i32)> {
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
        let stderr = filter_ps_boilerplate(&raw_stderr);

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
    fn execute_stream(
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
        let stderr = filter_ps_boilerplate(&raw_stderr);

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

// Session registry (same pattern as BashSessionRegistry)

struct PSSessionRegistry {
    sessions: Mutex<Option<HashMap<PathBuf, Arc<Mutex<PowerShellSession>>>>>,
    last_access: Mutex<Option<HashMap<PathBuf, Instant>>>,
}

const PS_IDLE_TIMEOUT_SECS: u64 = 300;

impl PSSessionRegistry {
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

    fn get_or_create(&self, cwd: PathBuf) -> Result<Arc<Mutex<PowerShellSession>>> {
        self.ensure_init();

        {
            if let Some(ref mut times) = *self
                .last_access
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
            {
                times.insert(cwd.clone(), Instant::now());
            }
        }

        let sessions_guard = self
            .sessions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(ref sessions) = *sessions_guard {
            if let Some(session) = sessions.get(&cwd) {
                return Ok(Arc::clone(session));
            }
        }

        drop(sessions_guard);
        let session = Arc::new(Mutex::new(PowerShellSession::new(cwd.clone())?));

        let mut sessions_guard = self
            .sessions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(ref mut sessions) = *sessions_guard {
            if let Some(existing) = sessions.get(&cwd) {
                return Ok(Arc::clone(existing));
            }
            sessions.insert(cwd, Arc::clone(&session));
        }

        Ok(session)
    }

    fn remove(&self, cwd: &Path) -> Option<Arc<Mutex<PowerShellSession>>> {
        self.ensure_init();
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
        if let Some(ref mut times) = *self
            .last_access
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
        {
            times.remove(cwd);
        }
        removed
    }

    fn evict_idle(&self) {
        self.ensure_init();
        let now = Instant::now();
        let to_evict: Vec<PathBuf> = {
            let times_guard = self
                .last_access
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            match *times_guard {
                Some(ref times) => times
                    .iter()
                    .filter(|(_, last)| now.duration_since(**last).as_secs() > PS_IDLE_TIMEOUT_SECS)
                    .map(|(p, _)| p.clone())
                    .collect(),
                None => return,
            }
        };

        if !to_evict.is_empty() {
            let mut sessions_guard = self
                .sessions
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(ref mut sessions) = *sessions_guard {
                for cwd in &to_evict {
                    sessions.remove(cwd);
                }
            }
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

static PS_SESSION_REGISTRY: PSSessionRegistry = PSSessionRegistry::new();

// Rate limiter

struct PSRateLimiter {
    active: AtomicUsize,
    max_concurrent: usize,
}

impl PSRateLimiter {
    const fn new(max_concurrent: usize) -> Self {
        Self {
            active: AtomicUsize::new(0),
            max_concurrent,
        }
    }

    fn try_acquire(&self) -> Result<PSPermit<'_>> {
        loop {
            let current = self.active.load(Ordering::Relaxed);
            if current >= self.max_concurrent {
                return Err(anyhow!("rate limit exceeded"));
            }
            if self
                .active
                .compare_exchange(current, current + 1, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok()
            {
                return Ok(PSPermit { limiter: self });
            }
        }
    }

    fn active_count(&self) -> usize {
        self.active.load(Ordering::Relaxed)
    }
}

struct PSPermit<'a> {
    limiter: &'a PSRateLimiter,
}

impl Drop for PSPermit<'_> {
    fn drop(&mut self) {
        self.limiter.active.fetch_sub(1, Ordering::Relaxed);
    }
}

static PS_RATE_LIMITER: PSRateLimiter = PSRateLimiter::new(4);

// PowerShellTool

pub struct PowerShellTool;

impl Tool for PowerShellTool {
    fn name(&self) -> &'static str {
        "powershell"
    }

    fn description(&self) -> &'static str {
        "Run PowerShell commands in a persistent session. \
         Supports PowerShell Core (pwsh 7+, cross-platform) and Windows PowerShell (5.1). \
         Use PowerShell cmdlets and syntax (e.g., Get-ChildItem, Select-String, $env:PATH). \
         PowerShell Core supports && and || chain operators; Windows PowerShell 5.1 does not. \
         Prefer dedicated tools for common operations: read_file/edit_file for file I/O, \
         grep for searching, glob for file matching. \
         Use powershell for: .NET operations, Windows-specific tasks, object pipeline processing, \
         and commands that need PS cmdlets (Get-Content, Invoke-WebRequest, etc.)."
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

    fn tags(&self) -> &[ToolTag] {
        &[ToolTag::Implement, ToolTag::Ops]
    }

    fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput> {
        crate::check_permission(self.permission(), ctx)?;

        if let Some(gate) = &ctx.plan_gate {
            gate.check_access(ctx.role, self.name())?;
        }

        let command = params
            .get("command")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("missing string parameter 'command'"))?
            .to_string();

        let restart = params
            .get("restart")
            .and_then(Value::as_bool)
            .unwrap_or(false);

        let timeout_secs = params
            .get("timeout_secs")
            .and_then(Value::as_u64)
            .unwrap_or(120)
            .min(600);

        // Validate command safety
        use crate::security::cross_platform::{
            allowed_commands, blocked_commands, validate_path_in_workspace, ShellType,
        };
        validate_path_in_workspace(&ctx.cwd, &ctx.cwd)?;

        let shell_type = ShellType::PowerShell;
        let binary_name = extract_binary_name(&command)?;
        let allowed_commands = allowed_commands(shell_type);
        if !allowed_commands
            .iter()
            .any(|cmd| cmd.eq_ignore_ascii_case(&binary_name))
        {
            anyhow::bail!(
                "command '{}' is not in allowed list for PowerShell",
                binary_name
            );
        }

        let blocked_commands = blocked_commands(shell_type);
        if blocked_commands
            .iter()
            .any(|cmd| cmd.eq_ignore_ascii_case(&binary_name))
        {
            anyhow::bail!("command '{}' is blocked for security reasons", binary_name);
        }

        let _permit = PS_RATE_LIMITER.try_acquire().map_err(|_| {
            anyhow!(
                "Rate limit exceeded: {} concurrent PowerShell commands already running.",
                PS_RATE_LIMITER.active_count()
            )
        })?;

        let start_time = Instant::now();

        let session = if restart {
            PS_SESSION_REGISTRY.remove(&ctx.cwd);
            PS_SESSION_REGISTRY.get_or_create(ctx.cwd.clone())?
        } else {
            PS_SESSION_REGISTRY.evict_idle();
            PS_SESSION_REGISTRY.get_or_create(ctx.cwd.clone())?
        };

        let command_clone = command.clone();
        let cwd_clone = ctx.cwd.clone();

        let (stdout, stderr, exit_code) = if let Ok(handle) = tokio::runtime::Handle::try_current()
        {
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
                                PS_SESSION_REGISTRY.remove(&cwd_clone);
                                let fresh = PS_SESSION_REGISTRY.get_or_create(cwd_clone)?;
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
                            "pwsh command timed out, evicting session for {:?}",
                            ctx.cwd
                        );
                        PS_SESSION_REGISTRY.remove(&ctx.cwd);
                    }

                    result
                        .map_err(|_| anyhow!("command timed out after {timeout_secs}s"))?
                        .map_err(|e| anyhow!("command execution failed: {e}"))?
                })
            })
        } else {
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
                                PS_SESSION_REGISTRY.remove(&cwd_clone);
                                let fresh = PS_SESSION_REGISTRY.get_or_create(cwd_clone)?;
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
                            "pwsh command timed out, evicting session for {:?}",
                            cwd_for_evict
                        );
                        PS_SESSION_REGISTRY.remove(&cwd_for_evict);
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
            meta["shell"] = json!("powershell");
            if let Some(edition) = ps_edition() {
                meta["ps_edition"] = json!(match edition {
                    PSEdition::Core => "core",
                    PSEdition::Desktop => "desktop",
                });
            }
            if let Some(ver) = detect_ps_version() {
                meta["ps_version"] = json!(ver);
            }
            if exit_code != 0 {
                meta["failed"] = json!(true);
            }
            meta
        };

        Ok(ToolOutput::with_structured(output_text, metadata))
    }
}

impl ToolStreaming for PowerShellTool {
    fn execute_stream(&self, params: Value, ctx: &ToolContext) -> Result<StreamReceiver> {
        use crate::streaming::create_stream_channel;

        crate::check_permission(self.permission(), ctx)?;

        if let Some(gate) = &ctx.plan_gate {
            gate.check_access(ctx.role, self.name())?;
        }

        let command = params
            .get("command")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();

        let restart = params
            .get("restart")
            .and_then(Value::as_bool)
            .unwrap_or(false);

        let timeout_secs = params
            .get("timeout_secs")
            .and_then(Value::as_u64)
            .unwrap_or(120)
            .min(600);

        // Validate
        use crate::security::cross_platform::{allowed_commands, blocked_commands, ShellType};
        let shell_type = ShellType::PowerShell;
        let binary_name = extract_binary_name(&command).unwrap_or_default();
        let allowed_commands = allowed_commands(shell_type);
        if !allowed_commands
            .iter()
            .any(|cmd| cmd.eq_ignore_ascii_case(&binary_name))
        {
            return Err(anyhow!("command not in allowed list for PowerShell"));
        }
        let blocked_commands = blocked_commands(shell_type);
        if blocked_commands
            .iter()
            .any(|cmd| cmd.eq_ignore_ascii_case(&binary_name))
        {
            return Err(anyhow!("command is blocked for security reasons"));
        }

        let (sender, receiver) = create_stream_channel();

        let session = if restart {
            PS_SESSION_REGISTRY.remove(&ctx.cwd);
            PS_SESSION_REGISTRY.get_or_create(ctx.cwd.clone())?
        } else {
            PS_SESSION_REGISTRY.evict_idle();
            PS_SESSION_REGISTRY.get_or_create(ctx.cwd.clone())?
        };

        let cwd_for_evict = ctx.cwd.clone();

        // Spawn blocking task for streaming execution
        let _ = thread::spawn(move || {
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
                PS_SESSION_REGISTRY.remove(&cwd_for_evict);
                let fresh = match PS_SESSION_REGISTRY.get_or_create(cwd_for_evict) {
                    Ok(f) => f,
                    Err(e) => {
                        let _ = sender.send(StreamChunk::new(format!("Error: {e}\n")));
                        let _ = sender.send(StreamChunk::done());
                        return;
                    }
                };
                let s = fresh
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                let _ = s.execute_stream(&command, timeout_secs, sender);
                return;
            }
            let _ = s.execute_stream(&command, timeout_secs, sender);
        });

        Ok(receiver)
    }
}

// Helpers

/// Extract the binary/command name from a PowerShell command string.
/// Handles both cmdlet names (`Get-ChildItem`) and external binaries (`git`).
fn extract_binary_name(command: &str) -> anyhow::Result<String> {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return Err(anyhow!("empty command"));
    }

    // PowerShell commands often start with a cmdlet like Get-Something
    // or an alias like gci, ls. Split on first whitespace or pipe.
    let first_token = trimmed
        .split(|c: char| c.is_whitespace() || c == '|' || c == ';')
        .next()
        .unwrap_or(trimmed);

    // Strip any path components
    let name = if first_token.contains('/') {
        first_token.rsplit('/').next().unwrap_or(first_token)
    } else if first_token.contains('\\') {
        first_token.rsplit('\\').next().unwrap_or(first_token)
    } else {
        first_token
    };

    Ok(name.to_lowercase())
}

// Tests

#[cfg(test)]
mod tests {
    use super::*;

    fn pwsh_available() -> bool {
        find_pwsh().is_some()
    }

    #[test]
    fn test_extract_binary_name_cmdlet() {
        assert_eq!(
            extract_binary_name("Get-ChildItem -Path .").unwrap(),
            "get-childitem"
        );
    }

    #[test]
    fn test_extract_binary_name_alias() {
        assert_eq!(extract_binary_name("gci -Path .").unwrap(), "gci");
    }

    #[test]
    fn test_extract_binary_name_external() {
        assert_eq!(extract_binary_name("git status").unwrap(), "git");
    }

    #[test]
    fn test_extract_binary_name_pipe() {
        assert_eq!(
            extract_binary_name("Get-Process | Where-Object { $_.CPU -gt 100 }").unwrap(),
            "get-process"
        );
    }

    #[test]
    fn test_extract_binary_name_path() {
        assert_eq!(
            extract_binary_name("/usr/bin/python3 -c 'hello'").unwrap(),
            "python3"
        );
    }

    #[test]
    fn test_extract_binary_name_empty() {
        assert!(extract_binary_name("").is_err());
        assert!(extract_binary_name("   ").is_err());
    }

    #[test]
    fn test_ps_boilerplate_filtering() {
        assert!(is_ps_boilerplate("PowerShell 7.4.0"));
        assert!(is_ps_boilerplate("PS C:\\Users> "));
        assert!(is_ps_boilerplate("Windows PowerShell"));
        assert!(!is_ps_boilerplate("Hello, World!"));
        assert!(!is_ps_boilerplate("Get-Process"));
    }

    #[test]
    fn test_filter_ps_boilerplate_multiline() {
        let input = "PowerShell 7.4.0\nHello\nPS C:\\> \nWorld\nWrite-Output '---END---'";
        let filtered = filter_ps_boilerplate(input);
        assert_eq!(filtered, "Hello\nWorld");
    }

    #[test]
    fn test_find_pwsh() {
        // This test just verifies the function doesn't panic.
        // It may return None on systems without pwsh.
        let _ = find_pwsh();
    }

    #[test]
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

    #[test]
    fn test_tool_name_and_schema() {
        let tool = PowerShellTool;
        assert_eq!(tool.name(), "powershell");
        let schema = tool.parameters_schema();
        assert!(schema["properties"]["command"].is_object());
        assert!(schema["properties"]["restart"].is_object());
        assert!(schema["properties"]["timeout_secs"].is_object());
    }

    #[test]
    fn test_rate_limiter() {
        let limiter = PSRateLimiter::new(2);
        let _p1 = limiter.try_acquire().unwrap();
        let _p2 = limiter.try_acquire().unwrap();
        assert!(limiter.try_acquire().is_err());
        drop(_p1);
        let _p3 = limiter.try_acquire().unwrap();
    }

    #[test]
    fn test_ps_edition_from_binary() {
        // Verify edition detection doesn't panic
        let edition = ps_edition();
        if pwsh_available() {
            assert!(edition.is_some());
            // pwsh binary should always report Core edition
            let ed = edition.unwrap();
            assert_eq!(ed, PSEdition::Core);
            assert!(ed.supports_chain_operators());
        }
    }

    #[test]
    fn test_edition_desktop_no_chain_operators() {
        assert!(!PSEdition::Desktop.supports_chain_operators());
        assert!(PSEdition::Core.supports_chain_operators());
    }

    #[test]
    fn test_find_pwsh_cached() {
        // Calling twice should return the same result (cached via OnceLock)
        let first = find_pwsh();
        let second = find_pwsh();
        assert_eq!(first, second);
    }

    #[test]
    fn test_detect_ps_version() {
        // Just verify it doesn't panic
        if pwsh_available() {
            let ver = detect_ps_version();
            assert!(ver.is_some());
            let v = ver.unwrap();
            // Should look like a version string (e.g., "7.4.0" or "5.1.22")
            assert!(v.contains('.'), "version should contain a dot: {v}");
        }
    }
}
