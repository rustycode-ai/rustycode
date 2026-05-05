//! macOS Seatbelt sandbox backend using `sandbox-exec`.

use std::process::Stdio;

use crate::error::SandboxError;
use crate::policy::NetworkAccess;
use crate::{SandboxPolicy, SandboxResult};

/// Characters that could break out of SBPL string literals.
const SBPL_DANGEROUS: &[char] = &['"', '\\', '(', ')', '\n', '\r', '\0'];

/// Validate a path is safe to embed in SBPL. Returns the display form or an error.
fn validate_sbpl_path(path: &std::path::Path) -> Result<std::path::Display<'_>, SandboxError> {
    let s = path.to_string_lossy();
    if s.contains(SBPL_DANGEROUS) {
        return Err(SandboxError::ExecutionFailed(format!(
            "Path contains disallowed characters: {}",
            s
        )));
    }
    Ok(path.display())
}

/// Generate SBPL (Seatbelt Policy Language) from a `SandboxPolicy`.
fn generate_sbpl(policy: &SandboxPolicy) -> Result<String, SandboxError> {
    let mut rules = Vec::new();

    rules.push("(version 1)".to_string());
    rules.push("(deny default)".to_string());

    // Allow reading from specified paths
    for path in &policy.read_paths {
        let p = validate_sbpl_path(path)?;
        rules.push(format!(r#"(allow file-read* (subpath "{p}"))"#));
    }

    // Allow writing to specified paths
    for path in &policy.write_paths {
        let p = validate_sbpl_path(path)?;
        rules.push(format!(r#"(allow file-write* (subpath "{p}"))"#));
    }

    // Process execution
    rules.push("(allow process-exec (literal \"/bin/sh\"))".to_string());
    rules.push("(allow process-exec (literal \"/bin/bash\"))".to_string());
    rules.push("(allow process-exec (literal \"/bin/zsh\"))".to_string());
    rules.push("(allow process-exec (literal \"/usr/bin/env\"))".to_string());

    // Network
    match policy.network {
        NetworkAccess::Denied => {
            rules.push("(deny network*)".to_string());
        }
        NetworkAccess::Allowed => {
            rules.push("(allow network*)".to_string());
        }
    }

    // Always allow basic signals and sysctl
    rules.push("(allow signal)".to_string());
    rules.push("(allow sysctl-read)".to_string());

    Ok(rules.join("\n"))
}

/// Check if sandbox-exec is available on this system.
pub fn is_seatbelt_available() -> bool {
    std::path::Path::new("/usr/bin/sandbox-exec").exists()
}

/// Execute a command under macOS Seatbelt sandbox.
pub async fn execute_sandboxed(
    command: &str,
    policy: &SandboxPolicy,
) -> Result<SandboxResult, SandboxError> {
    if !is_seatbelt_available() {
        return Err(SandboxError::NotAvailable(
            "sandbox-exec not found".to_string(),
        ));
    }

    let sbpl = generate_sbpl(policy)?;

    let mut cmd = tokio::process::Command::new("/usr/bin/sandbox-exec");
    cmd.arg("-p")
        .arg(&sbpl)
        .arg("/bin/sh")
        .arg("-c")
        .arg(command)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    // Pass through allowed environment variables
    if !policy.env_passthrough.is_empty() {
        cmd.env_clear();
        for var in &policy.env_passthrough {
            if let Ok(val) = std::env::var(var) {
                cmd.env(var, val);
            }
        }
    }

    let timed_out = if let Some(timeout_secs) = policy.timeout_secs {
        match tokio::time::timeout(std::time::Duration::from_secs(timeout_secs), cmd.output()).await
        {
            Ok(Ok(output)) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);
                if stdout.contains('\u{fffd}') || stderr.contains('\u{fffd}') {
                    tracing::debug!(
                        "sandbox output contained non-UTF-8 bytes (replaced with U+FFFD)"
                    );
                }
                return Ok(SandboxResult {
                    exit_code: output.status.code(),
                    stdout: stdout.to_string(),
                    stderr: stderr.to_string(),
                    timed_out: false,
                });
            }
            Ok(Err(e)) => return Err(SandboxError::ExecutionFailed(e.to_string())),
            Err(_) => true,
        }
    } else {
        let output = cmd.output().await.map_err(SandboxError::Io)?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stdout.contains('\u{fffd}') || stderr.contains('\u{fffd}') {
            tracing::debug!("sandbox output contained non-UTF-8 bytes (replaced with U+FFFD)");
        }
        return Ok(SandboxResult {
            exit_code: output.status.code(),
            stdout: stdout.to_string(),
            stderr: stderr.to_string(),
            timed_out: false,
        });
    };

    // Kill the process on timeout
    Ok(SandboxResult {
        exit_code: None,
        stdout: String::new(),
        stderr: "Command timed out".to_string(),
        timed_out,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_generate_sbpl_restrictive() {
        let policy = SandboxPolicy::restrictive(PathBuf::from("/tmp/workspace").as_path());
        let sbpl = generate_sbpl(&policy).unwrap();
        assert!(sbpl.contains("(deny default)"));
        assert!(sbpl.contains("(deny network*)"));
        assert!(sbpl.contains(r#"(allow file-read* (subpath "/tmp/workspace"))"#));
    }

    #[test]
    fn test_generate_sbpl_permissive() {
        let policy = SandboxPolicy::permissive(PathBuf::from("/tmp/workspace").as_path());
        let sbpl = generate_sbpl(&policy).unwrap();
        assert!(sbpl.contains("(allow network*)"));
    }
}
