# rustycode-guard

Hook-based security and operation gating system for RustyCode.

## Purpose

Provides a comprehensive security framework for validating and controlling tool/command execution through hook-based rules. Implements 15+ security rules covering: sudo detection, protected path access, dangerous command patterns (rm -rf, git push --force), secret detection, path traversal, and content validation. Enables both deny decisions and interactive approval workflows.

## Key Types

- `HookInput` — Input data for hook evaluation (tool name, command, path, context)
- `HookResult` — Result of hook evaluation (deny, ask, warn, allow)
- `ToolGate` — Access control trait for role-based permissions
- Pre/Post/Permission hooks — Different hook types for different lifecycle stages
- Rule system (15+ built-in rules) — Configurable security policies

## Public API

```rust
use rustycode_guard::{pre_tool, HookInput, HookResult};

let input = HookInput {
    tool_name: "Bash".to_string(),
    tool_input: serde_json::json!({"command": "rm -rf /"}),
    // ... other fields
};

let result = pre_tool::evaluate(&input);
if result.permission_decision.is_some() {
    println!("Denied: {}", result.permission_decision_reason.unwrap());
}
```

## Dependencies

- `serde`/`serde_json` — Serialization/deserialization
- `anyhow` — Error handling
- `thiserror` — Error types

## Built-in Security Rules

- **R01**: Blocks sudo execution
- **R02**: Blocks access to protected paths (.env, .git, etc.)
- **R05**: Blocks `rm -rf` pattern
- **R06**: Blocks `git push --force`
- **R07**: Detects secrets in content (API keys, tokens, private keys)
- **R09**: Prevents path traversal attacks
- **R10**: Blocks `--no-verify` flag
- **R11**: Blocks `git reset --hard` to main/master
- **R12**: Blocks `git push origin main/master`
- **R15**: Validates content size (max 10MB default)

## Architecture Notes

- Sits at the boundary between user input and tool execution
- Complements `rustycode-thread-guard` for thread safety
- Used in TUI event handlers and CLI tool execution
- Extensible rule system for adding custom policies
- Interactive approval workflow support

## Testing

- Comprehensive rule coverage tests
- Integration tests for realistic scenarios
- Secret detection tests (API keys, tokens, private keys)
- Path validation tests

## See Also

- `rustycode-tools` — Tool execution framework
- `rustycode-thread-guard` — Thread safety checks
