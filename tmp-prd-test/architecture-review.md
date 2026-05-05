# Architecture Review: `crates/rustycode-tools/src/security/`

## Overview
~5,200 LOC across 8 modules. Centralized security validation for all tool execution.

## Module Map

| Module | LOC | Responsibility |
|--------|-----|----------------|
| `validation.rs` | ~1313 | Path validation, symlink detection, input sanitization |
| `permission.rs` | ~750 | `PermissionManager`, runtime allow/deny decisions |
| `patterns.rs` | ~749 | `ThreatScanner`, security pattern matching |
| `approve.rs` | ~732 | `SmartApprove`, interactive user approval flow |
| `permission_store.rs` | ~553 | Persistent permission records |
| `sandbox.rs` | ~593 | OS-level sandboxing (Landlock/macOS sandbox) |
| `trust.rs` | ~518 | `DirectoryTrust`, per-directory trust levels |
| `cross_platform.rs` | ~250 | Windows/WSL/Cygwin path normalization |

## Key Patterns (Good)

- **Defense in depth**: validation → permission → sandbox → trust — four layers before tool executes
- **Explicit resource limits**: `MAX_FILE_SIZE`, `MAX_PATH_LENGTH`, `MAX_RECURSION_DEPTH`, `MAX_REGEX_MATCHES` prevent DoS
- **Blocked lists**: extensions (`.env`, `.key`, `.pem`) and filenames (`credentials.json`, `.netrc`) are deny-first
- **Cross-platform awareness**: dedicated `cross_platform.rs` handles Windows/WSL/Cygwin edge cases
- **Persistent permissions**: `PermissionStore` avoids re-prompting users for previously approved actions

## Concerns

1. **Ambiguous glob re-exports**: `mod.rs` uses `pub use X::*` for all 8 modules with `#[allow(ambiguous_glob_reexports)]`. Two different `RiskLevel` types (in `patterns` and `permission`) collide — callers must use fully-qualified paths. This is fragile.
2. **`validation.rs` is oversized at 1313 LOC**: mixes path validation, symlink resolution, command sanitization, and resource limits. Should split into focused sub-modules.
3. **Sync std::fs in validation**: uses `std::fs` (blocking) rather than `tokio::fs` — blocks the async runtime during path checks.
4. **No audit logging**: security decisions (allow/deny/sandbox enforcement) aren't logged to `EventBus` for observability.
5. **Sandbox fallback is silent**: `SandboxLevel::Strict` silently degrades to `Path` on unsupported platforms without warning the user.

## Recommended Improvements

1. Replace glob re-exports with explicit named re-exports to eliminate `RiskLevel` ambiguity
2. Split `validation.rs` into `path.rs`, `command.rs`, `sanitizer.rs`
3. Switch `std::fs` calls to `tokio::fs` in validation path
4. Emit security events to `rustycode-bus::EventBus` for audit trail
5. Add `warn!()` log when sandbox degrades from Strict → Path
