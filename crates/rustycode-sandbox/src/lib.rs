//! OS-level sandboxing for safe command execution.
//!
//! Provides platform-specific sandbox backends:
//! - macOS: Seatbelt (sandbox-exec)
//! - Linux: landlock (planned)

pub mod error;
pub mod manager;
pub mod policy;

#[cfg(target_os = "macos")]
pub mod seatbelt;

#[cfg(target_os = "linux")]
pub mod linux;

#[cfg(target_os = "windows")]
pub mod windows;

pub use error::SandboxError;
pub use manager::SandboxManager;
pub use policy::{NetworkAccess, SandboxPolicy};

/// Result of a sandboxed execution.
#[derive(Debug, Clone)]
pub struct SandboxResult {
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub timed_out: bool,
}
