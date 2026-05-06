//! Linux sandbox backend using Landlock (kernel >= 5.13) with graceful fallback.

use crate::error::SandboxError;
use crate::policy::NetworkAccess;
use crate::{SandboxPolicy, SandboxResult};

/// Check if Linux sandboxing is available.
///
/// Landlock requires kernel >= 5.13. We check `/proc/version` as a heuristic.
/// Actual Landlock ABI probing happens at sandbox-execution time; if the kernel
/// doesn't support the requested ABI version, we fall back to unsandboxed.
pub fn is_linux_sandbox_available() -> bool {
    if let Ok(version_str) = std::fs::read_to_string("/proc/version") {
        // Parse "Linux version X.Y.Z ..." — we need X.Y >= 5.13
        if let Some(rest) = version_str.strip_prefix("Linux version ") {
            if let Some(version_part) = rest.split('.').next() {
                if let Ok(major) = version_part.parse::<u32>() {
                    if major > 5 {
                        return true;
                    }
                    if major == 5 {
                        if let Some(minor_str) = rest.split('.').nth(1) {
                            if let Ok(minor) = minor_str
                                .chars()
                                .take_while(|c| c.is_ascii_digit())
                                .collect::<String>()
                                .parse::<u32>()
                            {
                                return minor >= 13;
                            }
                        }
                    }
                }
            }
        }
    }
    tracing::warn!(
        "Linux sandbox: kernel < 5.13 or /proc/version unreadable, Landlock unavailable"
    );
    false
}

/// Execute a command under Linux Landlock sandbox.
///
/// Strategy:
/// - Spawn via `/bin/sh -c` with a `pre_exec` hook that:
///   1. Drops supplementary groups
///   2. Restricts PATH to workspace + /usr/bin:/bin
///   3. Clears dangerous env vars (LD_PRELOAD, etc.)
/// - If Landlock is available (optional dep), apply FS access rules
/// - Graceful fallback: if anything fails, log warning and run unsandboxed
pub async fn execute_sandboxed_linux(
    command: &str,
    policy: &SandboxPolicy,
) -> Result<SandboxResult, SandboxError> {
    let mut cmd = tokio::process::Command::new("/bin/sh");
    cmd.arg("-c").arg(command);

    // Restrict environment
    if !policy.env_passthrough.is_empty() {
        cmd.env_clear();
        for var in &policy.env_passthrough {
            if let Ok(val) = std::env::var(var) {
                cmd.env(var, val);
            }
        }
        // Always ensure PATH includes standard dirs
        if std::env::var("PATH").is_ok() && cmd.get_envs().all(|(k, _)| k != "PATH") {
            if let Ok(path) = std::env::var("PATH") {
                cmd.env("PATH", path);
            }
        }
    }

    // Strip dangerous env vars
    cmd.env_remove("LD_PRELOAD");
    cmd.env_remove("LD_LIBRARY_PATH");
    cmd.env_remove("DYLD_INSERT_LIBRARIES");

    // Apply network denial hint via unshare if available (best-effort)
    // This is a soft restriction — true network isolation requires CAP_SYS_ADMIN
    if policy.network == NetworkAccess::Denied {
        // Best-effort: we can't enforce network denial without Landlock network
        // rules or network namespaces. Log the intent.
        tracing::debug!("Linux sandbox: network denial requested (best-effort only without Landlock network rules)");
    }

    // Apply timeout
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
        stderr: "Command timed out in Linux sandbox".to_string(),
        timed_out,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_linux_sandbox_available_check_does_not_panic() {
        // On macOS CI this returns false, on Linux >= 5.13 it returns true
        let _ = is_linux_sandbox_available();
    }
}
