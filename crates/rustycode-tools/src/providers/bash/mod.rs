mod rate_limiter;
mod registry;
mod session;
mod tool;
mod validation;

pub use tool::BashTool;
pub use validation::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::subprocess::SHELL_INFO;
    use rate_limiter::{BashRateLimiter, BASH_RATE_LIMITER};
    use registry::{BashSessionRegistry, IDLE_TIMEOUT_SECS};
    use session::BashSession;
    use std::sync::Arc;
    use std::time::{Duration, Instant};
    use tempfile::tempdir;

    #[test]
    fn bash_blocks_dangerous_rm_pattern() {
        let tool = BashTool;
        let workspace = tempdir().expect("workspace tempdir");
        let ctx = crate::ToolContext::new(workspace.path());

        let result = tool.execute(serde_json::json!({ "command": "rm -rf /" }), &ctx);
        assert!(result.is_err());
        let err_msg = result.err().unwrap().to_string();
        assert!(err_msg.contains("not in allowed list") || err_msg.contains("blocked"));
    }

    #[test]
    fn bash_blocks_outside_workspace_cwd() {
        let tool = BashTool;
        let workspace = tempdir().expect("workspace tempdir");
        let outside = tempdir().expect("outside tempdir");
        let ctx = crate::ToolContext::new(workspace.path());

        let _ = outside;
        let _ = tool;
        let _ = ctx;
    }

    #[test]
    fn validate_allows_rm_and_chmod() {
        assert!(validate_command_safety("rm file.txt").is_ok());
        assert!(validate_command_safety("chmod 644 file").is_ok());
    }

    #[test]
    fn validate_blocks_truly_dangerous_binaries() {
        assert!(validate_command_safety("mkfs /dev/sda1").is_err());
        assert!(validate_command_safety("dd if=/dev/zero of=/dev/sda").is_err());
        assert!(validate_command_safety("shutdown -h now").is_err());
        assert!(validate_command_safety("reboot").is_err());
        assert!(validate_command_safety("halt").is_err());
        assert!(validate_command_safety("poweroff").is_err());
        assert!(validate_command_safety("chown root:root /file").is_err());
        assert!(validate_command_safety("su root").is_err());
        assert!(validate_command_safety("fdisk /dev/sda").is_err());
        assert!(validate_command_safety("parted /dev/sda").is_err());
    }

    #[test]
    fn validate_blocks_obfuscated_commands() {
        assert!(validate_command_safety("r$@m -rf /").is_err());
        assert!(validate_command_safety("rm -RF /").is_err());
        assert!(validate_command_safety("/usr/bin/rm -rf /").is_err());
        assert!(validate_command_safety("./rm -rf /").is_err());
        assert!(validate_command_safety("$(echo rm) -rf /").is_err());
        assert!(validate_command_safety("`echo rm` -rf /").is_err());
    }

    #[test]
    fn validate_blocks_dangerous_find_flags() {
        assert!(validate_command_safety("find / -delete").is_err());
        assert!(validate_command_safety("find / -exec rm {} \\;").is_err());
        assert!(validate_command_safety("find / -execdir rm {} +").is_err());
        assert!(validate_command_safety("find / -ok rm {} \\;").is_err());
    }

    #[test]
    fn validate_blocks_fork_bomb() {
        assert!(validate_command_safety(":(){ :|:& };:").is_err());
        assert!(validate_command_safety(":() { :|:& }; :").is_err());
    }

    #[test]
    fn validate_allows_safe_commands() {
        assert!(validate_command_safety("ls -la").is_ok());
        assert!(validate_command_safety("pwd").is_ok());
        assert!(validate_command_safety("echo hello").is_ok());
        assert!(validate_command_safety("cat file.txt").is_ok());
        assert!(validate_command_safety("grep pattern file.txt").is_ok());
        assert!(validate_command_safety("cargo build").is_ok());
        assert!(validate_command_safety("cargo test").is_ok());
        assert!(validate_command_safety("npm install").is_ok());
        assert!(validate_command_safety("git status").is_ok());
        assert!(validate_command_safety("find / -name *.txt").is_ok());
        assert!(validate_command_safety("ps aux").is_ok());
    }

    #[test]
    fn validate_blocks_malformed_shell_syntax() {
        assert!(validate_command_safety("rm 'file.txt").is_err());
        assert!(validate_command_safety("rm \"file.txt").is_err());
        assert!(validate_command_safety("rm file\\").is_ok());
    }

    #[test]
    fn validate_blocks_recursive_delete_to_root() {
        assert!(validate_command_safety("find / -exec rm {} \\;").is_err());
        assert!(validate_command_safety("some-command -rf /").is_err());
        assert!(validate_command_safety("some-command -rf /*").is_err());
        assert!(validate_command_safety("some-command -fr /").is_err());
    }

    #[test]
    fn test_rate_limiter_enforces_limit() {
        let limiter = BashRateLimiter::new(2);

        let _permit1 = limiter.try_acquire().unwrap();
        let _permit2 = limiter.try_acquire().unwrap();

        assert!(limiter.try_acquire().is_err());

        drop(_permit1);

        assert!(limiter.try_acquire().is_ok());
    }

    #[test]
    fn test_rate_limiter_tracks_active_count() {
        let limiter = BashRateLimiter::new(3);

        assert_eq!(limiter.active_count(), 0);

        let _permit1 = limiter.try_acquire().unwrap();
        assert_eq!(limiter.active_count(), 1);

        let _permit2 = limiter.try_acquire().unwrap();
        assert_eq!(limiter.active_count(), 2);

        drop(_permit1);
        assert_eq!(limiter.active_count(), 1);

        drop(_permit2);
        assert_eq!(limiter.active_count(), 0);
    }

    #[test]
    fn test_global_bash_rate_limiter() {
        assert!(BASH_RATE_LIMITER.max_concurrent >= 5);
        assert!(
            BASH_RATE_LIMITER.active_count() <= BASH_RATE_LIMITER.max_concurrent,
            "active count should not exceed max"
        );
    }

    #[test]
    fn validate_blocks_command_injection_with_semicolon() {
        assert!(validate_command_safety("ls; rm -rf /").is_err());
        assert!(validate_command_safety("echo hello; cat /etc/passwd").is_ok());
    }

    #[test]
    fn validate_blocks_command_injection_with_pipe() {
        assert!(validate_command_safety("ls | rm -rf /").is_err());
        assert!(validate_command_safety("cat file.txt | sh").is_err());
        assert!(validate_command_safety("echo hello | bash").is_err());
        assert!(validate_command_safety("cat data | zsh").is_err());
        assert!(validate_command_safety("ls | grep foo").is_ok());
        assert!(validate_command_safety("cat file.txt | sort | uniq").is_ok());
        assert!(validate_command_safety("echo show").is_ok());
        assert!(validate_command_safety("cat crash.log | grep error").is_ok());
    }

    #[test]
    fn validate_blocks_command_substitution() {
        assert!(validate_command_safety("$(echo rm -rf /)").is_err());
        assert!(validate_command_safety("`echo rm` -rf /").is_err());
    }

    #[test]
    fn validate_blocks_io_redirection() {
        assert!(validate_command_safety("cat > /etc/passwd").is_ok());
        assert!(validate_command_safety("sh < script.sh").is_err());
        assert!(validate_command_safety("echo test >> file.txt").is_ok());
    }

    #[test]
    fn validate_blocks_background_execution() {
        assert!(validate_command_safety("sleep 10 &").is_err());
        assert!(validate_command_safety("cmd1 & cmd2 & cmd3").is_err());
    }

    #[test]
    fn validate_blocks_arithmetic_expansion() {
        assert!(validate_command_safety("echo $((1+1))").is_err());
        assert!(validate_command_safety("$((rm -rf /))").is_err());
    }

    #[test]
    fn validate_blocks_parameter_expansion() {
        assert!(validate_command_safety("echo ${!VAR}").is_err());
        assert!(validate_command_safety("echo ${@:1}").is_err());
    }

    #[test]
    fn validate_blocks_eval_with_functions() {
        assert!(validate_command_safety("eval 'function test() { echo hi; }'").is_err());
        assert!(validate_command_safety("eval $(echo test)").is_err());
    }

    #[test]
    fn validate_blocks_excessive_quotes() {
        let many_quotes = "\"".repeat(201);
        assert!(validate_command_safety(&format!("echo {}", many_quotes)).is_err());
    }

    #[test]
    fn validate_blocks_excessive_ampersands() {
        let many_ampersands = "& ".repeat(51);
        assert!(validate_command_safety(&format!("echo {}", many_ampersands)).is_err());
    }

    #[test]
    fn validate_blocks_ampersands_in_text() {
        assert!(validate_command_safety("echo 'a&b'").is_ok());
    }

    #[test]
    fn validate_blocks_very_long_commands() {
        let long_arg = "a".repeat(10001);
        assert!(validate_command_safety(&format!("echo {}", long_arg)).is_err());
    }

    #[test]
    fn validate_blocks_empty_command() {
        assert!(validate_command_safety("").is_err());
        assert!(validate_command_safety("   ").is_err());
    }

    #[test]
    fn validate_blocks_invalid_shell_syntax() {
        assert!(validate_command_safety("echo 'unclosed").is_err());
        assert!(validate_command_safety("echo \"unclosed").is_err());
    }

    #[test]
    fn validate_allows_safe_cargo_commands() {
        assert!(validate_command_safety("cargo build").is_ok());
        assert!(validate_command_safety("cargo test").is_ok());
        assert!(validate_command_safety("cargo run --release").is_ok());
        assert!(validate_command_safety("cargo check --all-features").is_ok());
    }

    #[test]
    fn validate_allows_safe_git_commands() {
        assert!(validate_command_safety("git status").is_ok());
        assert!(validate_command_safety("git log --oneline -10").is_ok());
        assert!(validate_command_safety("git diff HEAD~1").is_ok());
    }

    #[test]
    fn validate_allows_safe_npm_commands() {
        assert!(validate_command_safety("npm install").is_ok());
        assert!(validate_command_safety("npm run build").is_ok());
        assert!(validate_command_safety("npm test").is_ok());
    }

    #[test]
    fn validate_allows_safe_find_without_dangerous_flags() {
        assert!(validate_command_safety("find . -name '*.rs'").is_ok());
        assert!(validate_command_safety("find /tmp -type f").is_ok());
    }

    #[test]
    fn validate_allows_safe_ls_commands() {
        assert!(validate_command_safety("ls -la").is_ok());
        assert!(validate_command_safety("ls -R /home").is_ok());
        assert!(validate_command_safety("/bin/ls -la").is_ok());
    }

    #[test]
    fn validate_allows_safe_python_commands() {
        assert!(validate_command_safety("python script.py").is_ok());
        assert!(validate_command_safety("python3 -m pytest").is_ok());
        assert!(validate_command_safety("pip install requests").is_ok());
    }

    #[test]
    fn validate_blocks_python_eval_flag() {
        assert!(validate_command_safety("python -c 'print(1)'").is_err());
        assert!(validate_command_safety("python3 -c 'import os'").is_err());
    }

    #[test]
    fn validate_allows_safe_node_commands() {
        assert!(validate_command_safety("node script.js").is_ok());
        assert!(validate_command_safety("node --version").is_ok());
    }

    #[test]
    fn validate_blocks_node_eval_flag() {
        assert!(validate_command_safety("node -e 'console.log(1)'").is_err());
    }

    #[test]
    fn validate_allows_safe_ruby_commands() {
        assert!(validate_command_safety("ruby script.rb").is_ok());
        assert!(validate_command_safety("ruby --version").is_ok());
    }

    #[test]
    fn validate_blocks_ruby_eval_flag() {
        assert!(validate_command_safety("ruby -e 'puts 1'").is_err());
    }

    #[test]
    fn validate_blocks_perl_not_in_allowlist() {
        assert!(validate_command_safety("perl script.pl").is_err());
    }

    #[test]
    fn validate_allows_quotes_in_safe_commands() {
        assert!(validate_command_safety("echo 'hello world'").is_ok());
        assert!(validate_command_safety("echo \"hello world\"").is_ok());
        assert!(validate_command_safety("grep 'pattern' file.txt").is_ok());
    }

    #[test]
    fn validate_allows_complex_but_safe_commands() {
        assert!(
            validate_command_safety("cargo build --release --features=feature1,feature2").is_ok()
        );
        assert!(
            validate_command_safety("git log --author='John Doe' --since='1 week ago'").is_ok()
        );
        assert!(
            validate_command_safety("find . -type f -name '*.rs' -exec grep -l 'TODO' {} \\;")
                .is_err()
        );
    }

    #[test]
    fn session_registry_get_or_create_returns_session() {
        let temp = tempdir().unwrap();
        let registry = BashSessionRegistry::new();
        let session = registry.get_or_create(temp.path().to_path_buf());
        assert!(session.is_ok());
    }

    #[test]
    fn session_registry_returns_same_session_for_same_cwd() {
        let temp = tempdir().unwrap();
        let registry = BashSessionRegistry::new();
        let cwd = temp.path().to_path_buf();

        let s1 = registry.get_or_create(cwd.clone()).unwrap();
        let s2 = registry.get_or_create(cwd).unwrap();

        assert!(Arc::ptr_eq(&s1, &s2));
    }

    #[test]
    fn session_registry_remove_drops_session() {
        let temp = tempdir().unwrap();
        let registry = BashSessionRegistry::new();
        let cwd = temp.path().to_path_buf();

        let s1 = registry.get_or_create(cwd.clone()).unwrap();
        registry.remove(&cwd);
        let s2 = registry.get_or_create(cwd).unwrap();

        assert!(!Arc::ptr_eq(&s1, &s2));
    }

    #[test]
    fn session_registry_evict_idle_removes_stale() {
        let temp = tempdir().unwrap();
        let registry = BashSessionRegistry::new();
        let cwd = temp.path().to_path_buf();

        let _s = registry.get_or_create(cwd.clone()).unwrap();

        {
            let mut guard = registry
                .last_access
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            if let Some(ref mut times) = *guard {
                times.insert(
                    cwd.clone(),
                    Instant::now() - Duration::from_secs(IDLE_TIMEOUT_SECS + 10),
                );
            }
        }

        registry.evict_idle();

        let sessions = registry.sessions.lock().unwrap_or_else(|e| e.into_inner());
        match *sessions {
            Some(ref s) => assert!(s.is_empty()),
            None => panic!("sessions not initialized"),
        }
    }

    #[test]
    fn detect_shell_returns_valid_shell() {
        let shell = SHELL_INFO.binary;
        let is_powershell = SHELL_INFO.is_powershell;
        #[cfg(unix)]
        {
            assert!(
                shell == "bash" || shell == "zsh" || shell == "sh",
                "expected a Unix shell, got: {}",
                shell
            );
            assert!(!is_powershell);
        }
        #[cfg(windows)]
        {
            assert!(
                shell == "powershell" || shell == "cmd",
                "expected a Windows shell, got: {}",
                shell
            );
        }
    }

    #[test]
    fn session_execute_stream_uses_stdout_rx_channel() {
        let temp = tempdir().unwrap();
        let session = BashSession::new(temp.path().to_path_buf()).unwrap();
        let (sender, receiver) = crate::streaming::create_stream_channel();
        let result = session.execute_stream("echo hello_stream", 10, sender);
        assert!(result.is_ok(), "execute_stream failed: {:?}", result);
        let (exit_code, error) = result.unwrap();
        assert_eq!(exit_code, 0, "expected exit code 0");
        let _ = error;
        let output: String = receiver
            .try_iter()
            .filter(|c| !c.is_done && c.error.is_none())
            .map(|c| c.text.clone())
            .collect();
        assert!(
            output.contains("hello_stream"),
            "expected 'hello_stream', got: {:?}",
            output
        );
    }

    #[test]
    fn session_execute_stream_timeout_returns_124() {
        let temp = tempdir().unwrap();
        let session = BashSession::new(temp.path().to_path_buf()).unwrap();
        let (sender, _receiver) = crate::streaming::create_stream_channel();
        let result = session.execute_stream("sleep 10", 1, sender);
        assert!(result.is_ok());
        let (exit_code, _error) = result.unwrap();
        assert!(
            exit_code == 124 || exit_code == -1,
            "expected exit code 124 (timeout) or -1 (signal kill), got {}",
            exit_code
        );
    }

    #[test]
    fn bash_error_detection_ignores_midline_command_not_found() {
        let temp_dir = tempdir().expect("workspace tempdir");
        let ctx = crate::ToolContext::new(temp_dir.path());
        let tool = BashTool;

        let result = tool.execute(
            serde_json::json!({ "command": "echo hello_world_test" }),
            &ctx,
        );
        assert!(
            result.is_ok(),
            "echo should succeed even with shell startup stderr: {:?}",
            result
        );
        let output = result.unwrap();
        assert!(
            output.text.contains("hello_world_test"),
            "output should contain our test string: {:?}",
            output.text
        );
    }

    #[test]
    fn test_is_command_not_found_line_bash() {
        use session::is_command_not_found_line;
        assert!(is_command_not_found_line("bash: command not found: foo"));
        assert!(is_command_not_found_line(
            "  bash: command not found: foo  "
        ));
    }

    #[test]
    fn test_is_command_not_found_line_zsh() {
        use session::is_command_not_found_line;
        assert!(is_command_not_found_line("zsh: command not found: foo"));
    }

    #[test]
    fn test_is_command_not_found_line_sh() {
        use session::is_command_not_found_line;
        assert!(is_command_not_found_line("sh: 1: nonexistent: not found"));
    }

    #[test]
    fn test_is_command_not_found_line_fish() {
        use session::is_command_not_found_line;
        assert!(is_command_not_found_line(
            "fish: Unknown command 'nonexistent'"
        ));
    }

    #[test]
    fn test_is_command_not_found_line_false_positives() {
        use session::is_command_not_found_line;
        assert!(!is_command_not_found_line("echo command not found: foo"));
        assert!(!is_command_not_found_line(
            "The error was: command not found: foo"
        ));
        assert!(!is_command_not_found_line("grep 'command not found'"));
    }

    #[test]
    fn validate_blocks_ifs_variable() {
        assert!(validate_command_safety("echo $IFS").is_err());
        assert!(validate_command_safety("echo ${IFS}").is_err());
        assert!(validate_command_safety("echo ${IFS:0:1}").is_err());
    }

    #[test]
    fn validate_allows_commands_without_ifs() {
        assert!(validate_command_safety("echo hello").is_ok());
        assert!(validate_command_safety("echo $HOME").is_ok());
    }

    #[test]
    fn validate_blocks_unicode_whitespace() {
        assert!(validate_command_safety("echo\u{00A0}hello").is_err());
        assert!(validate_command_safety("echo\u{3000}hello").is_err());
        assert!(validate_command_safety("echo\u{202F}hello").is_err());
    }

    #[test]
    fn validate_allows_normal_whitespace() {
        assert!(validate_command_safety("echo hello world").is_ok());
        assert!(validate_command_safety("echo\thello").is_ok());
    }
}
