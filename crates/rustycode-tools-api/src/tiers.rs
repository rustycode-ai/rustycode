use crate::tool_selection::UsageTracker;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum ToolTier {
    #[default]
    Default,
    Extended,
    Full,
}

#[must_use]
pub fn default_tool_set() -> HashSet<&'static str> {
    [
        "read_file",
        "edit_file",
        "write_file",
        "bash",
        "grep",
        "glob",
    ]
    .into_iter()
    .collect()
}

#[must_use]
pub fn extended_tool_set() -> HashSet<&'static str> {
    [
        "web_fetch",
        "notebook_edit",
        "lsp_diagnostics",
        "lsp_hover",
        "lsp_definition",
        "lsp_references",
        "lsp_completion",
        "lsp_implementation",
        "lsp_incoming_calls",
        "lsp_outgoing_calls",
        "lsp_document_symbols",
        "todo_write",
        "todo_update",
        "todo_read",
        "memory_search",
        "memory_list",
        "list_dir",
        "git_status",
        "git_diff",
        "git_log",
    ]
    .into_iter()
    .collect()
}

#[derive(Debug, Clone)]
pub struct ToolActivationManager {
    tier: ToolTier,
    scoped_tools: Option<HashSet<String>>,
    usage: UsageTracker,
}

impl Default for ToolActivationManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolActivationManager {
    pub fn new() -> Self {
        Self {
            tier: ToolTier::Extended,
            scoped_tools: None,
            usage: UsageTracker::new(),
        }
    }

    pub const fn current_tier(&self) -> ToolTier {
        self.tier
    }

    pub fn promote(&mut self, tier: ToolTier) {
        if tier > self.tier {
            self.tier = tier;
        }
    }

    pub fn with_scope(mut self, tools: Vec<String>) -> Self {
        self.scoped_tools = Some(tools.into_iter().collect());
        self
    }

    pub fn set_scope(&mut self, tools: Vec<String>) {
        self.scoped_tools = Some(tools.into_iter().collect());
    }

    pub fn clear_scope(&mut self) {
        self.scoped_tools = None;
    }

    pub const fn usage(&self) -> &UsageTracker {
        &self.usage
    }

    pub const fn usage_mut(&mut self) -> &mut UsageTracker {
        &mut self.usage
    }

    pub fn record_use(&mut self, tool: &str, success: bool) {
        self.usage.record(tool, success);
    }

    pub fn is_tool_allowed(&self, tool_name: &str) -> bool {
        let tier_allows = match self.tier {
            ToolTier::Default => default_tool_set().contains(tool_name),
            ToolTier::Extended => {
                default_tool_set().contains(tool_name) || extended_tool_set().contains(tool_name)
            }
            ToolTier::Full => true,
        };

        if !tier_allows {
            return false;
        }

        self.scoped_tools
            .as_ref()
            .is_none_or(|scope| scope.contains(tool_name))
    }

    pub fn allowed_tools(&self) -> Vec<String> {
        let base: Vec<String> = match self.tier {
            ToolTier::Default => default_tool_set().into_iter().map(String::from).collect(),
            ToolTier::Extended => default_tool_set()
                .into_iter()
                .chain(extended_tool_set())
                .map(String::from)
                .collect(),
            ToolTier::Full => self
                .scoped_tools
                .as_ref()
                .map_or_else(Vec::new, |scope| scope.iter().cloned().collect()),
        };

        if self.tier == ToolTier::Full {
            return base;
        }

        match &self.scoped_tools {
            Some(scope) => base
                .into_iter()
                .filter(|tool| scope.contains(tool))
                .collect(),
            None => base,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::float_cmp)]
mod tests {
    use super::*;

    #[test]
    fn tool_tier_default_is_default() {
        assert_eq!(ToolTier::default(), ToolTier::Default);
    }

    #[test]
    fn tool_tier_ordering() {
        assert!(ToolTier::Default < ToolTier::Extended);
        assert!(ToolTier::Extended < ToolTier::Full);
    }

    #[test]
    fn default_tools_contains_core_six() {
        let defaults = default_tool_set();
        assert!(defaults.contains("read_file"));
        assert!(defaults.contains("edit_file"));
        assert!(defaults.contains("write_file"));
        assert!(defaults.contains("bash"));
        assert!(defaults.contains("grep"));
        assert!(defaults.contains("glob"));
        assert_eq!(defaults.len(), 6);
    }

    #[test]
    fn extended_tools_contains_expected() {
        let extended = extended_tool_set();
        assert!(extended.contains("web_fetch"));
        assert!(extended.contains("notebook_edit"));
        assert!(extended.contains("lsp_diagnostics"));
        assert!(extended.contains("lsp_hover"));
        assert!(extended.contains("lsp_definition"));
        assert!(extended.contains("lsp_references"));
        assert!(extended.contains("lsp_completion"));
        assert!(extended.contains("todo_write"));
        assert!(extended.contains("memory_search"));
        assert!(extended.contains("memory_list"));
    }

    #[test]
    fn usage_tracker_records_invocation() {
        let mut tracker = UsageTracker::new();
        tracker.record("read_file", true);
        tracker.record("read_file", true);
        tracker.record("bash", false);

        assert_eq!(tracker.invocation_count("read_file"), 2);
        assert_eq!(tracker.invocation_count("bash"), 1);
        assert_eq!(tracker.invocation_count("write_file"), 0);
    }

    #[test]
    fn usage_tracker_tracks_success_rate() {
        let mut tracker = UsageTracker::new();
        tracker.record("bash", true);
        tracker.record("bash", true);
        tracker.record("bash", false);

        let rate = tracker.success_rate("bash");
        assert!((rate - 0.667).abs() < 0.05);
    }

    #[test]
    fn usage_tracker_success_rate_unknown_tool_is_zero() {
        let tracker = UsageTracker::new();
        assert_eq!(tracker.success_rate("nonexistent"), 0.0);
    }

    #[test]
    fn activation_manager_starts_extended() {
        let manager = ToolActivationManager::new();
        assert_eq!(manager.current_tier(), ToolTier::Extended);
        assert!(manager.is_tool_allowed("read_file"));
        assert!(manager.is_tool_allowed("lsp_hover"));
        assert!(manager.is_tool_allowed("web_fetch"));
    }

    #[test]
    fn activation_manager_promotes_monotonically() {
        let mut manager = ToolActivationManager::new();
        manager.promote(ToolTier::Extended);
        assert_eq!(manager.current_tier(), ToolTier::Extended);
        manager.promote(ToolTier::Default);
        assert_eq!(manager.current_tier(), ToolTier::Extended);
        manager.promote(ToolTier::Full);
        assert_eq!(manager.current_tier(), ToolTier::Full);
    }

    #[test]
    fn activation_manager_scope_filters_allowed_tools() {
        let manager = ToolActivationManager::new()
            .with_scope(vec!["read_file".to_string(), "bash".to_string()]);
        assert!(manager.is_tool_allowed("read_file"));
        assert!(manager.is_tool_allowed("bash"));
        assert!(!manager.is_tool_allowed("write_file"));
    }

    #[test]
    fn activation_manager_allowed_tools_respects_tier() {
        // Start at Default tier to test tier promotion behavior
        let mut manager = ToolActivationManager::new();
        manager.tier = ToolTier::Default;
        let defaults = manager.allowed_tools();
        assert!(defaults.contains(&"read_file".to_string()));
        assert!(!defaults.contains(&"web_fetch".to_string()));

        manager.promote(ToolTier::Extended);
        let extended = manager.allowed_tools();
        assert!(extended.contains(&"web_fetch".to_string()));
    }
}

// ── Tool filtering ──────────────────────────────────────────────────────────

/// Platform environment detected at startup, refreshable at runtime.
#[derive(Debug, Clone)]
#[allow(clippy::struct_excessive_bools)]
pub struct PlatformEnv {
    pub is_unix: bool,
    pub has_pwsh: bool,
    pub has_cmd: bool,
    /// tmux available — needed for session/team orchestration tools.
    pub has_tmux: bool,
    /// iTerm2 terminal — supports inline images, proprietary escape sequences.
    pub has_iterm2: bool,
}

impl Default for PlatformEnv {
    fn default() -> Self {
        Self {
            is_unix: cfg!(unix),
            has_pwsh: false,
            has_cmd: false,
            has_tmux: false,
            has_iterm2: false,
        }
    }
}

impl PlatformEnv {
    /// Probe the current platform for available shells, multiplexers, and terminals.
    pub fn probe() -> Self {
        Self {
            is_unix: cfg!(unix),
            has_pwsh: which::which("pwsh").is_ok(),
            has_cmd: cfg!(windows),
            has_tmux: which::which("tmux").is_ok(),
            has_iterm2: std::env::var("TERM_PROGRAM").is_ok_and(|v| v == "iTerm.app"),
        }
    }

    /// Re-probe all fields from the current environment.
    pub fn reprobe(&mut self) {
        *self = Self::probe();
    }
}

/// LLM provider/model capabilities relevant to tool selection.
#[derive(Debug, Clone)]
#[allow(clippy::struct_excessive_bools)]
pub struct ProviderCaps {
    pub supports_tools: bool,
    pub supports_parallel_tools: bool,
    pub supports_structured_output: bool,
    pub max_output_tokens: Option<usize>,
    /// True for local providers (Ollama, LiteRT) — limited schema handling.
    pub is_local: bool,
}

impl Default for ProviderCaps {
    fn default() -> Self {
        Self::full()
    }
}

impl ProviderCaps {
    /// Full cloud capabilities (Anthropic, OpenAI, etc.).
    pub const fn full() -> Self {
        Self {
            supports_tools: true,
            supports_parallel_tools: true,
            supports_structured_output: true,
            max_output_tokens: None,
            is_local: false,
        }
    }

    /// Minimal capabilities — local model with tool support but limited schemas.
    pub const fn local() -> Self {
        Self {
            supports_tools: true,
            supports_parallel_tools: false,
            supports_structured_output: false,
            max_output_tokens: Some(4096),
            is_local: true,
        }
    }

    /// No tool support at all.
    pub const fn no_tools() -> Self {
        Self {
            supports_tools: false,
            supports_parallel_tools: false,
            supports_structured_output: false,
            max_output_tokens: None,
            is_local: false,
        }
    }
}

/// Runtime environment detected by probing the workspace, refreshable at runtime.
#[derive(Debug, Clone)]
#[allow(clippy::struct_excessive_bools)]
pub struct RuntimeEnv {
    pub has_git_repo: bool,
    pub has_docker: bool,
    pub has_lsp_running: bool,
    /// Python interpreter available — needed for notebook/script tools.
    pub has_python: bool,
    /// Node.js available — needed for JS execution and npm/yarn tools.
    pub has_node: bool,
    pub cwd: PathBuf,
}

impl RuntimeEnv {
    /// Probe the runtime environment from the given working directory.
    pub fn probe(cwd: PathBuf) -> Self {
        Self {
            has_git_repo: cwd.join(".git").exists(),
            has_docker: which::which("docker").is_ok(),
            has_lsp_running: false, // caller updates this from LSP manager state
            has_python: which::which("python3").is_ok() || which::which("python").is_ok(),
            has_node: which::which("node").is_ok(),
            cwd,
        }
    }

    /// Assume everything is available.
    pub fn full(cwd: PathBuf) -> Self {
        Self {
            has_git_repo: true,
            has_docker: true,
            has_lsp_running: true,
            has_python: true,
            has_node: true,
            cwd,
        }
    }

    /// Re-probe mutable fields (docker, python, node) without changing cwd or lsp state.
    pub fn reprobe(&mut self) {
        self.has_git_repo = self.cwd.join(".git").exists();
        self.has_docker = which::which("docker").is_ok();
        // has_lsp_running is managed externally — not touched by reprobe
        self.has_python = which::which("python3").is_ok() || which::which("python").is_ok();
        self.has_node = which::which("node").is_ok();
    }
}

/// Combined filter for conditional tool registration.
#[derive(Debug, Clone)]
pub struct ToolFilter {
    pub platform: PlatformEnv,
    pub provider_caps: ProviderCaps,
    pub runtime: RuntimeEnv,
}

impl ToolFilter {
    /// Everything enabled — used by benchmarks and when no filtering is needed.
    pub fn full(cwd: PathBuf) -> Self {
        Self {
            platform: PlatformEnv {
                is_unix: cfg!(unix),
                has_pwsh: true,
                has_cmd: cfg!(windows),
                has_tmux: true,
                has_iterm2: false,
            },
            provider_caps: ProviderCaps::full(),
            runtime: RuntimeEnv::full(cwd),
        }
    }

    /// Probe everything from the current environment.
    pub fn probe(provider_caps: ProviderCaps, cwd: PathBuf) -> Self {
        Self {
            platform: PlatformEnv::probe(),
            provider_caps,
            runtime: RuntimeEnv::probe(cwd),
        }
    }

    /// Re-probe platform and runtime (shells, tools, terminals).
    /// Provider caps are not touched — update those via `filter.provider_caps = ...`.
    /// LSP running state is preserved (managed externally).
    pub fn reprobe(&mut self) {
        self.platform.reprobe();
        self.runtime.reprobe();
    }

    /// Whether any tools should be registered at all.
    pub const fn should_register_tools(&self) -> bool {
        self.provider_caps.supports_tools
    }

    /// Whether complex-schema tools (LSP suite, notebook, multiedit) should be offered.
    pub const fn should_register_complex_tools(&self) -> bool {
        self.provider_caps.supports_tools && !self.provider_caps.is_local
    }

    /// Whether git tools should be registered.
    pub const fn should_register_git(&self) -> bool {
        self.runtime.has_git_repo
    }

    /// Whether LSP tools should be registered.
    pub const fn should_register_lsp(&self) -> bool {
        self.runtime.has_lsp_running && self.should_register_complex_tools()
    }

    /// Whether PowerShell tool should be registered.
    pub const fn should_register_pwsh(&self) -> bool {
        self.platform.has_pwsh
    }

    /// Whether cmd.exe tool should be registered.
    pub const fn should_register_cmd(&self) -> bool {
        self.platform.has_cmd
    }

    /// Whether notebook/script tools requiring Python should be registered.
    pub const fn should_register_python(&self) -> bool {
        self.runtime.has_python
    }
}
