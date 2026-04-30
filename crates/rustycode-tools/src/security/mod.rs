//! Security module group (~21K LOC total)
//!
//! Sub-modules:
//! - validation.rs (~1313 LOC) — core path validation, sanitization, symlink detection
//! - permission.rs (~750 LOC) — `PermissionManager`, `PermissionAction`
//! - `permission_store.rs` (~553 LOC) — `PermissionRecord`, `PermissionStore`
//! - patterns.rs (~749 LOC) — security pattern matching, `ThreatScanner`
//! - sandbox.rs (~593 LOC) — Sandbox, `SandboxLevel`
//! - trust.rs (~518 LOC) — `DirectoryTrust`, `TrustEntry`
//! - approve.rs (~732 LOC) — `SmartApprove`
//! - `cross_platform.rs` — cross-platform path and command validation for Windows/WSL/Cygwin

pub mod approve;
pub mod cross_platform;
pub mod patterns;
pub mod permission;
pub mod permission_store;
pub mod sandbox;
pub mod trust;
pub mod validation;

// Re-export all public items from sub-modules so `crate::security::X` still resolves.
// This preserves the import paths used by downstream modules (edit.rs, apply_patch.rs, etc.)
// that import via `use crate::security::{validate_path, ...}`.
//
// Note: `patterns::RiskLevel` and `permission::RiskLevel` are different types with
// different variants. The glob re-export is ambiguous for `RiskLevel`; consumers
// should use the fully-qualified path (e.g., `crate::security::patterns::RiskLevel`)
// or the backward-compatible alias `crate::security_patterns::RiskLevel`.
#[allow(ambiguous_glob_reexports)]
pub use approve::*;
#[allow(ambiguous_glob_reexports)]
pub use cross_platform::*;
#[allow(ambiguous_glob_reexports)]
pub use patterns::*;
#[allow(ambiguous_glob_reexports)]
pub use permission::*;
#[allow(ambiguous_glob_reexports)]
pub use permission_store::*;
#[allow(ambiguous_glob_reexports)]
pub use sandbox::*;
#[allow(ambiguous_glob_reexports)]
pub use trust::*;
#[allow(ambiguous_glob_reexports)]
pub use validation::*;
