use anyhow::Result;
use std::path::{Path, PathBuf};
use std::time::Instant;
use tokio::process::Command;

use super::types::{
    Hook, HookAction, HookExecutionResult, HookInput, HookOutput, HookProfile, HookResult,
    HookStatus, HookTrigger, HooksConfig,
};
use super::{config, env, protocol};

/// Hook manager — loads and executes lifecycle hooks
#[derive(Clone)]
pub struct HookManager {
    hooks_dir: PathBuf,
    hooks: Vec<Hook>,
    /// Unified hooks loaded from Claude Code / Codex format configs.
    compiled_hooks: std::collections::HashMap<HookTrigger, Vec<config::CompiledHook>>,
    profile: HookProfile,
    session_id: String,
}

impl HookManager {
    pub fn new(hooks_dir: PathBuf, profile: HookProfile, session_id: String) -> Self {
        Self {
            hooks_dir,
            hooks: Vec::new(),
            compiled_hooks: std::collections::HashMap::new(),
            profile,
            session_id,
        }
    }

    pub fn hooks_dir(&self) -> &Path {
        &self.hooks_dir
    }

    /// Load hooks from hooks.json config
    pub async fn load_hooks(&mut self) -> Result<()> {
        let config_path = self.hooks_dir.join("hooks.json");
        if !config_path.exists() {
            return Ok(());
        }

        let content = tokio::fs::read_to_string(&config_path).await?;
        let config: HooksConfig = serde_json::from_str(&content)?;
        self.hooks = config.hooks;
        tracing::info!("Loaded {} hooks from {:?}", self.hooks.len(), config_path);
        Ok(())
    }

    /// Load hooks from all unified config sources (Claude Code / Codex format).
    /// Merges with any legacy hooks loaded via `load_hooks`.
    pub async fn load_unified_hooks(&mut self, project_dir: &Path) -> Result<()> {
        let user_config_dir = config::ConfigLoader::default_user_config_dir();
        self.compiled_hooks = config::ConfigLoader::load_all(project_dir, &user_config_dir).await?;
        let total: usize = self.compiled_hooks.values().map(|v| v.len()).sum();
        tracing::info!("Loaded {total} unified hooks for project {:?}", project_dir);
        Ok(())
    }

    /// Execute PreToolUse hooks with matcher filtering.
    pub async fn pre_tool_use(
        &self,
        tool_name: &str,
        tool_input: &serde_json::Value,
        cwd: &Path,
    ) -> Result<protocol::PreToolUseResult> {
        let hooks = self.compiled_hooks.get(&HookTrigger::PreToolUse);
        let relevant = match hooks {
            Some(h) => h
                .iter()
                .filter(|h| h.matcher.matches(tool_name))
                .collect::<Vec<_>>(),
            None => return Ok(protocol::PreToolUseResult::default()),
        };
        if relevant.is_empty() {
            return Ok(protocol::PreToolUseResult::default());
        }

        let input = protocol::HookProtocolInput::PreToolUse {
            session_id: self.session_id.clone(),
            tool_name: tool_name.to_string(),
            tool_input: tool_input.clone(),
            cwd: cwd.to_string_lossy().into(),
        };
        let env_vars = env::hook_env(
            protocol::HookEvent::PreToolUse,
            tool_name,
            &self.session_id,
            cwd,
        );

        for hook in &relevant {
            if let Some(msg) = &hook.status_message {
                tracing::info!("[hook] {msg}");
            }
            match self.run_compiled_hook(hook, &input, &env_vars).await {
                Ok(output) => {
                    if output.decision == Some(protocol::HookDecision::Block) {
                        return Ok(protocol::PreToolUseResult {
                            blocked: true,
                            reason: output.reason,
                            additional_context: output.additional_context,
                            system_message: output.system_message,
                        });
                    }
                }
                Err(e) => {
                    tracing::error!("PreToolUse hook '{}' failed: {e}", hook.command);
                }
            }
        }
        Ok(protocol::PreToolUseResult::default())
    }

    /// Execute PostToolUse hooks with matcher filtering.
    pub async fn post_tool_use(
        &self,
        tool_name: &str,
        tool_input: &serde_json::Value,
        tool_response: &mut serde_json::Value,
        cwd: &Path,
    ) -> Result<protocol::PostToolUseResult> {
        let hooks = self.compiled_hooks.get(&HookTrigger::PostToolUse);
        let relevant = match hooks {
            Some(h) => h
                .iter()
                .filter(|h| h.matcher.matches(tool_name))
                .collect::<Vec<_>>(),
            None => return Ok(protocol::PostToolUseResult::default()),
        };
        if relevant.is_empty() {
            return Ok(protocol::PostToolUseResult::default());
        }

        let input = protocol::HookProtocolInput::PostToolUse {
            session_id: self.session_id.clone(),
            tool_name: tool_name.to_string(),
            tool_input: tool_input.clone(),
            tool_response: tool_response.clone(),
            cwd: cwd.to_string_lossy().into(),
        };
        let env_vars = env::hook_env(
            protocol::HookEvent::PostToolUse,
            tool_name,
            &self.session_id,
            cwd,
        );

        let mut result = protocol::PostToolUseResult::default();
        for hook in &relevant {
            if let Some(msg) = &hook.status_message {
                tracing::info!("[hook] {msg}");
            }
            match self.run_compiled_hook(hook, &input, &env_vars).await {
                Ok(output) => {
                    if output.decision == Some(protocol::HookDecision::Block) {
                        let replacement = output
                            .reason
                            .unwrap_or_else(|| "Hook blocked tool output".to_string());
                        *tool_response = serde_json::json!(replacement);
                        result.replaced = true;
                        result.replacement_text = Some(replacement);
                    }
                    if let Some(ctx) = output.additional_context {
                        result.additional_context = Some(ctx);
                    }
                    if let Some(msg) = output.system_message {
                        result.system_message = Some(msg);
                    }
                }
                Err(e) => {
                    tracing::error!("PostToolUse hook '{}' failed: {e}", hook.command);
                }
            }
        }
        Ok(result)
    }

    /// Execute PermissionRequest hooks with matcher filtering.
    pub async fn permission_request(
        &self,
        tool_name: &str,
        tool_input: &serde_json::Value,
        cwd: &Path,
    ) -> Result<protocol::PermissionResult> {
        let hooks = self.compiled_hooks.get(&HookTrigger::PermissionRequest);
        let relevant = match hooks {
            Some(h) => h
                .iter()
                .filter(|h| h.matcher.matches(tool_name))
                .collect::<Vec<_>>(),
            None => return Ok(protocol::PermissionResult::default()),
        };
        if relevant.is_empty() {
            return Ok(protocol::PermissionResult::default());
        }

        let input = protocol::HookProtocolInput::PermissionRequest {
            session_id: self.session_id.clone(),
            tool_name: tool_name.to_string(),
            tool_input: tool_input.clone(),
            cwd: cwd.to_string_lossy().into(),
        };
        let env_vars = env::hook_env(
            protocol::HookEvent::PermissionRequest,
            tool_name,
            &self.session_id,
            cwd,
        );

        for hook in &relevant {
            match self.run_compiled_hook(hook, &input, &env_vars).await {
                Ok(output) => match output.decision {
                    Some(protocol::HookDecision::Allow | protocol::HookDecision::Approve) => {
                        return Ok(protocol::PermissionResult {
                            decision: Some(true),
                            reason: output.reason,
                        });
                    }
                    Some(protocol::HookDecision::Block) => {
                        return Ok(protocol::PermissionResult {
                            decision: Some(false),
                            reason: output.reason,
                        });
                    }
                    None => continue,
                },
                Err(e) => {
                    tracing::error!("PermissionRequest hook '{}' failed: {e}", hook.command);
                }
            }
        }
        Ok(protocol::PermissionResult::default())
    }

    /// Run a compiled hook, sending protocol input via stdin.
    async fn run_compiled_hook(
        &self,
        hook: &config::CompiledHook,
        input: &protocol::HookProtocolInput,
        env_vars: &std::collections::HashMap<String, String>,
    ) -> Result<protocol::HookProtocolOutput> {
        let input_json = serde_json::to_string(input)?;
        let start = Instant::now();
        let timeout = std::time::Duration::from_secs(hook.timeout_secs.max(1));

        use std::process::Stdio;
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let mut cmd = Command::new("sh");
        cmd.arg("-c").arg(&hook.command);
        for (k, v) in env_vars {
            cmd.env(k, v);
        }

        let mut child = cmd
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(input_json.as_bytes()).await?;
            drop(stdin);
        }

        let mut stdout_buf = Vec::new();
        let mut stderr_buf = Vec::new();

        let status = tokio::time::timeout(timeout, async {
            if let Some(mut stdout) = child.stdout.take() {
                let _ = stdout.read_to_end(&mut stdout_buf).await;
            }
            if let Some(mut stderr) = child.stderr.take() {
                let _ = stderr.read_to_end(&mut stderr_buf).await;
            }
            child.wait().await
        })
        .await
        .map_err(|_| {
            let _ = child.start_kill();
            let _ = child.try_wait();
            anyhow::anyhow!(
                "Hook '{}' timed out after {}s",
                hook.command,
                hook.timeout_secs
            )
        })??;

        let output = std::process::Output {
            status,
            stdout: stdout_buf,
            stderr: stderr_buf,
        };
        let duration_ms = start.elapsed().as_millis();
        let stdout = String::from_utf8_lossy(&output.stdout);
        let exit_code = output.status.code();

        if exit_code == Some(2) {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Ok(protocol::HookProtocolOutput {
                decision: Some(protocol::HookDecision::Block),
                reason: Some(if stderr.trim().is_empty() {
                    "Hook blocked via exit code 2".to_string()
                } else {
                    stderr.trim().to_string()
                }),
                ..Default::default()
            });
        }
        if stdout.trim().is_empty() {
            return Ok(protocol::HookProtocolOutput::default());
        }

        let parsed: protocol::HookProtocolOutput = serde_json::from_str(stdout.trim())
            .unwrap_or_else(|e| {
                tracing::debug!(
                    "Hook '{}' non-JSON stdout (exit {}): {}",
                    hook.command,
                    exit_code.unwrap_or(-1),
                    e
                );
                protocol::HookProtocolOutput::default()
            });
        tracing::debug!(
            "Hook '{}' completed in {}ms (exit {})",
            hook.command,
            duration_ms,
            exit_code.unwrap_or(-1)
        );
        Ok(parsed)
    }

    /// Execute hooks for a trigger event
    pub async fn execute(
        &self,
        trigger: HookTrigger,
        context: serde_json::Value,
    ) -> Result<HookExecutionResult> {
        let relevant: Vec<_> = self
            .hooks
            .iter()
            .filter(|h| h.trigger == trigger && h.enabled && self.profile_allows(h))
            .collect();
        if relevant.is_empty() {
            return Ok(HookExecutionResult::default());
        }

        let mut results = Vec::new();
        let mut should_block = false;
        let mut blocking_hook = None;

        for hook in &relevant {
            match self.run_hook(hook, trigger, &context).await {
                Ok(result) => {
                    if result.actions.contains(&HookAction::Block)
                        || result.status == HookStatus::Blocked
                    {
                        should_block = true;
                        blocking_hook = Some(hook.name.clone());
                        results.push(result);
                        break;
                    }
                    results.push(result);
                }
                Err(e) => {
                    tracing::error!("Hook {} failed: {}", hook.name, e);
                    if hook.fail_on_error {
                        return Err(e);
                    }
                    results.push(HookResult {
                        hook_name: hook.name.clone(),
                        status: HookStatus::Error,
                        exit_code: None,
                        message: Some(e.to_string()),
                        actions: Vec::new(),
                        duration_ms: 0,
                    });
                }
            }
        }
        let block_reason = blocking_hook
            .as_ref()
            .map(|name| format!("Hook '{name}' blocked execution"));
        Ok(HookExecutionResult {
            results,
            should_block,
            block_reason,
            blocking_hook,
        })
    }

    pub fn set_enabled(&mut self, name: &str, enabled: bool) -> Result<()> {
        let hook = self
            .hooks
            .iter_mut()
            .find(|h| h.name == name)
            .ok_or_else(|| anyhow::anyhow!("Hook not found: {name}"))?;
        hook.enabled = enabled;
        Ok(())
    }

    pub fn list_hooks(&self) -> &[Hook] {
        &self.hooks
    }
    pub const fn profile(&self) -> HookProfile {
        self.profile
    }

    /// Synchronous version of execute
    pub fn execute_blocking(
        &self,
        trigger: HookTrigger,
        context: serde_json::Value,
    ) -> HookExecutionResult {
        match tokio::runtime::Handle::try_current() {
            Ok(handle) => {
                let result =
                    tokio::task::block_in_place(|| handle.block_on(self.execute(trigger, context)));
                result.unwrap_or_default()
            }
            Err(_) => HookExecutionResult::default(),
        }
    }

    /// Synchronous version of pre_tool_use
    pub fn pre_tool_use_blocking(
        &self,
        tool_name: &str,
        tool_input: &serde_json::Value,
        cwd: &Path,
    ) -> protocol::PreToolUseResult {
        match tokio::runtime::Handle::try_current() {
            Ok(handle) => {
                let result = tokio::task::block_in_place(|| {
                    handle.block_on(self.pre_tool_use(tool_name, tool_input, cwd))
                });
                match result {
                    Ok(r) => r,
                    Err(e) => {
                        tracing::error!("PreToolUse hook error: {e}");
                        protocol::PreToolUseResult::default()
                    }
                }
            }
            Err(_) => protocol::PreToolUseResult::default(),
        }
    }

    /// Synchronous version of post_tool_use
    pub fn post_tool_use_blocking(
        &self,
        tool_name: &str,
        tool_input: &serde_json::Value,
        tool_response: &mut serde_json::Value,
        cwd: &Path,
    ) -> protocol::PostToolUseResult {
        match tokio::runtime::Handle::try_current() {
            Ok(handle) => {
                let result = tokio::task::block_in_place(|| {
                    handle.block_on(self.post_tool_use(tool_name, tool_input, tool_response, cwd))
                });
                match result {
                    Ok(r) => r,
                    Err(e) => {
                        tracing::error!("PostToolUse hook error: {e}");
                        protocol::PostToolUseResult::default()
                    }
                }
            }
            Err(_) => protocol::PostToolUseResult::default(),
        }
    }

    async fn run_hook(
        &self,
        hook: &Hook,
        trigger: HookTrigger,
        context: &serde_json::Value,
    ) -> Result<HookResult> {
        let input = HookInput {
            trigger,
            session_id: self.session_id.clone(),
            context: context.clone(),
            timestamp: chrono::Utc::now().to_rfc3339(),
        };
        let input_json = serde_json::to_string(&input)?;
        let start = Instant::now();
        let mut cmd = Command::new(&hook.script);
        cmd.args(&hook.args);

        let output = {
            use std::process::Stdio;
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            let timeout_duration = std::time::Duration::from_secs(if hook.timeout_secs > 0 {
                hook.timeout_secs
            } else {
                30
            });
            let mut child = cmd
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()?;
            if let Some(mut stdin) = child.stdin.take() {
                stdin.write_all(input_json.as_bytes()).await?;
                drop(stdin);
            }
            let mut stdout_buf = Vec::new();
            let mut stderr_buf = Vec::new();
            let status = tokio::time::timeout(timeout_duration, async {
                if let Some(mut stdout) = child.stdout.take() {
                    let _ = stdout.read_to_end(&mut stdout_buf).await;
                }
                if let Some(mut stderr) = child.stderr.take() {
                    let _ = stderr.read_to_end(&mut stderr_buf).await;
                }
                child.wait().await
            })
            .await
            .map_err(|_| {
                let _ = child.start_kill();
                let _ = child.try_wait();
                anyhow::anyhow!(
                    "Hook '{}' timed out after {}s",
                    hook.name,
                    hook.timeout_secs
                )
            })??;
            std::process::Output {
                status,
                stdout: stdout_buf,
                stderr: stderr_buf,
            }
        };

        let duration_ms = start.elapsed().as_millis();
        let stdout = String::from_utf8_lossy(&output.stdout);
        let exit_code = output.status.code();

        let hook_output: HookOutput = if !stdout.trim().is_empty() {
            serde_json::from_str(stdout.trim()).unwrap_or(HookOutput {
                status: if output.status.success() {
                    HookStatus::Ok
                } else {
                    HookStatus::Error
                },
                message: Some(format!("Non-JSON output: {}", stdout.trim())),
                actions: None,
            })
        } else {
            HookOutput {
                status: if output.status.success() {
                    HookStatus::Ok
                } else {
                    HookStatus::Error
                },
                message: None,
                actions: None,
            }
        };

        Ok(HookResult {
            hook_name: hook.name.clone(),
            status: hook_output.status,
            exit_code,
            message: hook_output.message,
            actions: hook_output.actions.unwrap_or_default(),
            duration_ms,
        })
    }

    fn profile_allows(&self, hook: &Hook) -> bool {
        match (self.profile, hook.profile.unwrap_or(HookProfile::Standard)) {
            (HookProfile::Strict, _) => hook.profile == Some(HookProfile::Strict),
            (HookProfile::Standard, p) => p <= HookProfile::Standard,
            (HookProfile::Minimal, _) => true,
        }
    }
}
