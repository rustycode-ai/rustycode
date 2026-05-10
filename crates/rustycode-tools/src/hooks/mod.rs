//! Hook system for lifecycle event extensibility
//!
//! This module provides:
//! - Configurable hooks that execute at lifecycle events (`PreToolUse`, `PostToolUse`, etc.)
//! - JSON stdin/stdout protocol for hook scripts
//! - Blocking semantics (hooks can prevent tool execution)
//! - Security profiles (Minimal, Standard, Strict)
//! - Claude Code / Codex compatible config format with matcher filtering
//! - Rich per-event protocol with mutable PostToolUse output
//!
//! # Hook Execution Flow
//!
//! ```text
//! Tool requested → PreToolUse hooks → [blocked?] → Execute tool → PostToolUse hooks
//! ```

pub mod config;
pub mod env;
pub mod matcher;
pub mod protocol;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Instant;
use tokio::process::Command;

/// Hook lifecycle triggers
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum HookTrigger {
    SessionStart,
    SessionEnd,
    PreToolUse,
    PostToolUse,
    PreCompact,
    PostCompact,
    /// Fires before processing user input, allowing hooks to inspect or block prompts.
    UserPromptSubmit,
    /// Fires before showing approval prompts, allowing hooks to auto-approve.
    PermissionRequest,
    Error,
}

impl std::fmt::Display for HookTrigger {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SessionStart => write!(f, "session_start"),
            Self::SessionEnd => write!(f, "session_end"),
            Self::PreToolUse => write!(f, "pre_tool_use"),
            Self::PostToolUse => write!(f, "post_tool_use"),
            Self::PreCompact => write!(f, "pre_compact"),
            Self::PostCompact => write!(f, "post_compact"),
            Self::UserPromptSubmit => write!(f, "user_prompt_submit"),
            Self::PermissionRequest => write!(f, "permission_request"),
            Self::Error => write!(f, "error"),
        }
    }
}

/// Hook execution profiles (security level)
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Default)]
#[serde(rename_all = "snake_case")]
pub enum HookProfile {
    Minimal,
    #[default]
    Standard,
    Strict,
}

/// Hook definition from config
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Hook {
    pub name: String,
    pub trigger: HookTrigger,
    pub script: PathBuf,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub timeout_secs: u64,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    pub profile: Option<HookProfile>,
    /// If true, hook failure blocks execution
    #[serde(default)]
    pub fail_on_error: bool,
}

const fn default_enabled() -> bool {
    true
}

/// Context passed to hook via stdin as JSON
#[derive(Serialize)]
pub struct HookInput {
    pub trigger: HookTrigger,
    pub session_id: String,
    pub context: serde_json::Value,
    pub timestamp: String,
}

/// Hook script stdout output
#[derive(Clone, Debug, Deserialize)]
pub struct HookOutput {
    pub status: HookStatus,
    #[serde(default)]
    pub message: Option<String>,
    pub actions: Option<Vec<HookAction>>,
}

/// Hook execution status
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum HookStatus {
    Ok,
    Warning,
    Error,
    Blocked,
}

/// Actions a hook can request
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum HookAction {
    Block,
    Log,
    Alert,
    Abort,
}

/// Result of running a single hook
#[derive(Clone, Debug)]
pub struct HookResult {
    pub hook_name: String,
    pub status: HookStatus,
    pub exit_code: Option<i32>,
    pub message: Option<String>,
    pub actions: Vec<HookAction>,
    pub duration_ms: u128,
}

/// Result of executing all hooks for a trigger, with blocking info
#[derive(Clone, Debug, Default)]
pub struct HookExecutionResult {
    pub results: Vec<HookResult>,
    pub should_block: bool,
    pub block_reason: Option<String>,
    pub blocking_hook: Option<String>,
}

/// Configuration file format
#[derive(Debug, Deserialize, Default)]
pub struct HooksConfig {
    #[serde(default)]
    pub profile: HookProfile,
    #[serde(default)]
    pub hooks: Vec<Hook>,
}

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
    /// Returns Ok with result — check `blocked` to see if execution should proceed.
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
    /// Hooks can return `decision: "block"` to replace the tool response.
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
                        // Replace tool response with hook's reason
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
    /// Returns Some(true) to auto-approve, Some(false) to auto-deny, None for no opinion.
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

    /// Run a compiled hook (from unified config), sending protocol input via stdin.
    async fn run_compiled_hook(
        &self,
        hook: &config::CompiledHook,
        input: &protocol::HookProtocolInput,
        env_vars: &std::collections::HashMap<String, String>,
    ) -> Result<protocol::HookProtocolOutput> {
        let input_json = serde_json::to_string(input)?;
        let start = Instant::now();

        let timeout = std::time::Duration::from_secs(hook.timeout_secs.max(1));

        let output = tokio::time::timeout(timeout, async {
            use std::process::Stdio;
            use tokio::io::AsyncWriteExt;

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

            child.wait_with_output().await
        })
        .await
        .map_err(|_| {
            anyhow::anyhow!(
                "Hook '{}' timed out after {}s",
                hook.command,
                hook.timeout_secs
            )
        })?
        .map_err(|e| anyhow::anyhow!("Hook '{}' failed: {e}", hook.command))?;

        let duration_ms = start.elapsed().as_millis();
        let stdout = String::from_utf8_lossy(&output.stdout);
        let exit_code = output.status.code();

        // Exit code 2 means block (Codex convention)
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

    /// Execute hooks for a trigger event, respecting blocking semantics
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

    /// Enable or disable a hook by name
    pub fn set_enabled(&mut self, name: &str, enabled: bool) -> Result<()> {
        let hook = self
            .hooks
            .iter_mut()
            .find(|h| h.name == name)
            .ok_or_else(|| anyhow::anyhow!("Hook not found: {name}"))?;
        hook.enabled = enabled;
        Ok(())
    }

    /// List all registered hooks
    pub fn list_hooks(&self) -> &[Hook] {
        &self.hooks
    }

    /// Get current profile
    pub const fn profile(&self) -> HookProfile {
        self.profile
    }

    /// Run a single hook script
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

        let output = tokio::time::timeout(
            std::time::Duration::from_secs(if hook.timeout_secs > 0 {
                hook.timeout_secs
            } else {
                30
            }),
            async {
                use std::process::Stdio;
                use tokio::io::AsyncWriteExt;

                let mut child = cmd
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn()?;

                if let Some(mut stdin) = child.stdin.take() {
                    stdin.write_all(input_json.as_bytes()).await?;
                    drop(stdin);
                }

                child.wait_with_output().await
            },
        )
        .await
        .map_err(|_| {
            anyhow::anyhow!(
                "Hook '{}' timed out after {}s",
                hook.name,
                hook.timeout_secs
            )
        })?
        .map_err(|e| anyhow::anyhow!("Hook '{}' failed to execute: {}", hook.name, e))?;

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

    /// Check if the current profile allows running this hook
    fn profile_allows(&self, hook: &Hook) -> bool {
        let hook_profile = hook.profile.unwrap_or(HookProfile::Standard);
        hook_profile <= self.profile
    }

    /// Synchronous wrapper for `execute` — uses tokio runtime if available.
    /// Falls back to returning no-block if no runtime is active (e.g. tests).
    ///
    /// Uses `block_in_place` to avoid panicking when called from within a
    /// multi-threaded tokio runtime (e.g. the streaming tool-execution pipeline).
    pub fn execute_blocking(
        &self,
        trigger: HookTrigger,
        context: serde_json::Value,
    ) -> HookExecutionResult {
        match tokio::runtime::Handle::try_current() {
            Ok(handle) => {
                let result =
                    tokio::task::block_in_place(|| handle.block_on(self.execute(trigger, context)));
                match result {
                    Ok(result) => result,
                    Err(e) => {
                        tracing::error!("Hook execution error: {e}");
                        HookExecutionResult::default()
                    }
                }
            }
            Err(_) => {
                // No tokio runtime — skip hooks gracefully
                tracing::debug!("No tokio runtime, skipping {trigger} hooks");
                HookExecutionResult::default()
            }
        }
    }

    /// Synchronous wrapper for `pre_tool_use`.
    ///
    /// Uses `block_in_place` to avoid panicking when called from within a
    /// multi-threaded tokio runtime (e.g. the streaming tool-execution pipeline).
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
                    Ok(result) => result,
                    Err(e) => {
                        tracing::error!("PreToolUse hook error: {e}");
                        protocol::PreToolUseResult::default()
                    }
                }
            }
            Err(_) => protocol::PreToolUseResult::default(),
        }
    }

    /// Synchronous wrapper for `post_tool_use`.
    ///
    /// Uses `block_in_place` to avoid panicking when called from within a
    /// multi-threaded tokio runtime (e.g. the streaming tool-execution pipeline).
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
                    Ok(result) => result,
                    Err(e) => {
                        tracing::error!("PostToolUse hook error: {e}");
                        protocol::PostToolUseResult::default()
                    }
                }
            }
            Err(_) => protocol::PostToolUseResult::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_dir() -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("rustycode-hooks-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn hook_trigger_display() {
        assert_eq!(HookTrigger::PreToolUse.to_string(), "pre_tool_use");
        assert_eq!(HookTrigger::SessionStart.to_string(), "session_start");
        assert_eq!(HookTrigger::Error.to_string(), "error");
        assert_eq!(
            HookTrigger::UserPromptSubmit.to_string(),
            "user_prompt_submit"
        );
        assert_eq!(
            HookTrigger::PermissionRequest.to_string(),
            "permission_request"
        );
    }

    #[test]
    fn hook_trigger_serde_roundtrip() {
        let json = serde_json::to_string(&HookTrigger::PreToolUse).unwrap();
        assert_eq!(json, "\"pre_tool_use\"");
        let parsed: HookTrigger = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, HookTrigger::PreToolUse);
    }

    #[test]
    fn hook_trigger_user_prompt_submit_serde() {
        let json = serde_json::to_string(&HookTrigger::UserPromptSubmit).unwrap();
        assert_eq!(json, "\"user_prompt_submit\"");
        let parsed: HookTrigger = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, HookTrigger::UserPromptSubmit);
    }

    #[test]
    fn hook_trigger_permission_request_serde() {
        let json = serde_json::to_string(&HookTrigger::PermissionRequest).unwrap();
        assert_eq!(json, "\"permission_request\"");
        let parsed: HookTrigger = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, HookTrigger::PermissionRequest);
    }

    #[test]
    fn hook_trigger_all_variants_deserialize() {
        let variants = [
            ("session_start", HookTrigger::SessionStart),
            ("session_end", HookTrigger::SessionEnd),
            ("pre_tool_use", HookTrigger::PreToolUse),
            ("post_tool_use", HookTrigger::PostToolUse),
            ("pre_compact", HookTrigger::PreCompact),
            ("post_compact", HookTrigger::PostCompact),
            ("user_prompt_submit", HookTrigger::UserPromptSubmit),
            ("permission_request", HookTrigger::PermissionRequest),
            ("error", HookTrigger::Error),
        ];
        for (json_str, expected) in variants {
            let parsed: HookTrigger = serde_json::from_str(&format!("\"{json_str}\"")).unwrap();
            assert_eq!(parsed, expected, "failed for {json_str}");
        }
    }

    #[test]
    fn hook_profile_ordering() {
        assert!(HookProfile::Minimal <= HookProfile::Standard);
        assert!(HookProfile::Standard <= HookProfile::Strict);
        assert!(HookProfile::Minimal <= HookProfile::Strict);
        assert!(HookProfile::Strict > HookProfile::Minimal);
    }

    #[test]
    fn hook_profile_default() {
        assert_eq!(HookProfile::default(), HookProfile::Standard);
    }

    #[test]
    fn hook_config_deserialization() {
        let json = r#"{
            "profile": "strict",
            "hooks": [
                {
                    "name": "lint-check",
                    "trigger": "post_tool_use",
                    "script": "./hooks/lint.sh",
                    "args": ["--strict"],
                    "timeout_secs": 30,
                    "enabled": true,
                    "fail_on_error": true
                }
            ]
        }"#;

        let config: HooksConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.profile, HookProfile::Strict);
        assert_eq!(config.hooks.len(), 1);
        assert_eq!(config.hooks[0].name, "lint-check");
        assert_eq!(config.hooks[0].trigger, HookTrigger::PostToolUse);
        assert!(config.hooks[0].fail_on_error);
        assert_eq!(config.hooks[0].args, vec!["--strict"]);
    }

    #[test]
    fn hook_config_empty_deserialization() {
        let json = "{}";
        let config: HooksConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.profile, HookProfile::Standard);
        assert!(config.hooks.is_empty());
    }

    #[test]
    fn hook_execution_result_default_no_block() {
        let result = HookExecutionResult::default();
        assert!(!result.should_block);
        assert!(result.block_reason.is_none());
        assert!(result.blocking_hook.is_none());
        assert!(result.results.is_empty());
    }

    #[test]
    fn hook_profile_allows_standard() {
        let dir = temp_dir();
        let mgr = HookManager::new(dir, HookProfile::Standard, "test".to_string());

        let hook_minimal = Hook {
            name: "h1".into(),
            trigger: HookTrigger::PreToolUse,
            script: PathBuf::from("true"),
            args: vec![],
            timeout_secs: 5,
            enabled: true,
            profile: Some(HookProfile::Minimal),
            fail_on_error: false,
        };
        let hook_standard = Hook {
            name: "h2".into(),
            trigger: HookTrigger::PreToolUse,
            script: PathBuf::from("true"),
            args: vec![],
            timeout_secs: 5,
            enabled: true,
            profile: Some(HookProfile::Standard),
            fail_on_error: false,
        };
        let hook_strict = Hook {
            name: "h3".into(),
            trigger: HookTrigger::PreToolUse,
            script: PathBuf::from("true"),
            args: vec![],
            timeout_secs: 5,
            enabled: true,
            profile: Some(HookProfile::Strict),
            fail_on_error: false,
        };

        assert!(mgr.profile_allows(&hook_minimal));
        assert!(mgr.profile_allows(&hook_standard));
        assert!(!mgr.profile_allows(&hook_strict));
    }

    #[test]
    fn hook_set_enabled() {
        let dir = temp_dir();
        let mut mgr = HookManager::new(dir, HookProfile::Standard, "test".to_string());
        mgr.hooks.push(Hook {
            name: "test-hook".into(),
            trigger: HookTrigger::PreToolUse,
            script: PathBuf::from("true"),
            args: vec![],
            timeout_secs: 5,
            enabled: true,
            profile: None,
            fail_on_error: false,
        });

        assert!(mgr.list_hooks()[0].enabled);
        mgr.set_enabled("test-hook", false).unwrap();
        assert!(!mgr.list_hooks()[0].enabled);
        mgr.set_enabled("test-hook", true).unwrap();
        assert!(mgr.list_hooks()[0].enabled);
    }

    #[test]
    fn hook_set_enabled_not_found() {
        let dir = temp_dir();
        let mut mgr = HookManager::new(dir, HookProfile::Standard, "test".to_string());
        assert!(mgr.set_enabled("nonexistent", true).is_err());
    }

    #[tokio::test]
    async fn load_hooks_no_config_file() {
        let dir = temp_dir();
        let mut mgr = HookManager::new(dir, HookProfile::Standard, "test".to_string());
        assert!(mgr.load_hooks().await.is_ok());
        assert!(mgr.list_hooks().is_empty());
    }

    #[tokio::test]
    async fn load_hooks_from_config() {
        let dir = temp_dir();
        let config = r#"{
            "profile": "standard",
            "hooks": [
                {
                    "name": "lint",
                    "trigger": "post_tool_use",
                    "script": "true",
                    "enabled": true
                }
            ]
        }"#;
        fs::write(dir.join("hooks.json"), config).unwrap();

        let mut mgr = HookManager::new(dir, HookProfile::Standard, "test".to_string());
        mgr.load_hooks().await.unwrap();
        assert_eq!(mgr.list_hooks().len(), 1);
        assert_eq!(mgr.list_hooks()[0].name, "lint");
    }

    #[tokio::test]
    async fn execute_no_matching_hooks() {
        let dir = temp_dir();
        let mgr = HookManager::new(dir, HookProfile::Standard, "test".to_string());
        let result = mgr
            .execute(HookTrigger::PreToolUse, serde_json::json!({"tool": "read"}))
            .await
            .unwrap();
        assert!(!result.should_block);
        assert!(result.results.is_empty());
    }

    #[tokio::test]
    async fn execute_hook_with_blocking_action() {
        let dir = temp_dir();

        // Create a script that outputs blocked status
        let script_path = dir.join("blocker.sh");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::write(
                &script_path,
                r#"#!/bin/sh
cat > /dev/null
echo '{"status":"blocked","message":"Not allowed","actions":["block"]}'
"#,
            )
            .unwrap();
            fs::set_permissions(&script_path, fs::Permissions::from_mode(0o755)).unwrap();
        }

        let mut mgr = HookManager::new(dir, HookProfile::Standard, "test".to_string());
        mgr.hooks.push(Hook {
            name: "blocker".into(),
            trigger: HookTrigger::PreToolUse,
            script: script_path,
            args: vec![],
            timeout_secs: 5,
            enabled: true,
            profile: None,
            fail_on_error: false,
        });

        let result = mgr
            .execute(
                HookTrigger::PreToolUse,
                serde_json::json!({"tool": "write"}),
            )
            .await
            .unwrap();

        assert!(result.should_block);
        assert_eq!(result.blocking_hook, Some("blocker".to_string()));
        assert!(result.block_reason.is_some());
    }

    #[tokio::test]
    async fn execute_hook_ok_status() {
        let dir = temp_dir();

        let script_path = dir.join("ok_hook.sh");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::write(
                &script_path,
                r#"#!/bin/sh
cat > /dev/null
echo '{"status":"ok"}'
"#,
            )
            .unwrap();
            fs::set_permissions(&script_path, fs::Permissions::from_mode(0o755)).unwrap();
        }

        let mut mgr = HookManager::new(dir, HookProfile::Standard, "test".to_string());
        mgr.hooks.push(Hook {
            name: "ok-hook".into(),
            trigger: HookTrigger::PostToolUse,
            script: script_path,
            args: vec![],
            timeout_secs: 5,
            enabled: true,
            profile: None,
            fail_on_error: false,
        });

        let result = mgr
            .execute(
                HookTrigger::PostToolUse,
                serde_json::json!({"tool": "read"}),
            )
            .await
            .unwrap();

        assert!(!result.should_block);
        assert_eq!(result.results.len(), 1);
        assert_eq!(result.results[0].status, HookStatus::Ok);
    }

    #[tokio::test]
    async fn execute_respects_disabled_hook() {
        let dir = temp_dir();

        let mut mgr = HookManager::new(dir, HookProfile::Standard, "test".to_string());
        mgr.hooks.push(Hook {
            name: "disabled".into(),
            trigger: HookTrigger::PreToolUse,
            script: PathBuf::from("/nonexistent/script"),
            args: vec![],
            timeout_secs: 5,
            enabled: false,
            profile: None,
            fail_on_error: false,
        });

        let result = mgr
            .execute(HookTrigger::PreToolUse, serde_json::json!({}))
            .await
            .unwrap();
        assert!(result.results.is_empty());
    }

    #[tokio::test]
    async fn execute_respects_profile_filtering() {
        let dir = temp_dir();

        let mut mgr = HookManager::new(dir, HookProfile::Minimal, "test".to_string());
        mgr.hooks.push(Hook {
            name: "strict-only".into(),
            trigger: HookTrigger::PreToolUse,
            script: PathBuf::from("/nonexistent/script"),
            args: vec![],
            timeout_secs: 5,
            enabled: true,
            profile: Some(HookProfile::Strict),
            fail_on_error: false,
        });

        let result = mgr
            .execute(HookTrigger::PreToolUse, serde_json::json!({}))
            .await
            .unwrap();
        assert!(result.results.is_empty());
    }

    #[test]
    fn hook_status_serde() {
        let json = serde_json::to_string(&HookStatus::Ok).unwrap();
        assert_eq!(json, "\"ok\"");
        let json = serde_json::to_string(&HookStatus::Blocked).unwrap();
        assert_eq!(json, "\"blocked\"");
    }

    #[test]
    fn hook_action_serde() {
        let json = serde_json::to_string(&HookAction::Block).unwrap();
        assert_eq!(json, "\"block\"");
        let json = serde_json::to_string(&HookAction::Log).unwrap();
        assert_eq!(json, "\"log\"");
    }

    #[test]
    fn hook_output_deserialize_ok() {
        let json = r#"{"status":"ok"}"#;
        let output: HookOutput = serde_json::from_str(json).unwrap();
        assert_eq!(output.status, HookStatus::Ok);
        assert!(output.message.is_none());
        assert!(output.actions.is_none());
    }

    #[test]
    fn hook_output_deserialize_with_actions() {
        let json = r#"{"status":"blocked","message":"Nope","actions":["block","log"]}"#;
        let output: HookOutput = serde_json::from_str(json).unwrap();
        assert_eq!(output.status, HookStatus::Blocked);
        assert_eq!(output.message, Some("Nope".to_string()));
        assert_eq!(
            output.actions,
            Some(vec![HookAction::Block, HookAction::Log])
        );
    }
}
