//! Dynamic HTTP header resolution via external scripts (headersHelper).
//!
//! Matches Claude Code's `headersHelper` config field: a path to a shell script
//! that prints JSON key-value pairs to stdout. The output is merged with any
//! static `headers` from the config.

use std::collections::HashMap;
use std::process::Stdio;
use tracing::{debug, warn};

/// Maximum output we accept from a headers-helper script (16 KiB).
const MAX_OUTPUT_BYTES: usize = 16 * 1024;

/// Execute a `headersHelper` script and return the headers it outputs.
///
/// The script must print a JSON object to stdout, e.g.:
/// ```json
/// {"Authorization": "Bearer token123", "X-Request-ID": "abc"}
/// ```
///
/// Non-UTF-8 output, non-zero exit codes, or malformed JSON are treated as
/// errors and result in an empty map (with a warning logged).
pub async fn resolve_headers(helper_path: &str) -> HashMap<String, String> {
    let output = match rustycode_tools::subprocess::new_tokio_shell_command(helper_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
    {
        Ok(o) => o,
        Err(e) => {
            warn!("headersHelper '{}' failed to execute: {}", helper_path, e);
            return HashMap::new();
        }
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        warn!(
            "headersHelper '{}' exited with {}: {}",
            helper_path,
            output.status,
            stderr.trim()
        );
        return HashMap::new();
    }

    if output.stdout.len() > MAX_OUTPUT_BYTES {
        warn!(
            "headersHelper '{}' output exceeded {} bytes, ignoring",
            helper_path, MAX_OUTPUT_BYTES
        );
        return HashMap::new();
    }

    let stdout = match String::from_utf8(output.stdout) {
        Ok(s) => s,
        Err(e) => {
            warn!(
                "headersHelper '{}' produced non-UTF-8 output: {}",
                helper_path, e
            );
            return HashMap::new();
        }
    };

    match serde_json::from_str::<HashMap<String, String>>(stdout.trim()) {
        Ok(headers) => {
            debug!(
                "headersHelper '{}' returned {} header(s)",
                helper_path,
                headers.len()
            );
            headers
        }
        Err(e) => {
            warn!(
                "headersHelper '{}' produced invalid JSON: {}",
                helper_path, e
            );
            HashMap::new()
        }
    }
}

/// Merge static headers with dynamic headers from a helper script.
///
/// Dynamic headers take precedence over static ones (same key overwrites).
pub async fn merge_headers(
    static_headers: &HashMap<String, String>,
    helper_path: Option<&str>,
) -> HashMap<String, String> {
    let mut merged = static_headers.clone();

    if let Some(path) = helper_path {
        if !path.is_empty() {
            let dynamic = resolve_headers(path).await;
            merged.extend(dynamic);
        }
    }

    merged
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_merge_headers_static_only() {
        let mut static_h = HashMap::new();
        static_h.insert("Authorization".to_string(), "Bearer token".to_string());

        let result = merge_headers(&static_h, None).await;
        assert_eq!(result.get("Authorization").unwrap(), "Bearer token");
        assert_eq!(result.len(), 1);
    }

    #[tokio::test]
    async fn test_merge_headers_dynamic_overwrites() {
        let mut static_h = HashMap::new();
        static_h.insert("Authorization".to_string(), "old-token".to_string());
        static_h.insert("X-Custom".to_string(), "value".to_string());

        let script = r#"echo '{"Authorization":"new-token"}'"#;

        let result = merge_headers(&static_h, Some(script)).await;
        assert_eq!(result.get("Authorization").unwrap(), "new-token");
        assert_eq!(result.get("X-Custom").unwrap(), "value");
        assert_eq!(result.len(), 2);
    }

    #[tokio::test]
    async fn test_resolve_headers_valid_json() {
        let script = r#"echo '{"X-Api-Key":"abc123","X-Request-ID":"xyz"}'"#;
        let result = resolve_headers(script).await;
        assert_eq!(result.get("X-Api-Key").unwrap(), "abc123");
        assert_eq!(result.get("X-Request-ID").unwrap(), "xyz");
    }

    #[tokio::test]
    async fn test_resolve_headers_invalid_json() {
        let script = r#"echo 'not json'"#;
        let result = resolve_headers(script).await;
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn test_resolve_headers_nonzero_exit() {
        let script = r#"echo 'error' >&2; exit 1"#;
        let result = resolve_headers(script).await;
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn test_resolve_headers_empty_path() {
        let result = merge_headers(&HashMap::new(), Some("")).await;
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn test_resolve_headers_missing_script() {
        let result = resolve_headers("/nonexistent/path/script.sh").await;
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn test_resolve_headers_large_output_rejected() {
        // Generate output exceeding MAX_OUTPUT_BYTES
        let script = format!(
            "head -c {} < /dev/zero | tr '\\0' 'x'",
            MAX_OUTPUT_BYTES + 1
        );
        let result = resolve_headers(&script).await;
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn test_merge_headers_no_helper_no_static() {
        let result = merge_headers(&HashMap::new(), None).await;
        assert!(result.is_empty());
    }
}
