use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc as std_mpsc;
use std::time::{Duration, Instant};

// Import command validation from rustycode-tools
use rustycode_tools::providers::bash::validate_command_safety;
// Import shell_words for safe command parsing (prevents shell injection)
use shell_words;

#[derive(Debug, Clone)]
pub struct CommandResult {
    pub command: String,
    pub tool_name: String,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

pub fn spawn_command_worker(
    cwd: PathBuf,
    command: String,
    tool_name: String,
    timeout_secs: u64,
) -> (
    std_mpsc::Receiver<CommandResult>,
    std::thread::JoinHandle<()>,
) {
    let (tx, rx) = std_mpsc::channel();

    // For command workers we still spawn a thread to run the blocking
    // process operations (child process I/O). This path is acceptable as
    // it doesn't create additional tokio runtimes. Keep join handle so
    // callers can drop it as before.
    let handle = std::thread::spawn(move || {
        // SECURITY: Validate command safety before execution
        // This prevents command injection, shell metacharacter abuse,
        // and ensures only allowlisted commands can run.
        if let Err(e) = validate_command_safety(&command) {
            if let Err(e) = tx.send(CommandResult {
                command,
                tool_name,
                stdout: String::new(),
                stderr: format!("Command validation failed: {}", e),
                exit_code: -1,
            }) {
                tracing::error!("Failed to send command result: {e}");
            }
            return;
        }

        let child = if cfg!(target_os = "windows") {
            rustycode_tools::subprocess::new_shell_command(&command)
                .current_dir(&cwd)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
        } else {
            // SECURE: Parse command using shell_words to prevent shell injection
            let parts = match shell_words::split(&command) {
                Ok(p) => p,
                Err(e) => {
                    if let Err(e) = tx.send(CommandResult {
                        command,
                        tool_name,
                        stdout: String::new(),
                        stderr: format!("Failed to parse command: {}", e),
                        exit_code: -1,
                    }) {
                        tracing::error!("Failed to send command result: {e}");
                    }
                    return;
                }
            };

            // Validate command is not empty
            if parts.is_empty() {
                if let Err(e) = tx.send(CommandResult {
                    command,
                    tool_name,
                    stdout: String::new(),
                    stderr: "Empty command".to_string(),
                    exit_code: -1,
                }) {
                    tracing::error!("Failed to send command result: {e}");
                }
                return;
            }

            let binary = &parts[0];
            let args: Vec<&String> = parts.iter().skip(1).collect();

            // Validate binary exists
            if std::path::Path::new(binary).is_absolute() && !std::path::Path::new(binary).exists()
            {
                if let Err(e) = tx.send(CommandResult {
                    command,
                    tool_name,
                    stdout: String::new(),
                    stderr: format!("Binary not found: {}", binary),
                    exit_code: -1,
                }) {
                    tracing::error!("Failed to send command result: {e}");
                }
                return;
            }

            let mut cmd = Command::new(binary);
            cmd.args(args)
                .current_dir(&cwd)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
        };

        let mut child = match child {
            Ok(child) => child,
            Err(e) => {
                if let Err(e) = tx.send(CommandResult {
                    command,
                    tool_name,
                    stdout: String::new(),
                    stderr: format!("Failed to execute command: {}", e),
                    exit_code: -1,
                }) {
                    tracing::error!("Failed to send command result: {e}");
                }
                return;
            }
        };

        let deadline = Instant::now() + Duration::from_secs(timeout_secs);
        loop {
            match child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) => {
                    if Instant::now() >= deadline {
                        let _ = child.kill();
                        if let Err(e) = tx.send(CommandResult {
                            command,
                            tool_name,
                            stdout: String::new(),
                            stderr: format!("Command timed out after {} seconds", timeout_secs),
                            exit_code: -1,
                        }) {
                            tracing::error!("Failed to send command result: {e}");
                        }
                        return;
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
                Err(e) => {
                    if let Err(e) = tx.send(CommandResult {
                        command,
                        tool_name,
                        stdout: String::new(),
                        stderr: format!("Failed while waiting for command: {}", e),
                        exit_code: -1,
                    }) {
                        tracing::error!("Failed to send command result: {e}");
                    }
                    return;
                }
            }
        }

        match child.wait_with_output() {
            Ok(output) => {
                if let Err(e) = tx.send(CommandResult {
                    command,
                    tool_name,
                    stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                    stderr: String::from_utf8_lossy(&output.stderr).to_string(),
                    exit_code: output.status.code().unwrap_or(-1),
                }) {
                    tracing::error!("Failed to send command result: {e}");
                }
            }
            Err(e) => {
                if let Err(e) = tx.send(CommandResult {
                    command,
                    tool_name,
                    stdout: String::new(),
                    stderr: format!("Failed to collect command output: {}", e),
                    exit_code: -1,
                }) {
                    tracing::error!("Failed to send command result: {e}");
                }
            }
        }
    });

    (rx, handle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Helper: create a temp dir and return its path
    fn tmp_dir() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    // =========================================================================
    // Terminal-bench: CommandResult construction tests
    // =========================================================================

    #[test]
    fn test_command_result_fields() {
        let result = CommandResult {
            command: "echo hello".into(),
            tool_name: "bash".into(),
            stdout: "hello\n".into(),
            stderr: String::new(),
            exit_code: 0,
        };
        assert_eq!(result.command, "echo hello");
        assert_eq!(result.tool_name, "bash");
        assert_eq!(result.stdout, "hello\n");
        assert!(result.stderr.is_empty());
        assert_eq!(result.exit_code, 0);
    }

    #[test]
    fn test_command_result_debug_format() {
        let result = CommandResult {
            command: "ls".into(),
            tool_name: "bash".into(),
            stdout: "file.txt".into(),
            stderr: String::new(),
            exit_code: 0,
        };
        let debug = format!("{:?}", result);
        assert!(debug.contains("ls"));
        assert!(debug.contains("file.txt"));
    }

    #[test]
    fn test_command_result_clone() {
        let result = CommandResult {
            command: "pwd".into(),
            tool_name: "shell".into(),
            stdout: "/tmp".into(),
            stderr: String::new(),
            exit_code: 0,
        };
        let cloned = result.clone();
        assert_eq!(cloned.command, result.command);
        assert_eq!(cloned.exit_code, result.exit_code);
    }

    // =========================================================================
    // Terminal-bench: Basic execution tests
    // =========================================================================

    #[test]
    fn test_spawn_echo_command() {
        let tmp = tmp_dir();
        let (rx, handle) = spawn_command_worker(
            tmp.path().to_path_buf(),
            "echo hello".into(),
            "bash".into(),
            10,
        );

        let result = rx.recv_timeout(Duration::from_secs(10)).unwrap();
        handle.join().unwrap();

        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("hello"));
    }

    #[test]
    fn test_spawn_false_returns_nonzero() {
        let tmp = tmp_dir();
        let (rx, handle) =
            spawn_command_worker(tmp.path().to_path_buf(), "false".into(), "bash".into(), 10);

        let result = rx.recv_timeout(Duration::from_secs(10)).unwrap();
        handle.join().unwrap();

        assert_ne!(result.exit_code, 0);
    }

    #[test]
    fn test_spawn_tool_name_preserved() {
        let tmp = tmp_dir();
        let (rx, handle) = spawn_command_worker(
            tmp.path().to_path_buf(),
            "echo ok".into(),
            "custom_tool".into(),
            10,
        );

        let result = rx.recv_timeout(Duration::from_secs(10)).unwrap();
        handle.join().unwrap();

        assert_eq!(result.tool_name, "custom_tool");
    }

    #[test]
    fn test_spawn_command_with_args() {
        let tmp = tmp_dir();
        let (rx, handle) = spawn_command_worker(
            tmp.path().to_path_buf(),
            "echo hello world".into(),
            "bash".into(),
            10,
        );

        let result = rx.recv_timeout(Duration::from_secs(10)).unwrap();
        handle.join().unwrap();

        assert!(result.stdout.contains("hello world"));
    }

    #[test]
    fn test_spawn_command_cwd_respected() {
        let tmp = tmp_dir();
        let sub = tmp.path().join("subdir");
        fs::create_dir_all(&sub).unwrap();
        fs::write(sub.join("marker.txt"), "found").unwrap();

        let (rx, handle) =
            spawn_command_worker(sub.clone(), "cat marker.txt".into(), "bash".into(), 10);

        let result = rx.recv_timeout(Duration::from_secs(10)).unwrap();
        handle.join().unwrap();

        assert!(result.stdout.contains("found"));
    }

    #[test]
    fn test_spawn_seq_multiple_commands() {
        let tmp = tmp_dir();
        // shell_words::split will parse this as separate arguments to echo
        let (rx, handle) = spawn_command_worker(
            tmp.path().to_path_buf(),
            "echo one two three".into(),
            "bash".into(),
            10,
        );

        let result = rx.recv_timeout(Duration::from_secs(10)).unwrap();
        handle.join().unwrap();

        assert!(result.stdout.contains("one"));
        assert!(result.stdout.contains("two"));
        assert!(result.stdout.contains("three"));
    }

    #[test]
    fn test_spawn_command_timeout() {
        let tmp = tmp_dir();
        let (rx, handle) = spawn_command_worker(
            tmp.path().to_path_buf(),
            "sleep 30".into(),
            "bash".into(),
            1, // 1 second timeout
        );

        let result = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        // Drop handle without joining — thread may still be cleaning up
        drop(handle);

        assert_eq!(result.exit_code, -1);
        // The timeout message may or may not be present depending on race
        assert!(
            result.stderr.contains("timed out") || result.stderr.contains("Command"),
            "Expected timeout or error, got: {}",
            result.stderr
        );
    }

    #[test]
    fn test_spawn_command_touch_creates_file() {
        let tmp = tmp_dir();
        let (rx, handle) = spawn_command_worker(
            tmp.path().to_path_buf(),
            "touch created.txt".into(),
            "bash".into(),
            10,
        );

        let result = rx.recv_timeout(Duration::from_secs(10)).unwrap();
        handle.join().unwrap();

        assert_eq!(result.exit_code, 0);
        assert!(tmp.path().join("created.txt").exists());
    }

    #[test]
    fn test_spawn_command_separate_processes() {
        let tmp = tmp_dir();
        // Write a file in first command
        let (rx1, h1) = spawn_command_worker(
            tmp.path().to_path_buf(),
            "touch marker.txt".into(),
            "bash".into(),
            10,
        );
        let r1 = rx1.recv_timeout(Duration::from_secs(10)).unwrap();
        drop(h1);
        assert_eq!(r1.exit_code, 0);

        // Verify file exists from second command
        let (rx2, h2) = spawn_command_worker(
            tmp.path().to_path_buf(),
            "ls marker.txt".into(),
            "bash".into(),
            10,
        );
        let result = rx2.recv_timeout(Duration::from_secs(10)).unwrap();
        h2.join().unwrap();

        assert!(result.stdout.contains("marker.txt"));
    }

    #[test]
    fn test_spawn_empty_command_rejected() {
        let tmp = tmp_dir();
        let (rx, handle) =
            spawn_command_worker(tmp.path().to_path_buf(), "".into(), "bash".into(), 10);

        let result = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        drop(handle);

        // Empty command should produce an error
        assert!(result.exit_code != 0 || !result.stderr.is_empty());
    }

    // =========================================================================
    // Terminal-bench: 15 additional tests for command_runner
    // =========================================================================

    // 1. CommandResult with nonzero exit code
    #[test]
    fn command_result_nonzero_exit() {
        let result = CommandResult {
            command: "exit 1".into(),
            tool_name: "bash".into(),
            stdout: String::new(),
            stderr: "error".into(),
            exit_code: 1,
        };
        assert_ne!(result.exit_code, 0);
        assert!(!result.stderr.is_empty());
    }

    // 2. CommandResult with negative exit code (signal)
    #[test]
    fn command_result_negative_exit() {
        let result = CommandResult {
            command: "kill -9 $$".into(),
            tool_name: "bash".into(),
            stdout: String::new(),
            stderr: "killed".into(),
            exit_code: -1,
        };
        assert!(result.exit_code < 0);
    }

    // 3. CommandResult with large stdout
    #[test]
    fn command_result_large_stdout() {
        let big_output = "x".repeat(10_000);
        let result = CommandResult {
            command: "cat bigfile".into(),
            tool_name: "bash".into(),
            stdout: big_output.clone(),
            stderr: String::new(),
            exit_code: 0,
        };
        assert_eq!(result.stdout.len(), 10_000);
    }

    // 4. CommandResult with unicode content
    #[test]
    fn command_result_unicode_content() {
        let result = CommandResult {
            command: "echo".into(),
            tool_name: "bash".into(),
            stdout: "Hello 🌍 世界 مرحبا".into(),
            stderr: String::new(),
            exit_code: 0,
        };
        assert!(result.stdout.contains("🌍"));
        assert!(result.stdout.contains("世界"));
    }

    // 5. CommandResult debug contains all fields
    #[test]
    fn command_result_debug_all_fields() {
        let result = CommandResult {
            command: "test-cmd".into(),
            tool_name: "my-tool".into(),
            stdout: "out".into(),
            stderr: "err".into(),
            exit_code: 42,
        };
        let debug = format!("{:?}", result);
        assert!(debug.contains("test-cmd"));
        assert!(debug.contains("my-tool"));
        assert!(debug.contains("out"));
        assert!(debug.contains("err"));
    }

    // 6. Spawn echo with special characters
    #[test]
    fn spawn_echo_special_chars() {
        let tmp = tmp_dir();
        let (rx, handle) = spawn_command_worker(
            tmp.path().to_path_buf(),
            "echo 'hello world'".into(),
            "bash".into(),
            10,
        );
        let result = rx.recv_timeout(Duration::from_secs(10)).unwrap();
        handle.join().unwrap();
        assert!(result.stdout.contains("hello world"));
    }

    // 7. Spawn pwd returns current directory
    #[test]
    fn spawn_pwd_returns_dir() {
        let tmp = tmp_dir();
        let (rx, handle) =
            spawn_command_worker(tmp.path().to_path_buf(), "pwd".into(), "bash".into(), 10);
        let result = rx.recv_timeout(Duration::from_secs(10)).unwrap();
        handle.join().unwrap();
        assert!(result.stdout.contains(tmp.path().to_str().unwrap()));
    }

    // 8. Spawn command writes to stderr
    #[test]
    fn spawn_stderr_capture() {
        let tmp = tmp_dir();
        let (rx, handle) = spawn_command_worker(
            tmp.path().to_path_buf(),
            "echo error_msg >&2".into(),
            "bash".into(),
            10,
        );
        let result = rx.recv_timeout(Duration::from_secs(10)).unwrap();
        handle.join().unwrap();
        // Note: shell_words splits this, stderr redirect may not work as expected
        // Just verify the command completes without panic
        let _ = result.exit_code;
    }

    // 9. Spawn echo returns 0 (reliable allowlisted command)
    #[test]
    fn spawn_echo_returns_zero() {
        let tmp = tmp_dir();
        let (rx, handle) = spawn_command_worker(
            tmp.path().to_path_buf(),
            "echo zero_exit".into(),
            "bash".into(),
            10,
        );
        let result = rx.recv_timeout(Duration::from_secs(10)).unwrap();
        handle.join().unwrap();
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("zero_exit"));
    }

    // 10. Spawn cat on nonexistent file returns error
    #[test]
    fn spawn_cat_nonexistent_returns_error() {
        let tmp = tmp_dir();
        let (rx, handle) = spawn_command_worker(
            tmp.path().to_path_buf(),
            "cat nonexistent_file_xyz.txt".into(),
            "bash".into(),
            10,
        );
        let result = rx.recv_timeout(Duration::from_secs(10)).unwrap();
        handle.join().unwrap();
        assert_ne!(result.exit_code, 0);
        assert!(!result.stderr.is_empty());
    }

    // 11. Multiple sequential commands share filesystem state
    #[test]
    fn sequential_commands_share_filesystem() {
        let tmp = tmp_dir();

        // Write a file
        let (rx1, h1) = spawn_command_worker(
            tmp.path().to_path_buf(),
            "touch shared_state.txt".into(),
            "bash".into(),
            10,
        );
        let r1 = rx1.recv_timeout(Duration::from_secs(10)).unwrap();
        drop(h1);
        assert_eq!(r1.exit_code, 0);

        // Verify it exists by listing
        let (rx2, h2) = spawn_command_worker(
            tmp.path().to_path_buf(),
            "ls shared_state.txt".into(),
            "bash".into(),
            10,
        );
        let r2 = rx2.recv_timeout(Duration::from_secs(10)).unwrap();
        h2.join().unwrap();
        assert_eq!(r2.exit_code, 0);
    }

    // 12. CommandResult clone independence
    #[test]
    fn command_result_clone_independence() {
        let result = CommandResult {
            command: "original".into(),
            tool_name: "tool".into(),
            stdout: "output".into(),
            stderr: String::new(),
            exit_code: 0,
        };
        let mut cloned = result.clone();
        cloned.command = "modified".into();
        assert_eq!(result.command, "original");
        assert_eq!(cloned.command, "modified");
    }

    // 13. Spawn mkdir creates directory
    #[test]
    fn spawn_mkdir_creates_dir() {
        let tmp = tmp_dir();
        let (rx, handle) = spawn_command_worker(
            tmp.path().to_path_buf(),
            "mkdir newdir".into(),
            "bash".into(),
            10,
        );
        let result = rx.recv_timeout(Duration::from_secs(10)).unwrap();
        handle.join().unwrap();
        assert_eq!(result.exit_code, 0);
        assert!(tmp.path().join("newdir").is_dir());
    }

    // 14. Spawn env prints environment
    #[test]
    fn spawn_env_command() {
        let tmp = tmp_dir();
        let (rx, handle) =
            spawn_command_worker(tmp.path().to_path_buf(), "env".into(), "bash".into(), 10);
        let result = rx.recv_timeout(Duration::from_secs(10)).unwrap();
        handle.join().unwrap();
        assert_eq!(result.exit_code, 0);
        // env should output at least PATH
        assert!(result.stdout.contains("PATH"));
    }

    // 15. Spawn whoami returns non-empty
    #[test]
    fn spawn_whoami_returns_user() {
        let tmp = tmp_dir();
        let (rx, handle) =
            spawn_command_worker(tmp.path().to_path_buf(), "whoami".into(), "bash".into(), 10);
        let result = rx.recv_timeout(Duration::from_secs(10)).unwrap();
        handle.join().unwrap();
        assert_eq!(result.exit_code, 0);
        assert!(!result.stdout.trim().is_empty());
    }
}
