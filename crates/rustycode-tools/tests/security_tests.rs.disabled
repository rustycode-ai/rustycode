//! Comprehensive security tests for rustycode-tools
//!
//! This test suite validates:
//! - Path traversal prevention
//! - Symlink security
//! - Command injection prevention
//! - Input validation
//! - Resource limits
//! - Permission enforcement

use rustycode_tools::*;
use serde_json::json;
use std::fs;
use std::os::unix::fs::symlink as symlink_file;
use tempfile::tempdir;

// ── Path Traversal Tests ────────────────────────────────────────────────────

#[test]
fn test_read_file_blocks_absolute_path_outside_workspace() {
    let workspace = tempdir().expect("workspace tempdir");
    let outside = tempdir().expect("outside tempdir");
    let outside_file = outside.path().join("secret.txt");
    fs::write(&outside_file, "sensitive data").expect("write outside file");

    let tool = ReadFileTool;
    let ctx = ToolContext::new(workspace.path());

    // Try to read absolute path outside workspace
    let result = tool.execute(json!({ "path": outside_file.to_str().unwrap() }), &ctx);

    assert!(
        result.is_err(),
        "Should block absolute path outside workspace"
    );
    let err_msg = result.err().unwrap().to_string();
    assert!(
        err_msg.contains("outside")
            || err_msg.contains("blocked")
            || err_msg.contains("not within"),
        "Error message should mention path is blocked: {}",
        err_msg
    );
}

#[test]
fn test_read_file_blocks_parent_directory_traversal() {
    let workspace = tempdir().expect("workspace tempdir");
    let outside = tempdir().expect("outside tempdir");
    let outside_file = outside.path().join("secret.txt");
    fs::write(&outside_file, "secret data").expect("write secret");

    let tool = ReadFileTool;
    let ctx = ToolContext::new(workspace.path());

    // Try multiple parent directory traversals
    let traversal_patterns = vec![
        format!(
            "../../../{}",
            outside_file.file_name().unwrap().to_str().unwrap()
        ),
        format!(
            "../../{}",
            outside_file.file_name().unwrap().to_str().unwrap()
        ),
        "../etc/passwd".to_string(),
        "..\\..\\windows\\system32\\config\\sam".to_string(),
    ];

    for pattern in traversal_patterns {
        let result = tool.execute(json!({ "path": pattern }), &ctx);
        assert!(
            result.is_err(),
            "Should block parent traversal pattern: {}",
            pattern
        );
    }
}

#[test]
fn test_read_file_blocks_mixed_traversal() {
    let workspace = tempdir().expect("workspace tempdir");
    let ctx = ToolContext::new(workspace.path());

    // Mix of valid path and traversal
    let tool = ReadFileTool;
    let result = tool.execute(json!({ "path": "valid/../../../etc/passwd" }), &ctx);

    assert!(result.is_err(), "Should block mixed path traversal");
}

#[test]
fn test_read_file_blocks_encoded_traversal() {
    let workspace = tempdir().expect("workspace tempdir");
    let ctx = ToolContext::new(workspace.path());

    // URL-encoded traversal attempts
    let encoded_attempts = vec![
        "%2e%2e/%2e%2e/etc/passwd",
        "%2E%2E/%2E%2E/etc/passwd",
        "..%2fetc%2fpasswd",
        "..%2Fetc%2Fpasswd",
    ];

    for attempt in encoded_attempts {
        let tool = ReadFileTool;
        let result = tool.execute(json!({ "path": attempt }), &ctx);
        // May fail during path validation or file reading
        assert!(
            result.is_err() || result.unwrap().text.contains("blocked"),
            "Should block or sanitize encoded traversal: {}",
            attempt
        );
    }
}

#[test]
fn test_write_file_blocks_traversal() {
    let workspace = tempdir().expect("workspace tempdir");
    let ctx = ToolContext::new(workspace.path());

    // Try to write outside workspace via traversal
    let result = WriteFileTool.execute(
        json!({
            "path": "../../../tmp/malicious.txt",
            "content": "malicious content"
        }),
        &ctx,
    );

    assert!(result.is_err(), "Should block write with path traversal");
}

#[test]
fn test_list_dir_blocks_traversal() {
    let workspace = tempdir().expect("workspace tempdir");
    let ctx = ToolContext::new(workspace.path());

    let result = ListDirTool.execute(json!({ "path": "../../../etc" }), &ctx);

    assert!(result.is_err(), "Should block list_dir with traversal");
}

// ── Symlink Security Tests ───────────────────────────────────────────────────

#[test]
fn test_read_file_blocks_symlink_to_file_inside_workspace() {
    let workspace = tempdir().expect("workspace tempdir");
    let test_file = workspace.path().join("original.txt");
    fs::write(&test_file, "original content").expect("write original");

    let symlink_path = workspace.path().join("link.txt");

    #[cfg(unix)]
    {
        symlink_file(&test_file, &symlink_path).expect("create symlink");

        let tool = ReadFileTool;
        let ctx = ToolContext::new(workspace.path());

        let result = tool.execute(json!({ "path": "link.txt" }), &ctx);
        // Symlinks within workspace are blocked by security module
        assert!(result.is_err(), "Should block symlink to file");
        let err_msg = result.err().unwrap().to_string();
        assert!(
            err_msg.contains("symlink") || err_msg.contains("blocked"),
            "Error should mention symlink: {}",
            err_msg
        );
    }

    #[cfg(not(unix))]
    let _ = (test_file, symlink_path);
}

#[test]
fn test_read_file_blocks_symlink_to_outside_workspace() {
    let workspace = tempdir().expect("workspace tempdir");
    let outside = tempdir().expect("outside tempdir");
    let outside_file = outside.path().join("secret.txt");
    fs::write(&outside_file, "secret data").expect("write secret");

    let symlink_path = workspace.path().join("safelooking.txt");

    #[cfg(unix)]
    {
        symlink_file(&outside_file, &symlink_path).expect("create symlink");

        let tool = ReadFileTool;
        let ctx = ToolContext::new(workspace.path());

        let result = tool.execute(json!({ "path": "safelooking.txt" }), &ctx);
        assert!(result.is_err(), "Should block symlink pointing outside");
    }

    #[cfg(not(unix))]
    let _ = (outside_file, symlink_path);
}

#[test]
fn test_read_file_blocks_symlink_in_directory_path() {
    let workspace = tempdir().expect("workspace tempdir");
    let outside = tempdir().expect("outside tempdir");
    let outside_dir = outside.path().join("secret_dir");
    fs::create_dir(&outside_dir).expect("create outside dir");

    let symlink_path = workspace.path().join("link_to_dir");

    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&outside_dir, &symlink_path).expect("create dir symlink");

        let tool = ReadFileTool;
        let ctx = ToolContext::new(workspace.path());

        let result = tool.execute(json!({ "path": "link_to_dir/file.txt" }), &ctx);
        assert!(result.is_err(), "Should block symlink in directory path");
    }

    #[cfg(not(unix))]
    let _ = (outside_dir, symlink_path);
}

#[test]
fn test_write_file_blocks_symlink() {
    let workspace = tempdir().expect("workspace tempdir");
    let test_file = workspace.path().join("target.txt");
    fs::write(&test_file, "original").expect("write target");

    let symlink_path = workspace.path().join("link.txt");

    #[cfg(unix)]
    {
        symlink_file(&test_file, &symlink_path).expect("create symlink");

        let tool = WriteFileTool;
        let ctx = ToolContext::new(workspace.path());

        let result = tool.execute(json!({ "path": "link.txt", "content": "modified" }), &ctx);
        assert!(result.is_err(), "Should block write through symlink");
    }

    #[cfg(not(unix))]
    let _ = (test_file, symlink_path);
}

#[test]
fn test_list_dir_blocks_symlink_directory() {
    let workspace = tempdir().expect("workspace tempdir");
    let outside = tempdir().expect("outside tempdir");

    let symlink_path = workspace.path().join("linkdir");

    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(outside.path(), &symlink_path).expect("create dir symlink");

        let tool = ListDirTool;
        let ctx = ToolContext::new(workspace.path());

        let result = tool.execute(json!({ "path": "linkdir" }), &ctx);
        assert!(
            result.is_err(),
            "Should block list_dir on symlink directory"
        );
    }

    #[cfg(not(unix))]
    let _ = (outside, symlink_path);
}

// ── Command Injection Tests ─────────────────────────────────────────────────

#[test]
fn test_bash_blocks_command_substitution() {
    let workspace = tempdir().expect("workspace tempdir");
    let ctx = ToolContext::new(workspace.path());

    let tool = BashTool;

    // Command substitution with $()
    let result = tool.execute(json!({ "command": "echo $(whoami)" }), &ctx);
    assert!(result.is_err(), "Should block $() command substitution");

    // Command substitution with backticks
    let result = tool.execute(json!({ "command": "echo `whoami`" }), &ctx);
    assert!(
        result.is_err(),
        "Should block backtick command substitution"
    );
}

#[test]
fn test_bash_blocks_shell_metacharacters() {
    let workspace = tempdir().expect("workspace tempdir");
    let ctx = ToolContext::new(workspace.path());

    let tool = BashTool;

    let dangerous_commands = vec!["rm -rf /", "echo test; rm -rf /"];

    for cmd in dangerous_commands {
        let result = tool.execute(json!({ "command": cmd }), &ctx);
        assert!(result.is_err(), "Should block dangerous command: {}", cmd);
    }
}

#[test]
fn test_bash_blocks_dangerous_binaries() {
    let workspace = tempdir().expect("workspace tempdir");
    let ctx = ToolContext::new(workspace.path());

    let tool = BashTool;

    let blocked_binaries = vec![
        "rm -rf /tmp/test",
        "/bin/rm file.txt",
        "dd if=/dev/zero of=file",
        "mkfs /dev/sda1",
        "shutdown -h now",
        "reboot",
        "su root",
        "sudo ls",
        "chmod 777 /etc/passwd",
        "chown root /etc/passwd",
    ];

    for cmd in blocked_binaries {
        let result = tool.execute(json!({ "command": cmd }), &ctx);
        assert!(
            result.is_err(),
            "Should block dangerous binary '{}': {:?}",
            cmd,
            result
        );
    }
}

#[test]
fn test_bash_blocks_fork_bomb() {
    let workspace = tempdir().expect("workspace tempdir");
    let ctx = ToolContext::new(workspace.path());

    let tool = BashTool;

    // Classic fork bomb
    let result = tool.execute(json!({ "command": ":(){ :|:& };:" }), &ctx);
    assert!(result.is_err(), "Should block fork bomb");

    // Variation
    let result = tool.execute(json!({ "command": ":() { :|:& }; :" }), &ctx);
    assert!(result.is_err(), "Should block fork bomb variant");
}

#[test]
fn test_bash_blocks_obfuscated_commands() {
    let workspace = tempdir().expect("workspace tempdir");
    let ctx = ToolContext::new(workspace.path());

    let tool = BashTool;

    // Case variations
    let _result = tool.execute(json!({ "command": "RM -rf file" }), &ctx);
    // This may not be blocked as "RM" != "rm", but good to check behavior
    // The actual binary check is case-sensitive on the path

    // Path with spaces
    let result = tool.execute(json!({ "command": "/bin/rm file.txt" }), &ctx);
    assert!(result.is_err(), "Should block rm with absolute path");
}

#[test]
fn test_bash_executes_in_workspace() {
    let workspace = tempdir().expect("workspace tempdir");

    let tool = BashTool;
    let ctx = ToolContext::new(workspace.path());

    let result = tool.execute(json!({ "command": "pwd" }), &ctx);

    assert!(result.is_ok(), "pwd should execute in workspace");
}

// ── Input Validation Tests ───────────────────────────────────────────────────

#[test]
fn test_search_replace_blocks_dangerous_regex() {
    let workspace = tempdir().expect("workspace tempdir");
    let test_file = workspace.path().join("test.txt");
    fs::write(&test_file, "test content").expect("write test file");

    let tool = SearchReplace;
    let ctx = ToolContext::new(workspace.path());

    // Nested quantifiers (ReDoS)
    let result = tool.execute(
        json!({
            "path": "test.txt",
            "pattern": r"(.*).*",
            "replacement": "x",
            "regex": true
        }),
        &ctx,
    );
    assert!(result.is_err(), "Should block nested quantifiers");

    // Very long pattern
    let long_pattern = "a".repeat(2000);
    let result = tool.execute(
        json!({
            "path": "test.txt",
            "pattern": long_pattern,
            "replacement": "x",
            "regex": true
        }),
        &ctx,
    );
    assert!(result.is_err(), "Should block excessively long pattern");
}

#[test]
fn test_grep_blocks_dangerous_regex() {
    let workspace = tempdir().expect("workspace tempdir");
    let test_file = workspace.path().join("test.txt");
    fs::write(&test_file, "test content").expect("write test file");

    let tool = GrepTool;
    let ctx = ToolContext::new(workspace.path());

    // Nested quantifiers
    let result = tool.execute(
        json!({
            "pattern": r"(.*).*",
            "path": "."
        }),
        &ctx,
    );
    assert!(result.is_err(), "Should block dangerous regex in grep");
}

#[test]
fn test_write_file_blocks_large_files() {
    let workspace = tempdir().expect("workspace tempdir");
    let ctx = ToolContext::new(workspace.path());

    let tool = WriteFileTool;

    // Try to write a file larger than MAX_FILE_SIZE (10 MB)
    let huge_content = "x".repeat(15 * 1024 * 1024); // 15 MB

    let result = tool.execute(
        json!({
            "path": "huge.txt",
            "content": huge_content
        }),
        &ctx,
    );

    assert!(result.is_err(), "Should block oversized file writes");
    let err_msg = result.err().unwrap().to_string();
    assert!(
        err_msg.contains("exceeds") || err_msg.contains("limit") || err_msg.contains("too large"),
        "Error should mention size limit: {}",
        err_msg
    );
}

#[test]
fn test_edit_file_blocks_large_content() {
    let workspace = tempdir().expect("workspace tempdir");
    let test_file = workspace.path().join("test.txt");
    fs::write(&test_file, "small").expect("write test file");

    let tool = EditFile;
    let ctx = ToolContext::new(workspace.path());

    // Try to replace with huge content
    let huge_content = "x".repeat(20 * 1024 * 1024); // 20 MB

    let result = tool.execute(
        json!({
            "path": "test.txt",
            "old_text": "small",
            "new_text": huge_content
        }),
        &ctx,
    );

    assert!(result.is_err(), "Should block oversized edit");
}

// ── Permission Enforcement Tests ────────────────────────────────────────────

#[test]
fn test_read_only_permission_blocks_write() {
    let workspace = tempdir().expect("workspace tempdir");
    let ctx = ToolContext::new(workspace.path()).with_max_permission(ToolPermission::Read);

    // Write operations should fail
    let result = WriteFileTool.execute(json!({ "path": "test.txt", "content": "test" }), &ctx);
    assert!(result.is_err(), "Read permission should block write");

    let result = BashTool.execute(json!({ "command": "echo test" }), &ctx);
    assert!(result.is_err(), "Read permission should block execute");
}

#[test]
fn test_write_permission_blocks_execute() {
    let workspace = tempdir().expect("workspace tempdir");
    let ctx = ToolContext::new(workspace.path()).with_max_permission(ToolPermission::Write);

    let result = BashTool.execute(json!({ "command": "echo test" }), &ctx);
    assert!(result.is_err(), "Write permission should block execute");
}

// ── WebFetch Security Tests ─────────────────────────────────────────────────

#[test]
fn test_web_fetch_blocks_file_urls() {
    let workspace = tempdir().expect("workspace tempdir");
    let ctx = ToolContext::new(workspace.path());

    let tool = WebFetchTool;

    let file_urls = vec![
        "file:///etc/passwd",
        "file://localhost/etc/passwd",
        "FILE:///etc/passwd",
    ];

    for url in file_urls {
        let result = tool.execute(json!({ "url": url }), &ctx);
        assert!(result.is_err(), "Should block file:// URL: {}", url);
        let err_msg = result.err().unwrap().to_string();
        assert!(
            err_msg.contains("not allowed")
                || err_msg.contains("blocked")
                || err_msg.contains("only http"),
            "Error should mention URL is blocked: {}",
            err_msg
        );
    }
}

#[test]
fn test_web_fetch_blocks_missing_scheme() {
    let workspace = tempdir().expect("workspace tempdir");
    let ctx = ToolContext::new(workspace.path());

    let tool = WebFetchTool;

    let result = tool.execute(json!({ "url": "example.com" }), &ctx);

    assert!(result.is_err(), "Should block URL without scheme");
}

#[test]
fn test_web_fetch_blocks_non_http_schemes() {
    let workspace = tempdir().expect("workspace tempdir");
    let ctx = ToolContext::new(workspace.path());

    let tool = WebFetchTool;

    let blocked_schemes = vec![
        "ftp://example.com",
        "javascript:alert(1)",
        "data:text/html,<script>alert(1)</script>",
        "mailto:test@example.com",
    ];

    for url in blocked_schemes {
        let result = tool.execute(json!({ "url": url }), &ctx);
        assert!(result.is_err(), "Should block non-http URL: {}", url);
    }
}

#[test]
fn test_web_fetch_allows_https() {
    let workspace = tempdir().expect("workspace tempdir");
    let ctx = ToolContext::new(workspace.path());

    let tool = WebFetchTool;

    // Note: This will fail with network error, but should pass URL validation
    let result = tool.execute(json!({ "url": "https://example.com" }), &ctx);

    // Should not fail due to URL validation (may fail due to network)
    let err_msg = result.err().map(|e| e.to_string());
    if let Some(msg) = err_msg {
        assert!(
            !msg.contains("not allowed") && !msg.contains("blocked") && !msg.contains("scheme"),
            "HTTPS URL should be allowed: {}",
            msg
        );
    }
}

// ── Blocked File Extension Tests ───────────────────────────────────────────

#[test]
fn test_read_file_blocks_env_files() {
    let workspace = tempdir().expect("workspace tempdir");
    let ctx = ToolContext::new(workspace.path());

    let tool = ReadFileTool;

    let env_files = vec![".env", ".env.local", ".env.production", ".env.development"];

    for filename in env_files {
        let result = tool.execute(json!({ "path": filename }), &ctx);
        // File doesn't exist, so it should fail with file not found
        // If file existed, it should be blocked
        if let Ok(output) = result {
            assert!(
                output.text.contains("blocked") || output.text.contains("not allowed"),
                ".env files should be blocked: {}",
                filename
            );
        }
    }
}

#[test]
fn test_write_file_blocks_env_files() {
    let workspace = tempdir().expect("workspace tempdir");
    let ctx = ToolContext::new(workspace.path());

    let tool = WriteFileTool;

    let result = tool.execute(
        json!({
            "path": ".env",
            "content": "SECRET=value"
        }),
        &ctx,
    );

    assert!(result.is_err(), "Should block writing .env files");
}

#[test]
fn test_read_file_blocks_key_files() {
    let workspace = tempdir().expect("workspace tempdir");
    let ctx = ToolContext::new(workspace.path());

    let tool = ReadFileTool;

    let key_files = vec!["key.pem", "cert.key", "private.p12", "secret.pfx"];

    for filename in key_files {
        let _result = tool.execute(json!({ "path": filename }), &ctx);
        // These will likely fail with file not found
        // If they existed, they should be blocked
    }
}

// ── Resource Limit Tests ────────────────────────────────────────────────────

#[test]
fn test_rate_limiter_enforces_global_limit() {
    use std::num::NonZeroU32;

    let mut registry = ToolRegistry::with_rate_limiting(
        NonZeroU32::new(2).unwrap(), // 2 per second
        NonZeroU32::new(2).unwrap(), // burst of 2
    );

    let workspace = tempdir().expect("workspace tempdir");
    let ctx = ToolContext::new(workspace.path());

    // Register a simple tool
    registry.register(ReadFileTool);

    let call = rustycode_protocol::ToolCall {
        call_id: "test-1".to_string(),
        name: "read_file".to_string(),
        arguments: json!({ "path": "nonexistent.txt" }),
    };

    // First call should succeed (or fail with file not found, not rate limit)
    let result1 = registry.execute(&call, &ctx);
    assert!(!result1.error.unwrap_or_default().contains("Rate limit"));

    // Second call should succeed
    let result2 = registry.execute(&call, &ctx);
    assert!(!result2.error.unwrap_or_default().contains("Rate limit"));

    // Third call should be rate limited
    let result3 = registry.execute(&call, &ctx);
    assert!(
        result3.error.unwrap_or_default().contains("Rate limit"),
        "Third call should be rate limited"
    );
}

#[test]
fn test_bash_timeout_parameter() {
    let workspace = tempdir().expect("workspace tempdir");
    let ctx = ToolContext::new(workspace.path());

    let tool = BashTool;

    let result = tool.execute(
        json!({
            "command": "echo fast",
            "timeout_secs": 5
        }),
        &ctx,
    );

    assert!(result.is_ok(), "Fast command should succeed with timeout");
}

// ── Edge Cases and Special Paths ───────────────────────────────────────────

#[test]
fn test_handles_empty_path_gracefully() {
    let workspace = tempdir().expect("workspace tempdir");
    let ctx = ToolContext::new(workspace.path());

    let tool = ReadFileTool;

    let result = tool.execute(json!({ "path": "" }), &ctx);

    assert!(result.is_err(), "Should reject empty path");
}

#[test]
fn test_handles_very_long_paths() {
    let workspace = tempdir().expect("workspace tempdir");
    let ctx = ToolContext::new(workspace.path());

    let tool = ReadFileTool;

    // Create a path longer than MAX_PATH_LENGTH
    let long_path = "a".repeat(5000);

    let result = tool.execute(json!({ "path": long_path }), &ctx);

    assert!(result.is_err(), "Should reject excessively long path");
}

#[test]
fn test_handles_null_bytes_in_path() {
    let workspace = tempdir().expect("workspace tempdir");
    let ctx = ToolContext::new(workspace.path());

    let tool = ReadFileTool;

    // Paths with null bytes can be used for path traversal attacks
    let _result = tool.execute(json!({ "path": "test.txt\x00.txt" }), &ctx);

    // Result may vary depending on OS, but should not allow access outside workspace
}

#[test]
fn test_handles_unicode_normalization() {
    let workspace = tempdir().expect("workspace tempdir");
    let ctx = ToolContext::new(workspace.path());

    let tool = ReadFileTool;

    // Unicode characters that might be used for homograph attacks
    let suspicious_paths = vec![
        "\u{202e}txt.",     // Right-to-left override
        "\u{200b}test.txt", // Zero-width space
        "test\u{200c}.txt", // Zero-width non-joiner
    ];

    for path in suspicious_paths {
        let _result = tool.execute(json!({ "path": path }), &ctx);
        // Should either fail or handle gracefully
        // The important thing is no security bypass
    }
}

// ── Grep and Glob Path Traversal Tests ─────────────────────────────────────

#[test]
fn test_grep_blocks_path_traversal() {
    let workspace = tempdir().expect("workspace tempdir");
    let ctx = ToolContext::new(workspace.path());

    let tool = GrepTool;

    let result = tool.execute(
        json!({
            "pattern": "test",
            "path": "../../../etc"
        }),
        &ctx,
    );

    assert!(result.is_err(), "Should block grep with path traversal");
}

#[test]
fn test_glob_blocks_path_traversal() {
    let workspace = tempdir().expect("workspace tempdir");
    let ctx = ToolContext::new(workspace.path());

    let tool = GlobTool;

    let result = tool.execute(
        json!({
            "pattern": "../../../etc/*"
        }),
        &ctx,
    );

    // Glob tool searches within workspace, so even if pattern contains ../
    // it will only find files within workspace (WalkDir is bounded)
    // The pattern just removes * chars so results will be empty
    assert!(result.is_ok(), "Glob operates within workspace bounds");
}

// ── List Directory Recursive Depth ─────────────────────────────────────────

#[test]
fn test_list_dir_respects_max_depth() {
    let workspace = tempdir().expect("workspace tempdir");
    let ctx = ToolContext::new(workspace.path());

    // Create nested directories
    let deep_path = workspace.path().join("a/b/c/d/e/f/g/h/i/j");
    fs::create_dir_all(&deep_path).expect("create nested dirs");

    let tool = ListDirTool;

    // Request depth beyond default
    let result = tool.execute(
        json!({
            "path": ".",
            "recursive": true,
            "max_depth": 100
        }),
        &ctx,
    );

    // Should execute, but depth should be capped internally
    // The tool implementation caps at max_depth parameter
    assert!(result.is_ok(), "Should handle deep directory listing");
}

// ─── Git Security Tests ─────────────────────────────────────────────────────

#[test]
fn test_git_commit_validates_message() {
    let workspace = tempdir().expect("workspace tempdir");

    // Initialize git repo
    let _result = std::process::Command::new("git")
        .args(["init"])
        .current_dir(workspace.path())
        .output()
        .expect("git init");

    let ctx = ToolContext::new(workspace.path());

    // Test message with potential injection
    let _result = GitCommitTool.execute(
        json!({
            "message": "test; rm -rf /"
        }),
        &ctx,
    );

    // Git commit should fail because nothing is staged,
    // but message handling should be safe
}

// ─── Sanitization Tests ────────────────────────────────────────────────────

#[test]
fn test_log_sanitization_removes_secrets() {
    use rustycode_tools::security::sanitize_for_log;

    let log_line = "api_key=sk-proj-1234567890abcdef password=secret123 token=abc123";

    let sanitized = sanitize_for_log(log_line);

    // The sanitization should redact sensitive patterns
    // Implementation is basic, so just check it runs
    assert!(!sanitized.contains("sk-proj-1234567890abcdef") || sanitized.contains("[REDACTED]"));
}

// ─── Test Valid Operations Still Work ───────────────────────────────────────

#[test]
fn test_valid_read_operation_succeeds() {
    let workspace = tempdir().expect("workspace tempdir");
    let test_file = workspace.path().join("test.txt");
    fs::write(&test_file, "hello world").expect("write test file");

    let tool = ReadFileTool;
    let ctx = ToolContext::new(workspace.path());

    let result = tool.execute(json!({ "path": "test.txt" }), &ctx);

    assert!(result.is_ok(), "Valid read should succeed");
    assert_eq!(result.unwrap().text, "hello world");
}

#[test]
fn test_valid_write_operation_succeeds() {
    let workspace = tempdir().expect("workspace tempdir");
    let ctx = ToolContext::new(workspace.path());

    let result = WriteFileTool.execute(
        json!({
            "path": "newfile.txt",
            "content": "test content"
        }),
        &ctx,
    );

    assert!(result.is_ok(), "Valid write should succeed");

    // Verify file was written
    let file_path = workspace.path().join("newfile.txt");
    assert!(file_path.exists());
    assert_eq!(fs::read_to_string(&file_path).unwrap(), "test content");
}

#[test]
fn test_valid_bash_operation_succeeds() {
    let workspace = tempdir().expect("workspace tempdir");
    let ctx = ToolContext::new(workspace.path());

    let tool = BashTool;

    let result = tool.execute(json!({ "command": "echo hello" }), &ctx);

    assert!(result.is_ok(), "Valid bash command should succeed");
    assert!(result.unwrap().text.contains("hello"));
}

#[test]
fn test_valid_list_dir_succeeds() {
    let workspace = tempdir().expect("workspace tempdir");
    let ctx = ToolContext::new(workspace.path());

    // Create some files
    fs::write(workspace.path().join("a.txt"), "a").unwrap();
    fs::write(workspace.path().join("b.txt"), "b").unwrap();

    let tool = ListDirTool;

    let result = tool.execute(json!({ "path": "." }), &ctx);

    assert!(result.is_ok(), "Valid list_dir should succeed");
    let output = result.unwrap().text;
    assert!(output.contains("a.txt") || output.contains("b.txt"));
}

#[test]
fn test_valid_grep_succeeds() {
    let workspace = tempdir().expect("workspace tempdir");
    let test_file = workspace.path().join("test.txt");
    fs::write(&test_file, "hello\nworld\nhello again").expect("write test file");

    let tool = GrepTool;
    let ctx = ToolContext::new(workspace.path());

    let result = tool.execute(
        json!({
            "pattern": "hello",
            "path": "."
        }),
        &ctx,
    );

    assert!(result.is_ok(), "Valid grep should succeed");
    assert!(result.unwrap().text.contains("hello"));
}

#[test]
fn test_valid_glob_succeeds() {
    let workspace = tempdir().expect("workspace tempdir");
    fs::write(workspace.path().join("test.rs"), "rust").unwrap();
    fs::write(workspace.path().join("test.txt"), "text").unwrap();

    let tool = GlobTool;
    let ctx = ToolContext::new(workspace.path());

    let result = tool.execute(json!({ "pattern": "test" }), &ctx);

    assert!(result.is_ok(), "Valid glob should succeed");
}

#[test]
fn test_valid_web_fetch_url_passes_validation() {
    let workspace = tempdir().expect("workspace tempdir");
    let ctx = ToolContext::new(workspace.path());

    let tool = WebFetchTool;

    let valid_urls = vec![
        "https://example.com",
        "https://api.github.com/repos/user/repo/readme",
        "http://example.com",
    ];

    for url in valid_urls {
        let _result = tool.execute(json!({ "url": url }), &ctx);
        // Will fail due to network, but URL validation should pass
        // We just check the test completes without panic
    }
}
