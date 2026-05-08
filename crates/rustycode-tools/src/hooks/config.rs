//! Unified hook config parser
//!
//! Parses both Claude Code / Codex format and legacy RustyCode format
//! into a single internal representation.
//!
//! ## Config discovery (priority order, all merged)
//!
//! 1. `<project>/.rustycode/hooks.json` — project hooks
//! 2. `~/.config/rustycode/hooks.json` — user hooks
//! 3. `~/.config/rustycode/settings.json` `hooks` key — Claude Code compat

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::matcher::ToolMatcher;
use super::HookTrigger;

// ── Claude Code / Codex format types ──

/// Handler kind — currently only "command", extensible for future wasm/native.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum HandlerKind {
    Command,
}

/// A single hook handler within a matcher group.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HookHandler {
    #[serde(rename = "type", default = "default_handler_kind")]
    pub kind: HandlerKind,
    pub command: String,
    #[serde(default)]
    pub timeout: Option<u64>,
    #[serde(default)]
    pub status_message: Option<String>,
}

fn default_handler_kind() -> HandlerKind {
    HandlerKind::Command
}

/// A matcher group: hooks that fire only when the tool name matches.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MatcherGroup {
    #[serde(default)]
    pub matcher: Option<String>,
    pub hooks: Vec<HookHandler>,
}

impl MatcherGroup {
    /// Build a compiled matcher for this group.
    pub fn tool_matcher(&self) -> Result<ToolMatcher> {
        match &self.matcher {
            None => Ok(ToolMatcher::match_all()),
            Some(pattern) => ToolMatcher::new(pattern),
        }
    }
}

/// Claude Code / Codex format: event name → list of matcher groups.
///
/// ```json
/// {
///   "hooks": {
///     "PreToolUse": [
///       { "matcher": "Edit|Write", "hooks": [{"type":"command","command":"fmt.sh"}] }
///     ]
///   }
/// }
/// ```
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct UnifiedHooksConfig {
    #[serde(default)]
    pub hooks: HashMap<String, Vec<MatcherGroup>>,
}

// ── Legacy RustyCode format (for backward compat) ──

/// Legacy hooks.json format.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct LegacyHooksConfig {
    #[serde(default)]
    pub profile: super::HookProfile,
    #[serde(default)]
    pub hooks: Vec<super::Hook>,
}

// ── Unified config loader ──

/// A fully resolved, compiled hook entry ready for execution.
#[derive(Clone, Debug)]
pub struct CompiledHook {
    pub trigger: HookTrigger,
    pub matcher: ToolMatcher,
    pub command: String,
    pub timeout_secs: u64,
    pub status_message: Option<String>,
}

/// Loads and merges hook configs from multiple sources.
pub struct ConfigLoader;

impl ConfigLoader {
    /// Load hooks from all known config sources, merging them.
    ///
    /// Returns compiled hooks grouped by trigger event.
    pub async fn load_all(
        project_dir: &Path,
        user_config_dir: &Path,
    ) -> Result<HashMap<HookTrigger, Vec<CompiledHook>>> {
        let mut unified = UnifiedHooksConfig::default();

        // Source 1: user hooks.json
        let user_hooks = user_config_dir.join("hooks.json");
        if user_hooks.exists() {
            Self::merge_file(&mut unified, &user_hooks).await?;
        }

        // Source 2: user settings.json (Claude Code compat — extract "hooks" key)
        let user_settings = user_config_dir.join("settings.json");
        if user_settings.exists() {
            Self::merge_settings_hooks(&mut unified, &user_settings).await?;
        }

        // Source 3: project hooks.json
        let project_hooks = project_dir.join(".rustycode").join("hooks.json");
        if project_hooks.exists() {
            Self::merge_file(&mut unified, &project_hooks).await?;
        }

        Self::compile(unified)
    }

    /// Load from a single file (auto-detecting format).
    pub async fn load_file(path: &Path) -> Result<HashMap<HookTrigger, Vec<CompiledHook>>> {
        let mut unified = UnifiedHooksConfig::default();
        Self::merge_file(&mut unified, path).await?;
        Self::compile(unified)
    }

    /// Merge a hooks config file into the unified config.
    /// Detects whether it's Claude Code format or legacy format.
    async fn merge_file(unified: &mut UnifiedHooksConfig, path: &Path) -> Result<()> {
        let content = tokio::fs::read_to_string(path)
            .await
            .with_context(|| format!("failed to read hooks config from {}", path.display()))?;

        // Try Claude Code / Codex format first
        if let Ok(cc_config) = serde_json::from_str::<UnifiedHooksConfig>(&content) {
            Self::merge_unified(unified, cc_config);
            return Ok(());
        }

        // Try legacy RustyCode format
        if let Ok(legacy) = serde_json::from_str::<LegacyHooksConfig>(&content) {
            Self::merge_legacy(unified, legacy, path);
            return Ok(());
        }

        tracing::warn!(
            "Hooks config at {} is neither unified nor legacy format — skipping",
            path.display()
        );
        Ok(())
    }

    /// Extract the "hooks" key from a settings.json (Claude Code compat).
    async fn merge_settings_hooks(unified: &mut UnifiedHooksConfig, path: &Path) -> Result<()> {
        let content = tokio::fs::read_to_string(path)
            .await
            .with_context(|| format!("failed to read settings from {}", path.display()))?;

        let settings: serde_json::Value = serde_json::from_str(&content)
            .with_context(|| format!("invalid JSON in {}", path.display()))?;

        if let Some(hooks_section) = settings.get("hooks") {
            if let Ok(cc_config) = serde_json::from_value::<UnifiedHooksConfig>(serde_json::json!({
                "hooks": hooks_section
            })) {
                Self::merge_unified(unified, cc_config);
            }
        }

        Ok(())
    }

    /// Merge a unified config into the target.
    fn merge_unified(target: &mut UnifiedHooksConfig, source: UnifiedHooksConfig) {
        for (event, groups) in source.hooks {
            target.hooks.entry(event).or_default().extend(groups);
        }
    }

    /// Convert legacy hooks into unified matcher groups.
    fn merge_legacy(unified: &mut UnifiedHooksConfig, legacy: LegacyHooksConfig, _path: &Path) {
        for hook in legacy.hooks {
            let event_key = hook.trigger.to_string();
            // Legacy hooks have no matcher — they match all tools
            let group = MatcherGroup {
                matcher: None,
                hooks: vec![HookHandler {
                    kind: HandlerKind::Command,
                    command: hook.script.to_string_lossy().into(),
                    timeout: if hook.timeout_secs > 0 {
                        Some(hook.timeout_secs)
                    } else {
                        None
                    },
                    status_message: None,
                }],
            };
            unified.hooks.entry(event_key).or_default().push(group);
        }
    }

    /// Compile unified config into trigger-grouped compiled hooks.
    fn compile(unified: UnifiedHooksConfig) -> Result<HashMap<HookTrigger, Vec<CompiledHook>>> {
        let mut compiled: HashMap<HookTrigger, Vec<CompiledHook>> = HashMap::new();

        for (event_key, groups) in &unified.hooks {
            let trigger = match Self::parse_trigger(event_key) {
                Some(t) => t,
                None => {
                    tracing::debug!("Unknown hook event '{event_key}' — skipping");
                    continue;
                }
            };

            for group in groups {
                let matcher = group.tool_matcher().with_context(|| {
                    format!(
                        "invalid matcher pattern '{}' for event '{event_key}'",
                        group.matcher.as_deref().unwrap_or("*")
                    )
                })?;

                for handler in &group.hooks {
                    compiled.entry(trigger).or_default().push(CompiledHook {
                        trigger,
                        matcher: matcher.clone(),
                        command: handler.command.clone(),
                        timeout_secs: handler.timeout.unwrap_or(30),
                        status_message: handler.status_message.clone(),
                    });
                }
            }
        }

        Ok(compiled)
    }

    /// Parse event name string to HookTrigger.
    /// Supports both PascalCase (Claude Code / Codex) and snake_case (legacy).
    fn parse_trigger(s: &str) -> Option<HookTrigger> {
        match s {
            "PreToolUse" | "pre_tool_use" => Some(HookTrigger::PreToolUse),
            "PostToolUse" | "post_tool_use" => Some(HookTrigger::PostToolUse),
            "PermissionRequest" | "permission_request" => Some(HookTrigger::PermissionRequest),
            "UserPromptSubmit" | "user_prompt_submit" => Some(HookTrigger::UserPromptSubmit),
            "SessionStart" | "session_start" => Some(HookTrigger::SessionStart),
            "SessionEnd" | "session_end" => Some(HookTrigger::SessionEnd),
            "Stop" => Some(HookTrigger::SessionEnd),
            "PreCompact" | "pre_compact" => Some(HookTrigger::PreCompact),
            "PostCompact" | "post_compact" => Some(HookTrigger::PostCompact),
            "Error" | "error" => Some(HookTrigger::Error),
            _ => None,
        }
    }

    /// Default user config directory.
    pub fn default_user_config_dir() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("~/.config"))
            .join("rustycode")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_unified_format() {
        let json = r#"{
            "hooks": {
                "PreToolUse": [
                    {
                        "matcher": "Edit|Write|Bash",
                        "hooks": [
                            {"type": "command", "command": "lint.sh", "timeout": 30}
                        ]
                    }
                ],
                "PostToolUse": [
                    {
                        "matcher": "Edit|Write",
                        "hooks": [
                            {"type": "command", "command": "fmt.sh"}
                        ]
                    }
                ]
            }
        }"#;

        let config: UnifiedHooksConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.hooks.len(), 2);

        let pre = &config.hooks["PreToolUse"];
        assert_eq!(pre.len(), 1);
        assert_eq!(pre[0].matcher, Some("Edit|Write|Bash".to_string()));
        assert_eq!(pre[0].hooks.len(), 1);
        assert_eq!(pre[0].hooks[0].command, "lint.sh");
        assert_eq!(pre[0].hooks[0].timeout, Some(30));
    }

    #[test]
    fn parse_empty_config() {
        let json = "{}";
        let config: UnifiedHooksConfig = serde_json::from_str(json).unwrap();
        assert!(config.hooks.is_empty());
    }

    #[test]
    fn compile_unified_config() {
        let json = r#"{
            "hooks": {
                "PreToolUse": [
                    {
                        "matcher": "Bash",
                        "hooks": [
                            {"command": "check.sh", "timeout": 10}
                        ]
                    }
                ]
            }
        }"#;

        let config: UnifiedHooksConfig = serde_json::from_str(json).unwrap();
        let compiled = ConfigLoader::compile(config).unwrap();

        let pre_hooks = compiled.get(&HookTrigger::PreToolUse).unwrap();
        assert_eq!(pre_hooks.len(), 1);
        assert!(pre_hooks[0].matcher.matches("Bash"));
        assert!(!pre_hooks[0].matcher.matches("Edit"));
        assert_eq!(pre_hooks[0].timeout_secs, 10);
    }

    #[test]
    fn compile_no_matcher_matches_all() {
        let json = r#"{
            "hooks": {
                "PostToolUse": [
                    {
                        "hooks": [
                            {"command": "log.sh"}
                        ]
                    }
                ]
            }
        }"#;

        let config: UnifiedHooksConfig = serde_json::from_str(json).unwrap();
        let compiled = ConfigLoader::compile(config).unwrap();

        let post_hooks = compiled.get(&HookTrigger::PostToolUse).unwrap();
        assert!(post_hooks[0].matcher.matches("Anything"));
    }

    #[test]
    fn parse_trigger_pascal_and_snake() {
        assert_eq!(
            ConfigLoader::parse_trigger("PreToolUse"),
            Some(HookTrigger::PreToolUse)
        );
        assert_eq!(
            ConfigLoader::parse_trigger("pre_tool_use"),
            Some(HookTrigger::PreToolUse)
        );
        assert_eq!(
            ConfigLoader::parse_trigger("PostToolUse"),
            Some(HookTrigger::PostToolUse)
        );
        assert_eq!(
            ConfigLoader::parse_trigger("Stop"),
            Some(HookTrigger::SessionEnd)
        );
        assert_eq!(ConfigLoader::parse_trigger("Unknown"), None);
    }

    #[test]
    fn legacy_format_converts_to_unified() {
        let json = r#"{
            "profile": "standard",
            "hooks": [
                {
                    "name": "lint",
                    "trigger": "post_tool_use",
                    "script": "./lint.sh",
                    "timeout_secs": 15
                }
            ]
        }"#;

        let legacy: LegacyHooksConfig = serde_json::from_str(json).unwrap();
        let mut unified = UnifiedHooksConfig::default();
        ConfigLoader::merge_legacy(&mut unified, legacy, Path::new("hooks.json"));

        let groups = unified.hooks.get("post_tool_use").unwrap();
        assert_eq!(groups.len(), 1);
        assert!(groups[0].matcher.is_none()); // Legacy hooks match all
        assert_eq!(groups[0].hooks[0].command, "./lint.sh");
        assert_eq!(groups[0].hooks[0].timeout, Some(15));
    }

    #[test]
    fn handler_kind_default_is_command() {
        let json = r#"{"command": "test.sh"}"#;
        let handler: HookHandler = serde_json::from_str(json).unwrap();
        assert_eq!(handler.kind, HandlerKind::Command);
    }

    #[test]
    fn extract_hooks_from_settings_json() {
        let settings = r#"{
            "model": "sonnet",
            "hooks": {
                "PreToolUse": [
                    {
                        "matcher": "Edit|Write",
                        "hooks": [
                            {"command": "fmt.sh"}
                        ]
                    }
                ]
            }
        }"#;

        let value: serde_json::Value = serde_json::from_str(settings).unwrap();
        let hooks_section = value.get("hooks").unwrap();
        let cc_config: UnifiedHooksConfig = serde_json::from_value(serde_json::json!({
            "hooks": hooks_section
        }))
        .unwrap();

        assert_eq!(cc_config.hooks.len(), 1);
        let pre = cc_config.hooks.get("PreToolUse").unwrap();
        assert_eq!(pre[0].matcher, Some("Edit|Write".to_string()));
    }
}
