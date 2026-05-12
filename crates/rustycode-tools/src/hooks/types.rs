use serde::{Deserialize, Serialize};
use std::path::PathBuf;

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
