//! `BashTool` — the `Tool` and `ToolStreaming` implementations.

use super::rate_limiter::BASH_RATE_LIMITER;
use super::registry::BASH_SESSION_REGISTRY;
use super::session::BashSession;
use super::validation::{ensure_path_within_workspace, validate_command_safety};
use crate::truncation::truncate_bash_output;
use crate::{ToolContext, ToolOutput, ToolPermission, ToolTag};
use anyhow::{anyhow, Result};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

/// Input parameters for `Bash`
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default, PartialEq, Eq)]
pub struct BashParams {
    /// The command to execute
    pub command: String,
    /// Optional timeout in milliseconds (max 600000)
    #[serde(
        rename = "timeout",
        alias = "timeout_secs",
        skip_serializing_if = "Option::is_none"
    )]
    pub timeout_secs: Option<u64>,
    /// Clear, concise description of what this command does in active voice. Never use words like "complex" or "risk" in the description — just describe what it does.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Set to true to run this command in the background. Use Read to read the output later.
    #[serde(default)]
    pub run_in_background: bool,
    /// If true, restart the bash session before executing the command
    #[serde(default)]
    #[schemars(skip)]
    pub restart: bool,
}

/// Execute a command in an isolated Docker container.
///
/// Falls back to normal execution if Docker is unavailable.
fn execute_in_docker(
    command: &str,
    workspace: &Path,
    ctx: &crate::ToolContext,
) -> Result<ToolOutput> {
    use crate::providers::docker_isolation::{DockerIsolation, DockerIsolationConfig};

    if !DockerIsolation::is_docker_available() {
        tracing::warn!(
            "Docker isolation requested but Docker not available, falling back to local execution"
        );
        return Err(anyhow!(
            "Docker isolation requested but Docker is not available. \
             Please install Docker or disable isolation mode."
        ));
    }

    let config = DockerIsolationConfig::new();
    let isolation = DockerIsolation::new(config);

    let result = isolation.execute(command, workspace)?;

    let truncated = truncate_bash_output(&result.stdout, &result.stderr, result.exit_code);
    let output = if result.exit_code == 0 {
        truncated.output
    } else {
        format!(
            "Exit code: {}\n\n{}{}",
            result.exit_code,
            truncated.output,
            if result.stderr.is_empty() {
                String::new()
            } else {
                format!("\nStderr:\n{}", result.stderr)
            }
        )
    };

    Ok(ToolOutput::text(output).with_metadata(ctx, || {
        json!({
            "exit_code": result.exit_code,
            "container_id": result.container_id,
            "duration_ms": result.duration_ms,
            "isolated": true
        })
    }))
}

/// Execute a command inside an OS-level sandbox.
///
/// Uses `rustycode-sandbox` for platform-specific isolation:
/// - macOS: Seatbelt (sandbox-exec)
/// - Linux: Landlock + env stripping
/// - Windows: Job Objects (stub)
fn execute_in_os_sandbox(command: &str, ctx: &ToolContext) -> Result<ToolOutput> {
    use rustycode_sandbox::{SandboxManager, SandboxPolicy};

    let manager =
        SandboxManager::new().map_err(|e| anyhow!("Failed to initialize OS sandbox: {e}"))?;

    let policy = SandboxPolicy::from_config(
        ctx.sandbox.allowed_paths.as_deref(),
        &ctx.sandbox.denied_paths,
        ctx.sandbox.timeout_secs,
        &ctx.cwd,
    );

    let sandbox_result = tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(manager.execute(command, &policy))
    })
    .map_err(|e| anyhow!("OS sandbox execution failed: {e}"))?;

    let truncated = truncate_bash_output(
        &sandbox_result.stdout,
        &sandbox_result.stderr,
        sandbox_result.exit_code.unwrap_or(-1),
    );

    let output = if sandbox_result.exit_code.unwrap_or(-1) == 0 {
        truncated.output
    } else {
        format!(
            "Exit code: {}\n\n{}{}",
            sandbox_result
                .exit_code
                .map(|c| c.to_string())
                .unwrap_or_else(|| "unknown".to_string()),
            truncated.output,
            if sandbox_result.stderr.is_empty() {
                String::new()
            } else {
                format!("\nStderr:\n{}", sandbox_result.stderr)
            },
        )
    };

    Ok(ToolOutput::text(output).with_metadata(ctx, || {
        json!({
            "exit_code": sandbox_result.exit_code,
            "os_sandbox": true,
            "sandbox_available": manager.is_available(),
            "timed_out": sandbox_result.timed_out,
        })
    }))
}

#[cfg(windows)]
fn try_native_fallback(command: &str, workspace: &Path) -> Option<Result<ToolOutput>> {
    use crate::native_tools::{native_cat, native_grep, native_ls};

    let trimmed = command.trim();
    let parts: Vec<&str> = trimmed.split_whitespace().collect();
    if parts.is_empty() {
        return None;
    }

    let binary = parts[0].to_lowercase();
    match binary.as_str() {
        "cat" if parts.len() == 2 && !parts[1].starts_with('-') => {
            let path = std::path::Path::new(parts[1]);
            if let Err(e) =
                crate::security::cross_platform::validate_path_in_workspace(path, workspace)
            {
                return Some(Err(e));
            }
            Some(
                native_cat(path)
                    .map(ToolOutput::text)
                    .map_err(|e| anyhow::anyhow!("native cat failed: {e}")),
            )
        }
        "ls" if parts.len() == 1 || (parts.len() == 2 && !parts[1].starts_with('-')) => {
            let target = if parts.len() == 2 {
                std::path::Path::new(parts[1])
            } else {
                std::path::Path::new(".")
            };
            if let Err(e) =
                crate::security::cross_platform::validate_path_in_workspace(target, workspace)
            {
                return Some(Err(e));
            }
            Some(
                native_ls(target)
                    .map(|files| ToolOutput::text(files.join("\n")))
                    .map_err(|e| anyhow::anyhow!("native ls failed: {e}")),
            )
        }
        "Grep" => {
            let mut args = parts[1..].iter().peekable();
            let mut pattern = None;
            let mut target_path = None;

            while let Some(&arg) = args.next() {
                if arg.starts_with('-') {
                    continue;
                }
                if pattern.is_none() {
                    pattern = Some(arg);
                } else {
                    target_path = Some(arg);
                }
            }

            match (pattern, target_path) {
                (Some(pat), Some(path)) => {
                    let grep_path = std::path::Path::new(path);
                    if let Err(e) = crate::security::cross_platform::validate_path_in_workspace(
                        grep_path, workspace,
                    ) {
                        return Some(Err(e));
                    }
                    Some(
                        native_grep(grep_path, pat)
                            .map(ToolOutput::text)
                            .map_err(|e| anyhow::anyhow!("native grep failed: {e}")),
                    )
                }
                _ => None,
            }
        }
        _ => None,
    }
}

rustycode_tools_api::define_tool! {
    pub struct BashTool;

    name: "Bash",
    description: "Executes a given bash command and returns its output.\nThe working directory persists between commands, but shell state does not. The shell environment is initialized from the user's profile (bash or zsh).\nIMPORTANT: Avoid using this tool to run `cat`, `head`, `tail`, `sed`, `awk`, or `echo` commands, unless explicitly instructed or after you have verified that a dedicated tool cannot accomplish your task. Instead, use the appropriate dedicated tool as this will provide a much better experience for the user:\n - Read files: Use Read (NOT cat/head/tail)\n - Edit files: Use Edit (NOT sed/awk)\n - Write files: Use Write (NOT echo >/cat <<EOF)\n - Communication: Output text directly (NOT echo/printf)\nWhile the Bash tool can do similar things, the dedicated tools have been optimized for correct permissions and access.",
    permission: ToolPermission::Execute,
    tags: [ToolTag::Implement, ToolTag::Ops],

    execute(params: BashParams, ctx) {
        crate::check_permission(ToolPermission::Execute, ctx)?;

        if let Some(gate) = &ctx.plan_gate {
            gate.check_access(ctx.role, "Bash")?;
        }

        let command = params.command;
        let restart = params.restart;
        let timeout_secs = params.timeout_secs
            .map(|t| if t > 600 { t / 1000 } else { t })
            .unwrap_or(120)
            .min(600);

        use crate::security::cross_platform::{
            allowed_commands, blocked_commands, validate_path_in_workspace, ShellType,
        };

        let shell_type = ShellType::Bash;
        validate_path_in_workspace(&ctx.cwd, &ctx.cwd)?;
        validate_command_safety(&command)?;

        let binary_name = super::validation::extract_binary_name(&command)?;
        let allowed_commands = allowed_commands(shell_type);
        if !allowed_commands.contains(&binary_name.as_str()) {
            anyhow::bail!("command '{}' is not in allowed list for bash", binary_name);
        }

        let blocked_commands = blocked_commands(shell_type);
        if blocked_commands.contains(&binary_name.as_str()) {
            anyhow::bail!("command '{}' is blocked for security reasons", binary_name);
        }

        let docker_requested = ctx.sandbox.docker_isolation
            || std::env::var("RUSTYCODE_DOCKER_ISOLATION")
                .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                .unwrap_or(false);

        if docker_requested {
            return execute_in_docker(&command, &ctx.cwd, ctx);
        }

        let os_sandbox_requested = ctx.sandbox.os_sandbox
            || std::env::var("RUSTYCODE_SANDBOX")
                .map(|v| {
                    let v = v.to_lowercase();
                    v == "seatbelt" || v == "landlock" || v == "os"
                })
                .unwrap_or(false);

        if os_sandbox_requested {
            return execute_in_os_sandbox(&command, ctx);
        }

        #[cfg(windows)]
        {
            if let Some(native_result) = try_native_fallback(&command, &ctx.cwd) {
                return native_result;
            }
        }

        let _permit = BASH_RATE_LIMITER.try_acquire().map_err(|()| {
            anyhow!(
                "Rate limit exceeded: {} concurrent bash commands already running. Maximum: {}. Please wait for current commands to complete.",
                BASH_RATE_LIMITER.active_count(),
                BASH_RATE_LIMITER.max_concurrent
            )
        })?;

        let start_time = Instant::now();

        let sandbox_mode = std::env::var("RUSTYCODE_SANDBOX").as_deref() == Ok("container");

        let session = if sandbox_mode || restart {
            BASH_SESSION_REGISTRY.remove(&ctx.cwd);
            BASH_SESSION_REGISTRY.get_or_create(ctx.cwd.clone())?
        } else {
            BASH_SESSION_REGISTRY.evict_idle();
            BASH_SESSION_REGISTRY.get_or_create(ctx.cwd.clone())?
        };

        let command_clone = command.clone();
        let cwd_clone = ctx.cwd.clone();
        let (stdout, stderr, exit_code) = if let Ok(handle) = tokio::runtime::Handle::try_current()
        {
            tokio::task::block_in_place(|| {
                handle.block_on(async {
                    let result = tokio::time::timeout(
                        Duration::from_secs(timeout_secs),
                        tokio::task::spawn_blocking(move || {
                            let s = session
                                .lock()
                                .unwrap_or_else(std::sync::PoisonError::into_inner);
                            let alive = s
                                .child
                                .lock()
                                .unwrap_or_else(std::sync::PoisonError::into_inner)
                                .is_some();
                            if !alive {
                                drop(s);
                                drop(session);
                                BASH_SESSION_REGISTRY.remove(&cwd_clone);
                                let fresh = BASH_SESSION_REGISTRY.get_or_create(cwd_clone)?;
                                let s = fresh
                                    .lock()
                                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                                return s.execute(&command_clone, timeout_secs);
                            }
                            s.execute(&command_clone, timeout_secs)
                        }),
                    )
                    .await;

                    if result.is_err() {
                        tracing::warn!(
                            "bash command timed out, evicting session for {:?}",
                            ctx.cwd
                        );
                        BASH_SESSION_REGISTRY.remove(&ctx.cwd);
                    }

                    result
                        .map_err(|_| anyhow!("command timed out after {timeout_secs}s"))?
                        .map_err(|e| anyhow!("command execution failed: {e}"))?
                })
            })
        } else {
            tokio::runtime::Runtime::new()
                .map_err(|e| anyhow!("failed to create tokio runtime: {e}"))?
                .block_on(async {
                    let cwd_for_evict = ctx.cwd.clone();
                    let result = tokio::time::timeout(
                        Duration::from_secs(timeout_secs),
                        tokio::task::spawn_blocking(move || {
                            let s = session
                                .lock()
                                .unwrap_or_else(std::sync::PoisonError::into_inner);
                            let alive = s
                                .child
                                .lock()
                                .unwrap_or_else(std::sync::PoisonError::into_inner)
                                .is_some();
                            if !alive {
                                drop(s);
                                drop(session);
                                BASH_SESSION_REGISTRY.remove(&cwd_clone);
                                let fresh = BASH_SESSION_REGISTRY.get_or_create(cwd_clone)?;
                                let s = fresh
                                    .lock()
                                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                                return s.execute(&command_clone, timeout_secs);
                            }
                            s.execute(&command_clone, timeout_secs)
                        }),
                    )
                    .await;

                    if result.is_err() {
                        tracing::warn!(
                            "bash command timed out, evicting session for {:?}",
                            cwd_for_evict
                        );
                        BASH_SESSION_REGISTRY.remove(&cwd_for_evict);
                    }

                    result
                        .map_err(|_| anyhow!("command timed out after {timeout_secs}s"))?
                        .map_err(|e| anyhow!("command execution failed: {e}"))?
                })
        }?;

        let execution_time = start_time.elapsed();

        let truncated = truncate_bash_output(&stdout, &stderr, exit_code);
        let mut output_text = truncated.as_str().to_string();

        let output_lower = output_text.to_lowercase();
        if exit_code != 0 {
            if output_lower.contains("python: command not found")
                || output_lower.contains("python: not found")
            {
                output_text.push_str("\n\nHINT: `python` is not available. Try `python3` instead. You can check with: which python3");
            } else if output_lower.contains("python3: command not found")
                || output_lower.contains("python3: not found")
            {
                output_text.push_str("\n\nHINT: `python3` is not available. Try `python` instead.");
            }
        }

        let metadata = {
            let mut meta = truncated.into_metadata();
            meta["exit_code"] = json!(exit_code);
            meta["command"] = json!(crate::security::validation::sanitize_for_log(&command));
            meta["execution_time_ms"] = json!(execution_time.as_millis());
            meta["timeout"] = json!(timeout_secs);
            if exit_code != 0 {
                meta["failed"] = json!(true);
            }
            meta
        };

        Ok(ToolOutput::text(output_text).with_metadata(ctx, || metadata))
    }
}

// ToolStreaming — kept as separate impl since define_tool! does not cover it.

impl crate::streaming::ToolStreaming for BashTool {
    fn execute_stream(
        &self,
        params: Value,
        ctx: &ToolContext,
    ) -> Result<crate::streaming::StreamReceiver> {
        use crate::streaming::{create_stream_channel, StreamChunk};

        crate::check_permission(ToolPermission::Execute, ctx)?;

        if let Some(gate) = &ctx.plan_gate {
            gate.check_access(ctx.role, "Bash")?;
        }

        let command = params
            .get("command")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                let actual = params
                    .get("command")
                    .map(ToString::to_string)
                    .unwrap_or_else(|| "null".to_string());
                anyhow!("missing string parameter 'command', got: {actual}")
            })?
            .to_string();

        let restart = params
            .get("restart")
            .and_then(Value::as_bool)
            .unwrap_or(false);

        let timeout_secs = params
            .get("timeout")
            .or_else(|| params.get("timeout_secs"))
            .and_then(Value::as_u64)
            .map(|t| if t > 600 { t / 1000 } else { t })
            .unwrap_or(120)
            .min(600);

        let cwd = ctx.cwd.clone();
        ensure_path_within_workspace(ctx, &cwd)?;
        validate_command_safety(&command)?;

        use crate::security::cross_platform::{allowed_commands, blocked_commands, ShellType};

        let shell_type = ShellType::Bash;
        let binary_name = super::validation::extract_binary_name(&command)?;
        let allowed_commands = allowed_commands(shell_type);
        if !allowed_commands.contains(&binary_name.as_str()) {
            anyhow::bail!("command '{}' is not in allowed list for bash", binary_name);
        }

        let blocked_commands = blocked_commands(shell_type);
        if blocked_commands.contains(&binary_name.as_str()) {
            anyhow::bail!("command '{}' is blocked for security reasons", binary_name);
        }

        let (sender, receiver) = create_stream_channel();
        let sender_clone = sender.clone();

        thread::spawn(move || {
            let start_time = Instant::now();

            let mut session = match BashSession::new(cwd) {
                Ok(s) => s,
                Err(e) => {
                    let _ = sender_clone.send(StreamChunk::error(e.to_string()));
                    return;
                }
            };

            if restart {
                if let Err(e) = session.restart() {
                    let _ = sender_clone.send(StreamChunk::error(e.to_string()));
                    return;
                }
            }

            let (exit_code, _error) =
                match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    session.execute_stream(&command, timeout_secs, sender_clone)
                })) {
                    Ok(Ok(result)) => result,
                    Ok(Err(e)) => {
                        let _ = sender.send(StreamChunk::error(e.to_string()));
                        return;
                    }
                    Err(_) => {
                        let _ = sender.send(StreamChunk::error(
                            "panic during command execution".to_string(),
                        ));
                        return;
                    }
                };

            let execution_time = start_time.elapsed();

            let metadata = json!({
                "exit_code": exit_code,
                "command": crate::security::validation::sanitize_for_log(&command),
                "execution_time_ms": execution_time.as_millis(),
                "timeout": timeout_secs,
                "streaming": true,
            });

            let _ = sender.send(StreamChunk::new(format!("\n[metadata] {metadata}\n")));
        });

        Ok(receiver)
    }
}
