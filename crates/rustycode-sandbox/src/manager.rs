use crate::error::SandboxError;
use crate::{SandboxPolicy, SandboxResult};

/// Manages sandbox backends across platforms.
#[derive(Default)]
pub struct SandboxManager {
    available: bool,
}

impl SandboxManager {
    pub fn new() -> Result<Self, SandboxError> {
        let available = Self::detect_availability();
        if available {
            tracing::info!(platform = std::env::consts::OS, "OS sandbox backend available");
        } else {
            tracing::warn!(
                platform = std::env::consts::OS,
                "No OS sandbox backend available, commands will run unsandboxed"
            );
        }
        Ok(Self { available })
    }

    /// Check if any sandbox backend is available on this platform.
    pub const fn is_available(&self) -> bool {
        self.available
    }

    fn detect_availability() -> bool {
        #[cfg(target_os = "macos")]
        {
            crate::seatbelt::is_seatbelt_available()
        }

        #[cfg(target_os = "linux")]
        {
            crate::linux::is_linux_sandbox_available()
        }

        #[cfg(target_os = "windows")]
        {
            crate::windows::is_windows_sandbox_available()
        }

        #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
        {
            tracing::warn!("No sandbox backend available for this platform");
            false
        }
    }

    /// Execute a command inside the sandbox.
    pub async fn execute(
        &self,
        command: &str,
        policy: &SandboxPolicy,
    ) -> Result<SandboxResult, SandboxError> {
        if !self.available {
            tracing::warn!(
                command = command,
                "SECURITY: sandbox unavailable, running unsandboxed (this is a security downgrade)"
            );
            return self.execute_unsandboxed(command).await;
        }

        #[cfg(target_os = "macos")]
        {
            crate::seatbelt::execute_sandboxed(command, policy).await
        }

        #[cfg(target_os = "linux")]
        {
            crate::linux::execute_sandboxed_linux(command, policy).await
        }

        #[cfg(target_os = "windows")]
        {
            crate::windows::execute_sandboxed_windows(command, policy).await
        }

        #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
        {
            self.execute_unsandboxed(command).await
        }
    }

    async fn execute_unsandboxed(&self, command: &str) -> Result<SandboxResult, SandboxError> {
        #[cfg(windows)]
        let (shell, arg) = ("cmd", "/C");
        #[cfg(not(windows))]
        let (shell, arg) = ("/bin/sh", "-c");

        let output = tokio::process::Command::new(shell)
            .arg(arg)
            .arg(command)
            .output()
            .await
            .map_err(SandboxError::Io)?;

        Ok(SandboxResult {
            exit_code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            timed_out: false,
        })
    }
}
