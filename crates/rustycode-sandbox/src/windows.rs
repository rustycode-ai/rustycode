//! Windows sandbox backend using Job Objects (stub).
//!
//! Full implementation will use:
//! - `CreateJobObjectW` + `SetInformationJobObject` for resource limits
//! - `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` for cleanup
//! - `CreateRestrictedToken` for path-based access control
//!
//! Currently runs unsandboxed with a warning and basic env stripping.

use crate::error::SandboxError;
use crate::policy::NetworkAccess;
use crate::{SandboxPolicy, SandboxResult};

/// Check if Windows sandboxing is available.
///
/// Job Objects are always available on Windows NT 5.1+, so this returns true
/// once the actual implementation is complete. For now, returns false (stub).
pub fn is_windows_sandbox_available() -> bool {
    tracing::warn!("Windows sandbox: job-object restrictions not yet implemented");
    false
}

/// Execute a command with Windows sandbox restrictions (stub).
///
/// Currently applies environment variable stripping only.
/// Full implementation will spawn inside a Job Object with restricted token.
pub async fn execute_sandboxed_windows(
    command: &str,
    policy: &SandboxPolicy,
) -> Result<SandboxResult, SandboxError> {
    tracing::warn!(
        "Windows sandbox: running with env restrictions only (job-object not yet implemented)"
    );

    let mut cmd = tokio::process::Command::new("cmd");
    cmd.arg("/C").arg(command);

    // Restrict environment variables
    if !policy.env_passthrough.is_empty() {
        cmd.env_clear();
        for var in &policy.env_passthrough {
            if let Ok(val) = std::env::var(var) {
                cmd.env(var, val);
            }
        }
        // Ensure PATH is always present after env_clear (setting twice is harmless).
        if let Ok(path) = std::env::var("PATH") {
            cmd.env("PATH", path);
        }
    }

    // Strip dangerous env vars
    cmd.env_remove("LD_PRELOAD");
    cmd.env_remove("LD_LIBRARY_PATH");

    if policy.network == NetworkAccess::Denied {
        tracing::debug!("Windows sandbox: network denial requested (not yet enforced)");
    }

    let timed_out = if let Some(timeout_secs) = policy.timeout_secs {
        match tokio::time::timeout(std::time::Duration::from_secs(timeout_secs), cmd.output()).await
        {
            Ok(Ok(output)) => {
                return Ok(SandboxResult {
                    exit_code: output.status.code(),
                    stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                    stderr: String::from_utf8_lossy(&output.stderr).to_string(),
                    timed_out: false,
                });
            }
            Ok(Err(e)) => return Err(SandboxError::ExecutionFailed(e.to_string())),
            Err(_) => true,
        }
    } else {
        let output = cmd.output().await.map_err(SandboxError::Io)?;
        return Ok(SandboxResult {
            exit_code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            timed_out: false,
        });
    };

    Ok(SandboxResult {
        exit_code: None,
        stdout: String::new(),
        stderr: "Command timed out in Windows sandbox".to_string(),
        timed_out,
    })
}
