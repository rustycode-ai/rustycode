//! Linux sandbox backend using Landlock (kernel >= 5.13) with graceful fallback.
#![allow(unexpected_cfgs)]

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
                                .take_while(char::is_ascii_digit)
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
/// When the `landlock` feature is enabled and the kernel supports it:
/// - Creates a Landlock ruleset restricting FS access to policy paths
/// - Applies the ruleset in the child process via `pre_exec`
/// - Strips dangerous env vars (LD_PRELOAD, LD_LIBRARY_PATH)
///
/// Graceful fallback: if Landlock is unavailable or the feature is disabled,
/// falls back to environment-variable restrictions only.
pub async fn execute_sandboxed_linux(
    command: &str,
    policy: &SandboxPolicy,
) -> Result<SandboxResult, SandboxError> {
    let mut cmd = tokio::process::Command::new("sh");
    cmd.arg("-c").arg(command);

    // Restrict environment
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
        tracing::debug!(
            "Linux sandbox: network denial requested (best-effort only without Landlock network rules)"
        );
    }

    // Apply Landlock FS restrictions via pre_exec (runs in child after fork, before exec)
    #[cfg(feature = "landlock")]
    {
        apply_landlock_pre_exec(&mut cmd, policy)?;
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

/// Set up Landlock FS access rules from the policy and apply via `pre_exec`.
///
/// The ruleset is built in the parent process (allocations OK here).
/// `restrict_self()` is called in the `pre_exec` closure in the child
/// process — it issues a single `landlock_restrict_self` syscall (async-signal-safe).
#[cfg(feature = "landlock")]
fn apply_landlock_pre_exec(
    cmd: &mut tokio::process::Command,
    policy: &SandboxPolicy,
) -> Result<(), SandboxError> {
    use std::os::unix::process::CommandExt;

    let ruleset = match build_landlock_ruleset(policy) {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(
                "Linux sandbox: Landlock ruleset creation failed ({e}), running with env restrictions only"
            );
            return Ok(());
        }
    };

    // SAFETY: `pre_exec` runs between fork and exec in the child process.
    // `RulesetCreated::restrict_self()` issues a single
    // `landlock_restrict_self` syscall on the success path (async-signal-safe).
    // On error, the child process will abort before exec, which is safe.
    unsafe {
        cmd.pre_exec(move || {
            ruleset
                .restrict_self()
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))
        });
    }

    Ok(())
}

/// Build a Landlock ruleset from the policy.
///
/// Creates a ruleset that:
/// 1. Denies all FS access by default
/// 2. Allows executing binaries from standard bin dirs
/// 3. Allows reading from lib dirs (needed for dynamic linker)
/// 4. Allows reading from `policy.read_paths`
/// 5. Allows reading + writing to `policy.write_paths`
#[cfg(feature = "landlock")]
fn build_landlock_ruleset(
    policy: &SandboxPolicy,
) -> Result<landlock::RulesetCreated, SandboxError> {
    use landlock::{AccessFs, PathBeneath, PathFd, Ruleset};

    let abi = landlock::ABI::V1;
    let read_access = AccessFs::from_read(abi);
    let write_access = AccessFs::from_write(abi);

    let mut ruleset = Ruleset::new()
        .handle_access(AccessFs::all())
        .map_err(|e| SandboxError::PolicyError(format!("Landlock ruleset init: {e}")))?
        .create()
        .map_err(|e| SandboxError::PolicyError(format!("Landlock ruleset create: {e}")))?;

    // Allow executing standard binaries
    for bin_dir in ["/usr/bin", "/bin", "/usr/local/bin"] {
        if let Ok(fd) = PathFd::new(bin_dir) {
            if let Err(e) = ruleset.add_rule(PathBeneath::new(fd, AccessFs::execute())) {
                tracing::debug!("Landlock: skipping bin dir {bin_dir}: {e}");
            }
        }
    }

    // Allow reading /usr/lib, /lib (needed for dynamic linker)
    for lib_dir in ["/usr/lib", "/lib", "/usr/local/lib"] {
        if let Ok(fd) = PathFd::new(lib_dir) {
            if let Err(e) = ruleset.add_rule(PathBeneath::new(fd, read_access)) {
                tracing::debug!("Landlock: skipping lib dir {lib_dir}: {e}");
            }
        }
    }

    // Allow reading from specified paths
    for path in &policy.read_paths {
        if let Ok(fd) = PathFd::new(path) {
            if let Err(e) = ruleset.add_rule(PathBeneath::new(fd, read_access)) {
                tracing::warn!("Landlock: skipping read path {}: {e}", path.display());
            }
        } else {
            tracing::debug!("Landlock: read path {} not found, skipping", path.display());
        }
    }

    // Allow writing to specified paths
    for path in &policy.write_paths {
        if let Ok(fd) = PathFd::new(path) {
            if let Err(e) = ruleset.add_rule(PathBeneath::new(fd, write_access)) {
                tracing::warn!("Landlock: skipping write path {}: {e}", path.display());
            }
        } else {
            tracing::debug!(
                "Landlock: write path {} not found, skipping",
                path.display()
            );
        }
    }

    Ok(ruleset)
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
