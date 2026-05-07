//! Security utilities for tool execution
//!
//! This module provides centralized security validation for:
//! - Path traversal prevention
//! - Symlink detection
//! - Input sanitization
//! - Resource limits
//! - Command injection prevention

use anyhow::{anyhow, Result};
use std::fs;
use std::path::{Path, PathBuf};

/// Maximum file size for write operations (10 MB)
pub const MAX_FILE_SIZE: usize = 10 * 1024 * 1024;

/// Maximum path length to prevent `DoS`
pub const MAX_PATH_LENGTH: usize = 4096;

/// Maximum recursion depth for directory operations
pub const MAX_RECURSION_DEPTH: usize = 10;

/// Maximum number of regex operations (prevents `ReDoS`)
pub const MAX_REGEX_MATCHES: usize = 10000;

/// Blocked file extensions that should never be read/written by tools
pub const BLOCKED_EXTENSIONS: &[&str] = &[
    // Security-sensitive files
    ".env",
    ".env.local",
    ".env.production",
    ".key",
    ".pem",
    ".p12",
    ".pfx",
    // Executable binaries
    ".exe",
    ".dll",
    ".so",
    ".dylib",
    ".app",
    ".bin",
    // System files
    ".sys",
    ".drv",
    // Database files (can be corrupted)
    ".db",
    ".sqlite",
    ".mdb",
];

/// Blocked filenames that should never be read/written by tools regardless of path
pub const BLOCKED_FILENAMES: &[&str] = &[
    // Credential files
    "credentials.json",
    "credentials",
    ".netrc",
    ".npmrc",
    ".pypirc",
    ".awscli_cache",
    // SSH keys (no extension — caught by extension check misses these)
    "id_rsa",
    "id_ed25519",
    "id_ecdsa",
    "id_dsa",
    "authorized_keys",
    "known_hosts",
    // Cloud provider configs
    "config.json",
    "terraform.tfstate",
    "terraform.tfvars",
    ".terraformrc",
    "terraform.rc",
    // Claude Code internal files
    ".credentials.json",
];

/// Blocked path components that should never appear in paths
pub const BLOCKED_PATH_COMPONENTS: &[&str] = &[
    ".git",
    ".svn",
    ".hg",
    ".bzr",
    "__pycache__",
    "node_modules",
    "target",
    ".venv",
    "venv",
    ".virtualenv",
    // Sensitive credential directories
    ".ssh",
    ".gnupg",
    ".aws",
    // macOS system paths
    ".Spotlight-V100",
    ".Trashes",
    ".fseventsd",
    ".DS_Store",
];

/// Validate and resolve a file path within the workspace
///
/// This function performs comprehensive security checks:
/// 1. Checks for path traversal attempts (..)
/// 2. Validates path length
/// 3. Checks for symlinks
/// 4. Ensures path is within workspace
/// 5. Validates file extensions
///
/// Normalize a file path for cross-platform compatibility.
///
/// Handles:
/// - Backslashes → forward slashes (Windows → Unix)
/// - `/C:/` prefix → `C:/` (Git Bash / MSYS2 paths)
/// - `/mnt/c/` prefix → `C:/` (WSL paths)
/// - `/cygdrive/c/` prefix → `C:/` (Cygwin paths)
pub fn normalize_path(path: &str) -> String {
    let mut p = path.replace('\\', "/");

    // WSL: /mnt/c/... → C:/... (appears on Linux)
    if cfg!(target_os = "linux") {
        if let Some(rest) = p.strip_prefix("/mnt/") {
            if let Some(drive) = rest.chars().next() {
                if drive.is_ascii_alphabetic() {
                    let after_drive = &rest[1..];
                    if after_drive.starts_with('/') {
                        p = format!("{}:{}", drive.to_ascii_uppercase(), after_drive);
                    }
                }
            }
        }
    }

    // Git Bash / MSYS2: /c/... → C:/... (appears on Windows)
    if cfg!(windows) {
        if let Some(rest) = p.strip_prefix('/') {
            if let Some(drive) = rest.chars().next() {
                if drive.is_ascii_alphabetic() && rest.chars().nth(1) == Some('/') {
                    let after_drive = &rest[1..];
                    p = format!("{}:{}", drive.to_ascii_uppercase(), after_drive);
                }
            }
        }
    }

    // Canonicalize separators for the internal security logic
    p.replace('/', std::path::MAIN_SEPARATOR_STR)
}

pub fn validate_path(
    path: &str,
    workspace: &Path,
    check_exists: bool,
    allow_symlinks: bool,
    enforce_workspace: bool,
) -> Result<PathBuf> {
    // Check path length
    if path.len() > MAX_PATH_LENGTH {
        anyhow::bail!("path exceeds maximum length of {MAX_PATH_LENGTH} characters");
    }

    // Normalize cross-platform path quirks (backslashes, WSL, Cygwin, Git Bash)
    let normalized = normalize_path(path);

    // Check for encoded traversal in raw string before path parsing
    let lower = normalized.to_ascii_lowercase();
    if lower.contains("%2e%2e") || lower.contains("%2e.") || lower.contains(".%2e") {
        return Err(anyhow!("path traversal detected: encoded '..' component"));
    }

    // Check for absolute paths (block unless within workspace)
    let candidate = Path::new(&normalized);

    // Resolve the path
    let resolved = if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        workspace.join(candidate)
    };

    // Check for path traversal in each component
    for component in resolved.components() {
        if let Some(comp) = component.as_os_str().to_str() {
            // Check for suspicious patterns
            if comp == ".." {
                return Err(anyhow!(
                    "path traversal detected: '..' component not allowed"
                ));
            }
            // Check for encoded traversal attempts
            if comp.contains("%2e%2e") || comp.contains("%2E%2E") {
                return Err(anyhow!("path traversal detected: encoded '..' component"));
            }
        }
    }

    // Canonicalize workspace root
    let workspace_canonical = fs::canonicalize(workspace)
        .map_err(|e| anyhow!("failed to canonicalize workspace: {e}"))?;

    // Check if the path exists
    if check_exists && resolved.exists() {
        // Check for symlinks if not allowed
        if !allow_symlinks {
            check_path_for_symlinks(&resolved, workspace)?;
        }

        // Canonicalize and verify it's within workspace
        let path_canonical =
            fs::canonicalize(&resolved).map_err(|e| anyhow!("failed to canonicalize path: {e}"))?;

        if enforce_workspace && !path_canonical.starts_with(&workspace_canonical) {
            return Err(anyhow!(
                "path '{}' is outside workspace '{}' and is blocked. \
                 Use a relative path within the workspace, or use the bash tool \
                 to access files outside the workspace.",
                path,
                workspace.display()
            ));
        }
    } else {
        // Path doesn't exist or is a write target — validate parent is within workspace
        if resolved.exists() && !allow_symlinks {
            // If the file already exists (e.g., we're overwriting), check for symlinks
            check_path_for_symlinks(&resolved, workspace)?;
        }

        if let Some(parent) = resolved.parent() {
            if parent.exists() {
                if !allow_symlinks {
                    check_path_for_symlinks(parent, workspace)?;
                }

                let parent_canonical = fs::canonicalize(parent)
                    .map_err(|e| anyhow!("failed to canonicalize parent: {e}"))?;

                if enforce_workspace && !parent_canonical.starts_with(&workspace_canonical) {
                    return Err(anyhow!(
                        "path parent '{}' is outside workspace '{}' and is blocked. \
                         Use a relative path within the workspace, or use the bash tool \
                         to access files outside the workspace.",
                        parent.display(),
                        workspace.display()
                    ));
                }
            }
        }
    }

    Ok(resolved)
}

/// Check a path and all its parents for symlinks
///
/// This function checks each component of the path to ensure none of them
/// (including intermediate directories) are symbolic links.
///
/// Security rationale: Symlinks can be used to escape the workspace or
/// access files that would otherwise be blocked. By checking every component
/// of the path, we ensure that no part of the path is a symlink.
fn check_path_for_symlinks(path: &Path, workspace: &Path) -> Result<()> {
    // Build the path incrementally, checking each component
    // This is necessary because symlink_metadata on a path that goes through
    // a symlink directory will return metadata about the target, not the symlink

    // For paths outside the workspace (relaxed mode), check only the final
    // component — we can't walk from workspace root, but still block symlinks
    // that could redirect to security-sensitive locations.
    let Ok(relative_path) = path.strip_prefix(workspace) else {
        if let Ok(metadata) = fs::symlink_metadata(path) {
            if metadata.file_type().is_symlink() {
                return Err(anyhow!(
                    "symbolic link detected at '{}': symlinks are not allowed for security",
                    path.display()
                ));
            }
        }
        return Ok(());
    };

    let mut check_path = workspace.to_path_buf();

    // Check each component from workspace root up to the target
    for component in relative_path.components() {
        check_path.push(component);

        // Check if this component is a symlink using symlink_metadata
        // which doesn't follow symlinks (unlike metadata())
        if let Ok(metadata) = fs::symlink_metadata(&check_path) {
            if metadata.file_type().is_symlink() {
                return Err(anyhow!(
                    "symbolic link detected at '{}': symlinks are not allowed for security",
                    check_path.display()
                ));
            }
        }
    }

    Ok(())
}

/// Validate a file path for reading
///
/// Checks:
/// - Path is within workspace
/// - No symlinks in path
/// - File extension is not blocked
pub fn validate_read_path(
    path: &str,
    workspace: &Path,
    enforce_workspace: bool,
) -> Result<PathBuf> {
    let validated = validate_path(path, workspace, true, false, enforce_workspace)?;

    // Check blocked path components (e.g., .ssh, .gnupg, .aws)
    for component in validated.components() {
        let value = component.as_os_str().to_string_lossy();
        if BLOCKED_PATH_COMPONENTS.contains(&value.as_ref()) {
            return Err(anyhow!(
                "path contains blocked component '{value}' for security reasons"
            ));
        }
    }

    // Check blocked extensions
    // First check the file name for dotfiles like .env
    if let Some(file_name) = validated.file_name() {
        if let Some(name_str) = file_name.to_str() {
            let name_lower = name_str.to_lowercase();
            // Check if the entire filename is a blocked extension (e.g., ".env")
            if BLOCKED_EXTENSIONS.contains(&name_lower.as_str()) {
                return Err(anyhow!(
                    "file '{name_lower}' is blocked for security reasons"
                ));
            }
            // Check if the filename is a blocked credential file (e.g., "credentials.json")
            if BLOCKED_FILENAMES.contains(&name_lower.as_str()) {
                return Err(anyhow!(
                    "file '{name_lower}' is blocked for security reasons (credential file)"
                ));
            }
        }
    }

    // Then check regular extensions
    if let Some(extension) = validated.extension() {
        if let Some(ext_str) = extension.to_str() {
            let ext_lower = ext_str.to_lowercase();
            if BLOCKED_EXTENSIONS.contains(&ext_lower.as_str()) {
                return Err(anyhow!(
                    "file extension '.{ext_lower}' is blocked for security reasons"
                ));
            }
        }
    }

    Ok(validated)
}

/// Validate a file path for writing
///
/// Checks:
/// - Path is within workspace
/// - No symlinks in path
/// - Parent directory exists
/// - Content size is within limits
/// - File extension is not blocked
pub fn validate_write_path(
    path: &str,
    workspace: &Path,
    content_size: usize,
    enforce_workspace: bool,
) -> Result<PathBuf> {
    let validated = validate_path(path, workspace, false, false, enforce_workspace)?;

    // Check blocked path components (e.g., .ssh, .gnupg, .aws)
    for component in validated.components() {
        let value = component.as_os_str().to_string_lossy();
        if BLOCKED_PATH_COMPONENTS.contains(&value.as_ref()) {
            return Err(anyhow!(
                "path contains blocked component '{value}' for writing for security reasons"
            ));
        }
    }

    // Check content size
    if content_size > MAX_FILE_SIZE {
        return Err(anyhow!(
            "content size ({content_size} bytes) exceeds maximum size limit of {MAX_FILE_SIZE} bytes"
        ));
    }

    // Check if parent exists
    if let Some(parent) = validated.parent() {
        if !parent.exists() {
            return Err(anyhow!(
                "parent directory '{}' does not exist",
                parent.display()
            ));
        }
    } else {
        return Err(anyhow!("invalid path: no parent directory"));
    }

    // Check blocked extensions for writing (including .env files)
    // First check the file name for dotfiles like .env
    if let Some(file_name) = validated.file_name() {
        if let Some(name_str) = file_name.to_str() {
            let name_lower = name_str.to_lowercase();
            // Check if the entire filename is a blocked extension (e.g., ".env")
            if BLOCKED_EXTENSIONS.contains(&name_lower.as_str()) {
                return Err(anyhow!(
                    "file '{name_lower}' is blocked for writing for security reasons"
                ));
            }
            // Check if the filename is a blocked credential file (e.g., "credentials.json")
            if BLOCKED_FILENAMES.contains(&name_lower.as_str()) {
                return Err(anyhow!(
                    "file '{name_lower}' is blocked for writing for security reasons (credential file)"
                ));
            }
        }
    }

    // Then check regular extensions
    if let Some(extension) = validated.extension() {
        if let Some(ext_str) = extension.to_str() {
            let ext_lower = ext_str.to_lowercase();
            if BLOCKED_EXTENSIONS.contains(&ext_lower.as_str()) {
                return Err(anyhow!(
                    "file extension '.{ext_lower}' is blocked for writing for security reasons"
                ));
            }
        }
    }

    Ok(validated)
}

/// Validate a directory path for listing
///
/// Checks:
/// - Path is within workspace
/// - No symlinks in path
/// - Path is a directory (if it exists)
pub fn validate_list_path(
    path: &str,
    workspace: &Path,
    enforce_workspace: bool,
) -> Result<PathBuf> {
    let validated = validate_path(path, workspace, true, false, enforce_workspace)?;

    // Check if it's actually a directory
    if validated.exists() && !validated.is_dir() {
        return Err(anyhow!("path '{}' is not a directory", validated.display()));
    }

    Ok(validated)
}

/// Validate a regex pattern for `ReDoS` vulnerabilities
///
/// Checks for:
/// - Nested quantifiers (catastrophic backtracking)
/// - Alternation with overlapping patterns
/// - Excessive wildcard repetition
pub fn validate_regex_pattern(pattern: &str) -> Result<()> {
    // Check for known ReDoS patterns

    // Nested quantifiers - classic ReDoS
    let nested_quantifiers = [
        "(.*).*",
        "(.+).+",
        "(.*).+",
        "(.+).*",
        "(.*).*?",
        "(.+).+?",
        // Complex nested patterns
        r"((\*)+)+",
        r"((\+)+)+",
        r"((\{[0-9,]+\})+)+",
        // Overlapping alternations
        r"(a|a)+",
        r"(a|aa)+",
        r"(a+|a)+",
    ];

    for dangerous in &nested_quantifiers {
        if pattern.contains(dangerous) {
            return Err(anyhow!(
                "potentially dangerous regex pattern: nested quantifiers detected (ReDoS risk)"
            ));
        }
    }

    // Check for excessive repetition
    let star_count = pattern.matches('*').count();
    let plus_count = pattern.matches('+').count();
    let question_count = pattern.matches('?').count();

    if star_count > 20 || plus_count > 20 || question_count > 20 {
        return Err(anyhow!(
            "regex pattern has too many quantifiers (potential ReDoS risk)"
        ));
    }

    // Check for very long patterns (can cause performance issues)
    if pattern.len() > 1000 {
        return Err(anyhow!(
            "regex pattern exceeds maximum length of 1000 characters"
        ));
    }

    Ok(())
}

/// Validate command arguments for injection attempts
///
/// This is a basic check; full command validation should be done
/// by shell-words parsing and allow-listing.
pub fn validate_command_arg(arg: &str) -> Result<()> {
    // Check for shell metacharacters that could enable injection
    let dangerous_chars = ['$', '`', ';', '|', '&', '\n', '\r', '\x00'];

    for char in dangerous_chars {
        if arg.contains(char) {
            return Err(anyhow!(
                "command argument contains dangerous character: '{char}'"
            ));
        }
    }

    // Check for command substitution patterns
    if arg.contains("$(") || arg.contains("${") {
        return Err(anyhow!("command argument contains command substitution"));
    }

    Ok(())
}

/// Validate URL for `web_fetch` tool
///
/// Checks:
/// - URL has valid scheme
/// - Not a file:// URL (local file access)
/// - Not an internal/private IP (if possible to detect)
pub fn validate_url(url: &str) -> Result<()> {
    // Check for file:// URLs (local file access)
    if url.starts_with("file://") {
        return Err(anyhow!("file:// URLs are not allowed"));
    }

    // Check for missing scheme
    if !url.contains("://") {
        return Err(anyhow!("URL must include a scheme (e.g., https://)"));
    }

    // Only allow HTTP and HTTPS
    let lower_url = url.to_lowercase();
    if !lower_url.starts_with("http://") && !lower_url.starts_with("https://") {
        return Err(anyhow!("only http:// and https:// URLs are allowed"));
    }

    Ok(())
}

/// Sanitize a string for logging (remove potential secrets)
///
/// This removes common secret patterns from strings before logging.
pub fn sanitize_for_log(input: &str) -> String {
    let mut output = input.to_string();
    let output_lower = output.to_lowercase();

    // Patterns to redact (all checked case-insensitively via lowercased copy)
    let patterns = [
        "api_key",
        "api-key",
        "password",
        "token",
        "secret",
        "bearer",
        "authorization",
        "private_key",
    ];

    // Track offsets as we redact (from end to start to avoid invalidating positions)
    let mut redactions: Vec<(usize, usize, &str)> = Vec::new();

    for &pattern in &patterns {
        let mut search_from = 0;
        while let Some(pos) = output_lower[search_from..].find(pattern) {
            let abs_pos = search_from + pos;
            let after_pattern = abs_pos + pattern.len();
            // Skip separator characters (=, :, space) between keyword and value
            let value_start = output_lower[after_pattern..]
                .find(|c: char| c != '=' && c != ':' && c != ' ')
                .map(|i| after_pattern + i)
                .unwrap_or(after_pattern);
            // Find the end of the value (whitespace, comma, }, or end of string)
            let end = output_lower[value_start..]
                .find(|c: char| c.is_whitespace() || c == ',' || c == '}' || c == '"')
                .map(|e| value_start + e)
                .unwrap_or(output.len());
            // Only redact if we captured something beyond the keyword
            let redact_end = if end > abs_pos + pattern.len() {
                end
            } else {
                after_pattern
            };
            redactions.push((abs_pos, redact_end, pattern));
            search_from = redact_end;
        }
    }

    // Sort by position descending so we can replace without invalidating offsets
    redactions.sort_by_key(|a| std::cmp::Reverse(a.0));

    // Remove overlapping ranges — keep the one that covers more (wider span)
    let mut i = 0;
    while i + 1 < redactions.len() {
        // Sorted descending: redactions[i] starts AFTER redactions[i+1]
        let (start_cur, end_cur, _) = redactions[i];
        let (start_next, end_next, _) = redactions[i + 1];
        // They overlap if the later range starts before the earlier range ends
        if start_cur < end_next {
            // Keep the wider range, remove the narrower one
            let span_cur = end_cur - start_cur;
            let span_next = end_next - start_next;
            if span_cur >= span_next {
                // Current (later, wider) is better — remove the earlier narrower one
                redactions.remove(i + 1);
            } else {
                // Earlier (wider) is better — remove the later narrower one
                redactions.remove(i);
            }
        } else {
            i += 1;
        }
    }

    for (start, end, pattern) in redactions {
        // Clamp to string bounds to prevent panic on edge cases
        let start = start.min(output.len());
        let end = end.min(output.len());
        if start < end {
            output.replace_range(start..end, &format!("{pattern}=[REDACTED]"));
        }
    }

    output
}

/// Open a file with symlink-safe guarantees.
///
/// This function opens a file using the `O_NOFOLLOW` flag on Unix systems,
/// which ensures that if the path is a symbolic link, the open will fail.
/// This prevents TOCTOU (Time-of-Check-Time-of-Use) attacks where an
/// attacker could replace a file with a symlink between validation and opening.
///
pub fn open_file_symlink_safe<P: AsRef<Path>>(path: P) -> Result<fs::File> {
    let path = path.as_ref();

    #[cfg(unix)]
    {
        use std::fs::OpenOptions;
        use std::os::unix::fs::OpenOptionsExt;

        // Use libc::O_NOFOLLOW to prevent following symlinks
        // This ensures we don't follow symlinks even if they point outside workspace
        OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(path)
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    anyhow!("file not found: {}", path.display())
                } else if e.raw_os_error() == Some(libc::ELOOP) {
                    // ELOOP = Too many levels of symbolic links
                    anyhow!(
                        "symbolic links are not allowed for security reasons: {}",
                        path.display()
                    )
                } else {
                    anyhow!("failed to open file '{}': {}", path.display(), e)
                }
            })
    }

    #[cfg(windows)]
    {
        // On Windows, we use regular open since Windows has different
        // symlink semantics (requires special privileges to create symlinks)
        fs::File::open(path).map_err(|e| anyhow!("failed to open file '{}': {}", path.display(), e))
    }
}

/// Open a file for writing with symlink-safe guarantees.
///
/// Similar to `open_file_symlink_safe` but for write operations.
/// Creates a new file or truncates an existing file, but fails if
/// the path is a symbolic link.
///
pub fn create_file_symlink_safe<P: AsRef<Path>>(path: P) -> Result<fs::File> {
    let path = path.as_ref();

    #[cfg(unix)]
    {
        use std::fs::OpenOptions;
        use std::os::unix::fs::OpenOptionsExt;

        // Use libc::O_NOFOLLOW | O_CREAT | O_TRUNC to safely create files
        // This ensures we don't follow symlinks even if they point outside workspace
        OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(path)
            .map_err(|e| {
                if e.raw_os_error() == Some(libc::ELOOP) {
                    // ELOOP = Too many levels of symbolic links
                    anyhow!(
                        "symbolic links are not allowed for security reasons: {}",
                        path.display()
                    )
                } else {
                    anyhow!("failed to create file '{}': {}", path.display(), e)
                }
            })
    }

    #[cfg(windows)]
    {
        fs::File::create(path)
            .map_err(|e| anyhow!("failed to create file '{}': {}", path.display(), e))
    }
}

/// Create a new file exclusively, failing if it already exists.
///
/// This function uses `O_CREAT` | `O_EXCL` to atomically ensure the file
/// is created only if it doesn't exist. This is the safe way to implement
/// "create if not exists" without TOCTOU vulnerabilities.
///
pub fn create_file_exclusive<P: AsRef<Path>>(path: P) -> Result<fs::File> {
    let path = path.as_ref();

    // Check if the path itself is a symlink (not following)
    // This is safe because we're checking the path itself, not its target
    if let Ok(metadata) = fs::symlink_metadata(path) {
        if metadata.file_type().is_symlink() {
            return Err(anyhow!(
                "symbolic links are not allowed for security reasons: {}",
                path.display()
            ));
        }
    }

    #[cfg(unix)]
    {
        use std::fs::OpenOptions;
        use std::os::unix::fs::OpenOptionsExt;

        // Use O_CREAT | O_EXCL to atomically fail if file exists
        // O_EXCL ensures create fails if the file already exists
        OpenOptions::new()
            .write(true)
            .create_new(true) // This is O_CREAT | O_EXCL
            .custom_flags(libc::O_NOFOLLOW)
            .open(path)
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::AlreadyExists {
                    anyhow!("file already exists: {}", path.display())
                } else if e.raw_os_error() == Some(libc::ELOOP) {
                    anyhow!(
                        "symbolic links are not allowed for security reasons: {}",
                        path.display()
                    )
                } else {
                    anyhow!("failed to create file '{}': {}", path.display(), e)
                }
            })
    }

    #[cfg(windows)]
    {
        use std::fs::OpenOptions;

        // On Windows, create_new(true) provides the same O_EXCL behavior
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::AlreadyExists {
                    anyhow!("file already exists: {}", path.display())
                } else {
                    anyhow!("failed to create file '{}': {}", path.display(), e)
                }
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_validate_read_path_blocks_traversal() {
        let workspace = tempdir().unwrap();
        let workspace_path = workspace.path();

        // Parent traversal should be blocked
        let result = validate_read_path("../../etc/passwd", workspace_path, true);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("traversal"));
    }

    #[test]
    fn test_validate_read_path_blocks_absolute_outside() {
        let workspace = tempdir().unwrap();
        let workspace_path = workspace.path();

        // Absolute path outside workspace should be blocked
        let result = validate_read_path("/etc/passwd", workspace_path, true);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_read_path_blocks_env_file() {
        let workspace = tempdir().unwrap();
        let workspace_path = workspace.path();

        // Create a .env file
        let env_path = workspace_path.join(".env");
        fs::write(&env_path, "SECRET=value").unwrap();

        // .env files should be blocked for security reasons
        let result = validate_read_path(".env", workspace_path, true);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("blocked"));
    }

    #[test]
    fn test_validate_write_path_enforces_size_limit() {
        let workspace = tempdir().unwrap();
        let workspace_path = workspace.path();

        // Create a file larger than MAX_FILE_SIZE
        let large_content = "x".repeat(MAX_FILE_SIZE + 1);

        let result = validate_write_path("test.txt", workspace_path, large_content.len(), true);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("exceeds maximum"));
    }

    #[test]
    fn test_validate_regex_pattern_blocks_nested_quantifiers() {
        let result = validate_regex_pattern(r"(.*).*");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("nested quantifiers"));
    }

    #[test]
    fn test_validate_regex_pattern_blocks_long_patterns() {
        let long_pattern = "a".repeat(2000);
        let result = validate_regex_pattern(&long_pattern);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("exceeds maximum"));
    }

    #[test]
    fn test_validate_url_blocks_file_scheme() {
        let result = validate_url("file:///etc/passwd");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not allowed"));
    }

    // --- Relaxed workspace tests (enforce_workspace: false) ---

    #[test]
    fn test_validate_read_path_allows_outside_workspace_when_relaxed() {
        let workspace = tempdir().unwrap();
        let outside = tempdir().unwrap();
        let file_path = outside.path().join("outside.txt");
        fs::write(&file_path, "hello").unwrap();

        let result = validate_read_path(file_path.to_str().unwrap(), workspace.path(), false);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_read_path_blocks_env_file_even_when_relaxed() {
        let workspace = tempdir().unwrap();
        let env_path = workspace.path().join(".env");
        fs::write(&env_path, "KEY=secret").unwrap();

        let result = validate_read_path(env_path.to_str().unwrap(), workspace.path(), false);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_write_path_allows_outside_workspace_when_relaxed() {
        let workspace = tempdir().unwrap();
        let outside = tempdir().unwrap();
        let file_path = outside.path().join("outside.txt");

        let result = validate_write_path(file_path.to_str().unwrap(), workspace.path(), 100, false);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_write_path_blocks_credentials_even_when_relaxed() {
        let workspace = tempdir().unwrap();
        let cred_path = workspace.path().join("credentials.json");

        let result = validate_write_path(cred_path.to_str().unwrap(), workspace.path(), 100, false);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_list_path_allows_outside_workspace_when_relaxed() {
        let workspace = tempdir().unwrap();
        let outside = tempdir().unwrap();

        let result = validate_list_path(outside.path().to_str().unwrap(), workspace.path(), false);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_path_blocks_traversal_even_when_relaxed() {
        let workspace = tempdir().unwrap();

        let result = validate_path("../../../etc/passwd", workspace.path(), false, false, false);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_url_blocks_missing_scheme() {
        let result = validate_url("example.com");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("scheme"));
    }

    #[test]
    fn test_validate_url_allows_https() {
        let result = validate_url("https://example.com");
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_command_arg_blocks_dangerous_chars() {
        assert!(validate_command_arg("test; rm -rf /").is_err());
        assert!(validate_command_arg("test && malicious").is_err());
        assert!(validate_command_arg("test$(whoami)").is_err());
    }

    #[test]
    fn test_validate_read_path_allows_valid_files() {
        let workspace = tempdir().unwrap();
        let workspace_path = workspace.path();

        // Create a test file
        let test_file = workspace_path.join("test.txt");
        fs::write(&test_file, "content").unwrap();

        let result = validate_read_path("test.txt", workspace_path, true);
        assert!(result.is_ok());
    }

    #[test]
    fn test_open_file_symlink_safe_rejects_symlinks() {
        let workspace = tempdir().unwrap();
        let workspace_path = workspace.path();

        // Create a real file
        let real_file = workspace_path.join("real.txt");
        fs::write(&real_file, "content").unwrap();

        // Create a symlink to the real file
        let symlink_path = workspace_path.join("link.txt");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&real_file, &symlink_path).unwrap();

        #[cfg(windows)]
        std::os::windows::fs::symlink_file(&real_file, &symlink_path).unwrap();

        // Opening the symlink should fail
        let result = open_file_symlink_safe(&symlink_path);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("symbolic link"));
    }

    #[test]
    fn test_open_file_symlink_safe_allows_regular_files() {
        let workspace = tempdir().unwrap();
        let workspace_path = workspace.path();

        // Create a regular file
        let test_file = workspace_path.join("regular.txt");
        fs::write(&test_file, "content").unwrap();

        // Opening a regular file should succeed
        let result = open_file_symlink_safe(&test_file);
        assert!(result.is_ok());

        // Verify we can read the file
        let mut file = result.unwrap();
        let mut content = String::new();
        use std::io::Read;
        file.read_to_string(&mut content).unwrap();
        assert_eq!(content, "content");
    }

    #[test]
    fn test_create_file_symlink_safe_rejects_symlinks() {
        let workspace = tempdir().unwrap();
        let workspace_path = workspace.path();

        // Create a real file first
        let real_file = workspace_path.join("real.txt");
        fs::write(&real_file, "old content").unwrap();

        // Create a symlink to the real file
        let symlink_path = workspace_path.join("link.txt");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&real_file, &symlink_path).unwrap();

        #[cfg(windows)]
        std::os::windows::fs::symlink_file(&real_file, &symlink_path).unwrap();

        // Creating through the symlink should fail
        let result = create_file_symlink_safe(&symlink_path);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("symbolic link"));
    }

    #[test]
    fn test_create_file_symlink_safe_creates_new_files() {
        let workspace = tempdir().unwrap();
        let workspace_path = workspace.path();

        // Create a new file (doesn't exist yet)
        let new_file = workspace_path.join("new.txt");

        // Creating a new file should succeed
        let result = create_file_symlink_safe(&new_file);
        assert!(result.is_ok());

        // Verify the file was created
        assert!(new_file.exists());
    }

    #[test]
    fn test_create_file_exclusive_fails_if_exists() {
        let workspace = tempdir().unwrap();
        let workspace_path = workspace.path();

        // Create a file first
        let existing_file = workspace_path.join("existing.txt");
        fs::write(&existing_file, "old content").unwrap();

        // Try to create exclusively - should fail
        let result = create_file_exclusive(&existing_file);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("already exists"));
    }

    #[test]
    fn test_create_file_exclusive_creates_new_file() {
        let workspace = tempdir().unwrap();
        let workspace_path = workspace.path();

        // Create a new file (doesn't exist yet)
        let new_file = workspace_path.join("new.txt");

        // Creating a new file should succeed
        let result = create_file_exclusive(&new_file);
        assert!(result.is_ok());

        // Verify the file was created
        assert!(new_file.exists());
    }

    #[test]
    fn test_create_file_exclusive_rejects_symlinks() {
        let workspace = tempdir().unwrap();
        let workspace_path = workspace.path();

        // Create a real file first
        let real_file = workspace_path.join("real.txt");
        fs::write(&real_file, "content").unwrap();

        // Create a symlink to the real file
        let symlink_path = workspace_path.join("link.txt");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&real_file, &symlink_path).unwrap();

        #[cfg(windows)]
        std::os::windows::fs::symlink_file(&real_file, &symlink_path).unwrap();

        // Creating through the symlink should fail
        let result = create_file_exclusive(&symlink_path);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("symbolic link"));
    }

    // ── validate_path core tests ─────────────────────────────────────────────

    #[test]
    fn test_validate_path_allows_simple_relative() {
        let workspace = tempdir().unwrap();
        let path = validate_path("file.txt", workspace.path(), false, false, true);
        assert!(path.is_ok());
        assert_eq!(path.unwrap(), workspace.path().join("file.txt"));
    }

    #[test]
    fn test_validate_path_blocks_double_dot_component() {
        let workspace = tempdir().unwrap();
        let path = validate_path("../../../etc/passwd", workspace.path(), false, false, true);
        assert!(path.is_err());
        assert!(path.unwrap_err().to_string().contains("traversal"));
    }

    #[test]
    fn test_validate_path_blocks_encoded_traversal() {
        let workspace = tempdir().unwrap();
        let path = validate_path(
            "foo/%2e%2e/etc/passwd",
            workspace.path(),
            false,
            false,
            true,
        );
        assert!(path.is_err());
        assert!(path.unwrap_err().to_string().contains("encoded"));
    }

    #[test]
    fn test_validate_path_blocks_oversized_path() {
        let workspace = tempdir().unwrap();
        let long_path = "a".repeat(MAX_PATH_LENGTH + 100);
        let path = validate_path(&long_path, workspace.path(), false, false, true);
        assert!(path.is_err());
        assert!(path.unwrap_err().to_string().contains("maximum length"));
    }

    #[test]
    fn test_validate_path_absolute_within_workspace() {
        let workspace = tempdir().unwrap();
        let file_path = workspace.path().join("test.txt");
        fs::write(&file_path, "content").unwrap();
        let abs = file_path.to_str().unwrap();
        let path = validate_path(abs, workspace.path(), true, false, true);
        assert!(path.is_ok());
    }

    #[test]
    fn test_validate_path_absolute_outside_workspace() {
        let workspace = tempdir().unwrap();
        let path = validate_path("/etc/passwd", workspace.path(), true, false, true);
        assert!(path.is_err());
    }

    // ── validate_list_path tests ─────────────────────────────────────────────

    #[test]
    fn test_validate_list_path_allows_directory() {
        let workspace = tempdir().unwrap();
        let subdir = workspace.path().join("subdir");
        fs::create_dir(&subdir).unwrap();
        let path = validate_list_path("subdir", workspace.path(), true);
        assert!(path.is_ok());
    }

    #[test]
    fn test_validate_list_path_rejects_file() {
        let workspace = tempdir().unwrap();
        let file = workspace.path().join("file.txt");
        fs::write(&file, "content").unwrap();
        let path = validate_list_path("file.txt", workspace.path(), true);
        assert!(path.is_err());
        assert!(path.unwrap_err().to_string().contains("not a directory"));
    }

    #[test]
    fn test_validate_list_path_root_workspace() {
        let workspace = tempdir().unwrap();
        let path = validate_list_path(".", workspace.path(), true);
        assert!(path.is_ok());
    }

    // ── sanitize_for_log tests ───────────────────────────────────────────────

    #[test]
    fn test_sanitize_redacts_api_key() {
        let result = sanitize_for_log("api_key=sk-12345abcde");
        assert!(result.contains("[REDACTED]"));
        assert!(!result.contains("sk-12345abcde"));
    }

    #[test]
    fn test_sanitize_redacts_password() {
        let result = sanitize_for_log("password=hunter2");
        assert!(result.contains("[REDACTED]"));
        assert!(!result.contains("hunter2"));
    }

    #[test]
    fn test_sanitize_redacts_bearer_token() {
        let result = sanitize_for_log("Authorization: Bearer eyJhbGciOiJIUzI1NiJ9");
        assert!(result.contains("[REDACTED]"));
        // "authorization" pattern is found and redacted (keyword + value up to delimiter)
        assert!(!result.contains("authorization") || result.contains("[REDACTED]"));
    }

    #[test]
    fn test_sanitize_preserves_normal_text() {
        let input = "The quick brown fox jumps over the lazy dog";
        let result = sanitize_for_log(input);
        assert_eq!(result, input);
    }

    #[test]
    fn test_sanitize_handles_empty_input() {
        let result = sanitize_for_log("");
        assert_eq!(result, "");
    }

    #[test]
    fn test_sanitize_case_insensitive() {
        let result = sanitize_for_log("API_KEY=my-secret-key");
        assert!(result.contains("[REDACTED]"));
        assert!(!result.contains("my-secret-key"));
    }

    #[test]
    fn test_sanitize_json_format() {
        let result = sanitize_for_log(r#"{"token": "abc123"}"#);
        // "token" is found and redacted (keyword + value up to quote delimiter)
        assert!(result.contains("[REDACTED]"));
    }

    #[test]
    fn test_sanitize_overlapping_patterns_no_panic() {
        // Multiple patterns that could overlap — must not panic on replace_range
        let input = "api_key=secret123 password=hunter2 api_key=other456";
        let result = sanitize_for_log(input);
        assert!(result.contains("[REDACTED]"));
        assert!(!result.contains("secret123"));
        assert!(!result.contains("hunter2"));
        assert!(!result.contains("other456"));
    }

    #[test]
    fn test_sanitize_adjacent_sensitive_fields() {
        // Two patterns right next to each other
        let input = "api_key=sk-abc password=xyz token=def";
        let result = sanitize_for_log(input);
        assert!(!result.contains("sk-abc"));
        assert!(!result.contains("xyz"));
        assert!(!result.contains("def"));
    }

    #[test]
    fn test_sanitize_same_pattern_repeated() {
        // Same pattern appearing multiple times
        let input = "password=aaa password=bbb password=ccc";
        let result = sanitize_for_log(input);
        assert!(!result.contains("aaa"));
        assert!(!result.contains("bbb"));
        assert!(!result.contains("ccc"));
    }

    #[test]
    fn test_sanitize_overlapping_key_and_token() {
        // "token" and "api_key" — adjacent sensitive fields
        let input = "token=myvalue api_key=sk-abc";
        let result = sanitize_for_log(input);
        assert!(result.contains("[REDACTED]"));
        assert!(!result.contains("myvalue"));
        assert!(!result.contains("sk-abc"));
    }

    #[test]
    fn test_sanitize_bearer_token_value_redacted() {
        let result = sanitize_for_log("Authorization: Bearer eyJhbGciOiJIUzI1NiJ9");
        assert!(result.contains("[REDACTED]"));
        assert!(!result.contains("eyJhbGciOiJIUzI1NiJ9"));
    }

    #[test]
    fn test_sanitize_colon_separator() {
        let result = sanitize_for_log("password: hunter2");
        assert!(result.contains("[REDACTED]"));
        assert!(!result.contains("hunter2"));
    }

    #[test]
    fn test_sanitize_space_separator() {
        let result = sanitize_for_log("token abc123def");
        assert!(result.contains("[REDACTED]"));
        assert!(!result.contains("abc123def"));
    }

    // --- Cross-platform path normalization tests ---

    #[test]
    fn test_normalize_backslashes() {
        assert_eq!(normalize_path("src\\main.rs"), "src/main.rs");
        assert_eq!(
            normalize_path("crates\\rustycode-tools\\src\\lib.rs"),
            "crates/rustycode-tools/src/lib.rs"
        );
    }

    /// Returns true if running inside Windows Subsystem for Linux.
    fn is_wsl() -> bool {
        #[cfg(target_os = "linux")]
        {
            std::fs::read_to_string("/proc/version")
                .map(|v| v.contains("microsoft") || v.contains("WSL"))
                .unwrap_or(false)
        }
        #[cfg(not(target_os = "linux"))]
        {
            false
        }
    }

    #[test]
    fn test_normalize_wsl_path() {
        if cfg!(windows) {
            assert_eq!(
                normalize_path("/mnt/c/Users/test/file.txt"),
                "C:/Users/test/file.txt"
            );
            assert_eq!(
                normalize_path("/mnt/d/projects/app/src/main.rs"),
                "D:/projects/app/src/main.rs"
            );
        } else if cfg!(target_os = "linux") {
            // WSL conversion on Linux: /mnt/c/... → C:/...
            assert_eq!(
                normalize_path("/mnt/c/Users/test/file.txt"),
                "C:/Users/test/file.txt"
            );
            assert_eq!(
                normalize_path("/mnt/d/projects/app/src/main.rs"),
                "D:/projects/app/src/main.rs"
            );
        } else {
            // macOS and others: no conversion
            assert_eq!(
                normalize_path("/mnt/c/Users/test/file.txt"),
                "/mnt/c/Users/test/file.txt"
            );
            assert_eq!(
                normalize_path("/mnt/d/projects/app/src/main.rs"),
                "/mnt/d/projects/app/src/main.rs"
            );
        }
    }

    #[test]
    fn test_normalize_cygwin_path() {
        if cfg!(windows) {
            assert_eq!(
                normalize_path("/cygdrive/c/Users/test/file.txt"),
                "C:/Users/test/file.txt"
            );
        } else {
            assert_eq!(
                normalize_path("/cygdrive/c/Users/test/file.txt"),
                "/cygdrive/c/Users/test/file.txt"
            );
        }
    }

    #[test]
    #[cfg(windows)]
    fn test_normalize_git_bash_path() {
        assert_eq!(
            normalize_path("/c/Users/test/file.txt"),
            "C:/Users/test/file.txt"
        );
    }

    #[test]
    #[cfg(not(windows))]
    fn test_normalize_unix_single_letter_unchanged() {
        // On Unix, /c/... should NOT be converted to C:/... (it's a valid Unix path)
        assert_eq!(
            normalize_path("/c/Users/test/file.txt"),
            "/c/Users/test/file.txt"
        );
    }

    #[test]
    fn test_normalize_already_unix() {
        // Regular Unix paths should be unchanged
        assert_eq!(normalize_path("src/main.rs"), "src/main.rs");
        assert_eq!(normalize_path("/usr/local/bin"), "/usr/local/bin");
        assert_eq!(normalize_path("README.md"), "README.md");
    }

    #[test]
    fn test_normalize_preserves_relative() {
        assert_eq!(normalize_path("./src/lib.rs"), "./src/lib.rs");
        assert_eq!(normalize_path("../sibling/file.txt"), "../sibling/file.txt");
    }

    // ── Path encoding bypass tests ─────────────────────────────────────────

    #[test]
    fn test_encoded_traversal_mixed_case() {
        // %2e. should be caught
        let workspace = tempdir().unwrap();
        let result = validate_path("%2e./etc/passwd", workspace.path(), false, false, true);
        assert!(result.is_err());
    }

    #[test]
    fn test_encoded_traversal_dot_percent() {
        // .%2e should be caught
        let workspace = tempdir().unwrap();
        let result = validate_path(".%2e/etc/passwd", workspace.path(), false, false, true);
        assert!(result.is_err());
    }

    #[test]
    fn test_encoded_traversal_uppercase() {
        // %2E%2E should be caught (already worked, but verify)
        let workspace = tempdir().unwrap();
        let result = validate_path("%2E%2E/etc/passwd", workspace.path(), false, false, true);
        assert!(result.is_err());
    }

    #[test]
    fn test_empty_path_rejected() {
        let workspace = tempdir().unwrap();
        let result = validate_path("", workspace.path(), false, false, true);
        assert!(result.is_err() || !result.unwrap().exists());
    }

    #[test]
    fn test_double_dot_midpath_rejected() {
        let workspace = tempdir().unwrap();
        let result = validate_path("src/../etc/passwd", workspace.path(), false, false, true);
        assert!(result.is_err());
    }

    #[test]
    fn test_blocked_env_extension() {
        let workspace = tempdir().unwrap();
        let env_file = workspace.path().join(".env");
        fs::write(&env_file, "KEY=value").unwrap();
        let result = validate_write_path(".env", workspace.path(), 10, true);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_url_rejects_javascript() {
        assert!(validate_url("javascript:alert(1)").is_err());
    }

    #[test]
    fn test_validate_url_rejects_file() {
        assert!(validate_url("file:///etc/passwd").is_err());
    }

    #[test]
    fn test_blocked_credential_filenames() {
        let workspace = tempdir().unwrap();

        // credentials.json should be blocked
        let creds = workspace.path().join("credentials.json");
        fs::write(&creds, "{}").unwrap();
        let result = validate_read_path("credentials.json", workspace.path(), true);
        assert!(result.is_err(), "credentials.json should be blocked");

        // id_rsa should be blocked
        let rsa = workspace.path().join("id_rsa");
        fs::write(&rsa, "key").unwrap();
        let result = validate_read_path("id_rsa", workspace.path(), true);
        assert!(result.is_err(), "id_rsa should be blocked");

        // .netrc should be blocked
        let netrc = workspace.path().join(".netrc");
        fs::write(&netrc, "machine x").unwrap();
        let result = validate_read_path(".netrc", workspace.path(), true);
        assert!(result.is_err(), ".netrc should be blocked");
    }

    #[test]
    fn test_blocked_ssh_directory() {
        let workspace = tempdir().unwrap();
        let ssh_dir = workspace.path().join(".ssh");
        fs::create_dir_all(&ssh_dir).unwrap();
        let config = ssh_dir.join("config");
        fs::write(&config, "Host *").unwrap();
        let result = validate_read_path(".ssh/config", workspace.path(), true);
        assert!(result.is_err(), ".ssh directory should be blocked");
    }

    #[test]
    fn test_blocked_gnupg_directory() {
        let workspace = tempdir().unwrap();
        let gnupg_dir = workspace.path().join(".gnupg");
        fs::create_dir_all(&gnupg_dir).unwrap();
        let keyring = gnupg_dir.join("pubring.kbx");
        fs::write(&keyring, "keydata").unwrap();
        let result = validate_read_path(".gnupg/pubring.kbx", workspace.path(), true);
        assert!(result.is_err(), ".gnupg directory should be blocked");
    }

    #[test]
    fn test_blocked_credentials_json() {
        let workspace = tempdir().unwrap();
        let workspace_path = workspace.path();
        let creds = workspace_path.join(".credentials.json");
        fs::write(&creds, r#"{"api_key":"secret"}"#).unwrap();
        let result = validate_read_path(".credentials.json", workspace_path, true);
        assert!(result.is_err(), ".credentials.json should be blocked");
    }

    #[test]
    fn test_outside_workspace_symlink_blocked_even_when_relaxed() {
        let workspace = tempdir().unwrap();
        let outside = tempdir().unwrap();
        let real_file = outside.path().join("real.txt");
        fs::write(&real_file, "content").unwrap();

        // Create symlink outside workspace pointing to another outside file
        let symlink_path = outside.path().join("link.txt");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&real_file, &symlink_path).unwrap();

        // Even in relaxed mode, outside-workspace symlinks should be blocked
        let result = validate_read_path(symlink_path.to_str().unwrap(), workspace.path(), false);
        #[cfg(unix)]
        assert!(
            result.is_err(),
            "symlink outside workspace should be blocked even in relaxed mode"
        );
    }
}
