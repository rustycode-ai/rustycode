//! Hook system for lifecycle event extensibility
//!
//! This module provides:
//! - Configurable hooks that execute at lifecycle events (`PreToolUse`, `PostToolUse`, etc.)
//! - JSON stdin/stdout protocol for hook scripts
//! - Blocking semantics (hooks can prevent tool execution)
//! - Security profiles (Minimal, Standard, Strict)
//! - Claude Code / Codex compatible config format with matcher filtering
//! - Rich per-event protocol with mutable PostToolUse output

pub mod config;
pub mod env;
pub mod manager;
pub mod matcher;
pub mod protocol;
pub mod types;

pub use manager::HookManager;
pub use types::{
    Hook, HookAction, HookExecutionResult, HookInput, HookOutput, HookProfile, HookResult,
    HookStatus, HookTrigger, HooksConfig,
};
