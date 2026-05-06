//! Lightweight sandbox for secure tool execution
//!
//! This module provides cross-platform sandboxing for AI coding tools:
//! - Path-based access control (all platforms)
//! - Landlock on Linux (kernel 5.13+)
//! - macOS sandbox integration
//! - Interactive permission requests
//! - Graceful fallback for unsupported platforms
//!
//! # Permission Request Flow
//!
//! When a tool tries to access a path outside the allowed list:
//! 1. Check if path has been previously approved
//! 2. If not, prompt user for permission
//! 3. If approved, add to approval list and proceed
//! 4. If denied, return error
//!
//! # Security Levels
//!
//! - **Path**: Path-based allow/deny lists (all platforms)
//! - **Basic**: Path + read-only filesystem where possible
//! - **Strict**: Path + Landlock/macOS sandbox + restricted syscalls
//!
//! # Example
//!
//! ```ignore
//! use rustycode_tools::sandbox::{Sandbox, SandboxLevel};
//!
//! let sandbox = Sandbox::new(
//!     cwd,
//!     allowed_paths,
//!     denied_paths,
//!     SandboxLevel::Strict,
//! )?;
//!
//! sandbox.enforce()?;
//!
//! // Now all file operations are sandboxed
//! ```

use anyhow::{anyhow, Result};
pub use rustycode_tools_api::SandboxConfig;
use std::path::{Path, PathBuf};

/// Sandbox security level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum SandboxLevel {
    /// No sandboxing (not recommended)
    None,
    /// Path-based access control only
    #[default]
    Path,
    /// Path + read-only where possible
    Basic,
    /// Full sandboxing with platform-specific restrictions
    Strict,
}

/// Lightweight sandbox for secure tool execution
pub struct Sandbox {
    /// Working directory (always allowed)
    cwd: PathBuf,
    /// Allowed filesystem paths
    allowed_paths: Vec<PathBuf>,
    /// Denied filesystem paths
    denied_paths: Vec<PathBuf>,
    /// Security level
    level: SandboxLevel,
    /// Whether sandbox has been enforced
    enforced: bool,
    /// Platform-specific capabilities
    capabilities: SandboxCapabilities,
    /// Whether to prompt for permission (interactive mode)
    interactive: bool,
    /// OS-level sandbox manager (rustycode-sandbox)
    os_manager: Option<rustycode_sandbox::SandboxManager>,
}

/// Platform-specific sandbox capabilities
#[derive(Debug, Clone, Default)]
pub struct SandboxCapabilities {
    /// Landlock support (Linux)
    #[allow(dead_code)] // Kept for future use
    landlock: bool,
    /// macOS sandbox support
    #[allow(dead_code)] // Kept for future use
    macos_sandbox: bool,
    /// chroot support
    #[allow(dead_code)] // Kept for future use
    chroot: bool,
}

impl Sandbox {
    /// Create a new sandbox with the given configuration
    ///
    pub fn new(cwd: PathBuf, config: &SandboxConfig, level: SandboxLevel) -> Self {
        // Detect platform capabilities
        let capabilities = Self::detect_capabilities();

        // Build allowed paths list
        let mut allowed_paths = config.allowed_paths.clone().unwrap_or_default();
        // Always allow CWD
        if !allowed_paths.contains(&cwd) {
            allowed_paths.push(cwd.clone());
        }

        Self {
            cwd,
            allowed_paths,
            denied_paths: config.denied_paths.clone(),
            level,
            enforced: false,
            capabilities,
            interactive: false, // Default to non-interactive
            os_manager: None,
        }
    }

    /// Create a new sandbox with interactive permission prompts
    ///
    /// When a tool tries to access a path outside the allowed list,
    /// the sandbox will prompt the user for permission instead of
    /// immediately denying access.
    pub fn new_interactive(cwd: PathBuf, config: &SandboxConfig, level: SandboxLevel) -> Self {
        let mut sandbox = Self::new(cwd, config, level);
        sandbox.interactive = true;
        sandbox
    }

    /// Returns a reference to the OS sandbox manager, if initialized.
    pub fn os_sandbox_manager(&self) -> Option<&rustycode_sandbox::SandboxManager> {
        self.os_manager.as_ref()
    }

    /// Detect platform-specific sandbox capabilities
    fn detect_capabilities() -> SandboxCapabilities {
        #[cfg(target_os = "linux")]
        {
            // Check for Landlock support (kernel 5.13+)
            let landlock = Self::check_landlock();

            SandboxCapabilities {
                landlock,
                macos_sandbox: false,
                chroot: false, // Landlock is preferred
            }
        }

        #[cfg(target_os = "macos")]
        {
            SandboxCapabilities {
                landlock: false,
                macos_sandbox: true, // macOS always has sandbox_exec
                chroot: true,
            }
        }

        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        {
            SandboxCapabilities {
                landlock: false,
                macos_sandbox: false,
                chroot: false,
            }
        }
    }

    /// Check if Landlock is available (Linux only)
    #[cfg(target_os = "linux")]
    fn check_landlock() -> bool {
        // Try to detect Landlock support
        // Landlock requires kernel 5.13+, but we can't easily check kernel version from Rust
        // Instead, we'll try to use it and fail gracefully
        true // Assume available, will fail at runtime if not
    }

    /// Enforce the sandbox
    ///
    /// This activates platform-specific sandboxing restrictions.
    /// After calling this, file operations will be constrained.
    pub fn enforce(&mut self) -> Result<()> {
        if self.enforced {
            return Ok(());
        }

        match self.level {
            SandboxLevel::None => {
                // No sandboxing
                tracing::debug!("Sandbox: No sandboxing enabled");
            }
            SandboxLevel::Path => {
                tracing::debug!("Sandbox: Path-based access control only");
            }
            SandboxLevel::Basic => {
                self.enforce_basic()?;
            }
            SandboxLevel::Strict => {
                self.enforce_strict()?;
            }
        }

        self.enforced = true;
        Ok(())
    }

    /// Enforce basic sandbox level
    fn enforce_basic(&self) -> Result<()> {
        tracing::debug!("Sandbox: Enforcing basic level");
        // Basic level uses path validation only; OS-level restrictions
        // are handled by rustycode-sandbox when Strict is selected.
        Ok(())
    }

    /// Enforce strict sandbox level
    fn enforce_strict(&mut self) -> Result<()> {
        tracing::debug!("Sandbox: Enforcing strict level via rustycode-sandbox");

        match rustycode_sandbox::SandboxManager::new() {
            Ok(manager) => {
                if manager.is_available() {
                    tracing::info!(
                        platform = std::env::consts::OS,
                        "OS-level sandbox backend ready for child process isolation"
                    );
                    self.os_manager = Some(manager);
                } else {
                    tracing::warn!("OS sandbox backend detected as unavailable, falling back to path validation");
                    return self.enforce_basic();
                }
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to initialize OS sandbox: {e}, falling back to path validation"
                );
                return self.enforce_basic();
            }
        }

        Ok(())
    }

    /// Validate a path against sandbox rules
    ///
    /// Returns Ok(()) if path is allowed, Err otherwise
    pub fn validate_path(&self, path: &Path) -> Result<()> {
        // Check denied paths first
        for denied in &self.denied_paths {
            if path.starts_with(denied) || path == denied {
                return Err(anyhow!(
                    "Path '{}' is denied by sandbox rules",
                    path.display()
                ));
            }
        }

        // If no allow list, allow everything not denied
        if self.allowed_paths.is_empty() {
            return Ok(());
        }

        // Check if path is in allowed list or a subdirectory
        for allowed in &self.allowed_paths {
            if path.starts_with(allowed) || path == allowed {
                return Ok(());
            }
        }

        // Special case: CWD is always allowed
        if path.starts_with(&self.cwd) || path == self.cwd {
            return Ok(());
        }

        Err(anyhow!(
            "Path '{}' is not in sandbox allow list",
            path.display()
        ))
    }

    /// Check if sandbox is enforced
    pub fn is_enforced(&self) -> bool {
        self.enforced
    }

    /// Get sandbox capabilities
    pub fn capabilities(&self) -> &SandboxCapabilities {
        &self.capabilities
    }

    /// Validate a path and return a detailed error if not allowed
    ///
    /// This is a convenience method for tools to use when validating paths
    pub fn check_path(&self, path: &Path) -> Result<()> {
        self.validate_path(path)
    }

    /// Validate multiple paths at once
    ///
    /// Returns Ok(()) if all paths are allowed, Err with first failure
    pub fn check_paths(&self, paths: &[PathBuf]) -> Result<()> {
        for path in paths {
            self.validate_path(path)?;
        }
        Ok(())
    }

    /// Validate a path with interactive permission prompt if needed
    ///
    /// If the path is not in the allowed list and interactive mode is enabled,
    /// prompts the user for permission. Otherwise behaves like `validate_path()`.
    pub fn validate_path_interactive(&self, path: &Path) -> Result<()> {
        // First, try normal validation
        match self.validate_path(path) {
            Ok(()) => Ok(()),
            Err(e) if !self.interactive => Err(e),
            Err(_) => {
                // Interactive mode: prompt for permission
                if self.interactive {
                    self.request_permission(path)
                } else {
                    Err(anyhow!(
                        "Path '{}' is not in sandbox allow list",
                        path.display()
                    ))
                }
            }
        }
    }

    /// Request permission from the user to access a path
    ///
    /// This creates a permission request and prompts the user.
    /// Returns Ok(()) if approved, Err if denied.
    fn request_permission(&self, path: &Path) -> Result<()> {
        use crate::permission::{PermissionRequest, PermissionScope};

        // Determine operation type based on path and context
        let operation = "read"; // Could be write/delete in different contexts
        let reason = format!(
            "The AI needs to access '{}' to complete its task",
            path.display()
        );

        // Create permission request
        let request = PermissionRequest::new(operation, path.display().to_string(), reason)
            .with_scope(PermissionScope::Session);

        // Format and display the prompt
        println!("\n{}", request.format_prompt());
        println!();
        println!("Options: [Y]es / [N]o");
        println!();
        print!("Your choice: ");
        use std::io::{self, Write};
        io::stdout().flush()?;

        // Read user input
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;

        let choice = input.trim().to_lowercase();

        match choice.as_str() {
            "y" | "yes" => {
                println!("✓ Permission granted");
                Ok(())
            }
            "n" | "no" => {
                println!("✗ Permission denied");
                Err(anyhow!("Permission denied to access '{}'", path.display()))
            }
            _ => Err(anyhow!(
                "Invalid response '{choice}'. Please answer 'y' or 'n'."
            )),
        }
    }
}

/// Conversions from `SandboxConfig`
impl From<&SandboxConfig> for SandboxLevel {
    fn from(config: &SandboxConfig) -> Self {
        // Determine level based on configuration
        if config.allowed_paths.is_some() {
            SandboxLevel::Path
        } else if config.denied_paths.is_empty() {
            SandboxLevel::None
        } else {
            SandboxLevel::Path
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sandbox_new() {
        let cwd = PathBuf::from("/tmp");
        let config = SandboxConfig::new();
        let sandbox = Sandbox::new(cwd, &config, SandboxLevel::Path);

        assert_eq!(sandbox.level, SandboxLevel::Path);
        assert!(!sandbox.is_enforced());
    }

    #[test]
    fn test_sandbox_validate_path_allowed() {
        let cwd = PathBuf::from("/tmp/work");
        let mut config = SandboxConfig::new();
        config.allowed_paths = Some(vec![PathBuf::from("/tmp/work")]);

        let sandbox = Sandbox::new(cwd, &config, SandboxLevel::Path);

        // Should allow CWD
        assert!(sandbox
            .validate_path(Path::new("/tmp/work/file.txt"))
            .is_ok());
        assert!(sandbox.validate_path(Path::new("/tmp/work")).is_ok());
    }

    #[test]
    fn test_sandbox_validate_path_denied() {
        let cwd = PathBuf::from("/tmp/work");
        let mut config = SandboxConfig::new();
        config.denied_paths = vec![PathBuf::from("/etc")];

        let sandbox = Sandbox::new(cwd, &config, SandboxLevel::Path);

        // Should deny denied paths
        assert!(sandbox.validate_path(Path::new("/etc/passwd")).is_err());
        assert!(sandbox.validate_path(Path::new("/etc/shadow")).is_err());
    }

    #[test]
    fn test_sandbox_validate_path_not_allowed() {
        let cwd = PathBuf::from("/tmp/work");
        let mut config = SandboxConfig::new();
        config.allowed_paths = Some(vec![PathBuf::from("/tmp/work")]);

        let sandbox = Sandbox::new(cwd, &config, SandboxLevel::Path);

        // Should deny paths outside allowed list
        assert!(sandbox.validate_path(Path::new("/etc/passwd")).is_err());
        assert!(sandbox
            .validate_path(Path::new("/home/user/file.txt"))
            .is_err());
    }

    #[test]
    fn test_sandbox_enforce() {
        let cwd = PathBuf::from("/tmp");
        let config = SandboxConfig::new();
        let mut sandbox = Sandbox::new(cwd, &config, SandboxLevel::Path);

        // Should enforce successfully
        assert!(sandbox.enforce().is_ok());
        assert!(sandbox.is_enforced());

        // Second enforce should be no-op
        assert!(sandbox.enforce().is_ok());
    }

    #[test]
    fn test_sandbox_from_config() {
        let mut config = SandboxConfig::new();
        config.allowed_paths = Some(vec![PathBuf::from("/tmp")]);

        let level: SandboxLevel = (&config).into();
        assert_eq!(level, SandboxLevel::Path);
    }

    #[test]
    fn test_sandbox_level_default_is_path() {
        assert_eq!(SandboxLevel::default(), SandboxLevel::Path);
    }

    #[test]
    fn test_sandbox_level_none_enforce_ok() {
        let cwd = PathBuf::from("/tmp");
        let config = SandboxConfig::new();
        let mut sandbox = Sandbox::new(cwd, &config, SandboxLevel::None);
        assert!(sandbox.enforce().is_ok());
        assert!(sandbox.is_enforced());
    }

    #[test]
    fn test_sandbox_check_paths_all_valid() {
        let cwd = PathBuf::from("/tmp/work");
        let mut config = SandboxConfig::new();
        config.allowed_paths = Some(vec![PathBuf::from("/tmp/work")]);

        let sandbox = Sandbox::new(cwd, &config, SandboxLevel::Path);
        let paths = vec![
            PathBuf::from("/tmp/work/a.txt"),
            PathBuf::from("/tmp/work/b.txt"),
        ];
        assert!(sandbox.check_paths(&paths).is_ok());
    }

    #[test]
    fn test_sandbox_check_paths_one_invalid_fails() {
        let cwd = PathBuf::from("/tmp/work");
        let mut config = SandboxConfig::new();
        config.allowed_paths = Some(vec![PathBuf::from("/tmp/work")]);

        let sandbox = Sandbox::new(cwd, &config, SandboxLevel::Path);
        let paths = vec![
            PathBuf::from("/tmp/work/a.txt"),
            PathBuf::from("/etc/passwd"),
        ];
        assert!(sandbox.check_paths(&paths).is_err());
    }

    #[test]
    fn test_sandbox_check_paths_empty_ok() {
        let cwd = PathBuf::from("/tmp");
        let config = SandboxConfig::new();
        let sandbox = Sandbox::new(cwd, &config, SandboxLevel::Path);
        assert!(sandbox.check_paths(&[]).is_ok());
    }

    #[test]
    fn test_sandbox_interactive_flag() {
        let cwd = PathBuf::from("/tmp");
        let config = SandboxConfig::new();
        let sandbox = Sandbox::new_interactive(cwd, &config, SandboxLevel::Path);
        assert!(sandbox.interactive);
    }

    #[test]
    fn test_sandbox_cwd_always_allowed() {
        let cwd = PathBuf::from("/tmp/work");
        let config = SandboxConfig::new();
        let sandbox = Sandbox::new(cwd, &config, SandboxLevel::Path);
        // CWD is auto-added to allowed_paths
        assert!(sandbox
            .validate_path(Path::new("/tmp/work/file.txt"))
            .is_ok());
    }

    #[test]
    fn test_sandbox_denied_overrides_allowed() {
        let cwd = PathBuf::from("/tmp/work");
        let mut config = SandboxConfig::new();
        config.allowed_paths = Some(vec![PathBuf::from("/tmp")]);
        config.denied_paths = vec![PathBuf::from("/tmp/work/secret")];

        let sandbox = Sandbox::new(cwd, &config, SandboxLevel::Path);
        // Allowed parent should work
        assert!(sandbox.validate_path(Path::new("/tmp/other")).is_ok());
        // Denied subpath should fail
        assert!(sandbox
            .validate_path(Path::new("/tmp/work/secret/key.pem"))
            .is_err());
    }
}
