//! Shared filesystem utilities used across multiple tool providers.
//!
//! `ReadFileTool` and `WriteFileTool` import `is_blocked_extension` and
//! `is_blocked_filename` from here.
//! Web content utilities (`WEB_FETCH_MAX_CHARS`, `is_html_content`,
//! `html_to_simple_markdown`, `truncate_to_char_boundary`) have been
//! moved to `crate::providers::web::content`.

pub mod apply_patch;
pub mod edit;
pub mod list_dir;
pub mod multiedit;
pub mod read_file;
pub mod write_file;

// Re-exports for backward-compatible access
#[allow(ambiguous_glob_reexports)]
pub use apply_patch::*;
#[allow(ambiguous_glob_reexports)]
pub use edit::*;
#[allow(ambiguous_glob_reexports)]
pub use list_dir::*;
#[allow(ambiguous_glob_reexports)]
pub use multiedit::*;
#[allow(ambiguous_glob_reexports)]
pub use read_file::*;
#[allow(ambiguous_glob_reexports)]
pub use write_file::*;

use crate::security::{validation::BLOCKED_FILENAMES, BLOCKED_EXTENSIONS};
use std::path::Path;

/// Check if a file extension is blocked for security reasons
pub(crate) fn is_blocked_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| {
            // BLOCKED_EXTENSIONS entries include the leading dot (e.g., ".env", ".exe")
            let dotted = format!(".{}", ext.to_lowercase());
            BLOCKED_EXTENSIONS.contains(&dotted.as_str())
        })
        .unwrap_or(false)
}

/// Check if a filename is blocked for security reasons
pub(crate) fn is_blocked_filename(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(|name| BLOCKED_FILENAMES.contains(&name))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── is_blocked_extension tests ─────────────

    #[test]
    fn test_is_blocked_extension_env() {
        assert!(!is_blocked_extension(Path::new(".env")));
        assert!(is_blocked_extension(Path::new("local.env")));
    }

    #[test]
    fn test_is_blocked_extension_secrets() {
        assert!(is_blocked_extension(Path::new("id_rsa.key")));
        assert!(is_blocked_extension(Path::new("cert.pem")));
        assert!(is_blocked_extension(Path::new("app.exe")));
        assert!(is_blocked_extension(Path::new("lib.so")));
        assert!(is_blocked_extension(Path::new("lib.dylib")));
    }

    #[test]
    fn test_is_blocked_extension_not_text() {
        assert!(!is_blocked_extension(Path::new("main.rs")));
        assert!(!is_blocked_extension(Path::new("app.py")));
        assert!(!is_blocked_extension(Path::new("README.md")));
    }

    #[test]
    fn test_is_blocked_filename_credentials() {
        assert!(is_blocked_filename(Path::new("credentials.json")));
        assert!(is_blocked_filename(Path::new(".credentials.json")));
        assert!(is_blocked_filename(Path::new("id_rsa")));
        assert!(is_blocked_filename(Path::new("id_ed25519")));
        assert!(is_blocked_filename(Path::new("terraform.tfstate")));
    }

    #[test]
    fn test_is_blocked_filename_not_blocked() {
        assert!(!is_blocked_filename(Path::new("package.json")));
        assert!(!is_blocked_filename(Path::new("Cargo.toml")));
        assert!(!is_blocked_filename(Path::new("main.rs")));
        assert!(!is_blocked_filename(Path::new("README.md")));
    }

    #[test]
    fn test_is_blocked_filename_nested_path() {
        assert!(is_blocked_filename(Path::new("/home/user/.ssh/id_rsa")));
        assert!(!is_blocked_filename(Path::new("/home/user/src/main.rs")));
    }
}
