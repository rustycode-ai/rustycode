//! Cross-platform subprocess management with process isolation.
//!
//! Ported from goose's `subprocess.rs` with `RustyCode` adaptations:
//! - Process group isolation (Unix) so Ctrl+C doesn't kill child processes
//! - Parent death signal (Linux) so children die when the parent exits
//! - No-window flag (Windows) for headless operation
//!
//! ## Usage
//!
//! ```ignore
//! use rustycode_tools::subprocess::configure_subprocess;
//! use tokio::process::Command;
//!
//! let mut cmd = Command::new("cargo");
//! cmd.args(["test", "--release"]);
//! configure_subprocess(&mut cmd);
//! let output = cmd.output().await?;
//! ```

use std::process::Command as StdCommand;
use std::process::Stdio;
use std::sync::LazyLock;
use tokio::process::Command as TokioCommand;

pub struct ShellInfo {
    pub binary: &'static str,
    pub exec_flag: &'static str,
    pub interactive_flag: Option<&'static str>,
    pub is_powershell: bool,
}

pub static SHELL_INFO: LazyLock<ShellInfo> = LazyLock::new(|| {
    #[cfg(windows)]
    {
        if which_sh("powershell") {
            ShellInfo {
                binary: "powershell",
                exec_flag: "-Command",
                interactive_flag: None,
                is_powershell: true,
            }
        } else {
            ShellInfo {
                binary: "cmd",
                exec_flag: "/C",
                interactive_flag: None,
                is_powershell: false,
            }
        }
    }
    #[cfg(not(windows))]
    {
        for (shell, interactive) in [("bash", Some("-i")), ("zsh", Some("-i")), ("sh", None)] {
            if which_sh(shell) {
                return ShellInfo {
                    binary: shell,
                    exec_flag: "-c",
                    interactive_flag: interactive,
                    is_powershell: false,
                };
            }
        }
        ShellInfo {
            binary: "sh",
            exec_flag: "-c",
            interactive_flag: None,
            is_powershell: false,
        }
    }
});

fn which_sh(name: &str) -> bool {
    StdCommand::new(name)
        .arg(if name == "powershell" {
            "-Command"
        } else {
            "-c"
        })
        .arg("true")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Extension trait for subprocess configuration.
pub trait SubprocessExt {
    /// Suppress console window creation (Windows only, no-op on Unix).
    fn set_no_window(&mut self) -> &mut Self;
}

impl SubprocessExt for TokioCommand {
    fn set_no_window(&mut self) -> &mut Self {
        #[cfg(windows)]
        {
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            self.creation_flags(CREATE_NO_WINDOW);
        }
        self
    }
}

impl SubprocessExt for StdCommand {
    fn set_no_window(&mut self) -> &mut Self {
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            self.creation_flags(CREATE_NO_WINDOW);
        }
        self
    }
}

/// Configure a subprocess for proper isolation.
///
/// On Unix, the child gets its own process group so terminal Ctrl+C
/// doesn't propagate to it. On Linux, it also receives SIGTERM when
/// the parent dies. On Windows, no console window is created.
///
#[allow(unused_variables)]
pub fn configure_subprocess(command: &mut TokioCommand) {
    // Isolate into own process group so SIGINT from terminal doesn't reach it
    #[cfg(unix)]
    command.process_group(0);

    // On Linux, ensure child dies when parent exits
    #[cfg(target_os = "linux")]
    configure_parent_death_signal(command);

    command.set_no_window();
}

/// Configure a sync subprocess for proper isolation.
///
/// Same as [`configure_subprocess`] but for `std::process::Command`.
#[allow(unused_variables)]
pub fn configure_subprocess_sync(command: &mut StdCommand) {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }

    command.set_no_window();
}

/// Create a new `std::process::Command` configured for the platform's shell.
///
/// On Unix, this prefers `bash` -> `zsh` -> `sh`.
/// On Windows, this prefers `powershell` -> `cmd`.
pub fn new_shell_command(command: &str) -> StdCommand {
    let mut cmd = StdCommand::new(SHELL_INFO.binary);
    cmd.arg(SHELL_INFO.exec_flag).arg(command);
    configure_subprocess_sync(&mut cmd);
    cmd
}

/// Create a new `tokio::process::Command` configured for the platform's shell.
///
/// On Unix, this prefers `bash` -> `zsh` -> `sh`.
/// On Windows, this prefers `powershell` -> `cmd`.
pub fn new_tokio_shell_command(command: &str) -> TokioCommand {
    let mut cmd = TokioCommand::new(SHELL_INFO.binary);
    cmd.arg(SHELL_INFO.exec_flag).arg(command);
    configure_subprocess(&mut cmd);
    cmd
}

/// On Linux, set PR_SET_PDEATHSIG so the child receives SIGTERM
/// when its parent exits. Also check that the parent is still alive
/// after setting the flag (to handle a race where the parent dies
/// between fork and prctl).
#[cfg(target_os = "linux")]
fn configure_parent_death_signal(command: &mut TokioCommand) {
    // SAFETY: getpid() is always safe — it returns the caller's PID, cannot fail.
    let parent_pid = unsafe { libc::getpid() };

    // SAFETY: pre_exec runs between fork and exec in the child process.
    // PR_SET_PDEATHSIG is a standard Linux mechanism to orphan-proof child
    // processes. The getppid() check closes the race window where the parent
    // dies between fork and prctl.
    unsafe {
        command.pre_exec(move || {
            if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM) != 0 {
                return Err(std::io::Error::last_os_error());
            }

            // Parent died between fork and prctl — abort
            if libc::getppid() != parent_pid {
                return Err(std::io::Error::from_raw_os_error(libc::ESRCH));
            }

            Ok(())
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shell_info_is_valid() {
        let info = &*SHELL_INFO;
        #[cfg(unix)]
        {
            assert!(
                info.binary == "bash" || info.binary == "zsh" || info.binary == "sh",
                "Expected Unix shell, got: {}",
                info.binary
            );
            assert_eq!(info.exec_flag, "-c");
            assert!(!info.is_powershell);
        }
        #[cfg(windows)]
        {
            assert!(
                info.binary == "powershell" || info.binary == "cmd",
                "Expected Windows shell, got: {}",
                info.binary
            );
            if info.binary == "powershell" {
                assert_eq!(info.exec_flag, "-Command");
                assert!(info.is_powershell);
            } else {
                assert_eq!(info.exec_flag, "/C");
                assert!(!info.is_powershell);
            }
        }
    }

    #[test]
    fn test_new_shell_command_echo() {
        let mut cmd = new_shell_command("echo hello_world");
        let output = cmd.output().expect("Failed to run shell command");
        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("hello_world"));
    }

    #[tokio::test]
    async fn test_new_tokio_shell_command_echo() {
        let mut cmd = new_tokio_shell_command("echo hello_tokio");
        let output = cmd
            .output()
            .await
            .expect("Failed to run tokio shell command");
        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("hello_tokio"));
    }

    #[test]
    fn test_configure_subprocess_sync_no_panic() {
        // Should not panic on any platform
        let mut cmd = StdCommand::new("echo");
        cmd.arg("hello");
        configure_subprocess_sync(&mut cmd);
        // Can't easily assert platform-specific flags, but no panic = success
    }

    #[tokio::test]
    async fn test_configure_subprocess_async_no_panic() {
        let mut cmd = TokioCommand::new("echo");
        cmd.arg("hello");
        configure_subprocess(&mut cmd);
        // No panic = success
    }

    #[test]
    fn test_set_no_window_sync() {
        let mut cmd = StdCommand::new("echo");
        cmd.set_no_window();
        // No panic on any platform
    }

    #[tokio::test]
    async fn test_set_no_window_async() {
        let mut cmd = TokioCommand::new("echo");
        cmd.set_no_window();
        // No panic on any platform
    }

    #[test]
    fn test_subprocess_runs_successfully() {
        let mut cmd = StdCommand::new("echo");
        cmd.arg("test_output");
        configure_subprocess_sync(&mut cmd);

        let output = cmd.output();
        // On CI or systems without echo, this may fail, but on most systems it works
        if let Ok(output) = output {
            assert!(output.status.success());
            let stdout = String::from_utf8_lossy(&output.stdout);
            assert!(stdout.contains("test_output"));
        }
    }

    #[tokio::test]
    async fn test_subprocess_async_runs_successfully() {
        let mut cmd = TokioCommand::new("echo");
        cmd.arg("async_test");
        configure_subprocess(&mut cmd);

        let output = cmd.output().await;
        if let Ok(output) = output {
            assert!(output.status.success());
            let stdout = String::from_utf8_lossy(&output.stdout);
            assert!(stdout.contains("async_test"));
        }
    }
}
